//! Waiting: [`Timer`] for waits that tests can control, and [`retry`] with
//! [`Backoff`] for retrying an operation.

use core::time::Duration;

use futures::future::BoxFuture;

/// Waits for a duration.
pub trait Timer: Send + Sync {
	/// Completes once `duration` has passed.
	fn sleep(&self, duration: Duration) -> BoxFuture<'static, ()>;
}

/// [`Timer`] over wall-clock time, on any executor.
pub struct RealTimer;

impl Timer for RealTimer {
	fn sleep(&self, duration: Duration) -> BoxFuture<'static, ()> {
		Box::pin(futures_timer::Delay::new(duration))
	}
}

#[cfg(test)]
mod manual;
mod retry;

#[cfg(test)]
pub use manual::ManualTimer;
pub use retry::{Backoff, retry};
