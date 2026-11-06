use super::*;
use crate::frame::{MESSAGE_TYPE_RECEIVE, MESSAGE_TYPE_REQUEST};
use std::sync::Mutex;
use truapi::{
	v02,
	versioned::{
		local_storage::HostLocalStorageReadRequest, system::HostInfoRequest,
		theme::HostThemeSubscribeRequest,
	},
};

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
fn frame_for(request_id: &str, trait_id: u8, method_id: u8, message_type: u8) -> ProtocolMessage {
	ProtocolMessage {
		request_id: request_id.into(),
		payload: Payload { trait_id, method_id, message_type, value: HostInfoRequest::V1.encode() },
	}
}

/// The `Response` `request_id` is answered with.
fn response_for(request_id: &str, trait_id: u8, method_id: u8, value: Vec<u8>) -> ProtocolMessage {
	ProtocolMessage {
		request_id: request_id.into(),
		payload: Payload { trait_id, method_id, message_type: MESSAGE_TYPE_RESPONSE, value },
	}
}

/// The `Response` a request at `(trait_id, method_id)` answers with.
fn expect_response(trait_id: u8, method_id: u8, value: Vec<u8>) -> Vec<ProtocolMessage> {
	vec![response_for("p:1", trait_id, method_id, value)]
}

fn make_frame(trait_id: u8, method_id: u8, message_type: u8, value: Vec<u8>) -> ProtocolMessage {
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
	dispatcher.on_request(ids, |_request_id, _request: HostInfoRequest, _cancel| {
		// A method's failure is a `Result` inside the response payload, so
		// an error path still hands back bytes to send.
		Box::pin(async move { vec![9, 8, 7] })
	});
	let transport = Arc::new(RecordingTransport::default());
	let frame = make_frame(7, 200, MESSAGE_TYPE_REQUEST, HostInfoRequest::V1.encode());
	futures::executor::block_on(dispatcher.dispatch(frame, transport.clone()));
	let sent = transport.sent();
	assert_eq!(sent.len(), 1, "exactly one response expected");
	assert_eq!(sent[0].payload.trait_id, 7);
	assert_eq!(sent[0].payload.method_id, 200);
	assert_eq!(sent[0].payload.value, vec![9, 8, 7]);
}

/// A newer product sends a version this host never learned. It is refused
/// as unsupported, naming the newest version the host decodes, before the
/// handler runs, rather than failing to decode as a malformed frame; a
/// version the host knows still reaches the handler.
#[test]
fn a_request_newer_than_the_handler_decodes_is_refused_with_its_newest_version() {
	let mut dispatcher = Dispatcher::new(test_spawner());
	let ids = MethodIds { trait_id: 7, method_id: 200 };
	let runs = Arc::new(std::sync::atomic::AtomicUsize::new(0));
	let runs_in_handler = runs.clone();
	dispatcher.on_request(
		ids,
		move |_request_id, _request: HostLocalStorageReadRequest, _cancel| {
			runs_in_handler.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
			Box::pin(async move { vec![1] })
		},
	);
	let transport = Arc::new(RecordingTransport::default());
	let newest = HostLocalStorageReadRequest::V2(v02::HostLocalStorageReadRequest {
		product: None,
		key: "k".into(),
	})
	.encode();
	// Variant index 2 would be V3, one past the newest this handler decodes.
	for (request_id, payload) in [("p:1", vec![2]), ("p:2", newest)] {
		let mut frame = make_frame(7, 200, MESSAGE_TYPE_REQUEST, payload);
		frame.request_id = request_id.into();
		futures::executor::block_on(dispatcher.dispatch(frame, transport.clone()));
	}

	assert_eq!(
		transport.sent(),
		vec![
			ProtocolMessage {
				request_id: "p:1".into(),
				payload: Payload {
					trait_id: PROTOCOL_ERROR_TRAIT_ID,
					method_id: PROTOCOL_ERROR_METHOD_ID,
					message_type: MESSAGE_TYPE_RESPONSE,
					value: VersionedProtocolError::V1(ProtocolErrorV1::UnsupportedVersion {
						trait_id: 7,
						method_id: 200,
						newest_version: 2,
					})
					.encode(),
				},
			},
			ProtocolMessage {
				request_id: "p:2".into(),
				payload: Payload {
					trait_id: 7,
					method_id: 200,
					message_type: MESSAGE_TYPE_RESPONSE,
					value: vec![1],
				},
			},
		]
	);
	assert_eq!(runs.load(std::sync::atomic::Ordering::SeqCst), 1);
	assert_eq!(
		*dispatcher.reported_violations.lock().expect("violation set"),
		HashSet::from([(7, 200)]),
		"a refusal is logged for its method, like any wire disagreement"
	);
}

