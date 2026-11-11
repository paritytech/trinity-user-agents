//! [`BackgroundTask`]: a spawned task that lives as long as its handle.

use core::future::Future;

use futures::{
	FutureExt,
	future::{AbortHandle, Abortable},
};

use crate::Spawner;

/// A task running on a [`Spawner`] until this handle is dropped.
pub struct BackgroundTask(AbortHandle);

impl BackgroundTask {
	/// Spawns `task` on `spawner`. Dropping the result stops it at its next
	/// await point.
	pub fn spawn(spawner: &Spawner, task: impl Future<Output = ()> + Send + 'static) -> Self {
		let (abort, registration) = AbortHandle::new_pair();
		let abortable = Abortable::new(task, registration).map(|_| ());
		spawner.spawn(abortable);
		Self(abort)
	}
}

impl Drop for BackgroundTask {
	fn drop(&mut self) {
		self.0.abort();
	}
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests;
