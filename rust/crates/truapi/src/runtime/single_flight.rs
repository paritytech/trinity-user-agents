//! One read per key at a time.
//!
//! A dotNS read opens its own Asset Hub follow and holds it until the read
//! settles. Callers asking for the same thing while a read is running wait for
//! its answer instead of opening a follow each, so a burst of them costs the
//! connection one follow slot.

use core::{future::Future, hash::Hash};
use std::{
	collections::HashMap,
	sync::{Arc, Mutex},
};

use futures::{
	FutureExt,
	channel::oneshot,
	future::{BoxFuture, Shared},
};

use tracing::{Instrument, instrument::WithSubscriber};

use crate::subscription::Spawner;

type Reads<K, V> = Arc<Mutex<HashMap<K, Shared<oneshot::Receiver<V>>>>>;

/// Reads keyed by `K` whose answers, `V`, are shared by every caller that
/// asked while the read ran.
///
/// A read runs as its own task, so it finishes whatever its callers do: one
/// that gives up on its own deadline does not cut the read short for the
/// rest. Nothing is kept once a read settles, so the next caller for that key
/// reads again, and a failed read is retried by whoever asks next.
#[derive(Clone)]
pub struct SingleFlight<K, V> {
	spawner: Spawner,
	reads: Reads<K, V>,
}

impl<K, V> SingleFlight<K, V>
where
	K: Eq + Hash + Clone + Send + 'static,
	V: Clone + Send + Sync + 'static,
{
	/// Reads spawned through `spawner`.
	pub fn new(spawner: Spawner) -> Self {
		Self { spawner, reads: Arc::default() }
	}

	/// Answers with the read already running for `key`, or starts `read` when
	/// none is. `None` when the read ended without answering, which happens
	/// only if its task was dropped or panicked.
	pub async fn run<F>(&self, key: K, read: impl FnOnce() -> F) -> Option<V>
	where
		F: Future<Output = V> + Send + 'static,
	{
		let (answer, start) = {
			let mut reads = self.reads.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
			match reads.get(&key) {
				Some(answer) => (answer.clone(), None),
				None => {
					let (sender, receiver) = oneshot::channel();
					let answer = receiver.shared();
					reads.insert(key.clone(), answer.clone());
					(answer, Some(sender))
				},
			}
		};
		if let Some(sender) = start {
			let settled = Settled { reads: self.reads.clone(), key };
			let read = read();
			// The read logs as the caller that started it would, under its span
			// and to its subscriber.
			let task: BoxFuture<'static, ()> = async move {
				let value = read.await;
				drop(settled);
				let _ = sender.send(value);
			}
			.in_current_span()
			.with_current_subscriber()
			.boxed();
			(self.spawner)(task);
		}
		answer.await.ok()
	}
}

/// Frees a read's key once its task ends, however it ends.
struct Settled<K: Eq + Hash, V> {
	reads: Reads<K, V>,
	key: K,
}

impl<K: Eq + Hash, V> Drop for Settled<K, V> {
	fn drop(&mut self) {
		self.reads
			.lock()
			.unwrap_or_else(|poisoned| poisoned.into_inner())
			.remove(&self.key);
	}
}

#[cfg(test)]
mod tests;
