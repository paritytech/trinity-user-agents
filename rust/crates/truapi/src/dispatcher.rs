//! Request dispatcher.
//!
//! Routes incoming frames to the appropriate trait method based on the
//! numeric `(trait, method)` wire discriminant pair. The handler set is
//! registered by the auto-generated
//! [`crate::generated::dispatcher::register`] function; this module provides
//! the framework that owns the registration tables and the routing logic.

use core::sync::atomic::{AtomicBool, Ordering};
use std::{
	collections::{HashMap, HashSet, VecDeque},
	sync::{Arc, Mutex},
};

use futures::future::BoxFuture;
use parity_scale_codec::Encode;
use tracing::{error, instrument};
use truapi::CancellationToken;

use crate::{
	frame::{
		MESSAGE_TYPE_CANCEL, MESSAGE_TYPE_INTERRUPT, MESSAGE_TYPE_REQUEST, MESSAGE_TYPE_RESPONSE,
		MESSAGE_TYPE_START, MESSAGE_TYPE_STOP, PROTOCOL_ERROR_KEY, PROTOCOL_ERROR_METHOD_ID,
		PROTOCOL_ERROR_TRAIT_ID, Payload, ProtocolErrorV1, ProtocolMessage, VersionedProtocolError,
		encode_cancelled_response,
	},
	generated::wire_table::MethodIds,
	subscription::{Spawner, SubscriptionManager, SubscriptionStream},
	transport::Transport,
};

/// A handler for a request-response method. TrUAPI service traits require
/// their returned futures to be [`Send`], allowing native dispatch to move
/// across executor threads while WASM remains free to poll the same future on
/// its local executor. The `request_id` is the per-frame identifier; handlers
/// thread it into the `CallContext` so trait methods can correlate
/// logs/cancellation with the originating request. The returned bytes are the
/// complete SCALE-encoded response payload on both the success and error
/// paths, since a method's failure is a `Result` inside that payload rather
/// than a failure to produce one.
///
/// The [`CancellationToken`] is the dispatcher's, not the handler's: a
/// `Cancel` frame naming this `request_id` fires it while the handler runs.
pub type RequestHandler =
	Arc<dyn Fn(String, Vec<u8>, CancellationToken) -> BoxFuture<'static, Vec<u8>> + Send + Sync>;

/// A handler for a subscription method. On the error path the handler
/// returns the complete SCALE-encoded `Interrupt` payload.
pub type SubscriptionHandler = Arc<
	dyn Fn(String, Vec<u8>) -> BoxFuture<'static, Result<SubscriptionStream, Vec<u8>>>
		+ Send
		+ Sync,
>;

/// A registered request handler plus the discriminants it replies on.
pub struct RequestEntry {
	ids: MethodIds,
	handler: RequestHandler,
}

/// A registered subscription handler plus the discriminants its frames carry.
pub struct SubscriptionEntry {
	ids: MethodIds,
	handler: SubscriptionHandler,
}

/// Routes incoming protocol messages to registered handlers, keyed on the
/// numeric `(trait, method)` wire discriminant pair.
pub struct Dispatcher {
	by_request: HashMap<(u8, u8), RequestEntry>,
	by_start: HashMap<(u8, u8), SubscriptionEntry>,
	subscriptions: SubscriptionManager,
	/// Trusted executable kind bound to this connection; `None` leaves the
	/// surface unrestricted for direct dispatcher embeddings.
	execution: Option<crate::platform::ProductExecutionKind>,
	/// `(trait, method)` pairs already reported on this connection. A peer
	/// whose wire table disagrees with ours is exactly what these reports are
	/// for, and exactly what would repeat them once per frame, so each pair is
	/// reported once and then stays quiet.
	reported_violations: Mutex<HashSet<(u8, u8)>>,
	/// Every request the dispatcher is running, or has been asked to
	/// withdraw before it arrived.
	requests: Mutex<RequestRegistry>,
}

/// How many withdrawals the dispatcher remembers for calls whose `Request` it
/// has not seen. Capped because a peer can cancel ids it never sends, and
/// evicted oldest first: an eviction only reopens the race this exists to
/// close, which is the trade that keeps the queue from growing without bound.
const MAX_EARLY_WITHDRAWALS: usize = 64;

/// The dispatcher's view of the requests it is responsible for.
#[derive(Default)]
struct RequestRegistry {
	/// Keyed by the `requestId` a `Cancel` frame names.
	in_flight: HashMap<String, InFlightRequest>,
	/// Ids withdrawn before their `Request` arrived, oldest first. Transports
	/// spawn a task per frame, so the pair a client wrote to the socket in
	/// order can reach dispatch out of order, and a `Cancel` that finds
	/// nothing would otherwise be lost. A short queue rather than a set: the
	/// cap is small enough that scanning beats a second index.
	///
	/// Keyed by id alone, like `handle_cancel` itself: the `(trait, method)`
	/// a cancel arrived on is not part of the correlation, so a recorded id
	/// withdraws whichever request next presents it. Safe because a client's
	/// ids are monotonic within a connection, but do not read the address as
	/// a second key.
	///
	/// A cancel arriving after its call already settled lands here too, since
	/// both cases look like a miss. That entry is inert, because a client's
	/// ids are unique for the life of a connection and this registry does not
	/// outlive one, and it is evicted in its turn. It only costs a slot, and
	/// a genuine early withdrawal is claimed within microseconds of being
	/// recorded, so crowding one out would take a burst of stale cancels
	/// between a `Cancel` and its own `Request`.
	early_withdrawals: VecDeque<String>,
}

