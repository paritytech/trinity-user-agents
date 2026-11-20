//! Waiting: [`Timer`] for waits that tests can control, and [`retry`] with
//! [`Backoff`] for retrying an operation. [`Clock`] reads the time, for code
//! whose timing a test drives.

use core::{future::Future, time::Duration};
use std::sync::OnceLock;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;

use futures::future::{BoxFuture, Either, ready};
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

use crate::unix_time::current_unix_secs;

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

/// `future`'s output, or `None` once `limit` passed first.
pub(crate) async fn within<T>(limit: Duration, future: impl Future<Output = T>) -> Option<T> {
	match futures::future::select(core::pin::pin!(future), futures_timer::Delay::new(limit)).await {
		Either::Left((output, _)) => Some(output),
		Either::Right(_) => None,
	}
}

#[cfg(test)]
mod manual;
mod retry;

#[cfg(test)]
pub use manual::ManualTimer;
pub use retry::{Backoff, retry, retry_on, retry_until_cancelled};
