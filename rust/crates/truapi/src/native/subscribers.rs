//! Streams following values the host pushes.

use std::collections::HashMap;

use futures::channel::mpsc;

/// One stream following a key.
struct Subscriber<S> {
	/// Tells this stream apart from others on the same key.
	subscription: u64,
	sender: mpsc::UnboundedSender<S>,
}

/// The streams following each key. A stream the product dropped is forgotten
/// on the next registration or send, so subscribing and dropping repeatedly
/// does not grow the collection while the value stays unchanged.
pub struct Subscribers<K, S> {
	next: u64,
	by_key: HashMap<K, Vec<Subscriber<S>>>,
}

// Written by hand: a derive would ask `K` and `S` to be `Default`, which the
// pushed values are not.
impl<K, S> Default for Subscribers<K, S> {
	fn default() -> Self {
		Self { next: 0, by_key: HashMap::new() }
	}
}

impl<K: Eq + core::hash::Hash, S: Clone> Subscribers<K, S> {
	/// Follow `key` through `sender`. Returns the subscription number.
	pub fn register(&mut self, key: K, sender: mpsc::UnboundedSender<S>) -> u64 {
		self.prune();
		self.next += 1;
		let subscription = self.next;
		self.by_key.entry(key).or_default().push(Subscriber { subscription, sender });
		subscription
	}

	/// Send `value` to every stream following `key`.
	pub fn send(&mut self, key: &K, value: &S) {
		if let Some(subscribers) = self.by_key.get_mut(key) {
			subscribers
				.retain(|subscriber| subscriber.sender.unbounded_send(value.clone()).is_ok());
		}
		self.prune();
	}

	/// Stop every stream following `key`.
	pub fn end(&mut self, key: &K) {
		self.by_key.remove(key);
	}

	/// Stop the one stream `subscription` on `key`.
	pub fn end_one(&mut self, key: &K, subscription: u64) {
		if let Some(subscribers) = self.by_key.get_mut(key) {
			subscribers.retain(|subscriber| subscriber.subscription != subscription);
		}
		self.prune();
	}

	/// How many keys are followed.
	#[cfg(test)]
	pub fn keys(&self) -> usize {
		self.by_key.len()
	}

	/// How many streams are registered.
	#[cfg(test)]
	pub fn len(&self) -> usize {
		self.by_key.values().map(Vec::len).sum()
	}

	fn prune(&mut self) {
		self.by_key.retain(|_, subscribers| {
			subscribers.retain(|subscriber| !subscriber.sender.is_closed());
			!subscribers.is_empty()
		});
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	// Theme and locale subscribers used to be dropped only when a change was
	// sent, so subscribe/drop cycles on an unchanged value piled up dead
	// senders; registering now forgets them.
	#[test]
	fn a_dropped_stream_is_forgotten_on_the_next_registration() {
		let mut subscribers = Subscribers::<(), u8>::default();
		for _ in 0..5 {
			let (sender, receiver) = mpsc::unbounded();
			subscribers.register((), sender);
			drop(receiver);
		}
		let (sender, _live) = mpsc::unbounded();
		subscribers.register((), sender);

		assert_eq!(subscribers.len(), 1);
	}
}
