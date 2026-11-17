//! A [`Timer`] for tests, whose clock moves only when the test says.

use core::time::Duration;
use std::sync::atomic::{AtomicUsize, Ordering};

use futures::{channel::oneshot, future::BoxFuture};
use parking_lot::Mutex;

use super::Timer;

/// A clock that moves only when a test advances it.
#[derive(Default)]
pub struct ManualTimer {
	state: Mutex<ManualTimerState>,
	sleeps: AtomicUsize,
}

#[derive(Default)]
struct ManualTimerState {
	now: Duration,
	sleepers: Vec<(Duration, oneshot::Sender<()>)>,
}

impl ManualTimer {
	/// Moves the clock forward, waking every sleeper whose deadline passed.
	pub fn advance(&self, by: Duration) {
		let mut state = self.state.lock();
		state.now += by;
		let now = state.now;
		let (due, waiting) = core::mem::take(&mut state.sleepers)
			.into_iter()
			.partition(|(deadline, _)| *deadline <= now);
		state.sleepers = waiting;
		drop(state);
		for (_, wake) in due {
			let _ = wake.send(());
		}
	}

	/// How many sleeps were started. A zero-duration sleep does not count:
	/// it never waits.
	pub fn sleeps(&self) -> usize {
		self.sleeps.load(Ordering::SeqCst)
	}

	/// How long each sleep still waiting has left, in the order they started.
	pub fn remaining(&self) -> Vec<Duration> {
		let state = self.state.lock();
		state.sleepers.iter().map(|(deadline, _)| *deadline - state.now).collect()
	}
}

impl Timer for ManualTimer {
	fn sleep(&self, duration: Duration) -> BoxFuture<'static, ()> {
		if duration.is_zero() {
			return Box::pin(futures::future::ready(()));
		}
		let (wake, woken) = oneshot::channel();
		let mut state = self.state.lock();
		let deadline = state.now + duration;
		state.sleepers.push((deadline, wake));
		self.sleeps.fetch_add(1, Ordering::SeqCst);
		Box::pin(async move {
			let _ = woken.await;
		})
	}
}
