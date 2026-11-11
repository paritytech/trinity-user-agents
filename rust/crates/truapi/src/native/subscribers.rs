//! Streams following values the host pushes.

use core::borrow::Borrow;
use std::collections::HashMap;

use futures::channel::mpsc;

/// The streams following each key. A stream the product dropped is forgotten
/// on the next registration or send, so subscribing and dropping repeatedly
/// does not grow the collection while the value stays unchanged.
pub struct Subscribers<K, S> {
	by_key: HashMap<K, Vec<mpsc::UnboundedSender<S>>>,
}

// Written by hand: a derive would ask `K` and `S` to be `Default`, which the
// pushed values are not.
impl<K, S> Default for Subscribers<K, S> {
	fn default() -> Self {
		Self { by_key: HashMap::new() }
	}
}

impl<K: Eq + core::hash::Hash, S: Clone> Subscribers<K, S> {
	/// Follow `key` through `sender`.
	pub fn register(&mut self, key: K, sender: mpsc::UnboundedSender<S>) {
		self.prune();
		self.by_key.entry(key).or_default().push(sender);
	}

	/// Send `value` to every stream following `key`.
	pub fn send<Q>(&mut self, key: &Q, value: &S)
	where
		K: Borrow<Q>,
		Q: Eq + core::hash::Hash + ?Sized,
	{
		if let Some(subscribers) = self.by_key.get_mut(key) {
			subscribers.retain(|sender| sender.unbounded_send(value.clone()).is_ok());
		}
		self.prune();
	}

	/// How many streams are registered.
	#[cfg(test)]
	pub fn len(&self) -> usize {
		self.by_key.values().map(Vec::len).sum()
	}

	fn prune(&mut self) {
		self.by_key.retain(|_, subscribers| {
			subscribers.retain(|sender| !sender.is_closed());
			!subscribers.is_empty()
		});
	}
}

#[cfg(test)]
mod tests;
