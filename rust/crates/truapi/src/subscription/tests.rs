use super::*;
use crate::frame::{
	MESSAGE_TYPE_CANCEL, MESSAGE_TYPE_RESPONSE, PROTOCOL_ERROR_METHOD_ID, PROTOCOL_ERROR_TRAIT_ID,
};
use futures::{FutureExt, stream};
use parity_scale_codec::Encode;
use std::{
	sync::atomic::{AtomicUsize, Ordering},
	task::{Context, Poll},
};
use truapi::v01;

/// Transport that records every frame and notifies waiters when it
/// reaches a target count. Used to wait for the subscription's
/// background thread to drain a known number of frames.
struct RecordingTransport {
	sent: Mutex<Vec<ProtocolMessage>>,
	cvar: std::sync::Condvar,
}

impl RecordingTransport {
	fn new() -> Self {
		Self { sent: Mutex::new(Vec::new()), cvar: std::sync::Condvar::new() }
	}
	fn sent(&self) -> Vec<ProtocolMessage> {
		self.sent.lock().unwrap().clone()
	}
	/// Wait until at least `count` frames have been recorded, or
	/// `timeout` elapses. Returns the number of frames recorded at
	/// wake-up time.
	fn wait_for(&self, count: usize, timeout: std::time::Duration) -> usize {
		let mut guard = self.sent.lock().unwrap();
		let deadline = std::time::Instant::now() + timeout;
		while guard.len() < count {
			let now = std::time::Instant::now();
			if now >= deadline {
				break;
			}
			let (new_guard, _) = self.cvar.wait_timeout(guard, deadline - now).unwrap();
			guard = new_guard;
		}
		guard.len()
	}
}

impl Transport for RecordingTransport {
	fn send(&self, message: ProtocolMessage) {
		self.sent.lock().unwrap().push(message);
		self.cvar.notify_all();
	}
	fn on_message(
		&self,
		_handler: Box<dyn Fn(ProtocolMessage) + Send + Sync>,
	) -> Box<dyn FnOnce()> {
		Box::new(|| {})
	}
}

fn dummy_stream(items: Vec<Vec<u8>>) -> SubscriptionStream {
	Box::pin(stream::iter(items.into_iter().map(SubscriptionOutput::Item)))
}

fn host_ids() -> MethodIds {
	MethodIds { trait_id: 195, method_id: 14 }
}

/// Product frame on [`host_ids`]'s address, tagged `message_type`
/// (`Start`=0, `Receive`=1, `Interrupt`=2, `Stop`=3) with `inner` as that
/// leg's own payload.
fn host_frame(request_id: &str, message_type: u8, inner: Vec<u8>) -> ProtocolMessage {
	ProtocolMessage {
		request_id: request_id.into(),
		payload: Payload {
			trait_id: host_ids().trait_id,
			method_id: host_ids().method_id,
			message_type,
			value: inner,
		},
	}
}

#[test]
fn host_request_returns_the_first_result_and_releases_its_subscription() {
	for expected in [
		Ok(7_u32),
		Err(CallError::Domain("declined".to_string())),
		Err(CallError::Denied),
		Err(CallError::HostFailure { reason: "unavailable".into() }),
	] {
		let manager = HostInitiatedSubscriptionManager::new();
		let transport = Arc::new(RecordingTransport::new());
		let mut request =
			Box::pin(manager.request::<u32, String>(host_ids(), vec![9], transport.clone()));
		assert_eq!(request.as_mut().now_or_never(), None);
		let mut frames = vec![host_frame("h:1", MESSAGE_TYPE_START, vec![9])];
		match &expected {
			Ok(value) => {
				manager.handle_message(host_frame("h:1", MESSAGE_TYPE_RECEIVE, value.encode()));
				manager.handle_message(host_frame("h:1", MESSAGE_TYPE_RECEIVE, 99_u32.encode()));
				frames.push(host_frame("h:1", MESSAGE_TYPE_STOP, vec![]));
			},
			Err(error) => {
				manager.handle_message(host_frame(
					"h:1",
					MESSAGE_TYPE_INTERRUPT,
					Err::<(), _>(error).encode(),
				));
			},
		}
		let result = request.as_mut().now_or_never();
		drop(request);
		assert_eq!(
			(result, manager.state.lock().unwrap().active.len(), transport.sent()),
			(Some(expected), 0, frames),
		);
	}
}

