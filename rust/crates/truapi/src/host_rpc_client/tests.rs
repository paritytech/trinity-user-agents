use super::*;
use std::sync::atomic::AtomicUsize;

use futures::{executor::block_on, stream::BoxStream};
use serde_json::{Value, json};
use subxt_rpcs::{RpcClient, client::rpc_params};

use crate::subscription::thread_per_subscription_spawner;

struct TrackingConnection {
	sender: Mutex<Option<mpsc::UnboundedSender<String>>>,
	receiver: Mutex<Option<mpsc::UnboundedReceiver<String>>>,
	sent: Mutex<Vec<Value>>,
	close_count: AtomicUsize,
}

impl TrackingConnection {
	fn new() -> Arc<Self> {
		let (tx, rx) = mpsc::unbounded();
		Arc::new(Self {
			sender: Mutex::new(Some(tx)),
			receiver: Mutex::new(Some(rx)),
			sent: Mutex::new(Vec::new()),
			close_count: AtomicUsize::new(0),
		})
	}

	fn close_count(&self) -> usize {
		self.close_count.load(Ordering::SeqCst)
	}

	fn sent(&self) -> Vec<Value> {
		self.sent.lock().unwrap().clone()
	}
}

impl JsonRpcConnection for TrackingConnection {
	fn send(&self, request: String) {
		let Ok(value) = serde_json::from_str::<Value>(&request) else {
			return;
		};
		self.sent.lock().unwrap().push(value.clone());
		let Some(id) = value.get("id").cloned() else {
			return;
		};
		if value.get("method").and_then(Value::as_str) == Some("sub") {
			let response = json!({
				"jsonrpc": "2.0",
				"id": id,
				"result": "sub-1",
			});
			if let Some(sender) = self.sender.lock().unwrap().as_ref() {
				let _ = sender.unbounded_send(response.to_string());
			}
		}
	}

	fn responses(&self) -> BoxStream<'static, String> {
		self.receiver.lock().unwrap().take().expect("responses called twice").boxed()
	}

	fn close(&self) {
		self.close_count.fetch_add(1, Ordering::SeqCst);
		self.sender.lock().unwrap().take();
	}
}

#[test]
fn dropping_one_shot_client_closes_connection_lease() {
	let connection = TrackingConnection::new();
	let spawner: Spawner = Arc::new(|_| {});

	{
		let client = HostRpcClient::new(connection.clone(), spawner);
		client.send_fire_and_forget("statement_submit", None).unwrap();
	}

	assert_eq!(connection.close_count(), 1);
}

#[test]
fn requests_without_arguments_serialize_empty_params() {
	let connection = TrackingConnection::new();
	let spawner: Spawner = Arc::new(|_| {});
	let client = HostRpcClient::new(connection.clone(), spawner);

	client.send_fire_and_forget("chainSpec_v1_chainName", None).unwrap();

	assert_eq!(connection.sent()[0]["params"], json!([]));
}

/// The close that fails waiting requests is the only trace a dead socket
/// leaves, so its warning says what was waiting and for how long.
#[test]
fn names_the_waiting_requests_and_their_ages_when_the_connection_closes() {
	let waiting = vec![
		("state_call", Duration::from_millis(1_100)),
		("chainHead_v1_call", Duration::from_millis(6_400)),
	];
	let reason = "no frame for 30s while a request waited";
	assert_eq!(
		[waiting_requests_warning(waiting, &reason), waiting_requests_warning(Vec::new(), &reason)],
		[
			Some(
				"json-rpc connection closed with 2 requests waiting: chainHead_v1_call 6.4 s, \
				 state_call 1.1 s: no frame for 30s while a request waited"
					.to_string()
			),
			None,
		],
	);
}

/// A reply that arrives but late is how a slow link shows before any
/// deadline trips, and a reply within the bound stays out of the log.
#[test]
fn names_a_reply_that_took_longer_than_the_slow_bound() {
	assert_eq!(
		[Duration::from_millis(6_400), SLOW_REPLY].map(|waited| slow_reply_warning(
			"chainHead_v1_call",
			"truapi:1265",
			waited
		)),
		[Some("slow json-rpc reply: chainHead_v1_call truapi:1265 after 6.4 s".to_string()), None],
	);
}

