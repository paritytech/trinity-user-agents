use core::future::Future;
use std::{
	sync::{
		Mutex,
		mpsc::{Receiver, Sender, channel},
	},
	time::Duration,
};

use super::*;

/// Paseo Next v2's relay, whose spec the catalog bundles. Spec-local queries
/// are answered from it without any network access.
const CATALOG_RELAY: [u8; 32] =
	match const_hex_decode("374057be67b355151f271ff70c3db98308c62c8adc48dc6724b6a009a1a014fd") {
		Some(bytes) => bytes,
		None => panic!("the constant is 32 bytes of hex"),
	};

/// `hex::decode` is not const, and a literal byte array here would not be
/// greppable against the catalog.
const fn const_hex_decode(hex: &str) -> Option<[u8; 32]> {
	let bytes = hex.as_bytes();
	if bytes.len() != 64 {
		return None;
	}
	let mut out = [0u8; 32];
	let mut i = 0;
	while i < 32 {
		let (high, low) = (nibble(bytes[i * 2]), nibble(bytes[i * 2 + 1]));
		match (high, low) {
			(Some(high), Some(low)) => out[i] = high << 4 | low,
			_ => return None,
		}
		i += 1;
	}
	Some(out)
}

const fn nibble(byte: u8) -> Option<u8> {
	match byte {
		b'0'..=b'9' => Some(byte - b'0'),
		b'a'..=b'f' => Some(byte - b'a' + 10),
		_ => None,
	}
}

/// Foreign listener stand-in: the real one lives in Swift or Kotlin, so this
/// exercises the same `with_foreign` trait the bindings implement.
struct Collector {
	messages: Sender<String>,
	closed: Mutex<Option<Sender<ChainCloseReason>>>,
}

impl Collector {
	fn new() -> (Arc<Self>, Receiver<String>, Receiver<ChainCloseReason>) {
		let (messages, message_rx) = channel();
		let (closed, closed_rx) = channel();
		let collector = Arc::new(Collector { messages, closed: Mutex::new(Some(closed)) });
		(collector, message_rx, closed_rx)
	}
}

/// Fails on the message at `fail_at`, recording what it was asked to do.
struct FailingListener {
	fail_at: usize,
	delivered: Mutex<Vec<String>>,
	closed: Mutex<Option<ChainCloseReason>>,
	closes: Mutex<usize>,
	reason: String,
}

impl ChainMessageListener for FailingListener {
	fn on_message(&self, message: String) -> Result<(), ChainProviderError> {
		let mut delivered = self.delivered.lock().expect("not poisoned");
		delivered.push(message);
		if delivered.len() == self.fail_at {
			return Err(ChainProviderError::Listener { reason: self.reason.clone() });
		}
		Ok(())
	}

	fn on_closed(&self, reason: ChainCloseReason) -> Result<(), ChainProviderError> {
		*self.closed.lock().expect("not poisoned") = Some(reason);
		*self.closes.lock().expect("not poisoned") += 1;
		Ok(())
	}
}

/// Reads the connection at the moment it is told the connection closed, so
/// the ordering is observed rather than inferred from a final flag.
struct OrderingListener {
	connection: Arc<ClosingConnection>,
	fails: bool,
	closed_while_open: Mutex<Option<bool>>,
	closes: Mutex<usize>,
}

impl ChainMessageListener for OrderingListener {
	fn on_message(&self, _message: String) -> Result<(), ChainProviderError> {
		if self.fails {
			return Err(ChainProviderError::Listener { reason: "cannot decode".to_string() });
		}
		Ok(())
	}

	fn on_closed(&self, _reason: ChainCloseReason) -> Result<(), ChainProviderError> {
		let open = !*self.connection.closed.lock().expect("not poisoned");
		*self.closed_while_open.lock().expect("not poisoned") = Some(open);
		*self.closes.lock().expect("not poisoned") += 1;
		Ok(())
	}
}

/// Records whether the pump closed it.
struct ClosingConnection {
	closed: Mutex<bool>,
}

impl JsonRpcConnection for ClosingConnection {
	fn send(&self, _request: String) {}

	fn responses(&self) -> BoxStream<'static, String> {
		Box::pin(futures::stream::empty())
	}

	fn close(&self) {
		*self.closed.lock().expect("not poisoned") = true;
	}
}

