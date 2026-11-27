use std::sync::{
	Arc,
	atomic::{AtomicBool, Ordering},
};

use futures::{channel::oneshot, executor::block_on};

use super::*;

/// A task that outlived its owner would keep running against state the
/// owner already gave up, so dropping the handle has to stop it.
#[test]
fn dropping_the_handle_stops_the_task() {
	let (release, released) = oneshot::channel::<()>();
	let (finish, finished) = oneshot::channel::<()>();
	let ran_past_the_wait = Arc::new(AtomicBool::new(false));
	let ran = ran_past_the_wait.clone();
	let task = Spawner::for_test().create_task(async move {
		let _ = released.await;
		ran.store(true, Ordering::SeqCst);
		let _ = finish.send(());
	});

	drop(task);
	let _ = release.send(());

	assert!(block_on(finished).is_err());
	assert!(!ran_past_the_wait.load(Ordering::SeqCst));
}

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
