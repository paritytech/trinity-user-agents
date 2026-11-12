//! Waiting: [`Timer`] for waits that tests can control, and [`retry`] with
//! [`Backoff`] for retrying an operation. [`Clock`] reads the time, for code
//! whose timing a test drives.

use core::time::Duration;

use futures::future::{BoxFuture, ready};

use crate::unix_time::current_unix_secs;

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

/// Reads the time.
pub trait Clock: Send + Sync {
	/// Seconds since the Unix epoch.
	fn unix_secs(&self) -> u64;
}

/// [`Clock`] over the local clock.
pub struct SystemClock;

impl Clock for SystemClock {
	fn unix_secs(&self) -> u64 {
		current_unix_secs()
	}
}

#[cfg(test)]
mod manual;
mod retry;

#[cfg(test)]
pub use manual::ManualTimer;
pub use retry::{Backoff, retry, retry_on, retry_until_cancelled};
