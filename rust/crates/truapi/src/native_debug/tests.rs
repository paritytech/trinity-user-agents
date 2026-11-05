use super::*;
use crate::host_core::{ChannelId, FrameDirection};

use tokio::{net::TcpListener, sync::oneshot};
use tokio_tungstenite::accept_async;

/// A tapped frame on the test channel.
fn frame(dir: FrameDirection, bytes: Vec<u8>) -> DebugEvent {
	DebugEvent::Frame { channel_id: ChannelId("myapp.dot".to_string()), dir, bytes }
}

/// A loopback port with nothing listening, so the sink's dials fail and its
/// queue never drains.
async fn unused_port() -> u16 {
	let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
	listener.local_addr().unwrap().port()
}

/// A debugger on a fresh loopback port that waits `accept_delay`, accepts one
/// connection, and hands back the first text message it receives.
async fn first_message_debugger(accept_delay: Duration) -> (u16, oneshot::Receiver<String>) {
	let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
	let port = listener.local_addr().unwrap().port();
	let (tx, rx) = oneshot::channel();
	tokio::spawn(async move {
		tokio::time::sleep(accept_delay).await;
		let (stream, _peer) = listener.accept().await.unwrap();
		let ws = accept_async(stream).await.unwrap();
		let (_write, mut read) = ws.split();
		let message = read.next().await.unwrap().unwrap();
		tx.send(message.into_text().unwrap()).unwrap();
	});
	(port, rx)
}

/// A debugger that accepts and immediately drops every connection gets no
/// frame, so the sink keeps doubling its pause instead of redialing in a
/// tight loop.
#[tokio::test]
async fn a_debugger_that_drops_every_connection_is_redialed_ever_more_slowly() {
	const WINDOW: Duration = Duration::from_millis(1500);

	let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
	let port = listener.local_addr().unwrap().port();
	let accepted = Arc::new(AtomicUsize::new(0));
	let counted = accepted.clone();
	tokio::spawn(async move {
		loop {
			let (stream, _peer) = listener.accept().await.unwrap();
			counted.fetch_add(1, Ordering::Relaxed);
			drop(accept_async(stream).await);
		}
	});

	let _sink = WsDebugSink::connect(&format!("ws://127.0.0.1:{port}")).unwrap();
	tokio::time::sleep(WINDOW).await;

	// Expect dials at 0, 0.2, 0.6 and 1.4 s. Resetting after every
	// connection would give eight dials, and no pause would give hundreds.
	let dials = accepted.load(Ordering::Relaxed);
	assert!((2..=4).contains(&dials), "{dials} dials in {WINDOW:?}");
}

#[tokio::test]
async fn emits_base64_envelope_with_product_vantage_dir() {
	let (port, rx) = first_message_debugger(Duration::ZERO).await;
	let sink = WsDebugSink::connect(&format!("ws://127.0.0.1:{port}")).unwrap();
	// `In` = product→core, i.e. the frame *left* the product → product-vantage "out".
	sink.emit(frame(FrameDirection::In, vec![1, 2, 3, 4]));

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

	// Hold the frame in the sink's queue: the sink dials, the accept waits.
	let (port, rx) = first_message_debugger(ACCEPT_DELAY).await;

	let sink = WsDebugSink::connect(&format!("ws://127.0.0.1:{port}")).unwrap();
	let before = observed_at_millis().expect("clock");
	sink.emit(frame(FrameDirection::Out, vec![7, 7]));
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

/// A drop while an earlier one is pending must add to the pending count, not
/// replace it, or the wire reports one loss where two happened. Asserted on
/// the pending counter because the wire can't tell "1 then 1" from "1 then 2"
/// without catching both envelopes.
#[tokio::test]
async fn a_drop_while_one_is_pending_accumulates_rather_than_replaces() {
	let port = unused_port().await;
	let sink = WsDebugSink::connect(&format!("ws://127.0.0.1:{port}")).unwrap();
	let big = vec![0u8; 4 * 1024 * 1024];
	let emit = || sink.emit(frame(FrameDirection::Out, big.clone()));

	// 1st fills the queue's byte budget; 2nd is shed with nothing pending.
	emit();
	emit();
	assert_eq!(sink.pending_dropped(), 1, "the first shed is pending");

	// 3rd drains that 1 onto its envelope, is itself shed, and must put the
	// drained count BACK alongside its own.
	emit();
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
	// With nothing listening the queue cannot drain, so the byte budget sheds
	// a frame deterministically.
	let port = unused_port().await;

	let sink = WsDebugSink::connect(&format!("ws://127.0.0.1:{port}")).unwrap();
	// 4 MiB → ~5.6 MiB of base64 per envelope: the first fits the 8 MiB
	// budget, the second pushes past it and is shed.
	let big = vec![0u8; 4 * 1024 * 1024];
	for _ in 0..2 {
		sink.emit(frame(FrameDirection::Out, big.clone()));
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
		sink.emit(frame(FrameDirection::Out, vec![7]));
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
	let (port, rx) = first_message_debugger(Duration::ZERO).await;
	// The bug only exists when the name resolves to something before the v4
	// address; on a v4-only resolver this still passes, it just proves less.
	let resolved = resolve_loopback_target(&format!("ws://localhost:{port}")).unwrap();
	assert!(!resolved.is_empty(), "localhost must resolve to at least one loopback address");

	let sink = WsDebugSink::connect(&format!("ws://localhost:{port}")).unwrap();
	sink.emit(frame(FrameDirection::Out, vec![9]));

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
			HostFeatureSupportedError, HostFeatureSupportedRequest, HostFeatureSupportedResponse,
			HostHandshakeRequest, HostInfoError, HostInfoRequest, HostInfoResponse,
			HostNavigateToError, HostNavigateToRequest, HostNavigateToResponse,
		},
	};

	/// Exercises only `System::handshake`'s default (host-side) codec check.
	struct HandshakeOnly;

	#[async_trait::async_trait]
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

	let handshake =
		|codec: u32| {
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
	// The bounded queue fills and further frames are dropped, never blocking emit.
	let port = unused_port().await;

	let sink = WsDebugSink::connect(&format!("ws://127.0.0.1:{port}")).unwrap();
	for _ in 0..(QUEUE_CAPACITY + 50) {
		sink.emit(frame(FrameDirection::Out, vec![1]));
	}
	assert!(sink.dropped() > 0, "a full queue must count drops, not block");
}

#[tokio::test]
async fn byte_budget_drops_large_frames_before_the_count_cap() {
	let port = unused_port().await;

	let sink = WsDebugSink::connect(&format!("ws://127.0.0.1:{port}")).unwrap();
	// ~2 MiB per frame; a handful blows past the 8 MiB byte budget long before
	// the 4096-frame count cap, so the BYTE cap is what drops here. Also
	// exercises reserve-before-send: emit must never panic on the counter even
	// as the writer task races it.
	let big = vec![0u8; 2 * 1024 * 1024];
	for _ in 0..8 {
		sink.emit(frame(FrameDirection::Out, big.clone()));
	}
	assert!(sink.dropped() > 0, "the byte budget must drop large frames well under the count cap");
}
