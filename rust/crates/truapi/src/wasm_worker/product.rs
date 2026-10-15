//! A wasm worker as the product of one [`ProductRuntime`].
//!
//! The worker calls the product runtime through its imports, with no frames.
//! The calls that the host starts into the product, such as a render or a
//! chat query, go through the runtime's host-initiated subscriptions as for a
//! script worker. Their `Start` and `Stop` frames come to the frame sink of
//! the runtime. [`WorkerFrames`] is that sink: it starts and stops the calls
//! in the worker, and gives the answers of the worker back to the runtime as
//! `Receive` and `Interrupt` frames. Thus each host-initiated method reaches
//! a wasm worker with no code for that method.

use std::{
	collections::HashMap,
	sync::{Arc, Mutex, OnceLock},
};

use parity_scale_codec::Decode;

use super::{HostCall, HostCallEvent, HostCalls, WasmEnv, WasmWorker, WasmWorkerError};
use crate::{
	CallError, FrameSink, ProductRuntime, ProductRuntimeControl,
	frame::{
		MESSAGE_TYPE_INTERRUPT, MESSAGE_TYPE_RECEIVE, MESSAGE_TYPE_START, MESSAGE_TYPE_STOP,
		Payload, ProtocolMessage,
	},
	generated::wire_table::{MethodIds, WIRE_TABLE, WireKind},
};

/// The prefix of the request ids of the host-initiated subscriptions.
const HOST_REQUEST_PREFIX: &str = "h:";

impl WasmWorker {
	/// Instantiate `wasm` as the product of the runtime that `open` builds over
	/// the frame sink it gets. The worker calls that runtime through its
	/// imports, and the calls that the host starts through the runtime, such
	/// as `render` or `chat_messages_query`, reach the worker.
	pub fn for_product_runtime(
		wasm: &[u8],
		open: impl FnOnce(Arc<dyn FrameSink>) -> ProductRuntime,
	) -> Result<(Self, ProductRuntime), WasmWorkerError> {
		let frames = Arc::new(WorkerFrames::default());
		let product_runtime = open(frames.clone());
		let worker = match Self::new(WasmEnv::for_product(product_runtime.host().clone()), wasm) {
			Ok(worker) => worker,
			Err(error) => {
				product_runtime.dispose();
				return Err(error);
			},
		};
		let connected = frames.connection.set((worker.host_calls(), product_runtime.control()));
		debug_assert!(connected.is_ok(), "a new sink has no connection");
		Ok((worker, product_runtime))
	}
}

/// The frame sink of the product runtime of a wasm worker.
#[derive(Default)]
struct WorkerFrames {
	connection: OnceLock<(HostCalls, ProductRuntimeControl)>,
	/// The open calls by request id. `None` while the call starts.
	open: Arc<Mutex<HashMap<String, Option<HostCall>>>>,
}

impl FrameSink for WorkerFrames {
	fn emit_frame(&self, frame: Vec<u8>) {
		let Ok(message) = ProtocolMessage::decode(&mut frame.as_slice()) else {
			return;
		};
		let Some((calls, control)) = self.connection.get() else {
			return;
		};
		if !message.request_id.starts_with(HOST_REQUEST_PREFIX) {
			return;
		}
		match message.payload.message_type {
			MESSAGE_TYPE_START => self.start(calls, control, message),
			MESSAGE_TYPE_STOP => {
				self.open
					.lock()
					.expect("open host calls mutex poisoned")
					.remove(&message.request_id);
			},
			_ => {},
		}
	}
}

impl WorkerFrames {
	fn start(&self, calls: &HostCalls, control: &ProductRuntimeControl, message: ProtocolMessage) {
		let ids =
			MethodIds { trait_id: message.payload.trait_id, method_id: message.payload.method_id };
		let request_id = message.request_id;
		let answers = {
			let control = control.clone();
			let open = self.open.clone();
			let request_id = request_id.clone();
			move |event: HostCallEvent| {
				let (message_type, value) = match event {
					HostCallEvent::Item(item) => (MESSAGE_TYPE_RECEIVE, item),
					HostCallEvent::End(end) => {
						open.lock().expect("open host calls mutex poisoned").remove(&request_id);
						(MESSAGE_TYPE_INTERRUPT, end)
					},
				};
				let payload = Payload {
					trait_id: ids.trait_id,
					method_id: ids.method_id,
					message_type,
					value,
				};
				control.receive_host_call_answer(ProtocolMessage {
					request_id: request_id.clone(),
					payload,
				});
			}
		};
		let Some(method) = subscription_name(ids) else {
			answers(HostCallEvent::failure(CallError::Unsupported));
			return;
		};
		self.open
			.lock()
			.expect("open host calls mutex poisoned")
			.insert(request_id.clone(), None);
		let call = calls.start(method, message.payload.value, answers);
		// An end or a `Stop` that came first removed the entry, and the call
		// then drops here, which stops nothing that still runs.
		if let Some(slot) =
			self.open.lock().expect("open host calls mutex poisoned").get_mut(&request_id)
		{
			*slot = Some(call);
		}
	}
}

/// The wire name of the subscription method with `ids`.
fn subscription_name(ids: MethodIds) -> Option<&'static str> {
	WIRE_TABLE.iter().find_map(|entry| match entry.kind {
		WireKind::Subscription(entry_ids) if entry_ids == ids => Some(entry.method),
		_ => None,
	})
}
