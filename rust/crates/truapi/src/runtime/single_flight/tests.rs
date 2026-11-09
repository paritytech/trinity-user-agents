use super::*;
use core::sync::atomic::{AtomicUsize, Ordering};
use futures::{FutureExt, channel::oneshot, executor::block_on, future::join_all};

/// A read that counts itself and answers once `gate` opens.
async fn gated_read(
	reads: &AtomicUsize,
	gate: futures::future::Shared<oneshot::Receiver<u32>>,
) -> u32 {
	reads.fetch_add(1, Ordering::SeqCst);
	gate.await.unwrap()
}

#[test]
fn a_settled_read_is_not_kept_so_the_next_caller_reads_again() {
	let flight = SingleFlight::<&str, Result<u32, String>>::default();
	let reads = AtomicUsize::new(0);
	let read = |answer: Result<u32, String>| {
		let reads = &reads;
		move || async move {
			reads.fetch_add(1, Ordering::SeqCst);
			answer
		}
	};

	let first = block_on(flight.run("peopl", read(Err("node closed".to_string()))));
	let second = block_on(flight.run("peopl", read(Ok(7))));

	assert_eq!(
		(first, second, reads.load(Ordering::SeqCst)),
		(Err("node closed".to_string()), Ok(7), 2)
	);
}

#[test]
fn reads_for_different_keys_run_side_by_side() {
	let flight = SingleFlight::<&str, u32>::default();
	let reads = AtomicUsize::new(0);
	let (open, gate) = oneshot::channel();
	let gate = gate.shared();

	let answers = block_on(async {
		let callers = join_all(
			["peopl", "coffer", "peopl"]
				.map(|key| flight.run(key, || gated_read(&reads, gate.clone()))),
		);
		let opener = async {
			open.send(7).unwrap();
		};
		futures::join!(callers, opener).0
	});

	assert_eq!((answers, reads.load(Ordering::SeqCst)), (vec![7; 3], 2));
}

#[test]
fn a_waiter_reads_for_itself_when_the_read_it_waited_on_is_dropped() {
	let flight = SingleFlight::<&str, u32>::default();

	let answer = block_on(async {
		let mut first = Box::pin(flight.run("peopl", futures::future::pending::<u32>));
		assert!(futures::poll!(first.as_mut()).is_pending());
		let mut waiter = Box::pin(flight.run("peopl", || async { 7 }));
		assert!(futures::poll!(waiter.as_mut()).is_pending());
		drop(first);
		waiter.await
	});

	assert_eq!(answer, 7);
}
