//! Waiting: [`Timer`] for waits that tests can control, and [`retry`] with
//! [`Backoff`] for retrying an operation.

use core::time::Duration;
use std::sync::OnceLock;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;

use futures::future::{BoxFuture, ready};
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

/// Waits for a duration, and tells the time.
pub trait Timer: Send + Sync {
	/// Completes once `duration` has passed, at once for a zero duration.
	fn sleep(&self, duration: Duration) -> BoxFuture<'static, ()>;

	/// The time since a fixed point in this process, which never goes back.
	fn now(&self) -> Duration;
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

	fn now(&self) -> Duration {
		static ORIGIN: OnceLock<Instant> = OnceLock::new();
		ORIGIN.get_or_init(Instant::now).elapsed()
	}
}

#[cfg(test)]
mod manual;
mod retry;

#[cfg(test)]
pub use manual::ManualTimer;
pub use retry::{Backoff, retry, retry_on, retry_until_cancelled};
