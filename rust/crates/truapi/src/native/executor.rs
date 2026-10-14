//! The process-wide Tokio runtime that native core work runs on.

use std::{
	io,
	sync::{Arc, Mutex, OnceLock},
};

use futures::future::BoxFuture;
use tokio::runtime::{Handle, Runtime};

use crate::subscription::Spawner;

/// Process-wide executor shared by every native host runtime and product bridge.
///
/// The runtime intentionally lives until process exit. Host runtimes and
/// product bridges have independent lifecycles, so shutting the executor down
/// with any one of them would interrupt the others.
pub struct SharedNativeExecutor {
	runtime: Runtime,
}

impl SharedNativeExecutor {
	fn new() -> io::Result<Self> {
		let runtime = tokio::runtime::Builder::new_multi_thread()
			.thread_name("truapi-native-worker")
			.enable_all()
			.build()
			.map_err(|err| io::Error::other(err.to_string()))?;
		let executor = Self { runtime };
		tracing::info!(worker_threads = executor.worker_threads(), "native core runtime started");
		Ok(executor)
	}

	/// Handle for spawning onto the runtime from any thread, inside it or not.
	pub fn handle(&self) -> Handle {
		self.runtime.handle().clone()
	}

	/// Number of worker threads the runtime schedules tasks on.
	pub fn worker_threads(&self) -> usize {
		self.runtime.metrics().num_workers()
	}

	/// Spawner that runs core tasks on this runtime, whichever thread spawns them.
	pub fn spawner(&self) -> Spawner {
		let handle = self.handle();
		Arc::new(move |task: BoxFuture<'static, ()>| {
			handle.spawn(task);
		})
	}
}

static SHARED_NATIVE_EXECUTOR: OnceLock<SharedNativeExecutor> = OnceLock::new();
static SHARED_NATIVE_EXECUTOR_INIT: Mutex<()> = Mutex::new(());

/// The shared executor, built on first use.
pub fn shared_native_executor() -> io::Result<&'static SharedNativeExecutor> {
	if let Some(executor) = SHARED_NATIVE_EXECUTOR.get() {
		return Ok(executor);
	}

	// Serialize fallible initialization without caching a transient thread
	// creation failure for the rest of the process.
	let _guard = SHARED_NATIVE_EXECUTOR_INIT
		.lock()
		.unwrap_or_else(|poisoned| poisoned.into_inner());
	if let Some(executor) = SHARED_NATIVE_EXECUTOR.get() {
		return Ok(executor);
	}

	let executor = SharedNativeExecutor::new()?;
	Ok(SHARED_NATIVE_EXECUTOR.get_or_init(|| executor))
}

#[cfg(test)]
mod tests {
	use super::*;
	use futures::FutureExt;

	#[test]
	fn shared_executor_uses_multithread_scheduler() {
		let executor = shared_native_executor().expect("shared native executor");
		let handle = executor.handle();
		assert_eq!(handle.runtime_flavor(), tokio::runtime::RuntimeFlavor::MultiThread);

		// Each task blocks one runtime worker at the barrier. They can only
		// both complete if the executor actually schedules them concurrently
		// on distinct worker threads.
		if executor.worker_threads() < 2 {
			return;
		}
		let barrier = Arc::new(std::sync::Barrier::new(2));
		let first = handle.spawn({
			let barrier = barrier.clone();
			async move {
				let worker = std::thread::current().id();
				barrier.wait();
				worker
			}
		});
		let second = handle.spawn(async move {
			let worker = std::thread::current().id();
			barrier.wait();
			worker
		});

		let client = tokio::runtime::Builder::new_current_thread()
			.enable_all()
			.build()
			.expect("test runtime");
		let (first, second) = client.block_on(async { tokio::join!(first, second) });
		assert_ne!(first.expect("first dispatch task"), second.expect("second dispatch task"),);
	}

	/// Core tasks run on the shared native runtime, including those spawned
	/// from a thread outside any runtime, such as a host thread.
	#[test]
	fn spawned_core_tasks_run_on_the_shared_runtime() {
		let executor = shared_native_executor().expect("shared native executor");
		let (runtime_tx, runtime_rx) = std::sync::mpsc::channel();

		executor.spawner()(
			async move {
				let runtime = Handle::try_current().map(|handle| handle.id());
				runtime_tx.send(runtime.ok()).unwrap();
			}
			.boxed(),
		);

		let ran_on = runtime_rx
			.recv_timeout(std::time::Duration::from_secs(5))
			.expect("spawned core task never ran");
		assert_eq!(ran_on, Some(executor.handle().id()));
	}

	#[test]
	fn shared_executor_is_reused() {
		let first = shared_native_executor().expect("first executor access");
		let second = shared_native_executor().expect("second executor access");

		assert_eq!(first.handle().id(), second.handle().id());
	}
}