#[test]
fn the_pump_does_not_keep_its_connection_alive() {
	// The pump parks on a stream that only ends once the connection is
	// dropped and closed. A strong handle here would make that drop
	// unreachable, leaking the chain and the thread that waits on it.
	let connection: Arc<dyn JsonRpcConnection> =
		Arc::new(ClosingConnection { closed: Mutex::new(false) });
	let weak = Arc::downgrade(&connection);
	let listener = Arc::new(FailingListener {
		fail_at: usize::MAX,
		delivered: Mutex::new(Vec::new()),
		closed: Mutex::new(None),
		closes: Mutex::new(0),
		reason: "cannot decode".to_string(),
	});

	// Parked on a stream that never yields, which is where the pump spends
	// its life. Polled first so the future is actually running: an async
	// body executes nothing until then, so dropping before the first poll
	// would prove nothing.
	let mut pump =
		Box::pin(pump_responses(Box::pin(futures::stream::pending()), weak.clone(), listener));
	let mut cx = core::task::Context::from_waker(futures::task::noop_waker_ref());
	assert!(pump.as_mut().poll(&mut cx).is_pending());

	drop(connection);
	assert!(weak.upgrade().is_none(), "a parked pump must not hold its connection alive");
}

#[test]
fn a_dropped_connection_still_reports_the_listener_that_failed() {
	// `close()` ends the stream asynchronously, so a host that dropped its
	// handle can still have buffered frames pumped at it. If a listener
	// rejects one of those, the reason must not fall back to the variant a
	// host reads as an ordinary ending.
	let listener = Arc::new(FailingListener {
		fail_at: 1,
		delivered: Mutex::new(Vec::new()),
		closed: Mutex::new(None),
		closes: Mutex::new(0),
		reason: "cannot decode".to_string(),
	});
	let connection: Arc<dyn JsonRpcConnection> =
		Arc::new(ClosingConnection { closed: Mutex::new(false) });
	let weak = Arc::downgrade(&connection);
	drop(connection);
	assert!(weak.upgrade().is_none(), "the owner has already dropped it");

	block_on(pump_responses(
		Box::pin(futures::stream::iter(["late".to_string()])),
		weak,
		listener.clone(),
	));

	assert_eq!(
		*listener.closed.lock().expect("not poisoned"),
		Some(ChainCloseReason::ListenerFailed { reason: "cannot decode".to_string() })
	);
	// The fixture records the last call, so without this a second
	// `on_closed` on this path would pass unnoticed.
	assert_eq!(*listener.closes.lock().expect("not poisoned"), 1);
}

#[test]
fn a_host_is_told_once_and_after_any_close_the_pump_performs() {
	// Both listeners read the connection at the moment they are told, so an
	// `on_closed` that ran before `close()` fails here rather than passing
	// on the final state of a flag nothing timed.
	let pump = |fails: bool| {
		let connection = Arc::new(ClosingConnection { closed: Mutex::new(false) });
		let listener = Arc::new(OrderingListener {
			connection: connection.clone(),
			fails,
			closed_while_open: Mutex::new(None),
			closes: Mutex::new(0),
		});
		block_on(pump_responses(
			Box::pin(futures::stream::iter(["first".to_string()])),
			Arc::downgrade(&(connection.clone() as Arc<dyn JsonRpcConnection>)),
			listener.clone(),
		));
		listener
	};

	let failed = pump(true);
	assert_eq!(
		*failed.closes.lock().expect("not poisoned"),
		1,
		"a host that reconnects on teardown must not be told twice"
	);
	// The pump closed this one, so it must be closed before the host hears
	// about it: a host reconnecting inside `on_closed` would otherwise do
	// it against a handle still accepting sends.
	assert_eq!(*failed.closed_while_open.lock().expect("not poisoned"), Some(false));

	let ended = pump(false);
	assert_eq!(*ended.closes.lock().expect("not poisoned"), 1);
	// Nothing failed, so the pump closes nothing: the handle is the owner's
	// to drop, and `on_closed` reports the stream ending, not a close the
	// core performed.
	assert_eq!(*ended.closed_while_open.lock().expect("not poisoned"), Some(true));
}