impl RequestRegistry {
	/// Record that `request_id` was withdrawn before its `Request` arrived.
	fn remember_early_withdrawal(&mut self, request_id: &str) {
		if self.early_withdrawals.iter().any(|id| id == request_id) {
			return;
		}
		if self.early_withdrawals.len() == MAX_EARLY_WITHDRAWALS {
			self.early_withdrawals.pop_front();
		}
		self.early_withdrawals.push_back(request_id.to_string());
	}
}

/// What a `Request` frame may do with the id it names.
enum Reservation {
	/// Run the handler with this token.
	Start(CancellationToken),
	/// A `Cancel` for this id arrived first. The call answers `Cancelled`
	/// without the handler running at all, so one withdrawn before it started
	/// never reaches a person.
	AlreadyWithdrawn,
	/// The id is already running, and is refused rather than displaced.
	Duplicate,
}

/// A request the dispatcher is currently running, as a `Cancel` frame can
/// reach it.
struct InFlightRequest {
	cancel: CancellationToken,
	/// Set only when a `Cancel` frame fired `cancel`, never when a runtime
	/// attached its own timeout. Without that split a host would answer
	/// `CallError::Cancelled` to a peer that never asked for the variant and
	/// cannot decode it.
	withdrawn: AtomicBool,
}

impl Dispatcher {
	/// Construct a dispatcher whose subscriptions are driven on `spawner`.
	pub fn new(spawner: Spawner) -> Self {
		Self {
			by_request: HashMap::new(),
			by_start: HashMap::new(),
			subscriptions: SubscriptionManager::new(spawner),
			execution: None,
			reported_violations: Mutex::new(HashSet::new()),
			requests: Mutex::new(RequestRegistry::default()),
		}
	}

	/// Construct a dispatcher bound to a trusted executable kind.
	pub fn for_execution(
		spawner: Spawner,
		execution: crate::platform::ProductExecutionKind,
	) -> Self {
		Self { execution: Some(execution), ..Self::new(spawner) }
	}

	/// Run `report` only the first time `key` violates the protocol on this
	/// connection. The lock is released before `report` runs, so a report can
	/// never be held up by, or hold up, another frame's dispatch.
	fn report_once(&self, key: (u8, u8), report: impl FnOnce()) {
		let first = self
			.reported_violations
			.lock()
			.expect("dispatcher violation set mutex poisoned")
			.insert(key);
		if first {
			report();
		}
	}

	/// Return whether this connection may access a service execution kind.
	pub fn allows_execution(&self, required: crate::platform::ProductExecutionKind) -> bool {
		self.execution.is_none_or(|actual| actual == required)
	}

	/// Register a request-response handler, keyed on
	/// `(ids.trait_id, ids.method_id)`. Returns the previously registered
	/// entry if any; callers (the generated `dispatcher::register`) should
	/// treat `Some` as a programming error since each discriminant pair must
	/// own exactly one handler.
	pub fn on_request<F>(&mut self, ids: MethodIds, handler: F) -> Option<RequestEntry>
	where
		F: Fn(String, Vec<u8>, CancellationToken) -> BoxFuture<'static, Vec<u8>>
			+ Send
			+ Sync
			+ 'static,
	{
		self.by_request
			.insert((ids.trait_id, ids.method_id), RequestEntry { ids, handler: Arc::new(handler) })
	}

	/// Register a subscription handler, keyed on
	/// `(ids.trait_id, ids.method_id)`. A `Stop` frame arrives at this same
	/// address — [`dispatch`](Self::dispatch) checks its `message_type` and
	/// routes it to [`SubscriptionManager::handle_stop`] directly, without
	/// invoking this handler. Returns the previously registered entry if any.
	pub fn on_subscription<F>(&mut self, ids: MethodIds, handler: F) -> Option<SubscriptionEntry>
	where
		F: Fn(String, Vec<u8>) -> BoxFuture<'static, Result<SubscriptionStream, Vec<u8>>>
			+ Send
			+ Sync
			+ 'static,
	{
		self.by_start.insert(
			(ids.trait_id, ids.method_id),
			SubscriptionEntry { ids, handler: Arc::new(handler) },
		)
	}

