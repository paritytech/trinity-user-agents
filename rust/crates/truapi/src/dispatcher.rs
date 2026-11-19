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

use futures::future::{self, BoxFuture};
use parity_scale_codec::{Decode, DecodeAll, Encode};
use tracing::{error, instrument};
use truapi::{CancellationToken, versioned::Versioned};

use crate::{
	Spawner,
	frame::{
		MESSAGE_TYPE_CANCEL, MESSAGE_TYPE_INTERRUPT, MESSAGE_TYPE_REQUEST, MESSAGE_TYPE_RESPONSE,
		MESSAGE_TYPE_START, MESSAGE_TYPE_STOP, PROTOCOL_ERROR_KEY, PROTOCOL_ERROR_METHOD_ID,
		PROTOCOL_ERROR_TRAIT_ID, Payload, ProtocolErrorV1, ProtocolMessage, VersionedProtocolError,
		encode_cancelled_response, encode_malformed_frame_error,
	},
	generated::wire_table::MethodIds,
	subscription::{SubscriptionManager, SubscriptionStream},
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
	newest_version: u8,
	handler: RequestHandler,
}

/// A registered subscription handler plus the discriminants its frames carry.
pub struct SubscriptionEntry {
	ids: MethodIds,
	newest_version: u8,
	handler: SubscriptionHandler,
}

/// Answer `request_id` on the reserved protocol-error address.
fn send_protocol_error(
	transport: &dyn Transport,
	request_id: String,
	error: VersionedProtocolError,
) {
	transport.send(ProtocolMessage {
		request_id,
		payload: Payload {
			trait_id: PROTOCOL_ERROR_TRAIT_ID,
			method_id: PROTOCOL_ERROR_METHOD_ID,
			message_type: MESSAGE_TYPE_RESPONSE,
			value: error.encode(),
		},
	});
}