/// The spawn site and `pump_responses` both hold a guard, so an inner drop
/// must restore rather than clear: the outer one has to survive the drop of
/// the listener `Arc`, which runs foreign destructor code on this thread
/// while `block_on` is still entered.
#[test]
fn a_nested_pump_guard_does_not_clear_the_outer_flag() {
	assert!(!PUMPING.get());
	let outer = PumpGuard::enter();
	assert!(PUMPING.get());
	{
		let _inner = PumpGuard::enter();
		assert!(PUMPING.get());
	}
	assert!(
		PUMPING.get(),
		"an inner guard's drop must not clear a flag the outer guard still owns"
	);
	drop(outer);
	assert!(!PUMPING.get());
}

#[test]
fn connecting_from_a_listener_callback_is_refused_rather_than_aborting() {
	// The pump thread is already inside `block_on`, and `connect` blocks
	// again; `futures` answers that with a panic, which `panic = "abort"`
	// turns into a dead app. Reconnecting from `on_closed` is exactly what
	// `ChainCloseReason` invites, so it has to be an error a host can read.
	struct ReconnectingListener {
		provider: Arc<ChainProvider>,
		result: Mutex<Option<Result<(), ChainProviderError>>>,
	}

	impl ChainMessageListener for ReconnectingListener {
		fn on_message(&self, _message: String) -> Result<(), ChainProviderError> {
			Ok(())
		}

		fn on_closed(&self, _reason: ChainCloseReason) -> Result<(), ChainProviderError> {
			let reconnected =
				self.provider.connect(vec![0u8; 32], Arc::new(SilentListener)).map(|_| ());
			*self.result.lock().expect("not poisoned") = Some(reconnected);
			Ok(())
		}
	}

	struct SilentListener;

	impl ChainMessageListener for SilentListener {
		fn on_message(&self, _message: String) -> Result<(), ChainProviderError> {
			Ok(())
		}

		fn on_closed(&self, _reason: ChainCloseReason) -> Result<(), ChainProviderError> {
			Ok(())
		}
	}

	let listener =
		Arc::new(ReconnectingListener { provider: ChainProvider::new(), result: Mutex::new(None) });
	let connection = Arc::new(ClosingConnection { closed: Mutex::new(false) });

	block_on(pump_responses(
		Box::pin(futures::stream::iter(["first".to_string()])),
		Arc::downgrade(&(connection as Arc<dyn JsonRpcConnection>)),
		listener.clone(),
	));

	let reconnected = listener
		.result
		.lock()
		.expect("not poisoned")
		.take()
		.expect("the listener was told the connection closed");
	assert!(
		matches!(reconnected, Err(ChainProviderError::Connect { ref reason }) if reason.contains("listener callback")),
		"got {reconnected:?}"
	);
}

#[test]
fn a_listener_reason_is_bounded_before_it_crosses_back() {
	// The string is foreign-authored and unbounded at the source, and it
	// crosses the boundary twice.
	// Multi-byte on purpose: a bound written in bytes rather than
	// characters splits this mid-character, which panics, and `panic =
	// "abort"` on the shipping profile turns that into a process kill.
	let listener = Arc::new(FailingListener {
		fail_at: 1,
		delivered: Mutex::new(Vec::new()),
		closed: Mutex::new(None),
		closes: Mutex::new(0),
		reason: format!("HEAD{}", "\u{1f600}".repeat(CLOSE_REASON_MAX_CHARS * 2)),
	});
	let connection = Arc::new(ClosingConnection { closed: Mutex::new(false) });

	block_on(pump_responses(
		Box::pin(futures::stream::iter(["first".to_string()])),
		Arc::downgrade(&(connection.clone() as Arc<dyn JsonRpcConnection>)),
		listener.clone(),
	));

	let Some(ChainCloseReason::ListenerFailed { reason }) =
		listener.closed.lock().expect("not poisoned").clone()
	else {
		panic!("a failing listener closes with its own failure");
	};
	assert_eq!(reason.chars().count(), CLOSE_REASON_MAX_CHARS);
	assert!(reason.len() > CLOSE_REASON_MAX_CHARS, "counted in characters, not bytes");
	// The head is what names the failure, so a bound that kept the tail
	// would leave a host with an anonymous string.
	assert!(reason.starts_with("HEAD"), "the bound keeps the head, not the tail");
	// Astral chars are two UTF-16 code units each, so a bound counted in
	// those would land on half as many scalars.
	assert!(
		reason.encode_utf16().count() > CLOSE_REASON_MAX_CHARS,
		"counted in scalar values, not UTF-16 code units"
	);
	// The number is the contract a host reads in the doc, so pin the value
	// and not only the relationship to itself.
	assert_eq!(CLOSE_REASON_MAX_CHARS, 256);
}

