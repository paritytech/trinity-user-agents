//! Request/response calls from a host into a product execution.

use std::{
	collections::HashMap,
	sync::{Arc, Mutex},
};

use futures::channel::oneshot;
use parity_scale_codec::{Decode, DecodeLimit};

use crate::{
	CallError,
	frame::{
		IdFactory, MESSAGE_TYPE_CANCEL, MESSAGE_TYPE_REQUEST, MESSAGE_TYPE_RESPONSE,
		PROTOCOL_ERROR_KEY, Payload, ProtocolErrorV1, ProtocolMessage, VersionedProtocolError,
	},
	generated::wire_table::MethodIds,
	protocol_error::decode_protocol_error_payload,
	transport::Transport,
};

const REQUEST_ID_PREFIX: &str = "h:r:";
const MAX_RESPONSE_DECODE_DEPTH: u32 = 64;
const CLOSED_REASON: &str = "host-initiated requests are closed";

enum Reply {
	Response(Vec<u8>),
	Unsupported,
	UnknownProtocolError,
}

struct PendingRequest {
	ids: MethodIds,
	sender: oneshot::Sender<Reply>,
}

struct RequestState {
	ids: IdFactory,
	pending: HashMap<String, PendingRequest>,
	closed: bool,
}

/// Tracks one response for each host-initiated request on a connection.
pub struct HostInitiatedRequestManager {
	state: Arc<Mutex<RequestState>>,
}

impl Default for HostInitiatedRequestManager {
	fn default() -> Self {
		Self::new()
	}
}

impl HostInitiatedRequestManager {
	/// Create an open request registry for one product connection.
	pub fn new() -> Self {
		Self {
			state: Arc::new(Mutex::new(RequestState {
				ids: IdFactory::new(REQUEST_ID_PREFIX),
				pending: HashMap::new(),
				closed: false,
			})),
		}
	}

	/// Send an encoded request when polled and await its typed response.
	/// Dropping an outstanding call withdraws it with `Cancel`.
	pub async fn request<Response: Decode, Error: Decode>(
		&self,
		ids: MethodIds,
		payload: Vec<u8>,
		transport: Arc<dyn Transport>,
	) -> Result<Response, CallError<Error>> {
		let (sender, receiver) = oneshot::channel();
		let request_id = {
			let mut state = self.state.lock().expect("host request state mutex poisoned");
			if state.closed {
				return Err(CallError::HostFailure { reason: CLOSED_REASON.into() });
			}
			let request_id = state.ids.next_id();
			state.pending.insert(request_id.clone(), PendingRequest { ids, sender });
			request_id
		};
		let _pending = PendingCall {
			request_id: request_id.clone(),
			ids,
			state: self.state.clone(),
			transport: transport.clone(),
		};
		transport.send(ProtocolMessage {
			request_id,
			payload: Payload {
				trait_id: ids.trait_id,
				method_id: ids.method_id,
				message_type: MESSAGE_TYPE_REQUEST,
				value: payload,
			},
		});
		match receiver.await {
			Ok(Reply::Response(bytes)) => {
				let mut input = &bytes[..];
				match Result::<Response, CallError<Error>>::decode_with_depth_limit(
					MAX_RESPONSE_DECODE_DEPTH,
					&mut input,
				) {
					Ok(result) if input.is_empty() => result,
					Ok(_) | Err(_) => Err(CallError::MalformedFrame {
						reason: "host-initiated response did not decode".into(),
					}),
				}
			},
			Ok(Reply::Unsupported) => Err(CallError::Unsupported),
			Ok(Reply::UnknownProtocolError) => Err(CallError::HostFailure {
				reason: "product reported a protocol error this build cannot read".into(),
			}),
			Err(_) => Err(CallError::HostFailure { reason: CLOSED_REASON.into() }),
		}
	}