#[test]
fn host_request_requires_an_answer_before_the_subscription_ends() {
	for (close_connection, reason) in [
		(false, "product closed without a response"),
		(true, "the product connection closed while the stream was open"),
	] {
		let manager = HostInitiatedSubscriptionManager::new();
		let transport = Arc::new(RecordingTransport::new());
		let mut request =
			Box::pin(manager.request::<u32, String>(host_ids(), vec![9], transport.clone()));
		assert_eq!(request.as_mut().now_or_never(), None);
		if close_connection {
			manager.close();
		} else {
			manager.handle_message(host_frame(
				"h:1",
				MESSAGE_TYPE_INTERRUPT,
				encode_clean_interrupt(),
			));
		}
		let result = request.as_mut().now_or_never();
		drop(request);
		assert_eq!(
			(result, manager.state.lock().unwrap().active.len(), transport.sent()),
			(
				Some(Err(CallError::HostFailure { reason: reason.into() })),
				0,
				vec![host_frame("h:1", MESSAGE_TYPE_START, vec![9])],
			),
		);
	}
}

#[test]
fn host_request_drop_stops_only_the_polled_call() {
	let manager = HostInitiatedSubscriptionManager::new();
	let transport = Arc::new(RecordingTransport::new());
	drop(manager.request::<u32, String>(host_ids(), vec![0], transport.clone()));
	let mut first =
		Box::pin(manager.request::<u32, String>(host_ids(), vec![1], transport.clone()));
	let mut second =
		Box::pin(manager.request::<u32, String>(host_ids(), vec![2], transport.clone()));
	assert_eq!(first.as_mut().now_or_never(), None);
	assert_eq!(second.as_mut().now_or_never(), None);
	drop(first);
	manager.handle_message(host_frame("h:2", MESSAGE_TYPE_RECEIVE, 7_u32.encode()));
	assert_eq!(
		(second.as_mut().now_or_never(), transport.sent()),
		(
			Some(Ok(7)),
			vec![
				host_frame("h:1", MESSAGE_TYPE_START, vec![1]),
				host_frame("h:2", MESSAGE_TYPE_START, vec![2]),
				host_frame("h:1", MESSAGE_TYPE_STOP, vec![]),
				host_frame("h:2", MESSAGE_TYPE_STOP, vec![]),
			],
		),
	);
}

#[test]
fn host_request_ignores_protocol_errors_on_other_legs() {
	let manager = HostInitiatedSubscriptionManager::new();
	let transport = Arc::new(RecordingTransport::new());
	let mut request = Box::pin(manager.request::<u32, String>(host_ids(), vec![], transport));
	assert_eq!(request.as_mut().now_or_never(), None);
	for message_type in
		[MESSAGE_TYPE_START, MESSAGE_TYPE_INTERRUPT, MESSAGE_TYPE_STOP, MESSAGE_TYPE_CANCEL]
	{
		manager.handle_message(ProtocolMessage {
			request_id: "h:1".into(),
			payload: Payload {
				trait_id: PROTOCOL_ERROR_TRAIT_ID,
				method_id: PROTOCOL_ERROR_METHOD_ID,
				message_type,
				value: VersionedProtocolError::V1(ProtocolErrorV1::UnsupportedMessage {
					trait_id: host_ids().trait_id,
					method_id: host_ids().method_id,
				})
				.encode(),
			},
		});
		assert_eq!(request.as_mut().now_or_never(), None);
	}
	manager.handle_message(host_frame("h:1", MESSAGE_TYPE_RECEIVE, 7_u32.encode()));
	assert_eq!(request.as_mut().now_or_never(), Some(Ok(7)));
}

#[test]
fn host_subscription_delivery_and_close_wake_outside_the_registry_lock() {
	use futures::task::ArcWake;
	use std::sync::atomic::AtomicBool;

	struct CheckUnlocked {
		manager: Arc<HostInitiatedSubscriptionManager>,
		woken: AtomicBool,
		unlocked: AtomicBool,
	}
	impl ArcWake for CheckUnlocked {
		fn wake_by_ref(waker: &Arc<Self>) {
			waker
				.unlocked
				.fetch_and(waker.manager.state.try_lock().is_ok(), Ordering::SeqCst);
			waker.woken.store(true, Ordering::SeqCst);
		}
	}
	enum Ending {
		Frame(ProtocolMessage),
		Close,
		Drop,
	}
	for terminal in [
		Ending::Frame(host_frame("h:1", MESSAGE_TYPE_RECEIVE, 7_u32.encode())),
		Ending::Frame(host_frame("h:1", MESSAGE_TYPE_INTERRUPT, encode_clean_interrupt())),
		Ending::Frame(ProtocolMessage {
			request_id: "h:1".into(),
			payload: Payload {
				trait_id: PROTOCOL_ERROR_TRAIT_ID,
				method_id: PROTOCOL_ERROR_METHOD_ID,
				message_type: MESSAGE_TYPE_RESPONSE,
				value: VersionedProtocolError::V1(ProtocolErrorV1::UnsupportedMessage {
					trait_id: host_ids().trait_id,
					method_id: host_ids().method_id,
				})
				.encode(),
			},
		}),
		Ending::Close,
		Ending::Drop,
	] {
		let manager = Arc::new(HostInitiatedSubscriptionManager::new());
		let mut subscription =
			manager.start::<u32, String>(host_ids(), vec![], Arc::new(RecordingTransport::new()));
		let wake = Arc::new(CheckUnlocked {
			manager: manager.clone(),
			woken: AtomicBool::new(false),
			unlocked: AtomicBool::new(true),
		});
		let waker = futures::task::waker(wake.clone());
		let mut context = Context::from_waker(&waker);
		assert!(Pin::new(&mut subscription).poll_next(&mut context).is_pending());
		match terminal {
			Ending::Frame(message) => {
				manager.handle_message(message);
			},
			Ending::Close => manager.close(),
			Ending::Drop => drop(subscription),
		}
		assert_eq!(
			(wake.woken.load(Ordering::SeqCst), wake.unlocked.load(Ordering::SeqCst)),
			(true, true),
		);
	}
}

