//! Operations available to consumers of a shared product execution.

use std::sync::Arc;

use super::{NativeProductExecution, NativeRendererObserver, NativeSubscription};
use crate::ProductRuntimeError;
use truapi::latest;

/// Chat and renderer operations on one shared product execution.
///
/// Retaining this handle does not keep the worker running.
#[derive(uniffi::Object)]
pub struct NativeWorkerExecution {
	execution: Arc<NativeProductExecution>,
}

/// Share the execution without exposing its lifecycle controls.
pub fn shared(execution: &Arc<NativeProductExecution>) -> Arc<NativeWorkerExecution> {
	Arc::new(NativeWorkerExecution { execution: execution.clone() })
}

#[uniffi::export]
impl NativeWorkerExecution {
	/// Deliver the host's complete Chat room list to this worker.
	pub fn notify_chat_rooms_changed(&self, rooms: Vec<latest::ChatRoom>) {
		self.execution.notify_chat_rooms_changed(rooms);
	}

	/// Deliver a native action to the worker's Chat subscription.
	pub fn publish_chat_action(
		&self,
		action: latest::HostChatActionSubscribeItem,
	) -> Result<(), ProductRuntimeError> {
		self.execution.publish_chat_action(action)
	}

	/// Open a renderer stream on the worker's current connection.
	pub fn render(
		&self,
		request: latest::ProductRendererRenderRequest,
		observer: Arc<dyn NativeRendererObserver>,
	) -> Result<Arc<NativeSubscription>, ProductRuntimeError> {
		self.execution.render(request, observer)
	}

	/// Deliver a host action to the worker that drew its body.
	pub fn publish_renderer_action(
		&self,
		item: latest::HostRendererActionSubscribeItem,
	) -> Result<(), ProductRuntimeError> {
		self.execution.publish_renderer_action(item)
	}
}
