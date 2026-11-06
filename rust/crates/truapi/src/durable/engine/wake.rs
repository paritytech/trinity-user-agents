//! [`WakeSignal`]: a conflating wake-up for any number of listeners.

use futures::channel::mpsc;
use parking_lot::Mutex;

/// Wakes every current listener. A wake sent while one is still pending for
/// a listener collapses into it, so a slow listener is never queued behind.
#[derive(Default)]
pub struct WakeSignal {
	listeners: Mutex<Vec<mpsc::Sender<()>>>,
}

impl WakeSignal {
	/// A new listener's stream of wakes.
	pub fn subscribe(&self) -> mpsc::Receiver<()> {
		let (sender, receiver) = mpsc::channel(0);
		self.listeners.lock().push(sender);
		receiver
	}

	/// Wakes every listener. Returns whether any listener was still there to
	/// hear it.
	pub fn wake(&self) -> bool {
		let mut listeners = self.listeners.lock();
		listeners.retain(|listener| !listener.is_closed());
		for listener in listeners.iter_mut() {
			// A full channel already holds a wake, which says the same.
			let _ = listener.try_send(());
		}
		!listeners.is_empty()
	}
}

#[cfg(test)]
mod tests;