#[test]
fn host_initiated_subscription_routes_items_and_sends_stop_on_drop() {
	let transport_typed = Arc::new(RecordingTransport::new());
	let transport: Arc<dyn Transport> = transport_typed.clone();
	let manager = HostInitiatedSubscriptionManager::new();
	let mut subscription =
		manager.start::<u32, v01::GenericError>(host_ids(), vec![0xaa], transport);

	assert_eq!(transport_typed.sent()[0].request_id, "h:1");
	assert_eq!(transport_typed.sent()[0].payload.trait_id, 195);
	assert_eq!(transport_typed.sent()[0].payload.method_id, 14);
	assert_eq!(transport_typed.sent()[0].payload.value, vec![0xaa]);

	assert!(manager.handle_message(host_frame("h:1", 1, 7_u32.encode())).is_none());
	assert_eq!(futures::executor::block_on(subscription.next()), Some(Ok(7)));

	drop(subscription);
	let frames = transport_typed.sent();
	assert_eq!(frames.len(), 2);
	assert_eq!(frames[1].request_id, "h:1");
	assert_eq!(frames[1].payload.trait_id, 195);
	assert_eq!(frames[1].payload.method_id, 14);
	assert_eq!(frames[1].payload.message_type, MESSAGE_TYPE_STOP);
	assert_eq!(frames[1].payload.value, Vec::<u8>::new());
}

#[test]
fn a_deeply_nested_host_item_is_refused_rather_than_exhausting_the_stack() {
	// A recursive product-supplied payload decodes until the thread's stack
	// is gone, and a stack overflow aborts the process rather than failing
	// the call -- no `catch_unwind` and no `panic = "abort"` handling
	// applies to it. The depth bound turns that into an ordinary refusal
	// that ends this subscription alone.
	let transport_typed = Arc::new(RecordingTransport::new());
	let transport: Arc<dyn Transport> = transport_typed.clone();
	let manager = HostInitiatedSubscriptionManager::new();
	let mut nested =
		manager.start::<NestedItem, v01::GenericError>(host_ids(), vec![], transport.clone());
	let mut healthy = manager.start::<NestedItem, v01::GenericError>(host_ids(), vec![], transport);

	// One `Deeper` byte per level, terminated by `Leaf`.
	let mut bomb = vec![0x01; (MAX_SUBSCRIPTION_DECODE_DEPTH as usize) * 4];
	bomb.push(0x00);
	manager.handle_message(host_frame("h:1", 1, bomb));
	assert!(matches!(futures::executor::block_on(nested.next()), Some(Err(_))));

	// A payload inside the bound still arrives, on its own subscription.
	manager.handle_message(host_frame("h:2", 1, NestedItem::Leaf.encode()));
	assert_eq!(futures::executor::block_on(healthy.next()), Some(Ok(NestedItem::Leaf)));
}

