//! Core-side invalidation signal for host-global session storage.
//!
//! The host owns persistence; the core owns decoding and projecting the
//! current blob into `SessionState` and `AuthState`. This notifier is just the
//! "the backing store may have changed" signal that drives a re-read.

use std::sync::{Arc, Mutex};

use futures::{channel::mpsc, stream::BoxStream};

/// Fan-out notifier for host session-storage change ticks.
#[derive(Default)]
pub struct SessionStoreChangeNotifier {
	subscribers: Mutex<Vec<mpsc::UnboundedSender<()>>>,
}

impl SessionStoreChangeNotifier {
	/// Create a notifier with no subscribers.
	pub fn new() -> Arc<Self> {
		Arc::new(Self::default())
	}

	/// Broadcast a storage-change tick to current subscribers.
	pub fn notify(&self) {
		let mut subscribers =
			self.subscribers.lock().expect("session-store notifier mutex poisoned");
		subscribers.retain(|tx| tx.unbounded_send(()).is_ok());
	}

	/// Subscribe to storage-change ticks.
	pub fn subscribe(&self) -> BoxStream<'static, ()> {
		let (tx, rx) = mpsc::unbounded();
		self.subscribers.lock().expect("session-store notifier mutex poisoned").push(tx);
		Box::pin(rx)
	}
}

#[cfg(test)]
mod tests;
