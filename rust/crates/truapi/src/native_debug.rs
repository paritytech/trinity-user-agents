//! Native (non-wasm) [`DebugSink`]: streams tapped frames to a loopback
//! `@parity/truapi-debugger` over a WebSocket.
//!
//! The native counterpart of `WasmDebugSink`: a dumb,
//! payload-blind byte-forwarder. Each [`DebugEvent::Frame`] is serialized to the
//! debugger's wire envelope - `{channelId, dir, frame}`, where `frame` is the
//! base64 of the untouched SCALE `ProtocolMessage` bytes - and sent as one WS
//! text message. Decoding lives in the debugger app, never here.
//!
//! Fire-and-forget by construction, per the [`DebugSink`] contract:
//! [`WsDebugSink::emit`] never blocks and never fails a dispatch. It only
//! serializes and pushes onto a bounded queue; a background task owns the socket,
//! reconnects with capped backoff, and drops frames (counted) when the queue is
//! full. A slow, absent, or crashed debugger loses traces, never a session.
//! Dropped frames are reported on the wire: the count shed since the previous
//! envelope rides the next one as `dropped`, so the debugger attributes the gap
//! to the link instead of reading it as a host that never answered.
//!
//! Localhost only: the target URL must be `ws://` on a loopback host. No `wss`,
//! no certificates, no LAN. Construct via [`WsDebugSink::connect`] from within a
//! Tokio runtime and install with [`crate::ProductRuntime::set_debug_sink`];
//! constructing one is a dev-only opt-in, so a host that never calls it leaves
//! the tap inert.

use core::{
	net::SocketAddr,
	sync::atomic::{AtomicU64, AtomicUsize, Ordering},
	time::Duration,
};
use std::sync::Arc;

use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use futures::{SinkExt, StreamExt};
use serde::Serialize;
use thiserror::Error;
use tokio::{net::TcpStream, runtime::Handle, sync::mpsc};
use tokio_tungstenite::{WebSocketStream, client_async, tungstenite::Message};
use tracing::debug;

use crate::{
	generated::wire_table::TRUAPI_WIRE_SCHEMA_HASH,
	host_core::{DebugEvent, DebugSink},
};

/// Bounded so a stalled or absent debugger applies backpressure as counted
/// drops, never unbounded memory growth on the observed session.
const QUEUE_CAPACITY: usize = 4096;

/// Byte budget alongside [`QUEUE_CAPACITY`]: one `ProtocolMessage` can be MBs, so
/// a count-only cap could still buffer unbounded RSS while the debugger is
/// absent. Whichever ceiling hits first drops the frame (counted), never blocks.
const MAX_QUEUE_BYTES: usize = 8 * 1024 * 1024;

/// Envelope version, mirroring the debugger's `WIRE_ENVELOPE_VERSION` and the web
/// host's constant. Kept in sync by hand.
const WIRE_ENVELOPE_VERSION: u32 = 1;

/// The host's wire codec version, matching `truapi::WIRE_CODEC_VERSION` (the
/// handshake `codec_version`). Stamped on the envelope so the debugger refuses
/// to decode a frame whose codec differs from its own, rather than resolving a
/// `(trait, method)` pair against the wrong contract.
const WIRE_CODEC_VERSION: u32 = truapi::WIRE_CODEC_VERSION as u32;

/// Port the debugger's server listens on (`@parity/truapi-debugger`'s
/// `npm run serve`), used when the debug URL omits one so `ws://localhost`
/// reaches the debugger instead of HTTP's port 80.
const DEBUGGER_DEFAULT_PORT: u16 = 9231;

/// Initial reconnect delay; doubles on each failed dial up to [`MAX_BACKOFF`].
const INITIAL_BACKOFF: Duration = Duration::from_millis(200);

/// Cap on the reconnect backoff.
const MAX_BACKOFF: Duration = Duration::from_secs(5);

/// Cap on a single dial + WS handshake; a port that accepts TCP but never
/// completes the upgrade must not park the writer task forever.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);