#[test]
fn the_depth_bound_lands_where_the_real_render_item_nests() {
	// The fixture above recurses through `Box`, which uses a different
	// `Decode` impl than the `Vec<Self>` the production type recurses
	// through. Pin the boundary on the type actually decoded here.
	fn nested(depth: u32) -> truapi::versioned::renderer::ProductRendererRenderItem {
		let mut node = truapi::v01::RendererNode::Nil;
		for _ in 0..depth {
			node = truapi::v01::RendererNode::Box {
				modifiers: Vec::new(),
				props: truapi::v01::BoxProps { content_alignment: None },
				children: vec![node],
			};
		}
		truapi::versioned::renderer::ProductRendererRenderItem::V1(node)
	}

	let decode = |depth: u32| {
		let bytes = nested(depth).encode();
		let mut input = &bytes[..];
		truapi::versioned::renderer::ProductRendererRenderItem::decode_with_depth_limit(
			MAX_SUBSCRIPTION_DECODE_DEPTH,
			&mut input,
		)
		.is_ok()
	};

	assert!(decode(MAX_SUBSCRIPTION_DECODE_DEPTH), "the limit must be usable");
	assert!(!decode(MAX_SUBSCRIPTION_DECODE_DEPTH + 1), "one past the limit must be refused");
}

/// Stands in for the recursive protocol payloads a product can supply.
#[derive(Debug, PartialEq, Eq, Encode, Decode)]
enum NestedItem {
	Leaf,
	Deeper(Box<NestedItem>),
}

#[test]
fn malformed_host_item_ends_only_its_render_instance() {
	let transport_typed = Arc::new(RecordingTransport::new());
	let transport: Arc<dyn Transport> = transport_typed.clone();
	let manager = HostInitiatedSubscriptionManager::new();
	let mut malformed =
		manager.start::<u32, v01::GenericError>(host_ids(), vec![], transport.clone());
	let mut healthy = manager.start::<u32, v01::GenericError>(host_ids(), vec![], transport);

	manager.handle_message(host_frame("h:1", 1, vec![0xff]));
	manager.handle_message(host_frame("h:2", 1, 9_u32.encode()));

	// A partial tree left on screen as final is the failure this prevents.
	assert_eq!(
		futures::executor::block_on(malformed.next()),
		Some(Err(CallError::MalformedFrame {
			reason: "host-initiated subscription item did not decode".to_string(),
		}))
	);
	assert_eq!(futures::executor::block_on(malformed.next()), None);
	assert_eq!(futures::executor::block_on(healthy.next()), Some(Ok(9)));
	assert_eq!(transport_typed.sent()[2].request_id, "h:1");
	assert_eq!(transport_typed.sent()[2].payload.trait_id, 195);
	assert_eq!(transport_typed.sent()[2].payload.method_id, 14);
}

#[test]
fn product_interrupt_ends_one_host_render_without_echoing_stop() {
	let transport_typed = Arc::new(RecordingTransport::new());
	let transport: Arc<dyn Transport> = transport_typed.clone();
	let manager = HostInitiatedSubscriptionManager::new();
	let mut declined = manager.start::<u32, v01::GenericError>(host_ids(), vec![], transport);

	// A declining product sends `Interrupt(Err(error))`. An
	// `Interrupt(Ok(()))` is a clean completion instead, covered by
	// `a_clean_host_interrupt_completes_the_stream_instead_of_erroring`.
	let interrupt =
		truapi::CallError::<v01::GenericError>::HostFailure { reason: "unavailable".to_string() };
	let declining = Err::<(), _>(interrupt.clone()).encode();
	manager.handle_message(host_frame("h:1", MESSAGE_TYPE_INTERRUPT, declining));

	// The product's own reason reaches the host, rather than a canned one.
	assert_eq!(futures::executor::block_on(declined.next()), Some(Err(interrupt)));
	assert_eq!(futures::executor::block_on(declined.next()), None);
	assert_eq!(transport_typed.sent().len(), 1);
}

/// An interrupt payload this build cannot read must settle the stream
/// with an error. Reading it as a clean end would tell the product its
/// render finished, on a frame that never said so.
#[test]
fn an_unreadable_host_interrupt_ends_the_stream_with_a_malformed_frame() {
	let transport_typed = Arc::new(RecordingTransport::new());
	let transport: Arc<dyn Transport> = transport_typed.clone();
	let manager = HostInitiatedSubscriptionManager::new();
	let mut undecodable =
		manager.start::<u32, v01::GenericError>(host_ids(), vec![], transport.clone());
	let mut trailing = manager.start::<u32, v01::GenericError>(host_ids(), vec![], transport);

	// A `Result` discriminant this build does not know, and a clean end
	// that does not stop where its payload does.
	manager.handle_message(host_frame("h:1", MESSAGE_TYPE_INTERRUPT, vec![0xff]));
	manager.handle_message(host_frame(
		"h:2",
		MESSAGE_TYPE_INTERRUPT,
		[encode_clean_interrupt(), vec![0xff]].concat(),
	));

	let malformed = Some(Err(CallError::MalformedFrame {
		reason: "host-initiated subscription interrupt did not decode".to_string(),
	}));
	assert_eq!(futures::executor::block_on(undecodable.next()), malformed);
	assert_eq!(futures::executor::block_on(undecodable.next()), None);
	assert_eq!(futures::executor::block_on(trailing.next()), malformed);
	assert_eq!(futures::executor::block_on(trailing.next()), None);
	// Only the two Start frames: a settled stream does not echo Stop.
	assert_eq!(transport_typed.sent().len(), 2);
}

