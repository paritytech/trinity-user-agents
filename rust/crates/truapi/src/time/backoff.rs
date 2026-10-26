//! [`Backoff`]: the pauses between retries of an operation that keeps failing.

use core::time::Duration;

/// Pauses between retries of an operation that keeps failing: the first pause,
/// then each one twice the one before, up to the longest.
///
/// The pauses never run out: as an [`Iterator`] it always returns `Some`, and
/// [`Backoff::next_pause`] takes the next pause without the `Option`.
#[derive(Debug)]
pub struct Backoff {
	first: Duration,
	longest: Duration,
	upcoming: Duration,
}

impl Backoff {
	/// Pauses from `first`, doubling up to `longest`. `first` must be nonzero,
	/// since a zero pause never grows, and at most `longest`.
	pub fn new(first: Duration, longest: Duration) -> Self {
		debug_assert!(
			!first.is_zero() && first <= longest,
			"a backoff needs a nonzero first pause no longer than the longest, got {first:?} and {longest:?}"
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
	use super::*;

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
