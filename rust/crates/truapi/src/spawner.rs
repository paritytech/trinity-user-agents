//! Platform executor for background tasks.

use core::future::Future;
#[cfg(test)]
use std::sync::Arc;
#[cfg(all(test, not(target_arch = "wasm32")))]
use std::sync::atomic::Ordering;

#[cfg(test)]
use futures::future::BoxFuture;

/// Background executor bound to a native Tokio runtime or the local WASM executor.
/// Native hosts retain ownership of their runtime; tasks submitted after its
/// shutdown are cancelled.
#[derive(Clone)]
pub struct Spawner(Executor);

#[derive(Clone)]
enum Executor {
	#[cfg(not(target_arch = "wasm32"))]
	Tokio(tokio::runtime::Handle),
	#[cfg(target_arch = "wasm32")]
	Wasm,
	#[cfg(test)]
	Test(Arc<dyn Fn(BoxFuture<'static, ()>) + Send + Sync>),
}

#[cfg(target_arch = "wasm32")]
impl Default for Spawner {
	fn default() -> Self {
		Self(Executor::Wasm)
	}
}

impl Spawner {
	/// Bind tasks to the host's runtime, including calls from outside that runtime.
	#[cfg(not(target_arch = "wasm32"))]
	pub fn new(handle: tokio::runtime::Handle) -> Self {
		Self(Executor::Tokio(handle))
	}

	/// Schedule background work on this executor.
	pub fn spawn(&self, task: impl Future<Output = ()> + Send + 'static) {
		match &self.0 {
			#[cfg(not(target_arch = "wasm32"))]
			Executor::Tokio(handle) => {
				handle.spawn(task);
			},
			#[cfg(target_arch = "wasm32")]
			Executor::Wasm => wasm_bindgen_futures::spawn_local(task),
			#[cfg(test)]
			Executor::Test(spawn) => spawn(Box::pin(task)),
		}
	}
}

#[cfg(test)]
impl Spawner {
	/// Control scheduling in tests that need to queue, gate, or observe tasks.
	pub fn with_test_scheduler(
		spawn: impl Fn(BoxFuture<'static, ()>) + Send + Sync + 'static,
	) -> Self {
		Self(Executor::Test(Arc::new(spawn)))
	}

	/// Keep blocking fixtures from stalling other tasks on native targets.
	pub fn for_test() -> Self {
		#[cfg(not(target_arch = "wasm32"))]
		{
			Self::with_test_scheduler(|task| {
				std::thread::spawn(move || futures::executor::block_on(task));
			})
		}
		#[cfg(target_arch = "wasm32")]
		{
			Self::with_test_scheduler(futures::executor::block_on)
		}
	}

	/// Track completed tasks while allowing each to run independently.
	#[cfg(not(target_arch = "wasm32"))]
	pub fn counting() -> (Self, Arc<std::sync::atomic::AtomicUsize>) {
		let finished = Arc::new(std::sync::atomic::AtomicUsize::new(0));
		let counter = finished.clone();
		let spawner = Self::with_test_scheduler(move |task| {
			let counter = counter.clone();
			std::thread::spawn(move || {
				futures::executor::block_on(task);
				counter.fetch_add(1, Ordering::SeqCst);
			});
		});
		(spawner, finished)
	}
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
	use super::*;

	#[test]
	fn native_spawner_uses_its_configured_runtime() {
		let runtime = tokio::runtime::Builder::new_multi_thread()
			.worker_threads(1)
			.build()
			.expect("task runtime");
		let caller = tokio::runtime::Builder::new_current_thread().build().expect("caller runtime");
		let spawner = Spawner::new(runtime.handle().clone());
		let (sender, receiver) = futures::channel::oneshot::channel();

		let actual_runtime = caller.block_on(async {
			spawner.spawn(async move {
				sender.send(tokio::runtime::Handle::current().id()).unwrap();
			});
			receiver.await.expect("task ran on its runtime")
		});

		assert_eq!(actual_runtime, runtime.handle().id());
	}

	#[test]
	fn native_spawner_drops_tasks_after_runtime_shutdown() {
		let runtime = tokio::runtime::Builder::new_current_thread().build().expect("task runtime");
		let spawner = Spawner::new(runtime.handle().clone());
		runtime.shutdown_background();
		let (sender, receiver) = std::sync::mpsc::channel();

		spawner.spawn(async move {
			sender.send(()).unwrap();
		});

		assert_eq!(
			receiver.recv_timeout(std::time::Duration::from_secs(1)),
			Err(std::sync::mpsc::RecvTimeoutError::Disconnected),
		);
	}
}