#[test]
fn protocol_error_ends_one_host_render_without_echoing_stop() {
	for error in [
		ProtocolErrorV1::UnsupportedMessage {
			trait_id: host_ids().trait_id,
			method_id: host_ids().method_id,
		},
		ProtocolErrorV1::UnsupportedVersion {
			trait_id: host_ids().trait_id,
			method_id: host_ids().method_id,
			newest_version: 1,
		},
	] {
		let transport_typed = Arc::new(RecordingTransport::new());
		let transport: Arc<dyn Transport> = transport_typed.clone();
		let manager = HostInitiatedSubscriptionManager::new();
		let mut unsupported =
			manager.start::<u32, v01::GenericError>(host_ids(), vec![], transport);

		manager.handle_message(ProtocolMessage {
			request_id: "h:1".into(),
			payload: Payload {
				trait_id: PROTOCOL_ERROR_TRAIT_ID,
				method_id: PROTOCOL_ERROR_METHOD_ID,
				message_type: MESSAGE_TYPE_RESPONSE,
				value: VersionedProtocolError::V1(error).encode(),
			},
		});

		assert_eq!(unsupported.next().now_or_never(), Some(Some(Err(CallError::Unsupported))));
		assert_eq!(unsupported.next().now_or_never(), Some(None));
		assert_eq!(transport_typed.sent().len(), 1);
	}
}

#[test]
fn an_unreadable_protocol_error_settles_the_host_render() {
	// `(255, 255)` is correlated to this request id, so a payload from a
	// later build is still this render's terminal even though its shape
	// cannot be attributed to a pair. Leaving the slot alive instead would
	// strand the stream on a peer that has already answered.
	let transport_typed = Arc::new(RecordingTransport::new());
	let transport: Arc<dyn Transport> = transport_typed.clone();
	let manager = HostInitiatedSubscriptionManager::new();
	let mut render = manager.start::<u32, v01::GenericError>(host_ids(), vec![], transport);

	manager.handle_message(ProtocolMessage {
		request_id: "h:1".into(),
		payload: Payload {
			trait_id: PROTOCOL_ERROR_TRAIT_ID,
			method_id: PROTOCOL_ERROR_METHOD_ID,
			message_type: MESSAGE_TYPE_RESPONSE,
			// A protocol-error version this build does not know.
			value: vec![1],
		},
	});

	assert_eq!(
		render.next().now_or_never(),
		Some(Some(Err(CallError::HostFailure {
			reason: "product reported a protocol error this build cannot read".to_string(),
		}))),
		"an unreadable protocol error must settle the stream, not leave it pending"
	);
	assert_eq!(render.next().now_or_never(), Some(None));
	// Only the Start frame: a settled render does not echo Stop.
	assert_eq!(transport_typed.sent().len(), 1);
}

#[test]
fn a_clean_host_interrupt_completes_the_stream_instead_of_erroring() {
	// `Interrupt` carries `Result<(), CallError<E>>`, so `Ok(())` (a
	// single `0` byte) is a clean completion. Reporting it as an error
	// would make every well-behaved product look like it had failed.
	let transport_typed = Arc::new(RecordingTransport::new());
	let transport: Arc<dyn Transport> = transport_typed.clone();
	let manager = HostInitiatedSubscriptionManager::new();
	let mut render = manager.start::<u32, v01::GenericError>(host_ids(), vec![], transport);

	manager.handle_message(host_frame("h:1", MESSAGE_TYPE_INTERRUPT, encode_clean_interrupt()));

	assert_eq!(
		render.next().now_or_never(),
		Some(None),
		"a clean interrupt must end the stream without an error"
	);
	assert_eq!(transport_typed.sent().len(), 1);
}

