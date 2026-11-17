//! One read per key at a time.
//!
//! A dotNS read takes several round trips to Asset Hub. Callers asking for the
//! same thing while a read is running wait for its answer instead of starting
//! their own, so a burst of them reads the chain once.

use core::{future::Future, hash::Hash};
use std::{
	collections::HashMap,
	sync::{Arc, Mutex, MutexGuard},
};

use futures::{
	FutureExt,
	channel::oneshot,
	future::{AbortHandle, Abortable, Shared},
};
use tracing::{Instrument, instrument::WithSubscriber};

use crate::Spawner;

/// Reads keyed by `K` whose answers, `V`, are shared by every caller that
/// asked while the read ran.
///
/// A read runs as its own task and keeps going while any caller still waits
/// on it, so one that gives up on its own deadline does not cut it short for
/// the rest. Once the last caller has given up the read is stopped, which
/// releases what it holds. Nothing is kept once a read settles, so the next
/// caller for that key reads again, and a failed read is retried by whoever
/// asks next.
#[derive(Clone)]
pub struct SingleFlight<K, V> {
	spawner: Spawner,
	reads: Arc<Mutex<Reads<K, V>>>,
}

/// The reads running now, each told apart from a later read of the same key.
struct Reads<K, V> {
	next_id: u64,
	running: HashMap<K, Running<V>>,
}

/// One read and how many callers still wait on it.
struct Running<V> {
	id: u64,
	answer: Shared<oneshot::Receiver<V>>,
	waiters: usize,
	abort: AbortHandle,
}

impl<K, V> SingleFlight<K, V>
where
	K: Eq + Hash + Clone + Send + 'static,
	V: Clone + Send + Sync + 'static,
{
	/// Reads spawned through `spawner`.
	pub fn new(spawner: Spawner) -> Self {
		let reads = Reads { next_id: 0, running: HashMap::new() };
		Self { spawner, reads: Arc::new(Mutex::new(reads)) }
	}

	/// Answers with the read already running for `key`, or starts `read` when
	/// none is. `None` when the read ended without answering, which happens
	/// only if its task was dropped or panicked.
	pub async fn run<F>(&self, key: K, read: impl FnOnce() -> F) -> Option<V>
	where
		F: Future<Output = V> + Send + 'static,
	{
		let (id, answer, start) = {
			let mut reads = self.lock();
			match reads.running.get_mut(&key) {
				Some(running) => {
					running.waiters += 1;
					(running.id, running.answer.clone(), None)
				},
				None => {
					let id = reads.next_id;
					reads.next_id += 1;
					let (sender, receiver) = oneshot::channel();
					let answer = receiver.shared();
					let (abort, registration) = AbortHandle::new_pair();
					let running = Running { id, answer: answer.clone(), waiters: 1, abort };
					reads.running.insert(key.clone(), running);
					(id, answer, Some((sender, registration)))
				},
			}
		};
		let _waiting = Release { flight: self.clone(), key: key.clone(), id, waiter: true };
		if let Some((sender, registration)) = start {
			let settled = Release { flight: self.clone(), key, id, waiter: false };
			let read = read();
			// The read logs as the caller that started it would, under its span
			// and to its subscriber.
			let task = async move {
				let value = read.await;
				drop(settled);
				let _ = sender.send(value);
			}
			.in_current_span()
			.with_current_subscriber();
			self.spawner.spawn(Abortable::new(task, registration).map(|_| ()));
		}
		answer.await.ok()
	}

	fn lock(&self) -> MutexGuard<'_, Reads<K, V>> {
		self.reads.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
	}
}

/// Read `id`'s hold on its key. A waiter's is let go when that caller stops
/// waiting, answered or not, and the last one stops the read if it is still
/// running. The read's own is let go when its task ends, however it ends.
struct Release<K, V>
where
	K: Eq + Hash + Clone + Send + 'static,
	V: Clone + Send + Sync + 'static,
{
	flight: SingleFlight<K, V>,
	key: K,
	id: u64,
	waiter: bool,
}

impl<K, V> Drop for Release<K, V>
where
	K: Eq + Hash + Clone + Send + 'static,
	V: Clone + Send + Sync + 'static,
{
	fn drop(&mut self) {
		let mut reads = self.flight.lock();
		let Some(running) = reads.running.get_mut(&self.key) else {
			return;
		};
		if running.id != self.id {
			return;
		}
		if self.waiter {
			running.waiters -= 1;
			if running.waiters > 0 {
				return;
			}
			running.abort.abort();
		}
		reads.running.remove(&self.key);
	}
}

#[cfg(test)]
mod tests;