#[test]
fn subxt_single_hash_unpin_is_normalized_for_host_providers() {
	let connection = TrackingConnection::new();
	let spawner: Spawner = Arc::new(|_| {});
	let client = HostRpcClient::new(connection.clone(), spawner);
	let params = RawValue::from_string(r#"["follow-id","0x1234"]"#.to_string()).unwrap();

	client.send_fire_and_forget("chainHead_v1_unpin", Some(params)).unwrap();

	assert_eq!(connection.sent()[0]["params"], json!(["follow-id", ["0x1234"]]),);
}

#[test]
fn subscription_stream_holds_connection_lease_until_dropped() {
	let connection = TrackingConnection::new();
	let client = HostRpcClient::new(connection.clone(), thread_per_subscription_spawner());
	let rpc_client = RpcClient::new(client.clone());

	let subscription = block_on(rpc_client.subscribe::<Value>("sub", rpc_params![], "unsub"))
		.expect("subscription should start");

	drop(rpc_client);
	drop(client);
	assert_eq!(connection.close_count(), 0);

	drop(subscription);
	assert_eq!(connection.close_count(), 1);
}

fn raw(text: &str) -> Box<RawValue> {
	RawValue::from_string(text.to_string()).expect("valid json")
}

fn drain(rx: &mut mpsc::UnboundedReceiver<Result<Box<RawValue>, RpcError>>) -> Vec<String> {
	let mut seen = Vec::new();
	while let Ok(item) = rx.try_recv() {
		seen.push(match item {
			Ok(value) => value.get().to_string(),
			Err(error) => format!("error: {error}"),
		});
	}
	seen
}

/// Answers a subscribe by emitting a notification for the id *before* the
/// ack, which is the order that produced `Stop` ahead of `Initialized`.
struct RacingConnection {
	sender: Mutex<Option<mpsc::UnboundedSender<String>>>,
	receiver: Mutex<Option<mpsc::UnboundedReceiver<String>>>,
}

impl RacingConnection {
	fn new() -> Arc<Self> {
		let (tx, rx) = mpsc::unbounded();
		Arc::new(Self { sender: Mutex::new(Some(tx)), receiver: Mutex::new(Some(rx)) })
	}

	fn emit(&self, frame: String) {
		if let Some(sender) = self.sender.lock().unwrap().as_ref() {
			let _ = sender.unbounded_send(frame);
		}
	}

	fn notify(&self, event: &str) {
		self.emit(
			json!({
				"jsonrpc": "2.0",
				"method": "sub",
				"params": { "subscription": "sub-1", "result": { "event": event } },
			})
			.to_string(),
		);
	}
}

impl JsonRpcConnection for RacingConnection {
	fn send(&self, request: String) {
		let Ok(value) = serde_json::from_str::<Value>(&request) else {
			return;
		};
		let Some(id) = value.get("id").cloned() else {
			return;
		};
		if value.get("method").and_then(Value::as_str) == Some("sub") {
			// Ahead of the ack, so the router sees it with no sink yet.
			self.notify("buffered");
			self.emit(json!({ "jsonrpc": "2.0", "id": id, "result": "sub-1" }).to_string());
		}
	}

	fn responses(&self) -> BoxStream<'static, String> {
		self.receiver.lock().unwrap().take().expect("responses called twice").boxed()
	}

	fn close(&self) {
		self.sender.lock().unwrap().take();
	}
}

/// The regression this refactor exists for, driven through the real
/// `subscribe` rather than a hand-rolled copy of its critical section: a
/// notification that arrives before the ack is replayed, and stays ahead of
/// one that arrives after.
#[test]
fn subscribe_replays_what_arrived_before_the_ack_and_keeps_it_first() {
	let connection = RacingConnection::new();
	let client = HostRpcClient::new(connection.clone(), thread_per_subscription_spawner());
	let rpc_client = RpcClient::new(client.clone());

	let mut subscription = block_on(rpc_client.subscribe::<Value>("sub", rpc_params![], "unsub"))
		.expect("subscription should start");

	connection.notify("live");

	// Read on a worker with a deadline: a subscription that never replays
	// would otherwise block this test forever, and a lost notification
	// should report as a failure rather than as a CI timeout.
	let (done_tx, done_rx) = std::sync::mpsc::channel();
	std::thread::spawn(move || {
		let first = block_on(subscription.next());
		let second = block_on(subscription.next());
		let _ = done_tx.send((first, second));
	});
	let (first, second) = done_rx
		.recv_timeout(std::time::Duration::from_secs(10))
		.expect("subscribe did not deliver both notifications");

	let first = first.expect("a buffered notification").expect("decodes");
	let second = second.expect("a live notification").expect("decodes");
	assert_eq!(first, json!({ "event": "buffered" }));
	assert_eq!(second, json!({ "event": "live" }));
}