	/// Process an incoming protocol message, sending any responses or
	/// subscription frames through `transport`. A `(trait, method)` pair with
	/// no registered handler is answered with a correlated protocol error
	/// rather than dropped, so a peer learns its frame went unhandled instead
	/// of waiting on a reply that never comes.
	#[instrument(skip_all, fields(runtime.method = "dispatcher.dispatch"))]
	pub async fn dispatch(&self, message: ProtocolMessage, transport: Arc<dyn Transport>) {
		let key = (message.payload.trait_id, message.payload.method_id);

		// Never answer a protocol error with a protocol error: two peers that
		// disagree would otherwise trade frames forever.
		if key == PROTOCOL_ERROR_KEY {
			return;
		}

		if let Some(entry) = self.by_request.get(&key) {
			// Dropped quietly when it names nothing: the call already
			// answered, which is the ordinary race rather than a violation.
			if message.payload.message_type == MESSAGE_TYPE_CANCEL {
				self.handle_cancel(&message.request_id);
				return;
			}
			// `Request` is the only other leg a request method receives. Its
			// `Response` shares this address, so without this guard a peer
			// whose table disagrees with ours, or this side's own outbound
			// response arriving here, would run the handler and be answered
			// with a `Response` to a non-request. Logged because the pair is
			// one we implement: an unknown pair is merely an incompatible
			// peer, but a known method receiving a leg it cannot have is a
			// bug on one side or the other.
			if message.payload.message_type != MESSAGE_TYPE_REQUEST {
				let message_type = message.payload.message_type;
				self.report_once(key, || {
					error!(
						trait_id = key.0,
						method_id = key.1,
						message_type,
						"dropping a frame whose message type a request method cannot receive"
					);
				});
				return;
			}
			let request_id = message.request_id.clone();
			// Registered before the handler is awaited so a `Cancel` that
			// arrives while it runs finds a token to fire, and so one that
			// arrived first is found here. An id already in flight is refused
			// rather than replacing the entry: a `Cancel` names a call by id
			// alone, so two live calls sharing one id leave neither
			// addressable.
			let value = match self.reserve_request(&request_id) {
				Reservation::Duplicate => {
					self.report_once(key, || {
						error!(
							trait_id = key.0,
							method_id = key.1,
							"dropping a request whose id is already in flight"
						);
					});
					return;
				},
				Reservation::AlreadyWithdrawn => encode_cancelled_response(),
				Reservation::Start(cancel) => {
					let value =
						(entry.handler)(request_id.clone(), message.payload.value, cancel).await;
					// The single terminal `Response` is how a peer learns its
					// cancel was heard, so it wins over whatever the handler
					// returned.
					if self.release_request(&request_id) {
						encode_cancelled_response()
					} else {
						value
					}
				},
			};
			transport.send(ProtocolMessage {
				request_id: message.request_id,
				payload: Payload {
					trait_id: entry.ids.trait_id,
					method_id: entry.ids.method_id,
					message_type: MESSAGE_TYPE_RESPONSE,
					value,
				},
			});
		} else if let Some(entry) = self.by_start.get(&key) {
			if message.payload.message_type == MESSAGE_TYPE_STOP {
				self.subscriptions.handle_stop(&message.request_id);
				return;
			}
			// `Start` and `Stop` are the only legs this side receives; a
			// subscription's `Receive` and `Interrupt` flow the other way and
			// share this address too, so anything else here would otherwise
			// start a subscription off a frame that is not a start.
			if message.payload.message_type != MESSAGE_TYPE_START {
				let message_type = message.payload.message_type;
				self.report_once(key, || {
					error!(
						trait_id = key.0,
						method_id = key.1,
						message_type,
						"dropping a frame whose message type a subscription cannot receive"
					);
				});
				return;
			}
			// Reserve the slot before awaiting the handler so a `_stop`
			// arriving while the handler resolves cancels the pending
			// subscription instead of racing the registration.
			let request_id = message.request_id.clone();
			let token = self.subscriptions.reserve(request_id.clone());
			let result = (entry.handler)(request_id, message.payload.value).await;
			match result {
				Ok(stream) => {
					self.subscriptions.activate(
						token,
						entry.ids.trait_id,
						entry.ids.method_id,
						stream,
						transport,
					);
				},
				Err(err_bytes) => {
					self.subscriptions.cancel_reservation(token);
					transport.send(ProtocolMessage {
						request_id: message.request_id,
						payload: Payload {
							trait_id: entry.ids.trait_id,
							method_id: entry.ids.method_id,
							message_type: MESSAGE_TYPE_INTERRUPT,
							value: err_bytes,
						},
					});
				},
			}
		} else {
			// Response / receive / interrupt frames are handled by the client
			// side and are never registered here, so they land in this arm too:
			// answering them is what tells a mismatched peer its frame was not
			// understood.
			let (trait_id, method_id) = key;
			// `ERROR` because this is the whole reason the crate's default
			// floor is `ERROR` (see the `logging` module doc): a host that
			// never calls `setLogLevel` still has to learn that its peer is
			// speaking a wire it does not understand. This is also the string
			// the local e2e docs tell people to grep for.
			self.report_once(key, || {
				error!(trait_id, method_id, "unknown wire discriminant pair");
			});
			// A codec 2 peer that asked for something unimplemented can read
			// the answer, and dropping it would leave the peer waiting forever.
			transport.send(ProtocolMessage {
				request_id: message.request_id,
				payload: Payload {
					trait_id: PROTOCOL_ERROR_TRAIT_ID,
					method_id: PROTOCOL_ERROR_METHOD_ID,
					message_type: MESSAGE_TYPE_RESPONSE,
					value: VersionedProtocolError::V1(ProtocolErrorV1::UnsupportedMessage {
						trait_id,
						method_id,
					})
					.encode(),
				},
			});
		}
	}

	/// Cancel every subscription currently owned by this dispatcher.
	pub fn cancel_subscriptions(&self) {
		self.subscriptions.cancel_all();
	}

	/// Decide what the `Request` naming `request_id` may do: start with a
	/// fresh token, answer `Cancelled` because a `Cancel` beat it here, or be
	/// refused because the id is already running.
	///
	/// The token is returned from the same locked section that stores it. A
	/// caller that re-read the registry to fetch it would be reading across a
	/// gap another thread can write in.
	fn reserve_request(&self, request_id: &str) -> Reservation {
		let mut registry =
			self.requests.lock().expect("dispatcher request registry mutex poisoned");
		if registry.in_flight.contains_key(request_id) {
			return Reservation::Duplicate;
		}
		if let Some(position) = registry.early_withdrawals.iter().position(|id| id == request_id) {
			registry.early_withdrawals.remove(position);
			return Reservation::AlreadyWithdrawn;
		}
		let cancel = CancellationToken::default();
		registry.in_flight.insert(
			request_id.to_string(),
			InFlightRequest { cancel: cancel.clone(), withdrawn: AtomicBool::new(false) },
		);
		Reservation::Start(cancel)
	}

	/// Drop `request_id`'s entry. Returns whether a `Cancel` frame withdrew
	/// this call while it ran.
	fn release_request(&self, request_id: &str) -> bool {
		let mut registry =
			self.requests.lock().expect("dispatcher request registry mutex poisoned");
		let Some(entry) = registry.in_flight.remove(request_id) else {
			return false;
		};
		entry.withdrawn.load(Ordering::Acquire)
	}

