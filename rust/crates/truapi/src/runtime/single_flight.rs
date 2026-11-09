//! One read per key at a time.
//!
//! A dotNS read opens its own Asset Hub follow and holds it until the read
//! settles. Callers asking for the same thing while a read is running wait for
//! its answer instead of opening a follow each, so a burst of them costs the
//! connection one follow slot.

use core::{future::Future, hash::Hash};
use std::{collections::HashMap, sync::Mutex};

use futures::channel::oneshot;

/// Reads keyed by `K` whose answers, `V`, are shared by every caller that
/// asked while the read ran.
///
/// Nothing is kept once a read settles: the next caller for that key reads
/// again, so a failed read is retried by whoever asks next.
pub struct SingleFlight<K, V> {
	waiting: Mutex<HashMap<K, Vec<oneshot::Sender<V>>>>,
}

impl<K, V> Default for SingleFlight<K, V> {
	fn default() -> Self {
		Self { waiting: Mutex::new(HashMap::new()) }
	}
}

impl<K: Eq + Hash + Clone, V: Clone> SingleFlight<K, V> {
	/// Answers with the read already running for `key`, or runs `read` when
	/// none is. A caller whose read is dropped before it settles leaves the
	/// key free, and one of the callers waiting on it runs its own read.
	pub async fn run<F>(&self, key: K, read: impl FnOnce() -> F) -> V
	where
		F: Future<Output = V>,
	{
		loop {
			let waiter = {
				let mut waiting =
					self.waiting.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
				match waiting.get_mut(&key) {
					Some(waiters) => {
						let (sender, receiver) = oneshot::channel();
						waiters.push(sender);
						Some(receiver)
					},
					None => {
						waiting.insert(key.clone(), Vec::new());
						None
					},
				}
			};
			match waiter {
				Some(receiver) => match receiver.await {
					Ok(value) => return value,
					Err(oneshot::Canceled) => continue,
				},
				None => {
					let running = Running { flight: self, key: Some(key) };
					let value = read().await;
					for waiter in running.settle() {
						let _ = waiter.send(value.clone());
					}
					return value;
				},
			}
		}
	}

	fn take(&self, key: &K) -> Vec<oneshot::Sender<V>> {
		self.waiting
			.lock()
			.unwrap_or_else(|poisoned| poisoned.into_inner())
			.remove(key)
			.unwrap_or_default()
	}
}

/// The key a caller is reading. Dropped before it settles, it frees the key
/// and drops every waiter's sender, which wakes them to read for themselves.
struct Running<'a, K: Eq + Hash + Clone, V: Clone> {
	flight: &'a SingleFlight<K, V>,
	key: Option<K>,
}

impl<K: Eq + Hash + Clone, V: Clone> Running<'_, K, V> {
	/// Frees the key and hands back the callers waiting on it.
	fn settle(mut self) -> Vec<oneshot::Sender<V>> {
		self.key.take().map(|key| self.flight.take(&key)).unwrap_or_default()
	}
}

impl<K: Eq + Hash + Clone, V: Clone> Drop for Running<'_, K, V> {
	fn drop(&mut self) {
		if let Some(key) = self.key.take() {
			drop(self.flight.take(&key));
		}
	}
}

#[cfg(test)]
mod tests;
