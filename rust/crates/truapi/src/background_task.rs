//! [`BackgroundTask`]: a spawned task that lives as long as its handle.

use core::future::Future;

use futures::{
	FutureExt,
	future::{AbortHandle, Abortable},
};

use crate::subscription::Spawner;

/// A task running on a [`Spawner`] until this handle is dropped.
pub struct BackgroundTask(AbortHandle);

impl BackgroundTask {
	/// Spawns `task` on `spawner`. Dropping the result stops it at its next
	/// await point.
	pub fn spawn(spawner: &Spawner, task: impl Future<Output = ()> + Send + 'static) -> Self {
		let (abort, registration) = AbortHandle::new_pair();
		let abortable = Abortable::new(task, registration).map(|_| ());
		spawner(abortable.boxed());
		Self(abort)
	}
}

impl Drop for BackgroundTask {
	fn drop(&mut self) {
		self.0.abort();
	}
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
	use std::sync::{
		Arc,
		atomic::{AtomicBool, Ordering},
	};

	use futures::{channel::oneshot, executor::block_on};

	use super::*;
	use crate::test_support::test_spawner;

	/// A task that outlived its owner would keep running against state the
	/// owner already gave up, so dropping the handle has to stop it.
	#[test]
	fn dropping_the_handle_stops_the_task() {
		let (release, released) = oneshot::channel::<()>();
		let (finish, finished) = oneshot::channel::<()>();
		let ran_past_the_wait = Arc::new(AtomicBool::new(false));
		let ran = ran_past_the_wait.clone();
		let task = BackgroundTask::spawn(&test_spawner(), async move {
			let _ = released.await;
			ran.store(true, Ordering::SeqCst);
			let _ = finish.send(());
		});

		drop(task);
		let _ = release.send(());

		assert!(block_on(finished).is_err());
		assert!(!ran_past_the_wait.load(Ordering::SeqCst));
	}
}
