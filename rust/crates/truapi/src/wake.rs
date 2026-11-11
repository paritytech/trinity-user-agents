//! [`WakeSignal`]: a conflating wake-up for any number of listeners.

use futures::channel::mpsc;
use parking_lot::Mutex;

/// Wakes every current listener. A wake sent while one is still pending for
/// a listener collapses into it, so a slow listener is never queued behind.
#[derive(Default)]
pub struct WakeSignal {
	pub(crate) listeners: Mutex<Vec<mpsc::Sender<()>>>,
}

impl WakeSignal {
	/// A new listener's stream of wakes. Listeners that went away are dropped
	/// here as well as on [`Self::wake`], so a wake that never comes holds no
	/// more than the listeners still there.
	pub fn subscribe(&self) -> mpsc::Receiver<()> {
		let (sender, receiver) = mpsc::channel(0);
		let mut listeners = self.listeners.lock();
		listeners.retain(|listener| !listener.is_closed());
		listeners.push(sender);
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
