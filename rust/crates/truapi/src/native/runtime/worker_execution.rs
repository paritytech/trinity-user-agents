//! Operations available to consumers of a shared product execution.

use std::sync::Arc;

use super::{
	NativeMaterialsObserver, NativeMaterialsResolution, NativeRendererObserver, NativeSubscription,
};
use crate::ProductRuntimeError;
use truapi::latest;

/// The consumer operations on one shared product execution.
///
/// Retaining this handle does not keep the worker running.
#[uniffi::export(rust)]
pub trait NativeWorkerExecution: Send + Sync {
	/// Deliver the host's complete Chat room list to this worker.
	fn notify_chat_rooms_changed(&self, rooms: Vec<latest::ChatRoom>);

	/// Publish a Chat action, buffering it until the execution subscribes.
	fn publish_chat_action(
		&self,
		action: latest::HostChatActionSubscribeItem,
	) -> Result<(), ProductRuntimeError>;

	/// Observe replacement trees until cancellation. Pocket keeps the latest face for offline use.
	fn render(
		&self,
		request: latest::ProductRendererRenderRequest,
		observer: Arc<dyn NativeRendererObserver>,
	) -> Result<Arc<NativeSubscription>, ProductRuntimeError>;

	/// Follow materials for a tree drawn by this worker.
	///
	/// Each fallback reaches the drawing execution's failure subscription. With `float_filterable`
	/// disabled, the generated shaders filter 32-bit float images themselves.
	fn resolve_materials(
		&self,
		tree: latest::RendererNode,
		context: latest::RenderContext,
		float_filterable: bool,
		observer: Box<dyn NativeMaterialsObserver>,
	) -> Arc<NativeMaterialsResolution>;

	/// Publish a renderer action, buffering it until the drawing execution subscribes.
	fn publish_renderer_action(
		&self,
		item: latest::HostRendererActionSubscribeItem,
	) -> Result<(), ProductRuntimeError>;
}
