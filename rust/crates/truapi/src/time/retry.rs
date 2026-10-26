//! Retrying an operation that keeps failing: [`retry`] runs the attempts until
//! one stops them, and a [`Backoff`] is the usual pauses between them.

use core::{ops::ControlFlow, time::Duration};

use super::Timer;

/// Runs `attempt` until it breaks, and returns what it broke with.
///
/// An attempt continues with a failure worth retrying and breaks with anything
/// else, including a failure no retry can mend. `pause_after` gets each such
/// failure, may report it, and returns the wait on `timer` before the next
/// attempt. To give up after some failures, `attempt` counts them and breaks.
pub async fn retry<Done, Failure, AttemptFuture>(
	timer: &dyn Timer,
	mut attempt: impl FnMut() -> AttemptFuture,
	mut pause_after: impl FnMut(Failure) -> Duration,
) -> Done
where
	AttemptFuture: Future<Output = ControlFlow<Done, Failure>>,
{
	loop {
		match attempt().await {
			ControlFlow::Break(done) => return done,
			ControlFlow::Continue(failure) => timer.sleep(pause_after(failure)).await,
		}
	}
}

/// Pauses that start at a first one and double up to a longest one, without
/// end.
///
/// They carry no jitter; add it before a backoff paces many clients that fail
/// together against one server.
#[derive(Debug)]
pub struct Backoff {
	first: Duration,
	longest: Duration,
	upcoming: Duration,
}

impl Backoff {
	/// Pauses from `first`, doubling up to `longest`.
	///
	/// # Panics
	///
	/// If `first` is zero, since a zero pause never grows, or longer than
	/// `longest`.
	pub const fn new(first: Duration, longest: Duration) -> Self {
		assert!(
			!first.is_zero() && first.as_nanos() <= longest.as_nanos(),
			"a backoff needs a nonzero first pause no longer than the longest"
		);
		Self { first, longest, upcoming: first }
	}

	/// The pause before the next attempt. The one after it is twice as long,
	/// up to the longest.
	pub fn next_pause(&mut self) -> Duration {
		let pause = self.upcoming;
		self.upcoming = pause.saturating_mul(2).min(self.longest);
		pause
	}

	/// Starts again from the first pause, once the operation made progress.
	pub fn reset(&mut self) {
		self.upcoming = self.first;
	}
}

#[cfg(test)]
mod tests {
	use core::iter;

	use futures::{
		executor::block_on,
		future::{BoxFuture, Ready, ready},
	};
	use parking_lot::Mutex;

	use super::*;

	/// Ends every wait at once and records how long each one was.
	#[derive(Default)]
	struct RecordingTimer {
		waits: Mutex<Vec<Duration>>,
	}

	impl Timer for RecordingTimer {
		fn sleep(&self, duration: Duration) -> BoxFuture<'static, ()> {
			self.waits.lock().push(duration);
			Box::pin(ready(()))
		}
	}

	/// Continues with its attempt number until attempt `done_at`, which breaks.
	fn breaking_at(done_at: u64) -> impl FnMut() -> Ready<ControlFlow<u64, u64>> {
		let mut attempts = 0;
		move || {
			attempts += 1;
			ready(if attempts == done_at {
				ControlFlow::Break(attempts)
			} else {
				ControlFlow::Continue(attempts)
			})
		}
	}

	fn pauses(backoff: &mut Backoff, count: usize) -> Vec<Duration> {
		iter::repeat_with(|| backoff.next_pause()).take(count).collect()
	}

	#[test]
	fn retry_attempts_at_once_then_waits_the_pause_each_failure_asks_for() {
		let timer = RecordingTimer::default();
		let mut failures = Vec::new();

		let done = block_on(retry(&timer, breaking_at(3), |failure| {
			failures.push(failure);
			Duration::from_secs(failure * 10)
		}));

		assert_eq!(
			(done, failures, timer.waits.into_inner()),
			(3, vec![1, 2], vec![Duration::from_secs(10), Duration::from_secs(20)])
		);
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
}