/// The impl exists so an undeclared foreign exception becomes a rejection
/// instead of the generic converter's panic, which `panic = "abort"` would
/// turn into a process kill. It also bounds the reason on that path.
#[test]
fn an_undeclared_foreign_error_converts_and_is_bounded() {
	let thrown = format!("HEAD{}", "\u{1f600}".repeat(CLOSE_REASON_MAX_CHARS * 2));
	let error =
		ChainProviderError::from(uniffi::UnexpectedUniFFICallbackError::new(thrown.clone()));

	let ChainProviderError::Listener { reason } = error else {
		panic!("an undeclared foreign error is reported as a listener rejection");
	};
	assert_eq!(reason.chars().count(), CLOSE_REASON_MAX_CHARS);
	assert!(reason.starts_with("HEAD"), "the bound keeps the head");
	assert!(
		thrown.chars().count() > CLOSE_REASON_MAX_CHARS,
		"the fixture has to exceed the bound for this to mean anything"
	);
}

#[test]
fn a_stream_that_ends_on_its_own_closes_as_such() {
	let listener = Arc::new(FailingListener {
		fail_at: usize::MAX,
		delivered: Mutex::new(Vec::new()),
		closed: Mutex::new(None),
		closes: Mutex::new(0),
		reason: "cannot decode".to_string(),
	});
	let connection = Arc::new(ClosingConnection { closed: Mutex::new(false) });
	let responses = futures::stream::iter(["first", "second"].map(str::to_string));

	block_on(pump_responses(
		Box::pin(responses),
		Arc::downgrade(&(connection.clone() as Arc<dyn JsonRpcConnection>)),
		listener.clone(),
	));

	// The ordinary ending, and the one a host may safely reconnect after.
	assert_eq!(*listener.closed.lock().expect("not poisoned"), Some(ChainCloseReason::StreamEnded));
	// Nothing failed, so the pump has no reason to close the handle: that
	// is the caller's to drop.
	assert!(!*connection.closed.lock().expect("not poisoned"));
}

#[test]
fn a_failing_listener_ends_the_connection_rather_than_going_deaf() {
	let listener = Arc::new(FailingListener {
		fail_at: 2,
		delivered: Mutex::new(Vec::new()),
		closed: Mutex::new(None),
		closes: Mutex::new(0),
		reason: "cannot decode".to_string(),
	});
	let connection = Arc::new(ClosingConnection { closed: Mutex::new(false) });
	let responses = futures::stream::iter(["first", "second", "third"].map(str::to_string));

	block_on(pump_responses(
		Box::pin(responses),
		Arc::downgrade(&(connection.clone() as Arc<dyn JsonRpcConnection>)),
		listener.clone(),
	));

	// Pumping stops at the failure rather than calling the listener again
	// for every remaining response.
	assert_eq!(
		listener.delivered.lock().expect("not poisoned").as_slice(),
		&["first".to_string(), "second".to_string()]
	);
	// The host is told the connection closed, and told which way: a host
	// that reconnected on this would walk straight back into the failure
	// that closed it.
	assert_eq!(
		*listener.closed.lock().expect("not poisoned"),
		Some(ChainCloseReason::ListenerFailed { reason: "cannot decode".to_string() })
	);
	// And the connection is closed, so later sends are refused at the
	// source instead of queueing against a receiver that is gone.
	assert!(*connection.closed.lock().expect("not poisoned"));
}

impl ChainMessageListener for Collector {
	fn on_message(&self, message: String) -> Result<(), ChainProviderError> {
		let _ = self.messages.send(message);
		Ok(())
	}

	fn on_closed(&self, reason: ChainCloseReason) -> Result<(), ChainProviderError> {
		if let Some(closed) = self.closed.lock().expect("not poisoned").take() {
			let _ = closed.send(reason);
		}
		Ok(())
	}
}

