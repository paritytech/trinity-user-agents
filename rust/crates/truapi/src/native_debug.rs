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
	ops::ControlFlow,
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
	time::Backoff,
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

/// Pauses between attempts to reach the debugger, doubling until a frame gets
/// through.
const RECONNECT_PAUSES: Backoff = Backoff::new(Duration::from_millis(200), Duration::from_secs(5));

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
	link: Arc<LinkState>,
}

/// One serialized envelope on its way to the writer task, plus the number of
/// shed frames stamped on it. Carrying the count alongside the line lets the
/// writer put it back if this envelope dies with the socket, so a drop is
/// reported exactly once and never silently swallowed.
struct QueuedFrame {
	line: String,
	shed: u64,
}

/// Counters the sink and its writer task share about frames in transit.
#[derive(Default)]
struct LinkState {
	/// Bytes of the envelopes waiting in the queue, held under [`MAX_QUEUE_BYTES`].
	queued_bytes: AtomicUsize,
	/// Lifetime total of lost frames, for logging and [`WsDebugSink::dropped`].
	dropped: AtomicU64,
	/// Drops not yet stamped on an envelope; the next one carries them.
	pending_dropped: AtomicU64,
}

impl LinkState {
	/// Reserve room for a `len`-byte envelope, or return `false` with nothing
	/// reserved if it would exceed the budget.
	///
	/// Reserve BEFORE handing the line to the channel: the writer task can recv
	/// and [`Self::release`] the instant `try_send` succeeds, so adding after
	/// would let that release run first and wrap the counter, an overflow panic
	/// in debug builds, on the frame path.
	fn reserve(&self, len: usize) -> bool {
		if self.queued_bytes.fetch_add(len, Ordering::Relaxed) + len > MAX_QUEUE_BYTES {
			self.release(len);
			return false;
		}
		true
	}

	/// Return a `len`-byte reservation, once its envelope has left the queue or
	/// never entered it.
	fn release(&self, len: usize) {
		self.queued_bytes.fetch_sub(len, Ordering::Relaxed);
	}

	/// Count one lost frame and put back the `carried` drops its envelope was
	/// reporting, so the next envelope reports them. Returns the lifetime total.
	fn count_drop(&self, carried: u64) -> u64 {
		self.pending_dropped.fetch_add(carried + 1, Ordering::Relaxed);
		self.dropped.fetch_add(1, Ordering::Relaxed) + 1
	}
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
		.map(|since| u64::try_from(since.as_millis()).unwrap_or(u64::MAX))
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
		let link = Arc::new(LinkState::default());
		tokio::spawn(writer_loop(url.to_string(), addrs, inbox, Arc::clone(&link)));
		Ok(Arc::new(Self { outbound, link }))
	}

	/// Number of frames dropped because the outbound queue was full (debugger
	/// absent or slower than the observed session). Never affects the session.
	pub fn dropped(&self) -> u64 {
		self.link.dropped.load(Ordering::Relaxed)
	}

	/// The count waiting to ride the next envelope, which tests can't see on the wire.
	#[cfg(test)]
	fn pending_dropped(&self) -> u64 {
		self.link.pending_dropped.load(Ordering::Relaxed)
	}

	/// Count a frame `emit` could not queue, carrying `carried` earlier drops.
	fn shed(&self, carried: u64, full: &str) {
		let dropped = self.link.count_drop(carried);
		debug!("truapi debug sink: {full} full, frame dropped (total {dropped})");
	}
}

impl DebugSink for WsDebugSink {
	fn emit(&self, event: DebugEvent) {
		let DebugEvent::Frame { channel_id, dir, bytes } = event;
		// Stamp the drops since the previous envelope on this one.
		let shed = self.link.pending_dropped.swap(0, Ordering::Relaxed);
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
			self.link.count_drop(shed);
			return;
		};
		let len = line.len();
		if !self.link.reserve(len) {
			self.shed(shed, "byte budget");
		} else if self.outbound.try_send(QueuedFrame { line, shed }).is_err() {
			self.link.release(len);
			self.shed(shed, "outbound queue");
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
	link: Arc<LinkState>,
) {
	let mut pauses = RECONNECT_PAUSES;
	loop {
		if let Some(stream) = dial(url.as_str(), &addrs).await &&
			forward(stream, &mut inbox, &mut pauses, &link).await.is_break()
		{
			return;
		}
		tokio::time::sleep(pauses.next_pause()).await;
		// The sink was dropped while we were retrying: give up.
		if inbox.is_closed() {
			return;
		}
	}
}

/// Drain queued frames to `stream` until the socket closes, resetting `pauses`
/// after each delivered frame. Returns `Break` once the sink is dropped.
async fn forward(
	stream: WebSocketStream<TcpStream>,
	inbox: &mut mpsc::Receiver<QueuedFrame>,
	pauses: &mut Backoff,
	link: &LinkState,
) -> ControlFlow<()> {
	let (mut write, mut read) = stream.split();
	// Poll the read half too, so tokio-tungstenite answers pings and notices a Close.
	loop {
		tokio::select! {
			queued = inbox.recv() => {
				let Some(QueuedFrame { line, shed }) = queued else {
					return ControlFlow::Break(());
				};
				link.release(line.len());
				if write.send(Message::Text(line)).await.is_err() {
					debug!("truapi debug sink: socket closed, reconnecting");
					link.count_drop(shed);
					return ControlFlow::Continue(());
				}
				// Only a delivered frame resets, so an accept-then-close server
				// still backs off instead of spinning on zero-delay reconnects.
				pauses.reset();
			}
			inbound = read.next() => {
				let Some(Ok(_)) = inbound else {
					debug!("truapi debug sink: read side closed, reconnecting");
					return ControlFlow::Continue(());
				};
			}
		}
	}
}

#[cfg(test)]
mod tests;