/// Replay order, pinned without a thread: two buffered items and one that
/// arrives after activation must come out in arrival order. The threaded
/// test below only observes this when the deliverer wins the lock race, so
/// the deterministic version is what actually guards it.
#[test]
fn buffered_events_replay_in_arrival_order() {
	let client = HostRpcClient::new(TrackingConnection::new(), Arc::new(|_| {}));
	let (tx, mut rx) = mpsc::unbounded();

	client
		.inner
		.deliver_or_buffer_subscription_item("sub-1".to_string(), raw(r#"{"event":"first"}"#));
	client
		.inner
		.deliver_or_buffer_subscription_item("sub-1".to_string(), raw(r#"{"event":"second"}"#));

	client.inner.subscriptions.lock().unwrap().activate("sub-1".to_string(), tx);

	client
		.inner
		.deliver_or_buffer_subscription_item("sub-1".to_string(), raw(r#"{"event":"live"}"#));

	assert_eq!(
		drain(&mut rx),
		vec![
			r#"{"event":"first"}"#.to_string(),
			r#"{"event":"second"}"#.to_string(),
			r#"{"event":"live"}"#.to_string(),
		],
		"buffered events must replay oldest first, ahead of anything later"
	);
}

/// The lock is taken from `Drop`, so a poisoned one must not turn a failure
/// somewhere else into a second panic during unwinding, which aborts the
/// process rather than reporting anything.
#[test]
fn a_poisoned_subscription_lock_still_closes_cleanly() {
	let client = HostRpcClient::new(TrackingConnection::new(), Arc::new(|_| {}));
	let inner = Arc::clone(&client.inner);

	let poisoned = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
		let _guard = inner.subscriptions.lock().unwrap();
		panic!("poisoning the subscription lock on purpose");
	}));
	assert!(poisoned.is_err(), "the helper panic should have been caught");
	assert!(
		client.inner.subscriptions.is_poisoned(),
		"the lock should be poisoned for this test to mean anything"
	);

	// Would panic a second time under `unwrap`, aborting the test binary.
	client.inner.close_with_error(client_error("peer went away"));
	client.inner.unsubscribe("sub-1", "unsub");
}

/// A notification delivered while activation is mid-flight waits for the
/// same lock the replay runs under, so it lands behind it.
#[test]
fn a_live_notification_cannot_overtake_replay_in_progress() {
	let client = HostRpcClient::new(TrackingConnection::new(), Arc::new(|_| {}));
	let (tx, mut rx) = mpsc::unbounded();

	client
		.inner
		.deliver_or_buffer_subscription_item("sub-1".to_string(), raw(r#"{"event":"buffered"}"#));

	let (ready_tx, ready_rx) = std::sync::mpsc::channel();
	let deliverer = {
		let inner = Arc::clone(&client.inner);
		std::thread::spawn(move || {
			let _ = ready_tx.send(());
			inner.deliver_or_buffer_subscription_item(
				"sub-1".to_string(),
				raw(r#"{"event":"live"}"#),
			);
		})
	};

	// Wait outside the critical section: a panic under the guard would
	// poison the lock and turn a failure into a process abort.
	ready_rx.recv().expect("deliverer started");
	client
		.inner
		.subscriptions
		.lock()
		.unwrap()
		.activate("sub-1".to_string(), tx.clone());

	deliverer.join().expect("deliverer finished");

	assert_eq!(
		drain(&mut rx),
		vec![r#"{"event":"buffered"}"#.to_string(), r#"{"event":"live"}"#.to_string(),],
		"the live notification overtook the buffered one"
	);
}

/// Closing reports the failure to whoever is reading, but never ahead of
/// events already queued for them.
#[test]
fn a_close_error_lands_behind_older_events() {
	let client = HostRpcClient::new(TrackingConnection::new(), Arc::new(|_| {}));
	let (tx, mut rx) = mpsc::unbounded();

	client
		.inner
		.deliver_or_buffer_subscription_item("sub-1".to_string(), raw(r#"{"event":"first"}"#));
	client
		.inner
		.subscriptions
		.lock()
		.unwrap()
		.activate("sub-1".to_string(), tx.clone());

	client.inner.close_with_error(client_error("peer went away"));

	let seen = drain(&mut rx);
	assert_eq!(seen.len(), 2, "expected the event and then the close error");
	assert_eq!(seen[0], r#"{"event":"first"}"#);
	assert!(
		seen[1].contains("json-rpc connection closed"),
		"close error should arrive last, got {:?}",
		seen[1]
	);
}

#[test]
fn closing_drops_buffers_that_have_no_reader() {
	let client = HostRpcClient::new(TrackingConnection::new(), Arc::new(|_| {}));
	client
		.inner
		.deliver_or_buffer_subscription_item("orphan".to_string(), raw(r#"{"event":"x"}"#));

	client.inner.close_with_error(client_error("peer went away"));

	let remaining = client.inner.subscriptions.lock().unwrap().entries.len();
	assert_eq!(remaining, 0, "close should leave no subscription state behind");
}

/// A frame off the wire reaches the subscription it names. Without this a
/// dropped notification surfaces as a hung suite rather than a failure.
#[test]
fn a_notification_frame_is_routed_to_its_subscription() {
	let client = HostRpcClient::new(TrackingConnection::new(), Arc::new(|_| {}));
	let (tx, mut rx) = mpsc::unbounded();
	client.inner.subscriptions.lock().unwrap().activate("sub-1".to_string(), tx);

	client
		.inner
		.handle_frame(
			&json!({
				"jsonrpc": "2.0",
				"method": "sub",
				"params": { "subscription": "sub-1", "result": { "event": "routed" } },
			})
			.to_string(),
		)
		.expect("frame parses");

	assert_eq!(drain(&mut rx), vec![r#"{"event":"routed"}"#.to_string()]);
}

/// Unsubscribe forgets the subscription and tells the peer.
#[test]
fn unsubscribe_forgets_the_subscription_and_tells_the_host() {
	let connection = TrackingConnection::new();
	let client = HostRpcClient::new(connection.clone(), Arc::new(|_| {}));
	let (tx, _rx) = mpsc::unbounded();
	client.inner.subscriptions.lock().unwrap().activate("sub-1".to_string(), tx);

	client.inner.unsubscribe("sub-1", "unsub");

	let remaining = client.inner.subscriptions.lock().unwrap().entries.len();
	assert_eq!(remaining, 0, "unsubscribe should forget the subscription");
	let sent = connection.sent();
	let last = sent.last().expect("an unsubscribe request was sent");
	assert_eq!(last.get("method").and_then(Value::as_str), Some("unsub"));
	assert_eq!(last.get("params"), Some(&json!(["sub-1"])));
}

#[test]
fn dropping_the_subscription_stream_unsubscribes() {
	let connection = TrackingConnection::new();
	let client = HostRpcClient::new(connection.clone(), thread_per_subscription_spawner());
	let rpc_client = RpcClient::new(client.clone());

	let subscription = block_on(rpc_client.subscribe::<Value>("sub", rpc_params![], "unsub"))
		.expect("subscription should start");
	drop(subscription);

	let sent = connection.sent();
	assert!(
		sent.iter()
			.any(|frame| frame.get("method").and_then(Value::as_str) == Some("unsub")),
		"dropping the stream should unsubscribe, sent: {sent:?}"
	);
}

/// Re-using a live id replaces the sink rather than leaving the old one
/// attached, so the newest subscriber is the one that receives.
#[test]
fn re_activating_an_id_replaces_the_sink() {
	let client = HostRpcClient::new(TrackingConnection::new(), Arc::new(|_| {}));
	let (first_tx, mut first_rx) = mpsc::unbounded();
	let (second_tx, mut second_rx) = mpsc::unbounded();
	{
		let mut state = client.inner.subscriptions.lock().unwrap();
		state.activate("sub-1".to_string(), first_tx);
		state.activate("sub-1".to_string(), second_tx);
	}

	client
		.inner
		.deliver_or_buffer_subscription_item("sub-1".to_string(), raw(r#"{"event":"x"}"#));

	assert_eq!(drain(&mut first_rx), Vec::<String>::new());
	assert_eq!(drain(&mut second_rx), vec![r#"{"event":"x"}"#.to_string()]);
}

/// A closed client refuses new work instead of hanging on a dead peer.
#[test]
fn a_closed_client_refuses_requests_and_subscriptions() {
	let client = HostRpcClient::new(TrackingConnection::new(), thread_per_subscription_spawner());
	let rpc_client = RpcClient::new(client.clone());
	client.inner.close_with_error(client_error("peer went away"));

	// Bounded: without the closed check these block on a peer that will
	// never answer, and a hung suite diagnoses nothing.
	let (done_tx, done_rx) = std::sync::mpsc::channel();
	std::thread::spawn(move || {
		let request = block_on(rpc_client.request::<Value>("any", rpc_params![])).is_err();
		let subscribe =
			block_on(rpc_client.subscribe::<Value>("sub", rpc_params![], "unsub")).is_err();
		let _ = done_tx.send((request, subscribe));
	});
	let (request_failed, subscribe_failed) = done_rx
		.recv_timeout(std::time::Duration::from_secs(10))
		.expect("a closed client should refuse rather than hang");

	assert!(request_failed, "a request on a closed client should fail");
	assert!(subscribe_failed, "a subscribe on a closed client should fail");
}

fn waiting_request(client: &HostRpcClient, sent_at: Instant) {
	let (tx, _rx) = oneshot::channel();
	client.inner.pending.lock().unwrap().insert(
		"truapi:1".to_string(),
		PendingRequest { method: "chainHead_v1_call".to_string(), tx, sent_at },
	);
}

/// A request with nothing at all arriving after it is what a dead socket
/// looks like, so the check trips once the request has waited the whole
/// deadline.
#[test]
fn a_request_waiting_out_the_deadline_in_silence_trips_the_check() {
	let client = HostRpcClient::new(TrackingConnection::new(), Arc::new(|_| {}));
	let sent_at = Instant::now();
	*client.inner.last_frame_at.lock().unwrap() = sent_at;
	waiting_request(&client, sent_at);

	let tripped = [REQUEST_SILENCE_DEADLINE - Duration::from_secs(1), REQUEST_SILENCE_DEADLINE]
		.map(|wait| client.inner.silent_past_deadline(sent_at + wait));

	assert_eq!(tripped, [false, true]);
}

/// Any frame proves the socket alive, and a request waiting behind a slow
/// frame is not stuck, so the wait counts from the last frame.
#[test]
fn a_frame_after_the_request_restarts_the_wait() {
	let client = HostRpcClient::new(TrackingConnection::new(), Arc::new(|_| {}));
	let sent_at = Instant::now();
	let frame_after = Duration::from_secs(20);
	waiting_request(&client, sent_at);
	*client.inner.last_frame_at.lock().unwrap() = sent_at + frame_after;

	let tripped = [REQUEST_SILENCE_DEADLINE, frame_after + REQUEST_SILENCE_DEADLINE]
		.map(|wait| client.inner.silent_past_deadline(sent_at + wait));

	assert_eq!(tripped, [false, true]);
}

/// A connection with a frame on its way whose last byte came `no_byte_for`
/// ago.
struct DownloadingConnection {
	no_byte_for: Duration,
}

impl JsonRpcConnection for DownloadingConnection {
	fn send(&self, _request: String) {}

	fn responses(&self) -> BoxStream<'static, String> {
		futures::stream::pending().boxed()
	}

	fn mid_message_silence(&self) -> Option<Duration> {
		Some(self.no_byte_for)
	}

	fn close(&self) {}
}

/// A frame larger than the deadline, such as a Bulletin block body on a slow
/// link, keeps the request waiting while its bytes arrive. A connection
/// whose bytes stopped is still closed at the deadline.
#[test]
fn bytes_still_arriving_keep_a_request_waiting() {
	let sent_at = Instant::now();
	let tripped = [Duration::from_secs(1), REQUEST_SILENCE_DEADLINE].map(|no_byte_for| {
		let client =
			HostRpcClient::new(Arc::new(DownloadingConnection { no_byte_for }), Arc::new(|_| {}));
		*client.inner.last_frame_at.lock().unwrap() = sent_at;
		waiting_request(&client, sent_at);
		client.inner.silent_past_deadline(sent_at + REQUEST_SILENCE_DEADLINE * 2)
	});

	assert_eq!(tripped, [false, true]);
}

/// The node side of a loopback WebSocket, past the client's first request,
/// and that request's id. A test answers by writing raw frames to the socket,
/// so a frame can arrive slowly, or control frames on their own.
async fn loopback_node(
	listener: tokio::net::TcpListener,
) -> (tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>, Value) {
	use tokio_tungstenite::tungstenite::Message;

	let (socket, _) = listener.accept().await.expect("the client connects");
	let mut node = tokio_tungstenite::accept_async(socket).await.expect("handshake");
	let Some(Ok(Message::Text(request))) = node.next().await else {
		panic!("the client sent no request");
	};
	let request: Value = serde_json::from_str(&request).expect("the request is JSON");
	(node, request["id"].clone())
}

/// A client over the WebSocket transport the CLI uses, to the node at
/// `address`.
async fn loopback_client(
	address: std::net::SocketAddr,
	silence_deadline: Duration,
) -> HostRpcClient {
	use truapi_provider::{ChainSource, EmbeddedChainProvider, platform::ChainProvider};

	let url = format!("ws://{address}").parse().expect("loopback URL");
	let connection = EmbeddedChainProvider::builder()
		.chain([7; 32], ChainSource::rpc_node(url))
		.build()
		.connect([7; 32])
		.await
		.expect("the loopback node accepts");
	let spawner: Spawner = Arc::new(|future| {
		tokio::spawn(future);
	});
	HostRpcClient::with_silence_deadline(Arc::from(connection), spawner, silence_deadline)
}

/// A server's header for an unmasked text frame of `length` bytes, in the
/// two-byte length form.
fn text_frame_header(length: usize) -> Vec<u8> {
	let length = u16::try_from(length).expect("the test frame fits a two-byte length");
	assert!(length > 125, "a shorter frame takes the one-byte length form");
	[[0x81, 126].as_slice(), &length.to_be_bytes()].concat()
}

/// A frame that takes longer than the deadline to arrive, as a Bulletin block
/// body does on a slow link, still answers its request while its bytes keep
/// coming.
#[tokio::test(flavor = "multi_thread")]
async fn a_frame_slower_than_the_deadline_still_answers() {
	use tokio::io::AsyncWriteExt;

	const RESULT_BYTES: usize = 48 * 1024;
	let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("loopback bind");
	let address = listener.local_addr().expect("listener address");
	tokio::spawn(async move {
		let (mut node, id) = loopback_node(listener).await;
		let result = "b".repeat(RESULT_BYTES);
		let response = json!({"jsonrpc": "2.0", "id": id, "result": result}).to_string();
		let frame = [text_frame_header(response.len()), response.into_bytes()].concat();
		for chunk in frame.chunks(8 * 1024) {
			node.get_mut().write_all(chunk).await.expect("the client is still reading");
			tokio::time::sleep(Duration::from_millis(250)).await;
		}
	});
	let rpc = RpcClient::new(loopback_client(address, Duration::from_millis(600)).await);

	let answered = rpc
		.request::<String>("chain_getBlock", rpc_params![])
		.await
		.map(|result| result.len())
		.map_err(|error| error.to_string());
	assert_eq!(answered, Ok(RESULT_BYTES));
}

/// Keepalive frames are no frame on its way: a request the node never answers
/// still fails at the deadline while pings and pongs keep arriving.
#[tokio::test(flavor = "multi_thread")]
async fn keepalive_frames_do_not_keep_an_unanswered_request_waiting() {
	use tokio::io::AsyncWriteExt;

	const PONG_THEN_PING: [u8; 4] = [0x8a, 0, 0x89, 0];
	let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("loopback bind");
	let address = listener.local_addr().expect("listener address");
	tokio::spawn(async move {
		let (mut node, _id) = loopback_node(listener).await;
		while node.get_mut().write_all(&PONG_THEN_PING).await.is_ok() {
			tokio::time::sleep(Duration::from_millis(100)).await;
		}
	});
	let rpc = RpcClient::new(loopback_client(address, Duration::from_millis(600)).await);
	let request = rpc.request::<Value>("chain_getBlock", rpc_params![]);

	let failure = tokio::time::timeout(Duration::from_secs(10), request)
		.await
		.expect("pings and pongs kept a request the node never answers waiting")
		.expect_err("the node never answers");
	assert!(
		failure.to_string().contains("no frame for 600ms while a request waited"),
		"unexpected failure: {failure}"
	);
}

/// A quiet connection with nothing outstanding is idle rather than dead,
/// and closing it would only make the next caller wait for a new one.
#[test]
fn silence_with_no_request_waiting_never_trips_the_check() {
	let client = HostRpcClient::new(TrackingConnection::new(), Arc::new(|_| {}));
	let frame_at = Instant::now();
	*client.inner.last_frame_at.lock().unwrap() = frame_at;

	assert!(!client.inner.silent_past_deadline(frame_at + REQUEST_SILENCE_DEADLINE * 10));
}

/// The response loop runs the check on its own: a request on a socket that
/// went silent fails with that cause, and the host is told to close the
/// socket.
#[test]
fn a_request_on_a_silent_connection_fails_and_closes_it() {
	let connection = TrackingConnection::new();
	let client = HostRpcClient::with_silence_deadline(
		connection.clone(),
		thread_per_subscription_spawner(),
		Duration::from_millis(100),
	);
	let rpc_client = RpcClient::new(client.clone());

	let (done_tx, done_rx) = std::sync::mpsc::channel();
	std::thread::spawn(move || {
		let request = block_on(rpc_client.request::<Value>("chainHead_v1_header", rpc_params![]));
		let _ = done_tx.send(request.map_err(|error| error.to_string()));
	});
	let failure = done_rx
		.recv_timeout(std::time::Duration::from_secs(10))
		.expect("a request on a silent connection should fail rather than hang")
		.expect_err("nothing answers the request");

	assert!(
		failure.contains("no frame for 100ms while a request waited"),
		"unexpected failure: {failure}"
	);
	assert!(client.is_closed(), "the client should refuse further requests");
	assert_eq!(connection.close_count(), 1, "the host should close the socket");
}

/// The cap counts subscriptions holding a buffer. Asserted against the map
/// itself rather than `pending_count`, so the counter cannot be its own
/// oracle.
#[test]
fn the_buffer_cap_counts_only_subscriptions_holding_items() {
	let client = HostRpcClient::new(TrackingConnection::new(), Arc::new(|_| {}));
	let (tx, _rx) = mpsc::unbounded();
	client.inner.subscriptions.lock().unwrap().activate("active".to_string(), tx);

	for index in 0..MAX_BUFFERED_SUBSCRIPTIONS {
		client
			.inner
			.deliver_or_buffer_subscription_item(format!("sub-{index}"), raw("1"));
	}
	client
		.inner
		.deliver_or_buffer_subscription_item("one-too-many".to_string(), raw("1"));

	// Read out before asserting: a failed assertion under the guard poisons
	// the lock and the client's own `Drop` then aborts instead of reporting.
	let (total, active_survives, over_cap) = {
		let state = client.inner.subscriptions.lock().unwrap();
		(
			state.entries.len(),
			matches!(state.entries.get("active"), Some(SubscriptionEntry::Active(_))),
			state.entries.contains_key("one-too-many"),
		)
	};
	assert_eq!(
		total,
		MAX_BUFFERED_SUBSCRIPTIONS + 1,
		"the active subscription must not consume a buffer slot"
	);
	assert!(active_survives, "the active subscription should still be active");
	assert!(!over_cap, "a subscription past the cap must not be buffered");
}

#[test]
fn a_buffer_stops_growing_at_its_item_cap() {
	let client = HostRpcClient::new(TrackingConnection::new(), Arc::new(|_| {}));
	for _ in 0..MAX_BUFFERED_ITEMS_PER_SUBSCRIPTION + 8 {
		client.inner.deliver_or_buffer_subscription_item("sub-1".to_string(), raw("1"));
	}

	let buffered = {
		let state = client.inner.subscriptions.lock().unwrap();
		match state.entries.get("sub-1") {
			Some(SubscriptionEntry::Pending(items)) => items.len(),
			_ => usize::MAX,
		}
	};
	assert_eq!(buffered, MAX_BUFFERED_ITEMS_PER_SUBSCRIPTION);
}