/// The dispatcher decodes a request before its handler sees it, so bytes
/// that do not decode in a version the handler knows are answered as
/// malformed and never reach it.
#[test]
fn a_request_that_does_not_decode_is_answered_malformed_without_running_the_handler() {
	let mut dispatcher = Dispatcher::new(test_spawner());
	let ids = MethodIds { trait_id: 7, method_id: 201 };
	let runs = Arc::new(std::sync::atomic::AtomicUsize::new(0));
	let runs_in_handler = runs.clone();
	dispatcher.on_request(
		ids,
		move |_request_id, _request: HostLocalStorageReadRequest, _cancel| {
			runs_in_handler.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
			Box::pin(async move { vec![1] })
		},
	);
	let transport = Arc::new(RecordingTransport::default());
	// V2, then an invalid `product` option tag.
	let payload = vec![1, 0xff];
	let reason = HostLocalStorageReadRequest::decode_all(&mut &payload[..])
		.expect_err("the payload must not decode")
		.to_string();
	futures::executor::block_on(
		dispatcher.dispatch(make_frame(7, 201, MESSAGE_TYPE_REQUEST, payload), transport.clone()),
	);

	assert_eq!(transport.sent(), expect_response(7, 201, encode_malformed_frame_error(reason)));
	assert_eq!(runs.load(std::sync::atomic::Ordering::SeqCst), 0);
}

/// A subscription's `Start` carries a version the same way, and a newer one
/// must not start a stream the handler cannot decode.
#[test]
fn a_start_newer_than_the_handler_decodes_is_refused_with_its_newest_version() {
	let mut dispatcher = Dispatcher::new(test_spawner());
	let ids = MethodIds { trait_id: 7, method_id: 50 };
	let invoked = Arc::new(std::sync::atomic::AtomicBool::new(false));
	let invoked_in_handler = invoked.clone();
	dispatcher.on_subscription(ids, move |_request_id, _request: HostThemeSubscribeRequest| {
		invoked_in_handler.store(true, std::sync::atomic::Ordering::SeqCst);
		Box::pin(async move { Ok(Box::pin(futures::stream::empty()) as SubscriptionStream) })
	});
	let transport = Arc::new(RecordingTransport::default());
	let frame = make_frame(7, 50, MESSAGE_TYPE_START, vec![1]);
	futures::executor::block_on(dispatcher.dispatch(frame, transport.clone()));

	assert_eq!(
		transport.sent(),
		vec![ProtocolMessage {
			request_id: "p:1".into(),
			payload: Payload {
				trait_id: PROTOCOL_ERROR_TRAIT_ID,
				method_id: PROTOCOL_ERROR_METHOD_ID,
				message_type: MESSAGE_TYPE_RESPONSE,
				value: VersionedProtocolError::V1(ProtocolErrorV1::UnsupportedVersion {
					trait_id: 7,
					method_id: 50,
					newest_version: 1,
				})
				.encode(),
			},
		}]
	);
	assert!(!invoked.load(std::sync::atomic::Ordering::SeqCst));
	assert!(!dispatcher.subscriptions.holds("p:1"), "a refused start keeps no slot");
}

/// A refusal is an answer like any other, so a `Cancel` that arrived first
/// still wins, and spends its withdrawal.
#[test]
fn an_early_withdrawal_answers_a_request_in_an_unknown_version_as_cancelled() {
	let mut dispatcher = Dispatcher::new(test_spawner());
	let ids = MethodIds { trait_id: 7, method_id: 66 };
	dispatcher.on_request(ids, move |_request_id, _request: HostInfoRequest, _cancel| {
		Box::pin(async move { vec![1] })
	});
	let transport = Arc::new(RecordingTransport::default());
	let dispatch = |message_type: u8| {
		let transport: Arc<dyn Transport> = transport.clone();
		futures::executor::block_on(
			dispatcher.dispatch(make_frame(7, 66, message_type, vec![1]), transport),
		);
	};
	dispatch(MESSAGE_TYPE_CANCEL);
	dispatch(MESSAGE_TYPE_REQUEST);
	dispatch(MESSAGE_TYPE_REQUEST);

	let refusal = ProtocolMessage {
		request_id: "p:1".into(),
		payload: Payload {
			trait_id: PROTOCOL_ERROR_TRAIT_ID,
			method_id: PROTOCOL_ERROR_METHOD_ID,
			message_type: MESSAGE_TYPE_RESPONSE,
			value: VersionedProtocolError::V1(ProtocolErrorV1::UnsupportedVersion {
				trait_id: 7,
				method_id: 66,
				newest_version: 1,
			})
			.encode(),
		},
	};
	assert_eq!(
		transport.sent(),
		[expect_response(7, 66, encode_cancelled_response()), vec![refusal]].concat()
	);
}

