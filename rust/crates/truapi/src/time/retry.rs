//! Retrying an operation that keeps failing: [`retry`] runs the attempts, and
//! a [`Backoff`] is the usual pauses between them.

use core::time::Duration;

use super::Timer;

/// Runs `attempt`, and again after each pause from `pauses` that
/// `should_retry` takes, waiting the pause on `timer`. Returns the last
/// outcome: the first one `should_retry` turns down, or the one after the last
/// pause.
///
/// `should_retry` sees each outcome with the pause before the next attempt, so
/// it decides which outcomes are worth another attempt and reports the ones it
/// retries. It is not asked once the pauses have run out.
pub async fn retry<Outcome, Attempt>(
	timer: &dyn Timer,
	pauses: impl IntoIterator<Item = Duration>,
	mut attempt: impl FnMut() -> Attempt,
	mut should_retry: impl FnMut(&Outcome, Duration) -> bool,
) -> Outcome
where
	Attempt: Future<Output = Outcome>,
{
	let mut pauses = pauses.into_iter();
	loop {
		let outcome = attempt().await;
		match pauses.next() {
			Some(pause) if should_retry(&outcome, pause) => timer.sleep(pause).await,
			_ => return outcome,
		}
	}
}

/// Pauses between retries of an operation that keeps failing: the first pause,
/// then each one twice the one before, up to the longest.
///
/// The pauses never run out: as an [`Iterator`] it always returns `Some`, and
/// [`Backoff::next_pause`] takes the next pause without the `Option`. A loop
/// that only retries hands it to [`retry`]; one that also does other work, or
/// starts over after a success, takes the pauses itself.
#[derive(Debug)]
pub struct Backoff {
	first: Duration,
	longest: Duration,
	upcoming: Duration,
}

impl Backoff {
	/// Pauses from `first`, doubling up to `longest`. `first` must be nonzero,
	/// since a zero pause never grows, and at most `longest`.
	pub const fn new(first: Duration, longest: Duration) -> Self {
		assert!(
			!first.is_zero() && first.as_nanos() <= longest.as_nanos(),
			"a backoff needs a nonzero first pause no longer than the longest"
		);
		Self { first, longest, upcoming: first }
	}

	/// The pause before the next retry, without moving past it.
	pub fn peek(&self) -> Duration {
		self.upcoming
	}

	/// The pause before the next retry. The one after it is twice as long, up
	/// to the longest.
	pub fn next_pause(&mut self) -> Duration {
		let pause = self.upcoming;
		self.upcoming = pause.saturating_mul(2).min(self.longest);
		pause
	}

	/// Starts again from the first pause, once the operation succeeded.
	pub fn reset(&mut self) {
		self.upcoming = self.first;
	}
}

impl Iterator for Backoff {
	type Item = Duration;

	fn next(&mut self) -> Option<Duration> {
		Some(self.next_pause())
	}
}

#[cfg(test)]
mod tests {
	use futures::{
		executor::block_on,
		future::{BoxFuture, ready},
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

	fn secs(pauses: &[u64]) -> Vec<Duration> {
		pauses.iter().copied().map(Duration::from_secs).collect()
	}

	#[test]
	fn retries_after_each_pause_until_an_outcome_is_turned_down() {
		let timer = RecordingTimer::default();
		let mut attempts = 0;
		let mut offered = Vec::new();

		let outcome = block_on(retry(
			&timer,
			secs(&[1, 2, 4, 8]),
			|| {
				attempts += 1;
				ready(attempts)
			},
			|attempt, pause| {
				offered.push(pause);
				*attempt < 3
			},
		));

		assert_eq!(
			(outcome, offered, timer.waits.into_inner()),
			(3, secs(&[1, 2, 4]), secs(&[1, 2]))
		);
	}

	#[test]
	fn once_the_pauses_run_out_the_last_outcome_is_returned_without_asking() {
		let timer = RecordingTimer::default();
		let mut attempts = 0;
		let mut asked = 0;

		let outcome = block_on(retry(
			&timer,
			secs(&[1, 2]),
			|| {
				attempts += 1;
				ready(Err::<(), _>(attempts))
			},
			|_, _| {
				asked += 1;
				true
			},
		));

		assert_eq!((outcome, asked, timer.waits.into_inner()), (Err(3), 2, secs(&[1, 2])));
	}

	#[test]
	fn pauses_double_from_the_first_and_hold_at_the_longest() {
		let pauses: Vec<u64> = Backoff::new(Duration::from_secs(1), Duration::from_secs(60))
			.take(9)
			.map(|pause| pause.as_secs())
			.collect();

		assert_eq!(pauses, vec![1, 2, 4, 8, 16, 32, 60, 60, 60]);
	}

	#[test]
	fn doubling_saturates_instead_of_overflowing() {
		let mut backoff = Backoff::new(Duration::from_secs(1), Duration::MAX);

		assert_eq!(backoff.nth(100), Some(Duration::MAX));
	}

	#[test]
	fn a_reset_after_success_restarts_the_whole_sequence() {
		let mut backoff = Backoff::new(Duration::from_millis(50), Duration::from_secs(5));
		let before: Vec<Duration> = backoff.by_ref().take(4).collect();

		backoff.reset();

		assert_eq!(backoff.take(4).collect::<Vec<_>>(), before);
	}

	#[test]
	fn peek_reads_the_pause_that_next_pause_then_takes() {
		let mut backoff = Backoff::new(Duration::from_millis(200), Duration::from_secs(5));
		backoff.next_pause();

		assert_eq!(
			[backoff.peek(), backoff.next_pause(), backoff.peek()],
			[Duration::from_millis(400), Duration::from_millis(400), Duration::from_millis(800)]
		);
	}
}
