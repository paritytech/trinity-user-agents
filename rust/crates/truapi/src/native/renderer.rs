//! Native observation of core streams: renderer trees, and the subscription
//! handle every native observation is cancelled through.

use std::sync::{Arc, Mutex};

use futures::{
	StreamExt,
	future::{AbortHandle, Abortable},
};
use truapi::{CallError, Subscription, latest::ProductRendererRenderItem};

use crate::{interrupt::interrupt_reason, subscription::Spawner};

/// Observer implemented by a native host to receive renderer tree replacements.
#[uniffi::export(callback_interface)]
pub trait NativeRendererObserver: Send + Sync {
	/// Deliver a complete replacement tree.
	fn on_update(&self, node: ProductRendererRenderItem);

	/// Report that the renderer stream ended without drawing further trees.
	/// The last tree delivered stands.
	fn on_complete(&self);

	/// Report that the product could not serve this render. The last tree
	/// delivered, if any, is partial and must not be treated as final.
	fn on_error(&self, reason: String);
}

/// Cancellable native observation of one core stream.
#[derive(uniffi::Object)]
pub struct NativeSubscription {
	abort: Mutex<Option<AbortHandle>>,
}

#[uniffi::export]
impl NativeSubscription {
	/// Stop delivering updates to the native observer.
	pub fn cancel(&self) {
		if let Some(abort) = self.abort.lock().expect("native subscription mutex poisoned").take() {
			abort.abort();
		}
	}
}

impl Drop for NativeSubscription {
	fn drop(&mut self) {
		self.cancel();
	}
}

/// Runs `observation` until it ends or the returned subscription is cancelled.
pub fn spawn_observation(
	spawner: &Spawner,
	observation: impl Future<Output = ()> + Send + 'static,
) -> Arc<NativeSubscription> {
	let (abort, registration) = AbortHandle::new_pair();
	(spawner)(Box::pin(async move {
		let _ = Abortable::new(observation, registration).await;
	}));
	Arc::new(NativeSubscription { abort: Mutex::new(Some(abort)) })
}

pub fn observe_renderer(
	mut stream: Subscription<ProductRendererRenderItem, CallError<truapi::latest::GenericError>>,
	observer: Arc<dyn NativeRendererObserver>,
	spawner: Spawner,
) -> Arc<NativeSubscription> {
	spawn_observation(&spawner, async move {
		while let Some(item) = stream.next().await {
			match item {
				Ok(node) => observer.on_update(node),
				Err(error) => return observer.on_error(interrupt_reason(error)),
			}
		}
		observer.on_complete();
	})
}