#[test]
fn a_genesis_hash_of_the_wrong_length_is_rejected() {
	let (listener, _messages, _closed) = Collector::new();
	let error = ChainProvider::new()
		.connect(vec![0u8; 31], listener)
		.err()
		.expect("a 31-byte genesis must be rejected");
	assert!(matches!(error, ChainProviderError::BadGenesis));
}

/// Foreign storage stand-in. The real one lives in Swift or Kotlin, so
/// this exercises the same `with_foreign` trait the bindings implement.
/// Each call records its genesis hash, and the blob when it was a save.
struct RecordingStore {
	calls: Mutex<Vec<(Vec<u8>, Option<String>)>>,
	failure: Option<String>,
}

impl RecordingStore {
	fn new(failure: Option<String>) -> Arc<Self> {
		Arc::new(Self { calls: Mutex::new(Vec::new()), failure })
	}

	fn answer(&self) -> Result<(), StorageClientError> {
		match &self.failure {
			Some(reason) => Err(StorageClientError::new(reason)),
			None => Ok(()),
		}
	}
}

#[async_trait::async_trait]
impl StorageClient for RecordingStore {
	async fn load(
		&self,
		genesis_hash: [u8; 32],
		_stack: Option<String>,
	) -> Result<Option<String>, StorageClientError> {
		self.calls.lock().expect("not poisoned").push((genesis_hash.to_vec(), None));
		self.answer().map(|()| Some("blob".to_owned()))
	}

	async fn save(
		&self,
		genesis_hash: [u8; 32],
		blob: String,
		_stack: Option<String>,
	) -> Result<(), StorageClientError> {
		self.calls
			.lock()
			.expect("not poisoned")
			.push((genesis_hash.to_vec(), Some(blob)));
		self.answer()
	}
}

#[test]
fn a_wrong_length_genesis_is_rejected_before_storage_is_asked() {
	// The check has to happen on this side: a host that got handed 31 bytes
	// would key its storage by them and answer a blob for a chain that has
	// no such hash.
	let store = RecordingStore::new(None);
	let provider = ChainProvider::with_storage(store.clone());

	for outcome in [
		block_on(provider.load_database(vec![0u8; 31])),
		block_on(provider.save_database(vec![0u8; 33])),
	] {
		assert!(matches!(outcome, Err(ChainProviderError::BadGenesis)), "got {outcome:?}");
	}
	assert!(store.calls.lock().expect("not poisoned").is_empty());
}

#[test]
fn an_undeclared_storage_error_converts_and_is_bounded() {
	let thrown = format!("HEAD{}", "\u{1f600}".repeat(CLOSE_REASON_MAX_CHARS * 2));
	let StorageClientError::Failed { reason } =
		StorageClientError::from(uniffi::UnexpectedUniFFICallbackError::new(thrown));

	assert_eq!(reason.chars().count(), CLOSE_REASON_MAX_CHARS);
	assert!(reason.starts_with("HEAD"), "the bound keeps the head");
}

/// Foreign lifecycle listener stand-in.
struct LifecycleCollector {
	states: Sender<ChainLifecycle>,
	ended: Sender<()>,
}

impl ChainLifecycleListener for LifecycleCollector {
	fn on_lifecycle(&self, state: ChainLifecycle) -> Result<(), ChainProviderError> {
		let _ = self.states.send(state);
		Ok(())
	}

	fn on_ended(&self) -> Result<(), ChainProviderError> {
		let _ = self.ended.send(());
		Ok(())
	}
}

/// A host shows sync progress from the first state on, and must be told
/// when the watch it stopped has actually ended.
#[test]
fn a_lifecycle_watch_reports_the_current_state_and_ends_when_stopped() {
	let provider = ChainProvider::new();
	let (listener, _messages, _closed) = Collector::new();
	let _connection = provider
		.connect(CATALOG_RELAY.to_vec(), listener)
		.expect("the catalog resolves its own relay genesis");

	let (states, state_rx) = channel();
	let (ended, ended_rx) = channel();
	let watch = provider
		.watch_lifecycle(CATALOG_RELAY.to_vec(), Arc::new(LifecycleCollector { states, ended }))
		.expect("a connected chain can be watched");
	state_rx
		.recv_timeout(Duration::from_secs(30))
		.expect("the current state is delivered first");

	watch.stop();
	ended_rx
		.recv_timeout(Duration::from_secs(30))
		.expect("stopping the watch fires on_ended");
}

