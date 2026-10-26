//! Waits: [`Timer`], which tests can control, and [`retry`], which retries an
//! operation with pauses such as a [`Backoff`] between attempts.

use core::time::Duration;

use futures::future::{BoxFuture, ready};

/// Waits for a duration.
pub trait Timer: Send + Sync {
	/// Completes once `duration` has passed, at once for a zero duration.
	fn sleep(&self, duration: Duration) -> BoxFuture<'static, ()>;
}

/// [`Timer`] over wall-clock time, on any executor.
pub struct RealTimer;

impl Timer for RealTimer {
	fn sleep(&self, duration: Duration) -> BoxFuture<'static, ()> {
		if duration.is_zero() {
			return Box::pin(ready(()));
		}
		Box::pin(futures_timer::Delay::new(duration))
	}
}

#[cfg(test)]
mod manual;
mod retry;

#[cfg(test)]
pub use manual::ManualTimer;
pub use retry::{Backoff, retry, retry_after};