	/// Fire the token of the call `request_id` names and mark it withdrawn,
	/// or remember the id when no such call has reached dispatch yet. The
	/// entry stays until the handler returns, so the call settles by its own
	/// path rather than this one.
	fn handle_cancel(&self, request_id: &str) {
		let mut registry =
			self.requests.lock().expect("dispatcher request registry mutex poisoned");
		let token = registry.in_flight.get(request_id).map(|entry| {
			entry.withdrawn.store(true, Ordering::Release);
			entry.cancel.clone()
		});
		match token {
			Some(token) => {
				drop(registry);
				token.cancel();
			},
			// The request has not reached dispatch yet, or never will.
			// Remembering the id is what stops a cancel that overtook its own
			// request from being lost.
			None => registry.remember_early_withdrawal(request_id),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::frame::{MESSAGE_TYPE_RECEIVE, MESSAGE_TYPE_REQUEST};
	use std::sync::Mutex;

	fn test_spawner() -> Spawner {
		#[cfg(not(target_arch = "wasm32"))]
		{
			crate::subscription::thread_per_subscription_spawner()
		}
		#[cfg(target_arch = "wasm32")]
		{
			Arc::new(futures::executor::block_on)
		}
	}

	#[derive(Default)]
	struct RecordingTransport {
		sent: Mutex<Vec<ProtocolMessage>>,
	}

	impl RecordingTransport {
		fn sent(&self) -> Vec<ProtocolMessage> {
			self.sent.lock().unwrap().clone()
		}
	}

	impl Transport for RecordingTransport {
		fn send(&self, message: ProtocolMessage) {
			self.sent.lock().unwrap().push(message);
		}
		fn on_message(
			&self,
			_handler: Box<dyn Fn(ProtocolMessage) + Send + Sync>,
		) -> Box<dyn FnOnce()> {
			Box::new(|| {})
		}
	}

	/// A frame naming `request_id`, for the tests that need more than one.
	fn frame_for(
		request_id: &str,
		trait_id: u8,
		method_id: u8,
		message_type: u8,
	) -> ProtocolMessage {
		ProtocolMessage {
			request_id: request_id.into(),
			payload: Payload { trait_id, method_id, message_type, value: Vec::new() },
		}
	}

	/// The `Response` `request_id` is answered with.
	fn response_for(
		request_id: &str,
		trait_id: u8,
		method_id: u8,
		value: Vec<u8>,
	) -> ProtocolMessage {
		ProtocolMessage {
			request_id: request_id.into(),
			payload: Payload { trait_id, method_id, message_type: MESSAGE_TYPE_RESPONSE, value },
		}
	}

	/// The `Response` a request at `(trait_id, method_id)` answers with.
	fn expect_response(trait_id: u8, method_id: u8, value: Vec<u8>) -> Vec<ProtocolMessage> {
		vec![response_for("p:1", trait_id, method_id, value)]
	}

	fn make_frame(
		trait_id: u8,
		method_id: u8,
		message_type: u8,
		value: Vec<u8>,
	) -> ProtocolMessage {
		ProtocolMessage {
			request_id: "p:1".into(),
			payload: Payload { trait_id, method_id, message_type, value },
		}
	}

	#[test]
	fn dispatch_unknown_id_sends_correlated_protocol_error() {
		let dispatcher = Dispatcher::new(test_spawner());
		let transport = Arc::new(RecordingTransport::default());
		let transport_dyn: Arc<dyn Transport> = transport.clone();
		let frame = make_frame(250, 251, MESSAGE_TYPE_REQUEST, Vec::new());
		futures::executor::block_on(dispatcher.dispatch(frame, transport_dyn));
		// 250 != 251 on purpose: the reply must echo the pair in the order it
		// arrived, and equal values would let a transposition pass.
		assert_eq!(
			transport.sent(),
			vec![ProtocolMessage {
				request_id: "p:1".into(),
				payload: Payload {
					trait_id: PROTOCOL_ERROR_TRAIT_ID,
					method_id: PROTOCOL_ERROR_METHOD_ID,
					message_type: MESSAGE_TYPE_RESPONSE,
					value: VersionedProtocolError::V1(ProtocolErrorV1::UnsupportedMessage {
						trait_id: 250,
						method_id: 251,
					})
					.encode(),
				},
			}]
		);
	}

	#[test]
	fn dispatch_protocol_error_does_not_send_another_error() {
		let dispatcher = Dispatcher::new(test_spawner());
		let transport = Arc::new(RecordingTransport::default());
		let frame = make_frame(
			PROTOCOL_ERROR_TRAIT_ID,
			PROTOCOL_ERROR_METHOD_ID,
			MESSAGE_TYPE_RESPONSE,
			VersionedProtocolError::V1(ProtocolErrorV1::UnsupportedMessage {
				trait_id: 250,
				method_id: 251,
			})
			.encode(),
		);
		futures::executor::block_on(dispatcher.dispatch(frame, transport.clone()));
		assert_eq!(transport.sent(), Vec::<ProtocolMessage>::new());
	}

	/// A handler error already owns the complete response payload. The
	/// dispatcher only routes it back to the same address the request
	/// arrived on — request and response now share one id.
	#[test]
	fn dispatch_request_handler_error_emits_response_payload() {
		let mut dispatcher = Dispatcher::new(test_spawner());
		let ids = MethodIds { trait_id: 7, method_id: 200 };
		dispatcher.on_request(ids, |_request_id, _bytes, _cancel| {
			// A method's failure is a `Result` inside the response payload, so
			// an error path still hands back bytes to send.
			Box::pin(async move { vec![9, 8, 7] })
		});
		let transport = Arc::new(RecordingTransport::default());
		let frame = make_frame(7, 200, MESSAGE_TYPE_REQUEST, Vec::new());
		futures::executor::block_on(dispatcher.dispatch(frame, transport.clone()));
		let sent = transport.sent();
		assert_eq!(sent.len(), 1, "exactly one response expected");
		assert_eq!(sent[0].payload.trait_id, 7);
		assert_eq!(sent[0].payload.method_id, 200);
		assert_eq!(sent[0].payload.value, vec![9, 8, 7]);
	}

	/// Registering two handlers under the same key must not silently
	/// overwrite. The contract chosen here is "loud": `on_request`
	/// returns the previous handler, so callers can detect collisions.
	#[test]
	fn register_request_twice_returns_previous_handler() {
		let mut dispatcher = Dispatcher::new(test_spawner());
		let ids = MethodIds { trait_id: 7, method_id: 200 };
		let prev = dispatcher
			.on_request(ids, |_request_id, _bytes, _cancel| Box::pin(async move { Vec::new() }));
		assert!(prev.is_none(), "first registration has no predecessor");
		let prev = dispatcher
			.on_request(ids, |_request_id, _bytes, _cancel| Box::pin(async move { Vec::new() }));
		assert!(prev.is_some(), "second registration must return the previous handler");
	}

	/// A `Stop` frame (`message_type == MESSAGE_TYPE_STOP`) arrives at the
	/// same address as `Start` and must route to
	/// `SubscriptionManager::handle_stop` directly — never invoking the
	/// registered handler, which would otherwise try to start a second
	/// subscription instead of cancelling the first.
	#[test]
	fn stop_frame_never_invokes_the_subscription_handler() {
		let mut dispatcher = Dispatcher::new(test_spawner());
		let ids = MethodIds { trait_id: 7, method_id: 50 };
		let invoked = Arc::new(std::sync::atomic::AtomicBool::new(false));
		let invoked_in_handler = invoked.clone();
		dispatcher.on_subscription(ids, move |_request_id, _bytes| {
			invoked_in_handler.store(true, std::sync::atomic::Ordering::SeqCst);
			Box::pin(async move { Ok(Box::pin(futures::stream::empty()) as SubscriptionStream) })
		});
		let transport = Arc::new(RecordingTransport::default());
		let transport_dyn: Arc<dyn Transport> = transport.clone();
		let frame = make_frame(7, 50, MESSAGE_TYPE_STOP, Vec::new());
		futures::executor::block_on(dispatcher.dispatch(frame, transport_dyn));
		assert!(
			!invoked.load(std::sync::atomic::Ordering::SeqCst),
			"a Stop frame must not invoke the subscription handler"
		);
		assert!(transport.sent().is_empty(), "handle_stop on an unknown request id emits no frame");
	}

	/// A request method's `Response` shares its address. Reading the leg off
	/// `message_type` is the only thing that stops an inbound `Response`, or
	/// any other leg, from being run as a fresh request and answered.
	/// `Cancel` is the one other leg this address accepts, and it is covered
	/// by its own tests rather than listed here.
	#[test]
	fn a_request_method_ignores_every_leg_but_request_and_cancel() {
		for message_type in [MESSAGE_TYPE_RESPONSE, MESSAGE_TYPE_INTERRUPT, MESSAGE_TYPE_STOP, 99] {
			let mut dispatcher = Dispatcher::new(test_spawner());
			let ids = MethodIds { trait_id: 7, method_id: 50 };
			let invoked = Arc::new(std::sync::atomic::AtomicBool::new(false));
			let invoked_in_handler = invoked.clone();
			dispatcher.on_request(ids, move |_request_id, _bytes, _cancel| {
				invoked_in_handler.store(true, std::sync::atomic::Ordering::SeqCst);
				Box::pin(async move { Vec::new() })
			});
			let transport = Arc::new(RecordingTransport::default());
			let transport_dyn: Arc<dyn Transport> = transport.clone();
			let frame = make_frame(7, 50, message_type, Vec::new());
			futures::executor::block_on(dispatcher.dispatch(frame, transport_dyn));
			assert!(
				!invoked.load(std::sync::atomic::Ordering::SeqCst),
				"message_type {message_type} must not invoke a request handler"
			);
			assert!(
				transport.sent().is_empty(),
				"message_type {message_type} must not be answered"
			);
		}
	}

	/// The whole point of the leg: a `Cancel` naming a call in flight fires
	/// the token that call's handler is holding, and the call still settles
	/// with exactly one `Response`.
	#[test]
	fn cancel_fires_the_in_flight_token_and_the_call_still_answers_once() {
		let mut dispatcher = Dispatcher::new(test_spawner());
		let ids = MethodIds { trait_id: 7, method_id: 60 };
		// The handler parks on its token, so the response it produces is
		// proof the cancel arrived rather than a race the test won by luck.
		dispatcher.on_request(ids, move |_request_id, _bytes, cancel| {
			Box::pin(async move {
				cancel.cancelled().await;
				vec![1, 2, 3]
			})
		});
		let transport = Arc::new(RecordingTransport::default());

		let request_transport: Arc<dyn Transport> = transport.clone();
		let cancel_transport: Arc<dyn Transport> = transport.clone();
		// `join` polls in argument order, so the request reaches its
		// registration before the cancel looks for it.
		futures::executor::block_on(futures::future::join(
			dispatcher
				.dispatch(make_frame(7, 60, MESSAGE_TYPE_REQUEST, Vec::new()), request_transport),
			dispatcher
				.dispatch(make_frame(7, 60, MESSAGE_TYPE_CANCEL, Vec::new()), cancel_transport),
		));

		assert_eq!(
			transport.sent(),
			expect_response(7, 60, encode_cancelled_response()),
			"a withdrawn call answers Cancelled exactly once"
		);
	}

	/// The substituted payload has to decode as *any* method's response leg,
	/// because the dispatcher holds only bytes and cannot name either of a
	/// method's payload types. Decoded here against a real generated pair to
	/// prove the shape rather than assert the two bytes back at themselves.
	#[test]
	fn a_cancelled_response_decodes_against_a_real_methods_leg() {
		use parity_scale_codec::DecodeAll;
		use truapi::versioned::account::{HostAccountGetError, HostAccountGetResponse};

		let bytes = encode_cancelled_response();
		let decoded: Result<HostAccountGetResponse, truapi::CallError<HostAccountGetError>> =
			DecodeAll::decode_all(&mut &bytes[..]).expect("decodes as a response leg");
		assert_eq!(decoded, Err(truapi::CallError::Cancelled));

		// Same bytes, a method whose payload types share nothing with the one
		// above: the encoding depends on neither.
		use truapi::versioned::system::{HostInfoError, HostInfoResponse};
		let decoded: Result<HostInfoResponse, truapi::CallError<HostInfoError>> =
			DecodeAll::decode_all(&mut &bytes[..]).expect("decodes as a response leg");
		assert_eq!(decoded, Err(truapi::CallError::Cancelled));
	}

	/// A `Cancel` names a call by id alone, so two live calls sharing one id
	/// would leave neither of them addressable. The second is refused, and
	/// the first stays the one that id reaches.
	///
	/// Driven through the registry rather than two dispatches: a duplicate
	/// that took the entry would park the first handler forever, and a test
	/// that reports a hang says less than one that names what broke.
	#[test]
	fn a_request_whose_id_is_already_in_flight_is_refused() {
		let dispatcher = Dispatcher::new(test_spawner());

		let Reservation::Start(first) = dispatcher.reserve_request("p:1") else {
			panic!("the first call registers");
		};
		assert!(
			matches!(dispatcher.reserve_request("p:1"), Reservation::Duplicate),
			"an id already in flight must be refused, not replaced"
		);

		dispatcher.handle_cancel("p:1");
		assert!(first.is_cancelled(), "the first call is still the one that id cancels");
	}

	/// The registry is the only thing holding a peer-supplied id, so a call
	/// that settles has to give its entry back. Without this the map grows
	/// for the life of the connection.
	#[test]
	fn a_settled_call_leaves_no_entry_behind() {
		let mut dispatcher = Dispatcher::new(test_spawner());
		let ids = MethodIds { trait_id: 7, method_id: 62 };
		dispatcher.on_request(ids, move |_request_id, _bytes, _cancel| {
			Box::pin(async move { vec![9, 9] })
		});
		let transport = Arc::new(RecordingTransport::default());

		let transport_dyn: Arc<dyn Transport> = transport.clone();
		futures::executor::block_on(
			dispatcher.dispatch(make_frame(7, 62, MESSAGE_TYPE_REQUEST, Vec::new()), transport_dyn),
		);

		assert!(
			dispatcher
				.requests
				.lock()
				.expect("dispatcher request registry mutex poisoned")
				.in_flight
				.is_empty(),
			"a settled call must release its id"
		);
	}

	/// A handler the peer never withdrew keeps its own result, so a runtime
	/// timeout still reports through the method's own error type and the
	/// `Cancelled` variant stays off the wire for a peer that never asked.
	#[test]
	fn a_call_no_one_withdrew_keeps_its_own_result() {
		let mut dispatcher = Dispatcher::new(test_spawner());
		let ids = MethodIds { trait_id: 7, method_id: 63 };
		// The handler fires its own token, standing in for an attached
		// timeout, and answers normally.
		dispatcher.on_request(ids, move |_request_id, _bytes, cancel| {
			Box::pin(async move {
				cancel.cancel();
				vec![9, 9]
			})
		});
		let transport = Arc::new(RecordingTransport::default());
		let transport_dyn: Arc<dyn Transport> = transport.clone();
		futures::executor::block_on(
			dispatcher.dispatch(make_frame(7, 63, MESSAGE_TYPE_REQUEST, Vec::new()), transport_dyn),
		);
		assert_eq!(
			transport.sent(),
			expect_response(7, 63, vec![9, 9]),
			"only a Cancel frame substitutes the response"
		);
	}

	/// Transports spawn a task per frame, so the ordered pair a client wrote
	/// to the socket can reach dispatch the other way round. The withdrawal
	/// has to survive that, or a product that aborts on navigation waits out
	/// its full deadline while the host keeps working.
	#[test]
	fn a_cancel_that_overtakes_its_request_still_withdraws_it() {
		let mut dispatcher = Dispatcher::new(test_spawner());
		let ids = MethodIds { trait_id: 7, method_id: 64 };
		let invocations = Arc::new(std::sync::atomic::AtomicUsize::new(0));
		let handler_invocations = invocations.clone();
		dispatcher.on_request(ids, move |_request_id, _bytes, _cancel| {
			handler_invocations.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
			Box::pin(async move { vec![1, 2, 3] })
		});
		let transport = Arc::new(RecordingTransport::default());

		let cancel_transport: Arc<dyn Transport> = transport.clone();
		futures::executor::block_on(
			dispatcher
				.dispatch(make_frame(7, 64, MESSAGE_TYPE_CANCEL, Vec::new()), cancel_transport),
		);
		let request_transport: Arc<dyn Transport> = transport.clone();
		futures::executor::block_on(
			dispatcher
				.dispatch(make_frame(7, 64, MESSAGE_TYPE_REQUEST, Vec::new()), request_transport),
		);

		assert_eq!(
			invocations.load(std::sync::atomic::Ordering::SeqCst),
			0,
			"a call withdrawn before it started must never run"
		);
		assert_eq!(
			transport.sent(),
			expect_response(7, 64, encode_cancelled_response()),
			"the overtaking cancel still withdraws the call it names"
		);
	}

	/// A tombstone is spent by the call it withdraws. Leaving it behind makes
	/// the id permanently unusable on that connection: every later `Request`
	/// naming it answers `Cancelled` and its handler never runs, which is a
	/// withdrawal the peer did not ask for.
	#[test]
	fn an_early_withdrawal_is_spent_by_the_call_it_withdraws() {
		let mut dispatcher = Dispatcher::new(test_spawner());
		let ids = MethodIds { trait_id: 7, method_id: 65 };
		let invocations = Arc::new(std::sync::atomic::AtomicUsize::new(0));
		let handler_invocations = invocations.clone();
		dispatcher.on_request(ids, move |_request_id, _bytes, _cancel| {
			handler_invocations.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
			Box::pin(async move { vec![4, 5, 6] })
		});
		let transport = Arc::new(RecordingTransport::default());

		let dispatch = |message_type: u8| {
			let transport: Arc<dyn Transport> = transport.clone();
			futures::executor::block_on(
				dispatcher.dispatch(make_frame(7, 65, message_type, Vec::new()), transport),
			);
		};
		dispatch(MESSAGE_TYPE_CANCEL);
		dispatch(MESSAGE_TYPE_REQUEST);
		dispatch(MESSAGE_TYPE_REQUEST);

		assert_eq!(
			invocations.load(std::sync::atomic::Ordering::SeqCst),
			1,
			"the second call reuses a spent id, so its handler must run"
		);
		assert_eq!(
			transport.sent(),
			[
				expect_response(7, 65, encode_cancelled_response()),
				expect_response(7, 65, vec![4, 5, 6]),
			]
			.concat(),
			"the withdrawal answers the first call only; the second gets its own result"
		);
	}

	/// The refusal has to be visible where frames arrive, not just in the
	/// registry: a duplicate must leave without answering, where a withdrawn
	/// call answers `Cancelled`. Crossing those two would either answer a
	/// duplicate twice or swallow a withdrawal.
	#[test]
	fn a_duplicate_request_frame_is_never_answered() {
		let mut dispatcher = Dispatcher::new(test_spawner());
		let ids = MethodIds { trait_id: 7, method_id: 66 };
		let invocations = Arc::new(std::sync::atomic::AtomicUsize::new(0));
		let handler_invocations = invocations.clone();
		dispatcher.on_request(ids, move |_request_id, _bytes, cancel| {
			handler_invocations.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
			Box::pin(async move {
				cancel.cancelled().await;
				vec![1, 2, 3]
			})
		});
		let transport = Arc::new(RecordingTransport::default());

		let first: Arc<dyn Transport> = transport.clone();
		let duplicate: Arc<dyn Transport> = transport.clone();
		// The cancel is third so the parked first handler finishes and this
		// test reports an assertion rather than hanging.
		let cancel: Arc<dyn Transport> = transport.clone();
		futures::executor::block_on(futures::future::join3(
			dispatcher.dispatch(make_frame(7, 66, MESSAGE_TYPE_REQUEST, Vec::new()), first),
			dispatcher.dispatch(make_frame(7, 66, MESSAGE_TYPE_REQUEST, Vec::new()), duplicate),
			dispatcher.dispatch(make_frame(7, 66, MESSAGE_TYPE_CANCEL, Vec::new()), cancel),
		));

		assert_eq!(
			invocations.load(std::sync::atomic::Ordering::SeqCst),
			1,
			"the duplicate must not run the handler a second time"
		);
		assert_eq!(
			transport.sent(),
			expect_response(7, 66, encode_cancelled_response()),
			"the duplicate adds no frame: exactly one Response settles the id"
		);
	}

	/// The queue is bounded, so a peer cancelling ids it never sends cannot
	/// grow it without limit. Eviction is oldest first, and costs only the
	/// race this mechanism exists to close.
	#[test]
	fn the_oldest_early_withdrawal_is_evicted_once_the_queue_is_full() {
		let mut dispatcher = Dispatcher::new(test_spawner());
		let ids = MethodIds { trait_id: 7, method_id: 65 };
		dispatcher
			.on_request(ids, move |_request_id, _bytes, _cancel| Box::pin(async move { vec![7] }));
		let transport = Arc::new(RecordingTransport::default());

		// One more than the queue holds, so `p:0` is pushed back out.
		for n in 0..=MAX_EARLY_WITHDRAWALS {
			let cancel: Arc<dyn Transport> = transport.clone();
			futures::executor::block_on(
				dispatcher
					.dispatch(frame_for(&format!("p:{n}"), 7, 65, MESSAGE_TYPE_CANCEL), cancel),
			);
		}

		let evicted: Arc<dyn Transport> = transport.clone();
		futures::executor::block_on(
			dispatcher.dispatch(frame_for("p:0", 7, 65, MESSAGE_TYPE_REQUEST), evicted),
		);
		let remembered: Arc<dyn Transport> = transport.clone();
		futures::executor::block_on(dispatcher.dispatch(
			frame_for(&format!("p:{MAX_EARLY_WITHDRAWALS}"), 7, 65, MESSAGE_TYPE_REQUEST),
			remembered,
		));

		assert_eq!(
			transport.sent(),
			vec![
				response_for("p:0", 7, 65, vec![7]),
				response_for(
					&format!("p:{MAX_EARLY_WITHDRAWALS}"),
					7,
					65,
					encode_cancelled_response()
				),
			],
			"the evicted id runs normally; the one still remembered is withdrawn"
		);
	}

	/// A `Cancel` that names nothing in flight is the ordinary lost race: the
	/// handler must not run and nothing may be sent back.
	#[test]
	fn cancel_for_an_unknown_request_id_is_a_no_op() {
		let mut dispatcher = Dispatcher::new(test_spawner());
		let ids = MethodIds { trait_id: 7, method_id: 61 };
		let invoked = Arc::new(std::sync::atomic::AtomicBool::new(false));
		let invoked_in_handler = invoked.clone();
		dispatcher.on_request(ids, move |_request_id, _bytes, _cancel| {
			invoked_in_handler.store(true, std::sync::atomic::Ordering::SeqCst);
			Box::pin(async move { Vec::new() })
		});
		let transport = Arc::new(RecordingTransport::default());
		let transport_dyn: Arc<dyn Transport> = transport.clone();
		let frame = make_frame(7, 61, MESSAGE_TYPE_CANCEL, Vec::new());
		futures::executor::block_on(dispatcher.dispatch(frame, transport_dyn));
		assert!(
			!invoked.load(std::sync::atomic::Ordering::SeqCst),
			"a cancel must never invoke a request handler"
		);
		assert!(
			transport.sent().is_empty(),
			"a cancel is never answered, least of all one that names nothing"
		);
	}

	/// A handler that ignores its token is unaffected: cancelling is a
	/// request, and the call settles on the handler's own result.
	#[test]
	fn cancel_does_not_change_the_result_of_a_handler_that_ignores_it() {
		let mut dispatcher = Dispatcher::new(test_spawner());
		let ids = MethodIds { trait_id: 7, method_id: 62 };
		dispatcher.on_request(ids, move |_request_id, _bytes, _cancel| {
			Box::pin(async move { vec![4, 5] })
		});
		let transport = Arc::new(RecordingTransport::default());
		let transport_dyn: Arc<dyn Transport> = transport.clone();
		futures::executor::block_on(
			dispatcher.dispatch(make_frame(7, 62, MESSAGE_TYPE_REQUEST, Vec::new()), transport_dyn),
		);
		let transport_dyn: Arc<dyn Transport> = transport.clone();
		futures::executor::block_on(
			dispatcher.dispatch(make_frame(7, 62, MESSAGE_TYPE_CANCEL, Vec::new()), transport_dyn),
		);
		assert_eq!(
			transport.sent(),
			expect_response(7, 62, vec![4, 5]),
			"the cancel arrived too late and is dropped"
		);
	}

	/// `Receive` and `Interrupt` flow host to product and share the start
	/// address, so they must not start a subscription when they arrive here.
	#[test]
	fn a_subscription_starts_only_on_a_start_leg() {
		for message_type in [MESSAGE_TYPE_RECEIVE, MESSAGE_TYPE_INTERRUPT, 99] {
			let mut dispatcher = Dispatcher::new(test_spawner());
			let ids = MethodIds { trait_id: 7, method_id: 50 };
			let invoked = Arc::new(std::sync::atomic::AtomicBool::new(false));
			let invoked_in_handler = invoked.clone();
			dispatcher.on_subscription(ids, move |_request_id, _bytes| {
				invoked_in_handler.store(true, std::sync::atomic::Ordering::SeqCst);
				Box::pin(
					async move { Ok(Box::pin(futures::stream::empty()) as SubscriptionStream) },
				)
			});
			let transport = Arc::new(RecordingTransport::default());
			let transport_dyn: Arc<dyn Transport> = transport.clone();
			let frame = make_frame(7, 50, message_type, Vec::new());
			futures::executor::block_on(dispatcher.dispatch(frame, transport_dyn));
			assert!(
				!invoked.load(std::sync::atomic::Ordering::SeqCst),
				"message_type {message_type} must not start a subscription"
			);
			assert!(
				transport.sent().is_empty(),
				"message_type {message_type} must not be answered"
			);
		}
	}

	/// A peer whose table is skewed sends the same unroutable pair on every
	/// frame. Each pair is answered every time, so the peer is never left
	/// waiting, but reported once so it cannot flood the log.
	#[test]
	fn an_unroutable_pair_is_answered_every_time_but_reported_once() {
		let dispatcher = Dispatcher::new(test_spawner());
		let transport = Arc::new(RecordingTransport::default());
		let transport_dyn: Arc<dyn Transport> = transport.clone();
		for _ in 0..3 {
			let frame = make_frame(250, 251, MESSAGE_TYPE_REQUEST, Vec::new());
			futures::executor::block_on(dispatcher.dispatch(frame, transport_dyn.clone()));
		}
		assert_eq!(
			transport.sent().len(),
			3,
			"every unroutable frame still earns its own protocol error"
		);
		assert_eq!(
			dispatcher.reported_violations.lock().expect("violation set").len(),
			1,
			"the pair is recorded once, so it is logged once"
		);
	}

	#[test]
	fn execution_filter_is_bound_to_the_connection() {
		let app =
			Dispatcher::for_execution(test_spawner(), crate::platform::ProductExecutionKind::App);
		let widget = Dispatcher::for_execution(
			test_spawner(),
			crate::platform::ProductExecutionKind::Widget,
		);
		let worker = Dispatcher::for_execution(
			test_spawner(),
			crate::platform::ProductExecutionKind::Worker,
		);

		assert!(!app.allows_execution(crate::platform::ProductExecutionKind::Worker));
		assert!(!widget.allows_execution(crate::platform::ProductExecutionKind::Worker));
		assert!(worker.allows_execution(crate::platform::ProductExecutionKind::Worker));
		assert!(widget.allows_execution(crate::platform::ProductExecutionKind::Widget));
		assert!(!app.allows_execution(crate::platform::ProductExecutionKind::Widget));
		assert!(!worker.allows_execution(crate::platform::ProductExecutionKind::Widget));
	}
}
