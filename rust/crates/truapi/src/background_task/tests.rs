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
	let task = BackgroundTask::spawn(&Spawner::for_test(), async move {
		let _ = released.await;
		ran.store(true, Ordering::SeqCst);
		let _ = finish.send(());
	});

	drop(task);
	let _ = release.send(());

	assert!(block_on(finished).is_err());
	assert!(!ran_past_the_wait.load(Ordering::SeqCst));
}