#[test]
fn unrelated_protocol_errors_do_not_end_a_host_render() {
	let transport_typed = Arc::new(RecordingTransport::new());
	let transport: Arc<dyn Transport> = transport_typed.clone();
	let manager = HostInitiatedSubscriptionManager::new();
	let mut render = manager.start::<u32, v01::GenericError>(host_ids(), vec![], transport);

	for value in [
		// A different (trait, method) pair than this render's own.
		VersionedProtocolError::V1(ProtocolErrorV1::UnsupportedMessage {
			trait_id: host_ids().trait_id,
			method_id: host_ids().method_id + 1,
		})
		.encode(),
		VersionedProtocolError::V1(ProtocolErrorV1::UnsupportedVersion {
			trait_id: host_ids().trait_id,
			method_id: host_ids().method_id + 1,
			newest_version: 1,
		})
		.encode(),
		vec![0, 0],
	] {
		manager.handle_message(ProtocolMessage {
			request_id: "h:1".into(),
			payload: Payload {
				trait_id: PROTOCOL_ERROR_TRAIT_ID,
				method_id: PROTOCOL_ERROR_METHOD_ID,
				message_type: MESSAGE_TYPE_RESPONSE,
				value,
			},
		});
	}
	assert_eq!(render.next().now_or_never(), None);

	manager.handle_message(host_frame("h:1", 1, 7_u32.encode()));
	assert_eq!(futures::executor::block_on(render.next()), Some(Ok(7)));
}

/// A closed connection must read as a failure, not a finish: hosts treat a
/// clean end as a body the product completed and never draw it again, so
/// after a resume the card would stay on screen without a render behind it.
#[test]
fn a_closed_connection_fails_open_streams_instead_of_finishing_them() {
	let transport_typed = Arc::new(RecordingTransport::new());
	let transport: Arc<dyn Transport> = transport_typed.clone();
	let manager = HostInitiatedSubscriptionManager::new();
	let mut render = manager.start::<u32, v01::GenericError>(host_ids(), vec![], transport);

	manager.close();

	assert!(matches!(
		futures::executor::block_on(render.next()),
		Some(Err(CallError::HostFailure { .. }))
	));
	assert_eq!(futures::executor::block_on(render.next()), None);
}

#[test]
fn closing_host_subscriptions_ends_streams_without_sending_stop() {
	let transport_typed = Arc::new(RecordingTransport::new());
	let transport: Arc<dyn Transport> = transport_typed.clone();
	let manager = HostInitiatedSubscriptionManager::new();
	let mut render = manager.start::<u32, v01::GenericError>(host_ids(), vec![], transport.clone());

	manager.close();

	assert!(matches!(
		futures::executor::block_on(render.next()),
		Some(Err(CallError::HostFailure { .. }))
	));
	assert_eq!(transport_typed.sent().len(), 1);

	// A start against a closed manager can never be served, so it
	// interrupts rather than completing as if it had run.
	let mut after_close = manager.start::<u32, v01::GenericError>(host_ids(), vec![], transport);
	assert!(matches!(
		futures::executor::block_on(after_close.next()),
		Some(Err(CallError::HostFailure { .. }))
	));
	assert_eq!(transport_typed.sent().len(), 1);
}

#[test]
fn host_subscription_ids_are_isolated_by_connection_manager() {
	let first_transport_typed = Arc::new(RecordingTransport::new());
	let first_transport: Arc<dyn Transport> = first_transport_typed.clone();
	let first = HostInitiatedSubscriptionManager::new();
	let mut first_render =
		first.start::<u32, v01::GenericError>(host_ids(), vec![], first_transport);

	let second_transport_typed = Arc::new(RecordingTransport::new());
	let second_transport: Arc<dyn Transport> = second_transport_typed.clone();
	let second = HostInitiatedSubscriptionManager::new();
	let mut second_render =
		second.start::<u32, v01::GenericError>(host_ids(), vec![], second_transport);

	assert_eq!(first_transport_typed.sent()[0].request_id, "h:1");
	assert_eq!(second_transport_typed.sent()[0].request_id, "h:1");

	first.handle_message(host_frame("h:1", 1, 7_u32.encode()));
	second.handle_message(host_frame("h:1", 1, 9_u32.encode()));

	assert_eq!(futures::executor::block_on(first_render.next()), Some(Ok(7)));
	assert_eq!(futures::executor::block_on(second_render.next()), Some(Ok(9)));
}

struct PendingDropStream {
	dropped: Arc<std::sync::atomic::AtomicBool>,
}

impl Drop for PendingDropStream {
	fn drop(&mut self) {
		self.dropped.store(true, Ordering::SeqCst);
	}
}

impl futures::Stream for PendingDropStream {
	type Item = SubscriptionOutput;

	fn poll_next(
		self: std::pin::Pin<&mut Self>,
		_cx: &mut Context<'_>,
	) -> Poll<Option<Self::Item>> {
		Poll::Pending
	}
}