/// Nothing runs a chain nobody connected to, so there is nothing to watch.
#[test]
fn watching_a_chain_nothing_is_connected_to_fails() {
	let (states, _state_rx) = channel();
	let (ended, _ended_rx) = channel();
	let error = ChainProvider::new()
		.watch_lifecycle(CATALOG_RELAY.to_vec(), Arc::new(LifecycleCollector { states, ended }))
		.err()
		.expect("an unconnected chain has no lifecycle");
	let ChainProviderError::Lifecycle { reason } = error else {
		panic!("expected a Lifecycle error");
	};
	assert!(reason.contains("connect to it first"), "unexpected: {reason}");
}

#[test]
fn a_genesis_hash_outside_the_catalog_is_rejected() {
	let (listener, _messages, _closed) = Collector::new();
	let error = ChainProvider::new()
		.connect(vec![0xab; 32], listener)
		.err()
		.expect("an unbundled genesis must be rejected");
	let ChainProviderError::Connect { reason } = error else {
		panic!("expected a Connect error");
	};
	assert!(reason.contains(&"ab".repeat(32)), "unexpected: {reason}");
}

/// The whole foreign contract end to end: connect by catalog genesis, send a
/// request, receive it on the listener, and see `on_closed` after
/// `disconnect()`.
#[test]
fn a_catalog_chain_answers_on_the_listener_and_closes() {
	let (listener, messages, closed) = Collector::new();
	let connection = ChainProvider::new()
		.connect(CATALOG_RELAY.to_vec(), listener)
		.expect("the catalog resolves its own relay genesis");

	connection.send(
		r#"{"jsonrpc":"2.0","id":1,"method":"chainSpec_v1_chainName","params":[]}"#.to_owned(),
	);
	let response = messages
		.recv_timeout(Duration::from_secs(30))
		.expect("smoldot answers spec-local queries without a network");
	assert!(response.contains("Paseo"), "unexpected: {response}");

	connection.disconnect();
	closed
		.recv_timeout(Duration::from_secs(30))
		.expect("disconnecting ends the stream and fires on_closed");
}

/// A native host fetches each spec itself, so a wrong one has to come back
/// naming its chain, and a spec that arrives late has to bring its chain up on
/// the provider the host already holds.
#[test]
fn a_stack_provider_checks_its_specs_and_takes_one_later() {
	use crate::test_stack::{PEOPLE_SPEC, RELAY_SPEC, descriptor_json, genesis};

	let link = "polkadotapp://stack?url=https%3A%2F%2Fstack.example%2Fdescriptor.json".to_owned();
	let descriptor = serde_json::to_vec(&descriptor_json("one")).expect("JSON");
	let (relay, people) = (genesis(RELAY_SPEC).to_vec(), genesis(PEOPLE_SPEC).to_vec());
	let spec = |genesis_hash: &Vec<u8>, spec: &str| LightSpec {
		genesis_hash: genesis_hash.clone(),
		spec: spec.as_bytes().to_vec(),
	};

	let listed = stack_light_specs(link.clone(), descriptor.clone()).expect("the link opens");
	assert_eq!(
		listed.iter().map(|listed| listed.genesis_hash.to_vec()).collect::<Vec<_>>()[..2],
		[relay.clone(), people.clone()],
		"a host reads what to fetch from the binding"
	);

	let refused =
		ChainProvider::for_stack(link.clone(), descriptor.clone(), vec![spec(&relay, PEOPLE_SPEC)])
			.err()
			.expect("People's spec is not the relay's");
	assert!(
		matches!(&refused, ChainProviderError::LightSpec { genesis_hash, reason }
			if *genesis_hash == relay && reason.contains("Relay")),
		"got {refused:?}"
	);

	let provider = ChainProvider::for_stack(link, descriptor, vec![spec(&relay, RELAY_SPEC)])
		.expect("the relay's spec matches the descriptor");
	let (listener, _messages, _closed) = Collector::new();
	assert!(
		matches!(
			provider.connect(people.clone(), listener.clone()),
			Err(ChainProviderError::Connect { .. })
		),
		"People has no spec yet"
	);
	provider
		.add_light_spec(people.clone(), PEOPLE_SPEC.as_bytes().to_vec())
		.expect("the spec matches the descriptor");
	provider
		.connect(people, listener)
		.expect("People syncs through the stack's relay");
}