	/// Consume host request replies, including stale `h:r:` frames.
	/// Return other frames for subscription or product-request dispatch.
	pub fn handle_message(&self, message: ProtocolMessage) -> Option<ProtocolMessage> {
		if !message.request_id.starts_with(REQUEST_ID_PREFIX) {
			return Some(message);
		}
		if message.payload.message_type != MESSAGE_TYPE_RESPONSE {
			return None;
		}
		let mut state = self.state.lock().expect("host request state mutex poisoned");
		let pending = state.pending.get(&message.request_id)?;
		let pair = (message.payload.trait_id, message.payload.method_id);
		let expected = (pending.ids.trait_id, pending.ids.method_id);
		let reply = if pair == PROTOCOL_ERROR_KEY {
			match decode_protocol_error_payload(&message.payload.value) {
				Ok(Some(VersionedProtocolError::V1(
					ProtocolErrorV1::UnsupportedMessage { trait_id, method_id } |
					ProtocolErrorV1::UnsupportedVersion { trait_id, method_id, .. },
				))) if (trait_id, method_id) == expected => Reply::Unsupported,
				Ok(None) => Reply::UnknownProtocolError,
				_ => return None,
			}
		} else if pair == expected {
			Reply::Response(message.payload.value)
		} else {
			return None;
		};
		let pending = state.pending.remove(&message.request_id)?;
		drop(state);
		let _ = pending.sender.send(reply);
		None
	}

	/// Finish pending calls and refuse new ones without sending frames.
	pub fn close(&self) {
		let pending = {
			let mut state = self.state.lock().expect("host request state mutex poisoned");
			state.closed = true;
			core::mem::take(&mut state.pending)
		};
		drop(pending);
	}
}

struct PendingCall {
	request_id: String,
	ids: MethodIds,
	state: Arc<Mutex<RequestState>>,
	transport: Arc<dyn Transport>,
}

