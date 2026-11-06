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
