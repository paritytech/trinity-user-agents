use core::iter;

use futures::{
	executor::block_on,
	future::{BoxFuture, ready},
};
use parking_lot::Mutex;

use super::*;

/// Finishes every wait immediately and records its duration.
#[derive(Default)]
struct RecordingTimer {
	waits: Mutex<Vec<Duration>>,
}

impl Timer for RecordingTimer {
	fn sleep(&self, duration: Duration) -> BoxFuture<'static, ()> {
		self.waits.lock().push(duration);
		Box::pin(ready(()))
	}

	fn now(&self) -> Duration {
		Duration::ZERO
	}
}

fn pauses(backoff: &mut Backoff, count: usize) -> Vec<Duration> {
	iter::repeat_with(|| backoff.next_pause()).take(count).collect()
}

#[test]
fn retry_attempts_at_once_then_waits_the_pause_each_failure_asks_for() {
	let timer = RecordingTimer::default();
	let mut attempts = 0;
	let mut failures = Vec::new();

	let done = block_on(retry_on(
		&timer,
		|| {
			attempts += 1;
			ready(if attempts == 3 {
				ControlFlow::Break(attempts)
			} else {
				ControlFlow::Continue(attempts)
			})
		},
		|failure| {
			failures.push(failure);
			Duration::from_secs(failure * 10)
		},
	));

	assert_eq!(
		(done, failures, timer.waits.into_inner()),
		(3, vec![1, 2], vec![Duration::from_secs(10), Duration::from_secs(20)])
	);
}

/// Cancelling during a pause returns the failure that started it, and no
/// further attempt is made.
#[test]
fn retry_on_until_stops_at_a_cancelled_pause() {
	let timer = PendingTimer;
	let cancel = CancellationToken::default();
	let mut attempts = 0;

	let result = block_on(async {
		let retrying = retry_until_cancelled(
			&timer,
			&cancel,
			|| {
				attempts += 1;
				ready(if attempts < 3 {
					ControlFlow::Continue(attempts)
				} else {
					ControlFlow::Break(())
				})
			},
			|_| Duration::from_secs(1),
		);
		let cancel = async {
			cancel.cancel();
		};
		futures::join!(retrying, cancel).0
	});

	assert_eq!((result, attempts), (Err(1), 1));
}

/// Every wait stays pending, so only a cancellation can end it.
struct PendingTimer;

impl Timer for PendingTimer {
	fn sleep(&self, _duration: Duration) -> BoxFuture<'static, ()> {
		Box::pin(futures::future::pending())
	}

	fn now(&self) -> Duration {
		Duration::ZERO
	}
}

#[test]
fn pauses_double_from_the_first_and_hold_at_the_longest() {
	let mut backoff = Backoff::new(Duration::from_secs(1), Duration::from_secs(60));

	let secs: Vec<u64> = pauses(&mut backoff, 9).iter().map(Duration::as_secs).collect();

	assert_eq!(secs, vec![1, 2, 4, 8, 16, 32, 60, 60, 60]);
}

#[test]
fn doubling_saturates_instead_of_overflowing() {
	let mut backoff = Backoff::new(Duration::from_secs(1), Duration::MAX);

	assert_eq!(pauses(&mut backoff, 100).last(), Some(&Duration::MAX));
}

#[test]
#[should_panic(expected = "a backoff needs a nonzero first pause")]
fn a_zero_first_pause_is_refused() {
	Backoff::new(Duration::ZERO, Duration::from_secs(5));
}

#[test]
#[should_panic(expected = "a backoff needs a nonzero first pause")]
fn a_first_pause_longer_than_the_longest_is_refused() {
	Backoff::new(Duration::from_secs(10), Duration::from_secs(5));
}

#[test]
fn a_reset_after_progress_restarts_the_whole_sequence() {
	let mut backoff = Backoff::new(Duration::from_millis(50), Duration::from_secs(5));
	let before = pauses(&mut backoff, 4);

	backoff.reset();

	assert_eq!(pauses(&mut backoff, 4), before);
}
