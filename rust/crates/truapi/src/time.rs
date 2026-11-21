//! Waiting: [`Timer`] for waits that tests can control, and [`retry`] with
//! [`Backoff`] for retrying an operation.

use core::{future::Future, time::Duration};

use futures::future::{BoxFuture, Either, ready};

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