/// Failure building a [`WsDebugSink`].
#[derive(Debug, Error)]
pub enum DebugSinkError {
	/// The debug URL did not parse.
	#[error("invalid debug url: {0}")]
	Url(#[from] url::ParseError),
	/// The debug URL was not `ws://` on a loopback host.
	#[error("debug url must be ws:// on a loopback host, got {0}")]
	NotLoopback(String),
	/// The debug URL host could not be resolved.
	#[error("could not resolve debug url host: {0}")]
	Resolve(#[from] std::io::Error),
	/// `connect` was called outside a Tokio runtime.
	#[error("WsDebugSink::connect must be called from within a Tokio runtime")]
	NoRuntime,
}

/// A dev-only [`DebugSink`] that forwards tapped frames to a loopback debugger
/// over a WebSocket, using the same `{channelId, dir, frame: base64}` envelope
/// the browser host sends.
pub struct WsDebugSink {
	outbound: mpsc::Sender<QueuedFrame>,
	dropped: Arc<AtomicU64>,
	pending_dropped: Arc<AtomicU64>,
	queued_bytes: Arc<AtomicUsize>,
}

/// One serialized envelope on its way to the writer task, plus the number of
/// shed frames stamped on it. Carrying the count alongside the line lets the
/// writer put it back if this envelope dies with the socket, so a drop is
/// reported exactly once and never silently swallowed.
struct QueuedFrame {
	line: String,
	shed: u64,
}

/// The wire envelope, matching the debugger's `parseWireMessage` / ingest
/// `DebugFrameEnvelope`: `dir` is product-vantage, `frame` is base64 SCALE bytes.
/// `v`/`codec` are the identity the debugger checks before decoding.
#[derive(Serialize)]
struct WireMessage<'a> {
	v: u32,
	codec: u32,
	schema: &'static str,
	#[serde(rename = "channelId")]
	channel_id: &'a str,
	dir: &'a str,
	/// Unix milliseconds at which this link saw the frame, stamped when the
	/// frame is enqueued rather than when it is written. This link buffers up to
	/// `QUEUE_CAPACITY` frames and reconnects with backoff, so a flushed backlog
	/// arrives at the debugger all at once: without this, every duration derived
	/// from arrival time collapses to the flush instant. Omitted when the clock
	/// cannot be read, so a failure reads as absent rather than as 1970.
	#[serde(rename = "observedAt", skip_serializing_if = "Option::is_none")]
	observed_at: Option<u64>,
	frame: String,
	/// Frames this link shed since the previous envelope. Omitted when zero, as
	/// the web link omits it, so the common envelope is unchanged; the debugger
	/// sums it per channel into `droppedByHost`.
	#[serde(skip_serializing_if = "is_zero")]
	dropped: u64,
}

fn is_zero(count: &u64) -> bool {
	*count == 0
}

/// Unix milliseconds now, or `None` if the clock predates the epoch. Runs on the
/// frame path, so it reads the clock and nothing else: no allocation, no lock,
/// and no `unwrap` that could panic in a caller's dispatch.
fn observed_at_millis() -> Option<u64> {
	std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.ok()
		.map(|since| since.as_millis() as u64)
}

/// Validate a debug URL and resolve it to the addresses to dial, in resolver
/// order.
///
/// Requires `ws://`, then RESOLVES the host and requires *every* resolved
/// address to be loopback. Resolving (rather than string-matching the host)
/// accepts all genuine loopback forms - 127.0.0.0/8, ::1, and a `localhost` that
/// resolves to them - and rejects anything resolving off-loopback, closing the
/// "validate one string, dial another" gap. `Url::socket_addrs` also handles
/// IPv6 bracket-stripping.
///
/// The port default is applied by hand rather than through `socket_addrs`'s
/// fallback closure: `ws` is a *special* scheme in the URL spec with a known
/// default of 80, so the closure is never consulted and a portless
/// `ws://127.0.0.1` would dial :80 instead of the debugger.
fn resolve_loopback_target(url: &str) -> Result<Vec<SocketAddr>, DebugSinkError> {
	let mut parsed = url::Url::parse(url)?;
	if parsed.scheme() != "ws" {
		return Err(DebugSinkError::NotLoopback(url.to_string()));
	}
	if parsed.port().is_none() {
		parsed
			.set_port(Some(DEBUGGER_DEFAULT_PORT))
			.map_err(|()| DebugSinkError::NotLoopback(url.to_string()))?;
	}
	let addrs = parsed.socket_addrs(|| Some(DEBUGGER_DEFAULT_PORT))?;
	if !all_loopback(&addrs) {
		return Err(DebugSinkError::NotLoopback(url.to_string()));
	}
	Ok(addrs)
}

/// Whether every resolved address is loopback, and there is at least one.
///
/// `all`, not `any`: a name that resolves to a mix of loopback and routable
/// addresses must be refused outright. Accepting it would let a split-horizon or
/// rebinding resolver hand us one loopback address to pass the check and a
/// routable one to dial. Lifted out of [`resolve_loopback_target`] so the mixed
/// case is testable without a resolver that produces one.
fn all_loopback(addrs: &[SocketAddr]) -> bool {
	!addrs.is_empty() && addrs.iter().all(|addr| addr.ip().is_loopback())
}

impl WsDebugSink {
	/// Build a sink targeting `url` and spawn its writer task.
	///
	/// `url` must be `ws://` on `127.0.0.1`, `localhost`, or `[::1]`. Returns
	/// immediately even if the debugger is not yet listening; the writer task
	/// dials lazily and reconnects. Must be called from within a Tokio runtime.
	pub fn connect(url: &str) -> Result<Arc<Self>, DebugSinkError> {
		// Capture *every* resolved loopback address and dial those directly (in
		// `writer_loop`), rather than re-resolving the URL string on each dial.
		// The WS handshake is therefore only ever sent to a checked loopback
		// peer - closing the resolve-then-dial gap where a mid-session resolver
		// change could send the handshake off-box.
		let addrs = resolve_loopback_target(url)?;

		// Return a Result rather than panicking inside tokio::spawn when called
		// outside a runtime.
		if Handle::try_current().is_err() {
			return Err(DebugSinkError::NoRuntime);
		}

		let (outbound, inbox) = mpsc::channel::<QueuedFrame>(QUEUE_CAPACITY);
		let dropped = Arc::new(AtomicU64::new(0));
		let pending_dropped = Arc::new(AtomicU64::new(0));
		let queued_bytes = Arc::new(AtomicUsize::new(0));
		tokio::spawn(writer_loop(
			url.to_string(),
			addrs,
			inbox,
			Arc::clone(&dropped),
			Arc::clone(&pending_dropped),
			Arc::clone(&queued_bytes),
		));
		Ok(Arc::new(Self { outbound, dropped, pending_dropped, queued_bytes }))
	}