/// Register a never-ending stream then immediately stop it. The
/// stream's first poll must observe cancellation and exit without
/// having pushed any frame.
#[test]
fn register_then_stop_emits_no_extra_frames() {
	let transport_typed = Arc::new(RecordingTransport::new());
	let transport_dyn: Arc<dyn Transport> = transport_typed.clone();
	let manager = SubscriptionManager::new(Spawner::for_test());
	let slow_stream: SubscriptionStream = Box::pin(stream::pending());
	manager.register("p:1".to_string(), 7, 99, slow_stream, transport_dyn);
	manager.handle_stop("p:1");
	// Give the worker thread a beat to observe the cancel.
	std::thread::sleep(std::time::Duration::from_millis(50));
	assert!(transport_typed.sent().is_empty(), "stopped subscription must not push any frame");
}

/// A host stream that fails after its first item must deliver that item,
/// then the interrupt carrying the failure, and stop there: the values
/// behind the failure never reach the peer, and no clean terminator
/// follows it.
#[test]
fn a_mid_stream_interrupt_ends_the_subscription_where_it_happens() {
	let transport_typed = Arc::new(RecordingTransport::new());
	let transport_dyn: Arc<dyn Transport> = transport_typed.clone();
	// Drive the worker on the caller's thread so the frame list is
	// complete by the time `register` returns: the assertion below is
	// that nothing follows the interrupt.
	let inline_spawner = Spawner::with_test_scheduler(futures::executor::block_on);
	let manager = SubscriptionManager::new(inline_spawner);
	let failure: CallError<v01::GenericError> =
		CallError::HostFailure { reason: "platform stream failed".to_string() };
	let items = subscription_stream(stream::iter(vec![Ok(1_u32), Err(failure.clone()), Ok(2_u32)]));
	manager.register("p:1".to_string(), 7, 99, items, transport_dyn);

	let frames = transport_typed.sent();
	// The item behind the interrupt is dropped, and a stream that ended
	// with one does not also report a clean end.
	assert_eq!(frames.len(), 2, "expected 1 receive frame + 1 interrupt");
	assert_eq!(frames[0].payload.message_type, MESSAGE_TYPE_RECEIVE);
	assert_eq!(frames[0].payload.value, 1_u32.encode());
	assert_eq!(frames[1].payload.message_type, MESSAGE_TYPE_INTERRUPT);
	assert_eq!(frames[1].payload.value, subscription_interrupt(failure));
}

/// A stream that yields 2 items then ends naturally must produce 2
/// `_receive` frames followed by one `_interrupt` frame.
#[test]
fn register_completion_emits_interrupt() {
	let transport_typed = Arc::new(RecordingTransport::new());
	let transport_dyn: Arc<dyn Transport> = transport_typed.clone();
	let manager = SubscriptionManager::new(Spawner::for_test());
	let items = dummy_stream(vec![vec![0xaa], vec![0xbb]]);
	manager.register("p:1".to_string(), 7, 99, items, transport_dyn);
	let observed = transport_typed.wait_for(3, std::time::Duration::from_secs(2));
	assert_eq!(observed, 3, "expected 2 receive frames + 1 interrupt");
	let frames = transport_typed.sent();
	assert_eq!(frames[0].payload.trait_id, 7);
	assert_eq!(frames[0].payload.method_id, 99);
	assert_eq!(frames[0].payload.message_type, MESSAGE_TYPE_RECEIVE);
	assert_eq!(frames[0].payload.value, vec![0xaa]);
	assert_eq!(frames[1].payload.method_id, 99);
	assert_eq!(frames[1].payload.message_type, MESSAGE_TYPE_RECEIVE);
	assert_eq!(frames[1].payload.value, vec![0xbb]);
	assert_eq!(frames[2].payload.trait_id, 7);
	assert_eq!(frames[2].payload.method_id, 99);
	assert_eq!(frames[2].payload.message_type, MESSAGE_TYPE_INTERRUPT);
	assert_eq!(frames[2].payload.value, encode_clean_interrupt());
}

/// Calling `handle_stop` twice on the same request id must be a
/// no-op the second time around (the entry has already been removed,
/// no panic, no extra frames).
#[test]
fn double_stop_is_idempotent() {
	let transport_typed = Arc::new(RecordingTransport::new());
	let transport_dyn: Arc<dyn Transport> = transport_typed.clone();
	let manager = SubscriptionManager::new(Spawner::for_test());
	let slow_stream: SubscriptionStream = Box::pin(stream::pending());
	manager.register("p:1".to_string(), 7, 99, slow_stream, transport_dyn);
	manager.handle_stop("p:1");
	// Second call must not panic and must not emit any frame.
	manager.handle_stop("p:1");
	std::thread::sleep(std::time::Duration::from_millis(50));
	assert!(transport_typed.sent().is_empty(), "double-stop must not emit any frame");
}

