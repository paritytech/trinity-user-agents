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
	fn worker_threads(&self) -> usize {
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
mod tests;