/// Routes incoming protocol messages to registered handlers, keyed on the
/// numeric `(trait, method)` wire discriminant pair.
pub struct Dispatcher {
	by_request: HashMap<(u8, u8), RequestEntry>,
	by_start: HashMap<(u8, u8), SubscriptionEntry>,
	subscriptions: SubscriptionManager,
	/// Trusted executable kind bound to this connection; `None` leaves the
	/// surface unrestricted for direct dispatcher embeddings.
	execution: Option<crate::platform::ExecutableKind>,
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
	pub fn for_execution(spawner: Spawner, execution: crate::platform::ExecutableKind) -> Self {
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

	/// Returns the error to send back when `payload` is in a version newer than
	/// `newest_version`. The first byte of a request is its version, starting
	/// at 0 for `V1`.
	fn unsupported_version(
		&self,
		ids: MethodIds,
		newest_version: u8,
		payload: &[u8],
	) -> Option<VersionedProtocolError> {
		let index = *payload.first()?;
		if index < newest_version {
			return None;
		}
		// The first byte is all a refusal reads, so a corrupt payload is
		// refused too. An implausible version tells the two apart.
		self.report_once((ids.trait_id, ids.method_id), || {
			error!(
				trait_id = ids.trait_id,
				method_id = ids.method_id,
				version = u16::from(index) + 1,
				newest_version,
				"refusing a request in a version this host does not know"
			);
		});
		Some(VersionedProtocolError::V1(ProtocolErrorV1::UnsupportedVersion {
			trait_id: ids.trait_id,
			method_id: ids.method_id,
			newest_version,
		}))
	}

	/// Return whether this connection may access a service execution kind.
	pub fn allows_execution(&self, required: crate::platform::ExecutableKind) -> bool {
		self.execution.is_none_or(|actual| actual == required)
	}

	/// Register a request-response handler, keyed on
	/// `(ids.trait_id, ids.method_id)`. Returns the previously registered
	/// entry if any; callers (the generated `dispatcher::register`) should
	/// treat `Some` as a programming error since each discriminant pair must
	/// own exactly one handler.
	///
	/// The handler takes the decoded `Request`. Without running it, the
	/// dispatcher answers a version newer than `Request::LATEST` with an
	/// `UnsupportedVersion` protocol error, and bytes that do not decode with
	/// `MalformedFrame`.
	pub fn on_request<Request, F>(&mut self, ids: MethodIds, handler: F) -> Option<RequestEntry>
	where
		Request: Versioned + Decode + 'static,
		F: Fn(String, Request, CancellationToken) -> BoxFuture<'static, Vec<u8>>
			+ Send
			+ Sync
			+ 'static,
	{
		let decode_then_handle: RequestHandler =
			Arc::new(move |request_id, bytes, cancel| match Request::decode_all(&mut &bytes[..]) {
				Ok(request) => handler(request_id, request, cancel),
				Err(error) => {
					let malformed = encode_malformed_frame_error(error.to_string());
					Box::pin(future::ready(malformed))
				},
			});
		self.by_request.insert(
			(ids.trait_id, ids.method_id),
			RequestEntry { ids, newest_version: Request::LATEST, handler: decode_then_handle },
		)
	}

	/// Register a subscription handler, keyed on
	/// `(ids.trait_id, ids.method_id)`. A `Stop` frame arrives at this same
	/// address — [`dispatch`](Self::dispatch) checks its `message_type` and
	/// routes it to [`SubscriptionManager::handle_stop`] directly, without
	/// invoking this handler. Returns the previously registered entry if any.
	///
	/// The handler takes the decoded `Start` payload, which the dispatcher
	/// checks as for [`on_request`](Self::on_request), ending the stream with a
	/// `MalformedFrame` interrupt when it does not decode.
	pub fn on_subscription<Request, F>(
		&mut self,
		ids: MethodIds,
		handler: F,
	) -> Option<SubscriptionEntry>
	where
		Request: Versioned + Decode + 'static,
		F: Fn(String, Request) -> BoxFuture<'static, Result<SubscriptionStream, Vec<u8>>>
			+ Send
			+ Sync
			+ 'static,
	{
		let decode_then_handle: SubscriptionHandler =
			Arc::new(move |request_id, bytes| match Request::decode_all(&mut &bytes[..]) {
				Ok(request) => handler(request_id, request),
				Err(error) => {
					let malformed = encode_malformed_frame_error(error.to_string());
					Box::pin(future::ready(Err(malformed)))
				},
			});
		self.by_start.insert(
			(ids.trait_id, ids.method_id),
			SubscriptionEntry { ids, newest_version: Request::LATEST, handler: decode_then_handle },
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
					let outcome = match self.unsupported_version(
						entry.ids,
						entry.newest_version,
						&message.payload.value,
					) {
						Some(refusal) => Err(refusal),
						None => {
							Ok((entry.handler)(request_id.clone(), message.payload.value, cancel)
								.await)
						},
					};
					// The single terminal answer is how a peer learns its cancel
					// was heard, so a `Cancel` wins over a refusal and over
					// whatever the handler returned.
					match (self.release_request(&request_id), outcome) {
						(true, _) => encode_cancelled_response(),
						(false, Ok(value)) => value,
						(false, Err(refusal)) => {
							send_protocol_error(transport.as_ref(), request_id, refusal);
							return;
						},
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
			// Refused the way a `Start` that fails to decode is: the reservation
			// has already replaced any live subscription under this id.
			if let Some(error) =
				self.unsupported_version(entry.ids, entry.newest_version, &message.payload.value)
			{
				self.subscriptions.cancel_reservation(token);
				send_protocol_error(transport.as_ref(), request_id, error);
				return;
			}
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
			send_protocol_error(
				transport.as_ref(),
				message.request_id,
				VersionedProtocolError::V1(ProtocolErrorV1::UnsupportedMessage {
					trait_id,
					method_id,
				}),
			);
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
mod tests;
