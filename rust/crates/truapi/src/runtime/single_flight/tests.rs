use super::*;
use core::sync::atomic::{AtomicUsize, Ordering};
use futures::{executor::block_on, future::join_all};

use crate::test_support::test_spawner;

/// A read that counts itself and answers once `gate` opens.
fn gated_read(
	reads: &Arc<AtomicUsize>,
	gate: &Shared<oneshot::Receiver<u32>>,
) -> impl FnOnce() -> BoxFuture<'static, u32> {
	let (reads, gate) = (reads.clone(), gate.clone());
	move || {
		async move {
			reads.fetch_add(1, Ordering::SeqCst);
			gate.await.unwrap()
		}
		.boxed()
	}
}

#[test]
fn a_settled_read_is_not_kept_so_the_next_caller_reads_again() {
	let flight = SingleFlight::<&str, Result<u32, String>>::new(test_spawner());
	let reads = Arc::new(AtomicUsize::new(0));
	let read = |answer: Result<u32, String>| {
		let reads = reads.clone();
		move || async move {
			reads.fetch_add(1, Ordering::SeqCst);
			answer
		}
	};

	let first = block_on(flight.run("peopl", read(Err("node closed".to_string()))));
	let second = block_on(flight.run("peopl", read(Ok(7))));

	assert_eq!(
		(first, second, reads.load(Ordering::SeqCst)),
		(Some(Err("node closed".to_string())), Some(Ok(7)), 2)
	);
}

#[test]
fn reads_for_different_keys_run_side_by_side() {
	let flight = SingleFlight::<&str, u32>::new(test_spawner());
	let reads = Arc::new(AtomicUsize::new(0));
	let (open, gate) = oneshot::channel();
	let gate = gate.shared();

	let answers = block_on(async {
		let callers = join_all(
			["peopl", "coffer", "peopl"].map(|key| flight.run(key, gated_read(&reads, &gate))),
		);
		let opener = async {
			open.send(7).unwrap();
		};
		futures::join!(callers, opener).0
	});

	assert_eq!((answers, reads.load(Ordering::SeqCst)), (vec![Some(7); 3], 2));
}

#[test]
fn a_read_still_answers_its_waiters_when_the_caller_that_started_it_gives_up() {
	let flight = SingleFlight::<&str, u32>::new(test_spawner());
	let reads = Arc::new(AtomicUsize::new(0));
	let (open, gate) = oneshot::channel();
	let gate = gate.shared();

	let answer = block_on(async {
		let mut first = Box::pin(flight.run("peopl", gated_read(&reads, &gate)));
		assert!(futures::poll!(first.as_mut()).is_pending());
		let mut waiter = Box::pin(flight.run("peopl", gated_read(&reads, &gate)));
		assert!(futures::poll!(waiter.as_mut()).is_pending());
		drop(first);
		open.send(7).unwrap();
		waiter.await
	});

	assert_eq!((answer, reads.load(Ordering::SeqCst)), (Some(7), 1));
}

/// Sets its flag when dropped, as a read's follow is released when the read is.
struct Held(Arc<std::sync::atomic::AtomicBool>);

impl Drop for Held {
	fn drop(&mut self) {
		self.0.store(true, Ordering::SeqCst);
	}
}

#[test]
fn a_read_every_caller_gave_up_on_is_stopped_and_lets_go_of_what_it_holds() {
	let flight = SingleFlight::<&str, u32>::new(test_spawner());
	let released = Arc::new(std::sync::atomic::AtomicBool::new(false));
	let held = Held(released.clone());

	block_on(async {
		let mut caller = Box::pin(flight.run("peopl", move || async move {
			let _held = held;
			futures::future::pending::<u32>().await
		}));
		assert!(futures::poll!(caller.as_mut()).is_pending());
	});

	let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
	while !released.load(Ordering::SeqCst) && std::time::Instant::now() < deadline {
		std::thread::yield_now();
	}
	assert!(released.load(Ordering::SeqCst), "the abandoned read still holds what it read with");
	assert_eq!(block_on(flight.run("peopl", || async { 7 })), Some(7));
}
