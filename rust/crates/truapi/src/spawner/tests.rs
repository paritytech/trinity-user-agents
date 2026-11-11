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