impl Drop for PendingCall {
	fn drop(&mut self) {
		let pending = self
			.state
			.lock()
			.expect("host request state mutex poisoned")
			.pending
			.remove(&self.request_id);
		if pending.is_some() {
			self.transport.send(ProtocolMessage {
				request_id: self.request_id.clone(),
				payload: Payload {
					trait_id: self.ids.trait_id,
					method_id: self.ids.method_id,
					message_type: MESSAGE_TYPE_CANCEL,
					value: Vec::new(),
				},
			});
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::frame::{MESSAGE_TYPE_INTERRUPT, PROTOCOL_ERROR_METHOD_ID, PROTOCOL_ERROR_TRAIT_ID};
	use core::future::Future;
	use futures::{FutureExt, executor::block_on, task::ArcWake};
	use parity_scale_codec::Encode;
	use std::sync::atomic::{AtomicBool, Ordering};

	#[derive(Default)]
	struct RecordingTransport {
		frames: Mutex<Vec<ProtocolMessage>>,
	}

	impl Transport for RecordingTransport {
		fn send(&self, message: ProtocolMessage) {
			self.frames.lock().unwrap().push(message);
		}

		fn on_message(
			&self,
			_handler: Box<dyn Fn(ProtocolMessage) + Send + Sync>,
		) -> Box<dyn FnOnce()> {
			Box::new(|| {})
		}
	}

	fn ids() -> MethodIds {
		MethodIds { trait_id: 195, method_id: 14 }
	}

	fn frame(request_id: &str, message_type: u8, value: Vec<u8>) -> ProtocolMessage {
		ProtocolMessage {
			request_id: request_id.into(),
			payload: Payload {
				trait_id: ids().trait_id,
				method_id: ids().method_id,
				message_type,
				value,
			},
		}
	}

	#[test]
	fn a_reply_keeps_the_full_typed_outcome_and_completes_once() {
		let manager = HostInitiatedRequestManager::new();
		let transport = Arc::new(RecordingTransport::default());
		let outcomes = [
			Ok(42_u32),
			Err(CallError::Domain("declined".to_string())),
			Err(CallError::Denied),
			Err(CallError::Unsupported),
			Err(CallError::MalformedFrame { reason: "bad request".into() }),
			Err(CallError::HostFailure { reason: "unavailable".into() }),
			Err(CallError::Cancelled),
		];
		let mut received = Vec::new();
		for outcome in &outcomes {
			let mut request =
				Box::pin(manager.request::<u32, String>(ids(), vec![9], transport.clone()));
			assert_eq!(request.as_mut().now_or_never(), None);
			let sent = transport.frames.lock().unwrap().last().unwrap().clone();
			let response = frame(&sent.request_id, MESSAGE_TYPE_RESPONSE, outcome.encode());
			manager.handle_message(response.clone());
			manager.handle_message(response);
			received.push(block_on(request));
		}
		assert_eq!(
			(received, transport.frames.lock().unwrap().len()),
			(outcomes.to_vec(), outcomes.len()),
		);
	}

	#[test]
	fn a_request_is_registered_before_a_reentrant_transport_answers() {
		struct ImmediateReply(Arc<HostInitiatedRequestManager>);
		impl Transport for ImmediateReply {
			fn send(&self, message: ProtocolMessage) {
				self.0.handle_message(frame(
					&message.request_id,
					MESSAGE_TYPE_RESPONSE,
					Ok::<u32, CallError<String>>(7).encode(),
				));
			}
			fn on_message(
				&self,
				_handler: Box<dyn Fn(ProtocolMessage) + Send + Sync>,
			) -> Box<dyn FnOnce()> {
				Box::new(|| {})
			}
		}
		let manager = Arc::new(HostInitiatedRequestManager::new());
		let transport = Arc::new(ImmediateReply(manager.clone()));
		assert_eq!(block_on(manager.request::<u32, String>(ids(), vec![], transport)), Ok(7));
	}

	#[test]
	fn dropping_an_outstanding_request_withdraws_only_that_call() {
		let manager = HostInitiatedRequestManager::new();
		let transport = Arc::new(RecordingTransport::default());
		let mut withdrawn =
			Box::pin(manager.request::<u32, String>(ids(), vec![5], transport.clone()));
		let mut remaining =
			Box::pin(manager.request::<u32, String>(ids(), vec![6], transport.clone()));
		assert_eq!(withdrawn.as_mut().now_or_never(), None);
		assert_eq!(remaining.as_mut().now_or_never(), None);
		drop(withdrawn);
		manager.handle_message(frame(
			"h:r:1",
			MESSAGE_TYPE_RESPONSE,
			Ok::<u32, CallError<String>>(1).encode(),
		));
		manager.handle_message(frame(
			"h:r:2",
			MESSAGE_TYPE_RESPONSE,
			Ok::<u32, CallError<String>>(2).encode(),
		));
		assert_eq!(
			(block_on(remaining), transport.frames.lock().unwrap().clone()),
			(
				Ok(2),
				vec![
					frame("h:r:1", MESSAGE_TYPE_REQUEST, vec![5]),
					frame("h:r:2", MESSAGE_TYPE_REQUEST, vec![6]),
					frame("h:r:1", MESSAGE_TYPE_CANCEL, vec![]),
				]
			),
		);
	}

	#[test]
	fn dropping_an_unpolled_or_answered_request_sends_no_cancel() {
		let manager = HostInitiatedRequestManager::new();
		let transport = Arc::new(RecordingTransport::default());
		drop(manager.request::<u32, String>(ids(), vec![], transport.clone()));
		let mut request =
			Box::pin(manager.request::<u32, String>(ids(), vec![3], transport.clone()));
		assert_eq!(request.as_mut().now_or_never(), None);
		manager.handle_message(frame(
			"h:r:1",
			MESSAGE_TYPE_RESPONSE,
			Ok::<u32, CallError<String>>(7).encode(),
		));
		drop(request);
		assert_eq!(
			*transport.frames.lock().unwrap(),
			vec![frame("h:r:1", MESSAGE_TYPE_REQUEST, vec![3])]
		);
	}

	#[test]
	fn wrong_method_or_leg_does_not_take_a_pending_reply() {
		let manager = HostInitiatedRequestManager::new();
		let transport = Arc::new(RecordingTransport::default());
		let mut request = Box::pin(manager.request::<u32, String>(ids(), vec![], transport));
		assert_eq!(request.as_mut().now_or_never(), None);
		let reply = frame("h:r:1", MESSAGE_TYPE_RESPONSE, Ok::<u32, CallError<String>>(7).encode());
		let mut wrong_method = reply.clone();
		wrong_method.payload.method_id += 1;
		manager.handle_message(wrong_method);
		let mut wrong_leg = reply.clone();
		wrong_leg.payload.message_type = MESSAGE_TYPE_INTERRUPT;
		manager.handle_message(wrong_leg);
		assert_eq!(request.as_mut().now_or_never(), None);
		manager.handle_message(reply);
		assert_eq!(block_on(request), Ok(7));
	}

	#[test]
	fn closing_finishes_pending_requests_and_rejects_new_ones_without_sending() {
		let manager = HostInitiatedRequestManager::new();
		let transport = Arc::new(RecordingTransport::default());
		let mut first = Box::pin(manager.request::<u32, String>(ids(), vec![], transport.clone()));
		let mut second = Box::pin(manager.request::<u32, String>(ids(), vec![], transport.clone()));
		assert_eq!(first.as_mut().now_or_never(), None);
		assert_eq!(second.as_mut().now_or_never(), None);
		manager.close();
		manager.close();
		let third = manager.request::<u32, String>(ids(), vec![], transport.clone());
		let closed =
			Err(CallError::HostFailure { reason: "host-initiated requests are closed".into() });
		assert_eq!(
			(
				[block_on(first), block_on(second), block_on(third)],
				transport.frames.lock().unwrap().len()
			),
			([closed.clone(), closed.clone(), closed], 2),
		);
	}

	#[test]
	fn unsupported_replies_settle_only_the_correlated_method() {
		let errors = |method_id| {
			[
				ProtocolErrorV1::UnsupportedMessage { trait_id: ids().trait_id, method_id },
				ProtocolErrorV1::UnsupportedVersion {
					trait_id: ids().trait_id,
					method_id,
					newest_version: 2,
				},
			]
		};
		let mut outcomes = Vec::new();
		for (unrelated, correlated) in
			errors(ids().method_id + 1).into_iter().zip(errors(ids().method_id))
		{
			let manager = HostInitiatedRequestManager::new();
			let transport = Arc::new(RecordingTransport::default());
			let mut request = Box::pin(manager.request::<u32, String>(ids(), vec![], transport));
			assert_eq!(request.as_mut().now_or_never(), None);
			let mut reply = frame("h:r:1", MESSAGE_TYPE_RESPONSE, vec![]);
			reply.payload.trait_id = PROTOCOL_ERROR_TRAIT_ID;
			reply.payload.method_id = PROTOCOL_ERROR_METHOD_ID;
			reply.payload.value = VersionedProtocolError::V1(unrelated).encode();
			manager.handle_message(reply.clone());
			assert_eq!(request.as_mut().now_or_never(), None);
			reply.payload.value = VersionedProtocolError::V1(correlated).encode();
			manager.handle_message(reply);
			outcomes.push(block_on(request));
		}
		assert_eq!(outcomes, vec![Err(CallError::Unsupported); 2]);
	}

	#[test]
	fn unknown_protocol_errors_finish_the_call_instead_of_leaving_it_pending() {
		let manager = HostInitiatedRequestManager::new();
		let transport = Arc::new(RecordingTransport::default());
		let mut request = Box::pin(manager.request::<u32, String>(ids(), vec![], transport));
		assert_eq!(request.as_mut().now_or_never(), None);
		let mut reply = frame("h:r:1", MESSAGE_TYPE_RESPONSE, vec![255]);
		reply.payload.trait_id = PROTOCOL_ERROR_TRAIT_ID;
		reply.payload.method_id = PROTOCOL_ERROR_METHOD_ID;
		manager.handle_message(reply);
		assert_eq!(
			block_on(request),
			Err(CallError::HostFailure {
				reason: "product reported a protocol error this build cannot read".into(),
			})
		);
	}

	#[test]
	fn malformed_or_trailing_reply_bytes_fail_only_their_call() {
		let manager = HostInitiatedRequestManager::new();
		let transport = Arc::new(RecordingTransport::default());
		let valid = Ok::<u32, CallError<String>>(7).encode();
		let mut trailing = valid.clone();
		trailing.push(0);
		let mut outcomes = Vec::new();
		for payload in [vec![0], trailing, valid] {
			let mut request =
				Box::pin(manager.request::<u32, String>(ids(), vec![], transport.clone()));
			assert_eq!(request.as_mut().now_or_never(), None);
			let sent = transport.frames.lock().unwrap().last().unwrap().clone();
			manager.handle_message(frame(&sent.request_id, MESSAGE_TYPE_RESPONSE, payload));
			outcomes.push(block_on(request));
		}
		let malformed = Err(CallError::MalformedFrame {
			reason: "host-initiated response did not decode".into(),
		});
		assert_eq!(outcomes, vec![malformed.clone(), malformed, Ok(7)]);
	}

	#[test]
	fn recursive_reply_payloads_have_a_depth_limit() {
		#[derive(Debug, PartialEq, Eq, Decode, Encode)]
		enum Nested {
			Leaf,
			Branch(Box<Nested>),
		}
		let manager = HostInitiatedRequestManager::new();
		let transport = Arc::new(RecordingTransport::default());
		let mut request =
			Box::pin(manager.request::<Nested, String>(ids(), vec![], transport.clone()));
		assert_eq!(request.as_mut().now_or_never(), None);
		let mut payload = vec![0];
		payload.extend(vec![1; 256]);
		payload.push(0);
		manager.handle_message(frame("h:r:1", MESSAGE_TYPE_RESPONSE, payload));
		let rejected = block_on(request);
		let mut healthy = Box::pin(manager.request::<Nested, String>(ids(), vec![], transport));
		assert_eq!(healthy.as_mut().now_or_never(), None);
		manager.handle_message(frame(
			"h:r:2",
			MESSAGE_TYPE_RESPONSE,
			Ok::<Nested, CallError<String>>(Nested::Leaf).encode(),
		));
		assert_eq!(
			(rejected, block_on(healthy)),
			(
				Err(CallError::MalformedFrame {
					reason: "host-initiated response did not decode".into()
				}),
				Ok(Nested::Leaf),
			)
		);
	}

	#[test]
	fn response_and_close_wake_callers_without_holding_the_registry_lock() {
		struct CheckUnlocked {
			manager: Arc<HostInitiatedRequestManager>,
			woken: AtomicBool,
		}
		impl ArcWake for CheckUnlocked {
			fn wake_by_ref(waker: &Arc<Self>) {
				assert!(waker.manager.state.try_lock().is_ok());
				waker.woken.store(true, Ordering::SeqCst);
			}
		}
		for close in [false, true] {
			let manager = Arc::new(HostInitiatedRequestManager::new());
			let transport = Arc::new(RecordingTransport::default());
			let mut request = Box::pin(manager.request::<u32, String>(ids(), vec![], transport));
			let wake =
				Arc::new(CheckUnlocked { manager: manager.clone(), woken: AtomicBool::new(false) });
			let waker = futures::task::waker(wake.clone());
			let mut context = core::task::Context::from_waker(&waker);
			assert!(request.as_mut().poll(&mut context).is_pending());
			if close {
				manager.close();
			} else {
				manager.handle_message(frame(
					"h:r:1",
					MESSAGE_TYPE_RESPONSE,
					Ok::<u32, CallError<String>>(7).encode(),
				));
			}
			assert!(wake.woken.load(Ordering::SeqCst));
		}
	}

	#[test]
	fn unrelated_frames_pass_through_and_stale_request_replies_are_consumed() {
		let manager = HostInitiatedRequestManager::new();
		let product = frame("p:1", MESSAGE_TYPE_REQUEST, vec![3]);
		let subscription = frame("h:1", MESSAGE_TYPE_RESPONSE, vec![3]);
		assert_eq!(
			[
				manager.handle_message(product.clone()),
				manager.handle_message(subscription.clone()),
				manager.handle_message(frame("h:r:0", MESSAGE_TYPE_RESPONSE, vec![3]))
			],
			[Some(product), Some(subscription), None],
		);
	}
}