/// The manager must drive subscriptions through the injected spawner,
/// not by reaching out to `std::thread::spawn` itself. The counter
/// inside the test spawner is the proof.
#[test]
fn subscription_uses_provided_spawner_not_native_thread() {
	let invocations = Arc::new(AtomicUsize::new(0));
	let invocations_for_spawner = invocations.clone();
	let spawner = Spawner::with_test_scheduler(move |fut| {
		invocations_for_spawner.fetch_add(1, Ordering::SeqCst);
		std::thread::spawn(move || futures::executor::block_on(fut));
	});

	let transport_typed = Arc::new(RecordingTransport::new());
	let transport_dyn: Arc<dyn Transport> = transport_typed.clone();
	let manager = SubscriptionManager::new(spawner);
	let items = dummy_stream(vec![vec![0xcc]]);
	manager.register("p:1".to_string(), 7, 99, items, transport_dyn);

	// Wait for the worker future to drain to completion so we know
	// the spawner closure ran on this path.
	let _ = transport_typed.wait_for(2, std::time::Duration::from_secs(2));
	assert_eq!(
		invocations.load(Ordering::SeqCst),
		1,
		"spawner must be invoked exactly once per register",
	);
}

/// A `_stop` arriving before `activate` (the stop-before-register race on
/// non-serialized transports) must abort the subscription: no `_receive`
/// frames are emitted even though the stream had items to yield.
#[test]
fn stop_before_activate_aborts_subscription() {
	let transport_typed = Arc::new(RecordingTransport::new());
	let transport_dyn: Arc<dyn Transport> = transport_typed.clone();
	let manager = SubscriptionManager::new(Spawner::for_test());
	let token = manager.reserve("p:1".to_string());
	manager.handle_stop("p:1");
	let items = dummy_stream(vec![vec![0x01], vec![0x02]]);
	manager.activate(token, 7, 99, items, transport_dyn);
	std::thread::sleep(std::time::Duration::from_millis(50));
	assert!(
		transport_typed.sent().is_empty(),
		"a stop before activate must abort the subscription"
	);
}

/// Re-using a live request id (the duplicate-`_start` case) supersedes the
/// previous subscription rather than leaking it: the first stream is
/// stopped, only the second runs, and the superseded stream leaves no
/// frames behind.
#[test]
fn duplicate_start_supersedes_previous_without_leak() {
	let transport_typed = Arc::new(RecordingTransport::new());
	let transport_dyn: Arc<dyn Transport> = transport_typed.clone();
	let manager = SubscriptionManager::new(Spawner::for_test());

	// First subscription never yields; the second reservation for the
	// same id must stop it.
	let pending: SubscriptionStream = Box::pin(stream::pending());
	manager.register("p:1".to_string(), 7, 99, pending, transport_dyn.clone());

	// Second subscription yields one item then ends.
	let items = dummy_stream(vec![vec![0xaa]]);
	manager.register("p:1".to_string(), 7, 99, items, transport_dyn);

	// Exactly the second stream's frames appear: one receive + one
	// completion interrupt. The first (pending) stream contributes none.
	let observed = transport_typed.wait_for(2, std::time::Duration::from_secs(2));
	assert_eq!(observed, 2, "expected the second stream's receive + interrupt only");
	let frames = transport_typed.sent();
	assert_eq!(frames[0].payload.trait_id, 7);
	assert_eq!(frames[0].payload.method_id, 99);
	assert_eq!(frames[0].payload.value, vec![0xaa]);
	assert_eq!(frames[1].payload.trait_id, 7);
	assert_eq!(frames[1].payload.method_id, 99);

	manager.handle_stop("p:1");
	std::thread::sleep(std::time::Duration::from_millis(50));
	assert_eq!(transport_typed.sent().len(), 2, "no leaked frames from the superseded stream");
}

#[test]
fn cancel_all_stops_live_subscription_streams() {
	let transport_typed = Arc::new(RecordingTransport::new());
	let transport_dyn: Arc<dyn Transport> = transport_typed.clone();
	let manager = SubscriptionManager::new(Spawner::for_test());
	let dropped = Arc::new(std::sync::atomic::AtomicBool::new(false));
	let stream: SubscriptionStream = Box::pin(PendingDropStream { dropped: dropped.clone() });

	manager.register("p:1".to_string(), 7, 99, stream, transport_dyn);
	manager.cancel_all();

	let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
	while !dropped.load(Ordering::SeqCst) {
		assert!(
			std::time::Instant::now() < deadline,
			"cancel_all did not drop the live subscription stream"
		);
		std::thread::sleep(std::time::Duration::from_millis(5));
	}
	assert!(
		transport_typed.sent().is_empty(),
		"runtime disposal cancellation must not emit an interrupt frame"
	);
}
