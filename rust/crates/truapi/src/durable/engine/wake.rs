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
mod tests {
    use super::*;

    #[test]
    fn wakes_sent_while_one_is_pending_collapse_into_it() {
        let signal = WakeSignal::default();
        let mut listener = signal.subscribe();

        signal.wake();
        signal.wake();

        assert!(listener.try_recv().is_ok());
        assert!(!listener.try_recv().is_ok());
    }

    #[test]
    fn a_wake_with_no_listener_left_reports_that_nobody_heard_it() {
        let signal = WakeSignal::default();
        drop(signal.subscribe());

        assert!(!signal.wake());
    }
}