/// Two live calls sharing an id leave neither addressable, so a second
/// frame under an id still in flight is dropped whatever its version. A
/// refusal correlated to that id would settle the first call while its
/// handler still runs.
#[test]
fn a_request_in_an_unknown_version_under_an_id_in_flight_is_dropped() {
	let mut dispatcher = Dispatcher::new(test_spawner());
	let ids = MethodIds { trait_id: 7, method_id: 67 };
	dispatcher.on_request(ids, move |_request_id, _request: HostInfoRequest, _cancel| {
		Box::pin(async move { vec![1] })
	});
	let Reservation::Start(first) = dispatcher.reserve_request("p:1") else {
		panic!("the first call registers");
	};
	let transport = Arc::new(RecordingTransport::default());
	futures::executor::block_on(
		dispatcher.dispatch(make_frame(7, 67, MESSAGE_TYPE_REQUEST, vec![1]), transport.clone()),
	);

	assert_eq!(transport.sent(), Vec::<ProtocolMessage>::new());
	assert!(!first.is_cancelled());
}

/// Registering two handlers under the same key must not silently
/// overwrite. The contract chosen here is "loud": `on_request`
/// returns the previous handler, so callers can detect collisions.
#[test]
fn register_request_twice_returns_previous_handler() {
	let mut dispatcher = Dispatcher::new(test_spawner());
	let ids = MethodIds { trait_id: 7, method_id: 200 };
	let prev = dispatcher.on_request(ids, |_request_id, _request: HostInfoRequest, _cancel| {
		Box::pin(async move { Vec::new() })
	});
	assert!(prev.is_none(), "first registration has no predecessor");
	let prev = dispatcher.on_request(ids, |_request_id, _request: HostInfoRequest, _cancel| {
		Box::pin(async move { Vec::new() })
	});
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
	dispatcher.on_subscription(ids, move |_request_id, _request: HostThemeSubscribeRequest| {
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
		dispatcher.on_request(ids, move |_request_id, _request: HostInfoRequest, _cancel| {
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
		assert!(transport.sent().is_empty(), "message_type {message_type} must not be answered");
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
	dispatcher.on_request(ids, move |_request_id, _request: HostInfoRequest, cancel| {
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
		dispatcher.dispatch(
			make_frame(7, 60, MESSAGE_TYPE_REQUEST, HostInfoRequest::V1.encode()),
			request_transport,
		),
		dispatcher.dispatch(make_frame(7, 60, MESSAGE_TYPE_CANCEL, Vec::new()), cancel_transport),
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

/// The dispatcher answers a payload that does not decode before it can name
/// the method's payload types, so the bytes have to decode as any method's
/// response leg, and as a subscription's interrupt leg, whose `Err` arm has
/// the same shape.
#[test]
fn a_malformed_frame_error_decodes_against_real_methods_legs() {
	use truapi::versioned::{
		account::{HostAccountGetError, HostAccountGetResponse},
		system::{HostInfoError, HostInfoResponse},
		theme::HostThemeSubscribeError,
	};

	let reason = "bad payload".to_string();
	let bytes = encode_malformed_frame_error(reason.clone());
	let account: Result<HostAccountGetResponse, truapi::CallError<HostAccountGetError>> =
		DecodeAll::decode_all(&mut &bytes[..]).expect("decodes as a response leg");
	let info: Result<HostInfoResponse, truapi::CallError<HostInfoError>> =
		DecodeAll::decode_all(&mut &bytes[..]).expect("decodes as a response leg");
	let interrupt: Result<(), truapi::CallError<HostThemeSubscribeError>> =
		DecodeAll::decode_all(&mut &bytes[..]).expect("decodes as an interrupt leg");

	assert_eq!(
		(account, info, interrupt),
		(
			Err(truapi::CallError::MalformedFrame { reason: reason.clone() }),
			Err(truapi::CallError::MalformedFrame { reason: reason.clone() }),
			Err(truapi::CallError::MalformedFrame { reason }),
		)
	);
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
	dispatcher.on_request(ids, move |_request_id, _request: HostInfoRequest, _cancel| {
		Box::pin(async move { vec![9, 9] })
	});
	let transport = Arc::new(RecordingTransport::default());

	let transport_dyn: Arc<dyn Transport> = transport.clone();
	futures::executor::block_on(dispatcher.dispatch(
		make_frame(7, 62, MESSAGE_TYPE_REQUEST, HostInfoRequest::V1.encode()),
		transport_dyn,
	));

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
	dispatcher.on_request(ids, move |_request_id, _request: HostInfoRequest, cancel| {
		Box::pin(async move {
			cancel.cancel();
			vec![9, 9]
		})
	});
	let transport = Arc::new(RecordingTransport::default());
	let transport_dyn: Arc<dyn Transport> = transport.clone();
	futures::executor::block_on(dispatcher.dispatch(
		make_frame(7, 63, MESSAGE_TYPE_REQUEST, HostInfoRequest::V1.encode()),
		transport_dyn,
	));
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
	dispatcher.on_request(ids, move |_request_id, _request: HostInfoRequest, _cancel| {
		handler_invocations.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
		Box::pin(async move { vec![1, 2, 3] })
	});
	let transport = Arc::new(RecordingTransport::default());

	let cancel_transport: Arc<dyn Transport> = transport.clone();
	futures::executor::block_on(
		dispatcher.dispatch(make_frame(7, 64, MESSAGE_TYPE_CANCEL, Vec::new()), cancel_transport),
	);
	let request_transport: Arc<dyn Transport> = transport.clone();
	futures::executor::block_on(dispatcher.dispatch(
		make_frame(7, 64, MESSAGE_TYPE_REQUEST, HostInfoRequest::V1.encode()),
		request_transport,
	));

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
	dispatcher.on_request(ids, move |_request_id, _request: HostInfoRequest, _cancel| {
		handler_invocations.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
		Box::pin(async move { vec![4, 5, 6] })
	});
	let transport = Arc::new(RecordingTransport::default());

	let dispatch = |message_type: u8| {
		let transport: Arc<dyn Transport> = transport.clone();
		futures::executor::block_on(
			dispatcher
				.dispatch(make_frame(7, 65, message_type, HostInfoRequest::V1.encode()), transport),
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
	dispatcher.on_request(ids, move |_request_id, _request: HostInfoRequest, cancel| {
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
		dispatcher
			.dispatch(make_frame(7, 66, MESSAGE_TYPE_REQUEST, HostInfoRequest::V1.encode()), first),
		dispatcher.dispatch(
			make_frame(7, 66, MESSAGE_TYPE_REQUEST, HostInfoRequest::V1.encode()),
			duplicate,
		),
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
	dispatcher.on_request(ids, move |_request_id, _request: HostInfoRequest, _cancel| {
		Box::pin(async move { vec![7] })
	});
	let transport = Arc::new(RecordingTransport::default());

	// One more than the queue holds, so `p:0` is pushed back out.
	for n in 0..=MAX_EARLY_WITHDRAWALS {
		let cancel: Arc<dyn Transport> = transport.clone();
		futures::executor::block_on(
			dispatcher.dispatch(frame_for(&format!("p:{n}"), 7, 65, MESSAGE_TYPE_CANCEL), cancel),
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
			response_for(&format!("p:{MAX_EARLY_WITHDRAWALS}"), 7, 65, encode_cancelled_response()),
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
	dispatcher.on_request(ids, move |_request_id, _request: HostInfoRequest, _cancel| {
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
	dispatcher.on_request(ids, move |_request_id, _request: HostInfoRequest, _cancel| {
		Box::pin(async move { vec![4, 5] })
	});
	let transport = Arc::new(RecordingTransport::default());
	let transport_dyn: Arc<dyn Transport> = transport.clone();
	futures::executor::block_on(dispatcher.dispatch(
		make_frame(7, 62, MESSAGE_TYPE_REQUEST, HostInfoRequest::V1.encode()),
		transport_dyn,
	));
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
		dispatcher.on_subscription(ids, move |_request_id, _request: HostThemeSubscribeRequest| {
			invoked_in_handler.store(true, std::sync::atomic::Ordering::SeqCst);
			Box::pin(async move { Ok(Box::pin(futures::stream::empty()) as SubscriptionStream) })
		});
		let transport = Arc::new(RecordingTransport::default());
		let transport_dyn: Arc<dyn Transport> = transport.clone();
		let frame = make_frame(7, 50, message_type, Vec::new());
		futures::executor::block_on(dispatcher.dispatch(frame, transport_dyn));
		assert!(
			!invoked.load(std::sync::atomic::Ordering::SeqCst),
			"message_type {message_type} must not start a subscription"
		);
		assert!(transport.sent().is_empty(), "message_type {message_type} must not be answered");
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
	let app = Dispatcher::for_execution(test_spawner(), crate::platform::ExecutableKind::App);
	let widget = Dispatcher::for_execution(test_spawner(), crate::platform::ExecutableKind::Widget);
	let worker = Dispatcher::for_execution(test_spawner(), crate::platform::ExecutableKind::Worker);

	assert!(!app.allows_execution(crate::platform::ExecutableKind::Worker));
	assert!(!widget.allows_execution(crate::platform::ExecutableKind::Worker));
	assert!(worker.allows_execution(crate::platform::ExecutableKind::Worker));
	assert!(widget.allows_execution(crate::platform::ExecutableKind::Widget));
	assert!(!app.allows_execution(crate::platform::ExecutableKind::Widget));
	assert!(!worker.allows_execution(crate::platform::ExecutableKind::Widget));
}