	/// Number of frames dropped because the outbound queue was full (debugger
	/// absent or slower than the observed session). Never affects the session.
	pub fn dropped(&self) -> u64 {
		self.dropped.load(Ordering::Relaxed)
	}

	/// The count waiting to ride the next envelope. Test-only: the re-add in
	/// [`Self::count_drop`] is load-bearing and is otherwise only observable
	/// through a live socket, which makes it easy to leave untested.
	#[cfg(test)]
	fn pending_dropped(&self) -> u64 {
		self.pending_dropped.load(Ordering::Relaxed)
	}

	/// Account for one lost frame: `carried` drops had been drained onto the
	/// envelope that never made it, so they go back on the pending count
	/// alongside this one and ride the next envelope instead. Returns the new
	/// lifetime total, for logging.
	fn count_drop(&self, carried: u64) -> u64 {
		self.pending_dropped.fetch_add(carried + 1, Ordering::Relaxed);
		self.dropped.fetch_add(1, Ordering::Relaxed) + 1
	}
}

impl DebugSink for WsDebugSink {
	fn emit(&self, event: DebugEvent) {
		let DebugEvent::Frame { channel_id, dir, bytes } = event;
		// Drain the drops accumulated since the previous envelope and stamp them
		// on this one, as the web link does: a shed frame must reach the debugger
		// as a counted gap in the link, not as a host that never answered. If
		// this envelope is itself lost, `count_drop` puts the count back so it
		// rides the next one.
		let shed = self.pending_dropped.swap(0, Ordering::Relaxed);
		let message = WireMessage {
			v: WIRE_ENVELOPE_VERSION,
			codec: WIRE_CODEC_VERSION,
			schema: TRUAPI_WIRE_SCHEMA_HASH,
			channel_id: &channel_id.0,
			// Product-vantage string; never hand-mapped, so it cannot invert.
			dir: dir.wire_str(),
			observed_at: observed_at_millis(),
			frame: BASE64.encode(&bytes),
			dropped: shed,
		};
		let Ok(line) = serde_json::to_string(&message) else {
			self.count_drop(shed);
			return;
		};
		// Byte budget on top of the channel's count cap: one frame can be MBs, so
		// a count-only bound could still grow RSS without limit while the debugger
		// is absent. Reserve the frame's bytes BEFORE handing the line to the
		// channel: the writer task can recv and release (fetch_sub) the instant
		// try_send succeeds, so adding *after* would let that sub run first and
		// wrap the counter - an overflow panic in debug builds, on the frame path.
		// Reserve atomically, then release on any failure.
		let len = line.len();
		if self.queued_bytes.fetch_add(len, Ordering::Relaxed) + len > MAX_QUEUE_BYTES {
			// This reservation pushed us past the budget: back it out and drop.
			self.queued_bytes.fetch_sub(len, Ordering::Relaxed);
			let dropped = self.count_drop(shed);
			debug!("truapi debug sink: byte budget full, frame dropped (total {dropped})");
			return;
		}
		if self.outbound.try_send(QueuedFrame { line, shed }).is_err() {
			// Not enqueued after all: release the reservation. The frame is lost
			// (never the session); count it and log so the gap is attributable to
			// the link, not to the host.
			self.queued_bytes.fetch_sub(len, Ordering::Relaxed);
			let dropped = self.count_drop(shed);
			debug!("truapi debug sink: outbound queue full, frame dropped (total {dropped})");
		}
	}
}

/// Dial the pre-validated loopback candidates in resolver order and return the
/// first socket that completes the WS handshake.
///
/// Trying every candidate is what makes `ws://localhost:9231` work: `localhost`
/// commonly resolves to `::1` first while the debugger binds v4 only, so pinning
/// the first address would retry an address that can never deliver, forever.
/// Every candidate was checked as loopback in [`resolve_loopback_target`], the
/// addresses are not re-resolved, and the handshake runs over the
/// already-connected socket, so it can never reach an off-box peer. Each attempt
/// is bounded so a TCP-accepting but non-upgrading port can't park the task.
async fn dial(url: &str, addrs: &[SocketAddr]) -> Option<WebSocketStream<TcpStream>> {
	for addr in addrs {
		let dialed = tokio::time::timeout(HANDSHAKE_TIMEOUT, async {
			let tcp = TcpStream::connect(addr).await.ok()?;
			client_async(url, tcp).await.ok()
		})
		.await;
		match dialed {
			Ok(Some((stream, _response))) => return Some(stream),
			Ok(None) => debug!("truapi debug sink: dial/handshake to {addr} failed"),
			Err(_) => debug!("truapi debug sink: handshake to {addr} timed out"),
		}
	}
	None
}

/// Own the socket for the sink's lifetime: dial with capped backoff, then drain
/// the queue to the wire until the sink is dropped.
async fn writer_loop(
	url: String,
	addrs: Vec<SocketAddr>,
	mut inbox: mpsc::Receiver<QueuedFrame>,
	dropped: Arc<AtomicU64>,
	pending_dropped: Arc<AtomicU64>,
	queued_bytes: Arc<AtomicUsize>,
) {
	let mut backoff = INITIAL_BACKOFF;
	loop {
		let Some(stream) = dial(url.as_str(), &addrs).await else {
			tokio::time::sleep(backoff).await;
			backoff = (backoff * 2).min(MAX_BACKOFF);
			// The sink was dropped while we were retrying: give up.
			if inbox.is_closed() {
				return;
			}
			continue;
		};
		let (mut write, mut read) = stream.split();
		// Drain queued frames to the wire, and also poll the read half so
		// tokio-tungstenite answers server pings and observes a Close; being
		// forward-only, any inbound message is ignored. Reset backoff only on a
		// *delivered* frame, so an accept-then-close server still backs off
		// instead of spinning on zero-delay reconnects.
		loop {
			tokio::select! {
				queued = inbox.recv() => match queued {
					Some(QueuedFrame { line, shed }) => {
						// Off the queue now: release its bytes from the budget
						// before the (moving) send so the counter can't drift.
						queued_bytes.fetch_sub(line.len(), Ordering::Relaxed);
						match write.send(Message::Text(line)).await {
							Ok(()) => backoff = INITIAL_BACKOFF,
							Err(_) => {
								debug!("truapi debug sink: socket closed, reconnecting");
								// The in-flight line is lost across this reconnect.
								dropped.fetch_add(1, Ordering::Relaxed);
								// It carried `shed` earlier drops that therefore
								// never reached the debugger: make them pending
								// again (with this frame) so the next delivered
								// envelope still reports the whole gap.
								pending_dropped.fetch_add(shed + 1, Ordering::Relaxed);
								break;
							}
						}
					}
					// All senders dropped: the sink is gone, so is the host. Done.
					None => return,
				},
				inbound = read.next() => match inbound {
					Some(Ok(_)) => {} // forward-only: ignore any inbound message
					Some(Err(_)) | None => {
						debug!("truapi debug sink: read side closed, reconnecting");
						break;
					}
				},
			}
		}
		// Reconnect after an established socket dropped: back off here too.
		tokio::time::sleep(backoff).await;
		backoff = (backoff * 2).min(MAX_BACKOFF);
		if inbox.is_closed() {
			return;
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::host_core::{ChannelId, FrameDirection};

	use tokio::{net::TcpListener, sync::oneshot};
	use tokio_tungstenite::accept_async;

	#[tokio::test]
	async fn emits_base64_envelope_with_product_vantage_dir() {
		let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
		let port = listener.local_addr().unwrap().port();

		// Server side: accept one connection, capture the first text message.
		let (tx, rx) = oneshot::channel::<String>();
		tokio::spawn(async move {
			let (stream, _peer) = listener.accept().await.unwrap();
			let ws = accept_async(stream).await.unwrap();
			let (_write, mut read) = ws.split();
			let message = read.next().await.unwrap().unwrap();
			tx.send(message.into_text().unwrap()).unwrap();
		});

		let sink = WsDebugSink::connect(&format!("ws://127.0.0.1:{port}")).unwrap();
		// `In` = product→core, i.e. the frame *left* the product → product-vantage "out".
		sink.emit(DebugEvent::Frame {
			channel_id: ChannelId("myapp.dot".to_string()),
			dir: FrameDirection::In,
			bytes: vec![1, 2, 3, 4],
		});

		let text = tokio::time::timeout(Duration::from_secs(5), rx)
			.await
			.expect("debugger did not receive a frame")
			.unwrap();
		let value: serde_json::Value = serde_json::from_str(&text).unwrap();

		assert_eq!(value["channelId"], "myapp.dot");
		// Identity the debugger checks before decoding. Asserted as literals: a
		// constant on both sides would agree with itself even if the value the
		// debugger expects changed.
		assert_eq!(value["v"], 1);
		assert_eq!(value["codec"], 3);
		assert_eq!(value["v"], WIRE_ENVELOPE_VERSION);
		assert_eq!(value["codec"], WIRE_CODEC_VERSION);
		assert_eq!(value["schema"], TRUAPI_WIRE_SCHEMA_HASH);
		// Guard against re-inversion: In must serialize as product-vantage "out".
		assert_eq!(value["dir"], FrameDirection::In.wire_str());
		assert_eq!(value["dir"], "out");
		assert_eq!(value["frame"], BASE64.encode([1, 2, 3, 4]));
		// Nothing was shed, so the envelope stays exactly as the web link's:
		// `dropped` is absent rather than a noisy zero.
		assert!(
			value.get("dropped").is_none(),
			"a frame with no preceding drops must not carry a dropped count"
		);
	}

	/// `observedAt` is the link's clock when it took the frame, not when the frame
	/// reached the debugger. Proven against a debugger that accepts late: the
	/// frame sits in the queue while the socket is down, so an arrival-time stamp
	/// would read the flush instant and collapse the gap this test opens.
	#[tokio::test]
	async fn observed_at_is_stamped_when_the_frame_is_taken_not_when_it_flushes() {
		const ACCEPT_DELAY: Duration = Duration::from_millis(500);

		let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
		let port = listener.local_addr().unwrap().port();

		let (tx, rx) = oneshot::channel::<String>();
		tokio::spawn(async move {
			// Hold the frame in the sink's queue: the sink dials, the accept waits.
			tokio::time::sleep(ACCEPT_DELAY).await;
			let (stream, _peer) = listener.accept().await.unwrap();
			let ws = accept_async(stream).await.unwrap();
			let (_write, mut read) = ws.split();
			let message = read.next().await.unwrap().unwrap();
			tx.send(message.into_text().unwrap()).unwrap();
		});

		let sink = WsDebugSink::connect(&format!("ws://127.0.0.1:{port}")).unwrap();
		let before = observed_at_millis().expect("clock");
		sink.emit(DebugEvent::Frame {
			channel_id: ChannelId("myapp.dot".to_string()),
			dir: FrameDirection::Out,
			bytes: vec![7, 7],
		});
		let after = observed_at_millis().expect("clock");

		let text = tokio::time::timeout(Duration::from_secs(10), rx)
			.await
			.expect("debugger did not receive a frame")
			.unwrap();
		let arrived = observed_at_millis().expect("clock");
		let value: serde_json::Value = serde_json::from_str(&text).unwrap();

		let observed = value["observedAt"].as_u64().expect("envelope carries observedAt");
		assert!(
			(before..=after).contains(&observed),
			"observedAt {observed} is outside the emit window {before}..={after}"
		);
		// The flush really did happen later, so the previous assertion is not
		// vacuous: an arrival stamp would have landed here instead.
		assert!(
			arrived - observed >= ACCEPT_DELAY.as_millis() as u64 / 2,
			"expected a gap between the stamp and the flush; stamped {observed}, arrived {arrived}"
		);
	}

	/// A drop that happens while an earlier drop is still pending must ADD to the
	/// pending count, not replace it. `count_drop(carried)` re-adds the `carried`
	/// drops that had already been drained onto the envelope now being discarded -
	/// without that, the first drop is silently swallowed and the wire reports one
	/// loss where two happened.
	///
	/// Asserted on the pending counter rather than the wire because the wire only
	/// shows the total, and a total of "1 then 1" is indistinguishable from
	/// "1 then 2" unless you catch both envelopes.
	///
	/// Covers the BUDGET path only. Two sibling invariants remain uncovered, stated
	/// here rather than left to be rediscovered:
	///   - `writer_loop`'s reconnect re-add (`pending_dropped.fetch_add(shed + 1)`) is the same
	///     invariant on the socket-death path. Reaching it needs a live socket killed while an
	///     envelope carrying a shed count is in flight; mutating that `shed + 1` to `1` passes
	///     today.
	///   - the byte budget's `>` boundary. Hitting exactly `MAX_QUEUE_BYTES` needs a frame sized
	///     against base64 plus JSON overhead; `>` vs `>=` passes today.
	///   - the ATOMICITY of the re-add. `emit` always swaps pending to 0 before calling
	///     `count_drop`, so this test never observes a non-zero pending and `fetch_add` -> `store`
	///     passes. `store` would clobber a concurrent re-add from `writer_loop`, which is the whole
	///     reason it is `fetch_add`.

	#[tokio::test]
	async fn a_drop_while_one_is_pending_accumulates_rather_than_replaces() {
		let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
		let port = listener.local_addr().unwrap().port();
		drop(listener);

		let sink = WsDebugSink::connect(&format!("ws://127.0.0.1:{port}")).unwrap();
		let big = vec![0u8; 4 * 1024 * 1024];
		let emit = |sink: &WsDebugSink| {
			sink.emit(DebugEvent::Frame {
				channel_id: ChannelId("myapp.dot".to_string()),
				dir: FrameDirection::Out,
				bytes: big.clone(),
			});
		};

		// 1st fills the queue's byte budget; 2nd is shed with nothing pending.
		emit(&sink);
		emit(&sink);
		assert_eq!(sink.pending_dropped(), 1, "the first shed is pending");

		// 3rd drains that 1 onto its envelope, is itself shed, and must put the
		// drained count BACK alongside its own.
		emit(&sink);
		assert_eq!(sink.pending_dropped(), 2, "the drained count must be re-added, not replaced");
		assert_eq!(sink.dropped(), 2, "and the lifetime total counts both");
	}

	/// The mixed-resolution case, which no URL-level test reaches: every input a
	/// real resolver produces here is homogeneous (all loopback, or all not), so
	/// `all` vs `any` is indistinguishable through `resolve_loopback_target`. A
	/// name resolving to both must be refused - that is the split-horizon and
	/// DNS-rebinding shape the predicate exists for.
	#[test]
	fn a_mixed_resolution_is_not_loopback() {
		use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

		let loopback_v4 = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 9231);
		let loopback_v6 = SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), 9231);
		let routable = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1)), 9231);

		assert!(all_loopback(&[loopback_v4]));
		assert!(all_loopback(&[loopback_v4, loopback_v6]));
		assert!(!all_loopback(&[]), "an empty resolution is not loopback");
		assert!(!all_loopback(&[routable]));
		// The two orderings that `any` would wrongly accept.
		assert!(
			!all_loopback(&[loopback_v4, routable]),
			"loopback first must not launder a routable sibling"
		);
		assert!(
			!all_loopback(&[routable, loopback_v4]),
			"a routable address must be refused whatever its position"
		);
	}

	/// A shed frame must reach the debugger as a counted gap in the link. The
	/// debugger sums `dropped` per channel into `/stats.droppedByHost`, so
	/// without it a 4096-frame or 8 MiB shed reads as a host that never answered.
	#[tokio::test]
	async fn a_shed_frame_is_reported_as_dropped_on_the_next_envelope() {
		// Reserve a loopback port, then free it: with nothing listening the queue
		// cannot drain, so the byte budget sheds a frame deterministically.
		let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
		let port = listener.local_addr().unwrap().port();
		drop(listener);

		let sink = WsDebugSink::connect(&format!("ws://127.0.0.1:{port}")).unwrap();
		// 4 MiB → ~5.6 MiB of base64 per envelope: the first fits the 8 MiB
		// budget, the second pushes past it and is shed.
		let big = vec![0u8; 4 * 1024 * 1024];
		for _ in 0..2 {
			sink.emit(DebugEvent::Frame {
				channel_id: ChannelId("myapp.dot".to_string()),
				dir: FrameDirection::Out,
				bytes: big.clone(),
			});
		}
		assert_eq!(sink.dropped(), 1, "the byte budget must shed exactly one");

		// Bring the debugger up on that port and let the writer connect.
		let listener = TcpListener::bind(("127.0.0.1", port)).await.unwrap();
		// The reader reports a pair: (count on the envelope that carried it, count
		// on the NEXT envelope).
		// The second is what pins the reset: `dropped` is
		// `skip_serializing_if = "is_zero"`, so once the pending count is drained
		// the following envelope must OMIT the field entirely. Without asserting
		// that, dropping the reset leaves this test green - the first envelope
		// looks identical either way.
		let (tx, rx) = oneshot::channel::<(u64, Option<u64>)>();
		tokio::spawn(async move {
			let (stream, _peer) = listener.accept().await.unwrap();
			let ws = accept_async(stream).await.unwrap();
			let (_write, mut read) = ws.split();
			let mut carried: Option<u64> = None;
			while let Some(Ok(message)) = read.next().await {
				let Ok(text) = message.into_text() else {
					continue;
				};
				let value: serde_json::Value = serde_json::from_str(&text).unwrap();
				let dropped = value["dropped"].as_u64();
				match carried {
					// Still waiting for the envelope that carries the shed count.
					None => {
						if let Some(d) = dropped {
							carried = Some(d);
						}
					},
					// Got it; this is the next envelope after it.
					Some(first) => {
						tx.send((first, dropped)).unwrap();
						return;
					},
				}
			}
		});

		// The shed happened while the queue held an already-serialized envelope,
		// so the count rides the next frame emitted after it - exactly the web
		// link's "piggyback onto the next live frame".
		let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
		let mut rx = rx;
		loop {
			sink.emit(DebugEvent::Frame {
				channel_id: ChannelId("myapp.dot".to_string()),
				dir: FrameDirection::Out,
				bytes: vec![7],
			});
			match tokio::time::timeout(Duration::from_millis(250), &mut rx).await {
				Ok(received) => {
					let (first, next) = received.unwrap();
					assert_eq!(first, 1, "the shed frame must be reported once, on the wire");
					assert_eq!(
						next, None,
						"the pending count must reset after it rides an envelope, \
                         so the next one omits `dropped` entirely"
					);
					return;
				},
				Err(_) => {
					assert!(
						tokio::time::Instant::now() < deadline,
						"no envelope ever carried the shed frame's drop count"
					)
				},
			}
		}
	}

	#[test]
	fn rejects_non_loopback_and_non_ws_urls() {
		// 192.0.2.1 (TEST-NET-1) is a non-loopback IP literal, so no DNS is hit.
		assert!(WsDebugSink::connect("wss://127.0.0.1:9231").is_err());
		assert!(WsDebugSink::connect("ws://192.0.2.1:9231").is_err());
		assert!(WsDebugSink::connect("http://127.0.0.1:9231").is_err());
		assert!(WsDebugSink::connect("not a url").is_err());
	}

	#[tokio::test]
	async fn accepts_loopback_forms_at_validation() {
		for url in ["ws://127.0.0.1:9231", "ws://localhost:9231", "ws://[::1]:9231"] {
			assert!(WsDebugSink::connect(url).is_ok(), "should accept {url}");
		}
	}

	/// Accepting a URL is not the same as being able to deliver on it: on macOS
	/// `localhost` resolves to `::1` first while the debugger binds v4 only, so a
	/// sink that pins the first resolved address retries an address that can
	/// never deliver, forever. Every candidate must be tried.
	#[tokio::test]
	async fn delivers_through_localhost_to_a_v4_only_debugger() {
		let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
		let port = listener.local_addr().unwrap().port();
		// The bug only exists when the name resolves to something before the v4
		// address; on a v4-only resolver this still passes, it just proves less.
		let resolved = resolve_loopback_target(&format!("ws://localhost:{port}")).unwrap();
		assert!(!resolved.is_empty(), "localhost must resolve to at least one loopback address");

		let (tx, rx) = oneshot::channel::<String>();
		tokio::spawn(async move {
			let (stream, _peer) = listener.accept().await.unwrap();
			let ws = accept_async(stream).await.unwrap();
			let (_write, mut read) = ws.split();
			let message = read.next().await.unwrap().unwrap();
			tx.send(message.into_text().unwrap()).unwrap();
		});

		let sink = WsDebugSink::connect(&format!("ws://localhost:{port}")).unwrap();
		sink.emit(DebugEvent::Frame {
			channel_id: ChannelId("myapp.dot".to_string()),
			dir: FrameDirection::Out,
			bytes: vec![9],
		});

		let text = tokio::time::timeout(Duration::from_secs(20), rx)
			.await
			.expect("a v4-only debugger never received the frame via localhost")
			.unwrap();
		let value: serde_json::Value = serde_json::from_str(&text).unwrap();
		assert_eq!(value["frame"], BASE64.encode([9]));
	}

	/// A port-less debug URL must target the debugger, not HTTP's port 80.
	#[test]
	fn a_url_without_a_port_targets_the_debugger_port() {
		let addrs = resolve_loopback_target("ws://127.0.0.1").unwrap();
		assert_eq!(addrs.first().unwrap().port(), 9231);
		for addr in resolve_loopback_target("ws://localhost").unwrap() {
			assert_eq!(addr.port(), 9231, "every candidate uses the default port");
		}
	}

	/// The codec version stamped on the envelope is hand-mirrored from the
	/// generated TS `TRUAPI_CODEC_VERSION` (codegen emits only the schema hash to
	/// Rust). Bind it to the Rust-side authority on the same number: the codec
	/// version this host accepts in the handshake. A `--codec-version` bump that
	/// forgets this constant then fails here instead of stamping a frame the
	/// debugger reads as a foreign contract.
	#[test]
	fn stamped_codec_version_is_the_one_the_host_negotiates() {
		use truapi::{
			CallContext, CallError,
			api::System,
			v01,
			versioned::system::{
				HostFeatureSupportedError, HostFeatureSupportedRequest,
				HostFeatureSupportedResponse, HostHandshakeRequest, HostInfoError, HostInfoRequest,
				HostInfoResponse, HostNavigateToError, HostNavigateToRequest,
				HostNavigateToResponse,
			},
		};

		/// Exercises only `System::handshake`'s default (host-side) codec check.
		struct HandshakeOnly;

		#[truapi::async_trait]
		impl System for HandshakeOnly {
			async fn feature_supported(
				&self,
				_cx: &CallContext,
				_request: HostFeatureSupportedRequest,
			) -> Result<HostFeatureSupportedResponse, CallError<HostFeatureSupportedError>> {
				unreachable!("handshake-only host")
			}

			async fn navigate_to(
				&self,
				_cx: &CallContext,
				_request: HostNavigateToRequest,
			) -> Result<HostNavigateToResponse, CallError<HostNavigateToError>> {
				unreachable!("handshake-only host")
			}

			async fn host_info(
				&self,
				_cx: &CallContext,
				_request: HostInfoRequest,
			) -> Result<HostInfoResponse, CallError<HostInfoError>> {
				unreachable!("handshake-only host")
			}
		}

		let handshake = |codec: u32| {
			let cx = CallContext::with_request_id("codec:1".to_string());
			let codec_version = u8::try_from(codec).expect("codec version fits a u8");
			futures::executor::block_on(HandshakeOnly.handshake(
				&cx,
				HostHandshakeRequest::V1(v01::HostHandshakeRequest { codec_version }),
			))
		};

		assert!(
			handshake(WIRE_CODEC_VERSION).is_ok(),
			"the host must accept the codec version its debug envelopes stamp"
		);
		assert!(
			handshake(WIRE_CODEC_VERSION + 1).is_err(),
			"the stamped codec version must be the newest one the host accepts"
		);
	}

	#[tokio::test]
	async fn emit_is_nonblocking_and_counts_drops_when_debugger_absent() {
		// A loopback port with nothing listening: dials never succeed, so the
		// bounded queue fills and further frames are dropped, never blocking emit.
		let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
		let port = listener.local_addr().unwrap().port();
		drop(listener); // free the port; nothing is listening now

		let sink = WsDebugSink::connect(&format!("ws://127.0.0.1:{port}")).unwrap();
		for _ in 0..(QUEUE_CAPACITY + 50) {
			sink.emit(DebugEvent::Frame {
				channel_id: ChannelId("myapp.dot".to_string()),
				dir: FrameDirection::Out,
				bytes: vec![1],
			});
		}
		assert!(sink.dropped() > 0, "a full queue must count drops, not block");
	}

	#[tokio::test]
	async fn byte_budget_drops_large_frames_before_the_count_cap() {
		// Nothing listening: the writer never drains, so queued bytes accumulate.
		let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
		let port = listener.local_addr().unwrap().port();
		drop(listener);

		let sink = WsDebugSink::connect(&format!("ws://127.0.0.1:{port}")).unwrap();
		// ~2 MiB per frame; a handful blows past the 8 MiB byte budget long before
		// the 4096-frame count cap, so the BYTE cap is what drops here. Also
		// exercises reserve-before-send: emit must never panic on the counter even
		// as the writer task races it.
		let big = vec![0u8; 2 * 1024 * 1024];
		for _ in 0..8 {
			sink.emit(DebugEvent::Frame {
				channel_id: ChannelId("myapp.dot".to_string()),
				dir: FrameDirection::Out,
				bytes: big.clone(),
			});
		}
		assert!(
			sink.dropped() > 0,
			"the byte budget must drop large frames well under the count cap"
		);
	}
}
