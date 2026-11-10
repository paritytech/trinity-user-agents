//! Retrying an operation that fails: [`retry`] runs the attempts, and
//! [`Backoff`] supplies the pauses between them.

use core::{ops::ControlFlow, time::Duration};

use futures::{FutureExt, pin_mut};
use truapi::CancellationToken;

use super::{RealTimer, Timer};

/// Runs `attempt` until it returns `Break`, sleeping between attempts.
///
/// `Continue` carries a failure worth retrying; `pause_after` takes it and
/// returns how long to wait. `Break` carries the result, including a failure
/// retrying cannot fix. To give up after some number of failures, count them
/// in `attempt`.
pub async fn retry<Done, Failure, AttemptFuture>(
	attempt: impl FnMut() -> AttemptFuture,
	pause_after: impl FnMut(Failure) -> Duration,
) -> Done
where
	AttemptFuture: Future<Output = ControlFlow<Done, Failure>>,
{
	retry_on(&RealTimer, attempt, pause_after).await
}

/// [`retry`], sleeping on `timer`.
pub async fn retry_on<Done, Failure, AttemptFuture>(
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

/// [`retry_on`] that stops when `cancel` fires during a pause, returning
/// the failure that started it. It never drops an attempt in flight, which
/// could leave something open, such as a subscription.
pub async fn retry_until_cancelled<Done, Failure, AttemptFuture>(
	timer: &dyn Timer,
	cancel: &CancellationToken,
	mut attempt: impl FnMut() -> AttemptFuture,
	mut pause_after: impl FnMut(&Failure) -> Duration,
) -> Result<Done, Failure>
where
	AttemptFuture: Future<Output = ControlFlow<Done, Failure>>,
{
	loop {
		let failure = match attempt().await {
			ControlFlow::Break(done) => return Ok(done),
			ControlFlow::Continue(failure) => failure,
		};
		let paused = timer.sleep(pause_after(&failure)).fuse();
		let cancelled = cancel.cancelled().fuse();
		pin_mut!(paused, cancelled);
		futures::select_biased! {
			_ = cancelled => return Err(failure),
			() = paused => {},
		}
	}
}

/// An endless sequence of pauses that starts at a first pause and doubles up
/// to a cap.
///
/// The pauses have no jitter, so clients that fail together retry together.
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
	/// If `first` is zero, which would never grow, or is longer than `longest`.
	pub const fn new(first: Duration, longest: Duration) -> Self {
		assert!(
			!first.is_zero() && first.as_nanos() <= longest.as_nanos(),
			"a backoff needs a nonzero first pause no longer than the longest"
		);
		Self { first, longest, upcoming: first }
	}

	/// Returns the pause before the next attempt and doubles the following one,
	/// up to the cap.
	pub fn next_pause(&mut self) -> Duration {
		let pause = self.upcoming;
		self.upcoming = pause.saturating_mul(2).min(self.longest);
		pause
	}

	/// Starts again from the first pause. Call it once the operation makes
	/// progress.
	pub fn reset(&mut self) {
		self.upcoming = self.first;
	}
}

#[cfg(test)]
mod tests;
