//! Connection-scoped, host-fed action streams buffered until the product subscribes.

use core::sync::atomic::{AtomicU64, Ordering};
use std::collections::VecDeque;
use std::sync::Mutex;

use futures::StreamExt;
use futures::channel::mpsc;
use truapi::{CallError, Subscription};

use crate::host_core::ProductRuntimeError;

const ACTION_BUFFER_CAPACITY: usize = 64;

struct State<Item> {
    subscriber: Option<mpsc::UnboundedSender<Item>>,
    buffer: VecDeque<Item>,
    closed: bool,
}

impl<Item> Default for State<Item> {
    fn default() -> Self {
        Self {
            subscriber: None,
            buffer: VecDeque::new(),
            closed: false,
        }
    }
}

/// One product connection's stream of host-authored items of one kind.
pub struct ActionChannel<Item> {
    state: Mutex<State<Item>>,
    closed_reason: &'static str,
    /// When the host last published an item, in unix seconds, or zero before
    /// the first.
    last_published_secs: AtomicU64,
}

impl<Item: Send + 'static> ActionChannel<Item> {
    /// Create an empty channel; `closed_reason` names the stream in the
    /// interrupt a late subscriber sees after `close`.
    fn new(closed_reason: &'static str) -> Self {
        Self {
            state: Mutex::new(State::default()),
            closed_reason,
            last_published_secs: AtomicU64::new(0),
        }
    }

    /// One connection's Chat action stream.
    pub fn chat() -> Self {
        Self::new("chat is closed for this product connection")
    }

    /// One connection's Renderer action stream.
    pub fn renderer() -> Self {
        Self::new("renderer is closed for this product connection")
    }

    /// Open the product's subscription and drain buffered items first.
    pub fn subscribe<E: Send + 'static>(&self) -> Subscription<Item, CallError<E>> {
        let (sender, receiver) = mpsc::unbounded();
        let mut state = self.state.lock().expect("action channel mutex poisoned");
        if state.closed {
            return Subscription::interrupted(CallError::HostFailure {
                reason: self.closed_reason.to_string(),
            });
        }
        for item in state.buffer.drain(..) {
            let _ = sender.unbounded_send(item);
        }
        state.subscriber = Some(sender);
        Subscription::new(receiver.map(Ok))
    }

    /// Publish one item, buffering it until the product subscribes.
    pub fn publish(&self, item: Item) -> Result<(), ProductRuntimeError> {
        self.deliver(item)?;
        self.last_published_secs
            .store(crate::unix_time::current_unix_secs(), Ordering::Release);
        Ok(())
    }

    fn deliver(&self, mut item: Item) -> Result<(), ProductRuntimeError> {
        let mut state = self.state.lock().expect("action channel mutex poisoned");
        if state.closed {
            return Err(ProductRuntimeError::Closed);
        }
        if let Some(sender) = state.subscriber.as_ref() {
            match sender.unbounded_send(item) {
                Ok(()) => return Ok(()),
                Err(error) => item = error.into_inner(),
            }
            state.subscriber = None;
        }
        if state.buffer.len() == ACTION_BUFFER_CAPACITY {
            return Err(ProductRuntimeError::BufferFull);
        }
        state.buffer.push_back(item);
        Ok(())
    }

    /// Whether the host published an item within the last `window_secs`.
    pub fn published_within(&self, window_secs: u64) -> bool {
        let published_at = self.last_published_secs.load(Ordering::Acquire);
        published_at != 0
            && crate::unix_time::current_unix_secs().saturating_sub(published_at) <= window_secs
    }

    /// Backdate the last publish to `unix_secs`.
    #[cfg(test)]
    pub fn note_published_at(&self, unix_secs: u64) {
        self.last_published_secs.store(unix_secs, Ordering::Release);
    }

    /// End the current subscriber's stream while keeping buffered items for
    /// the next product connection that subscribes.
    pub fn detach(&self) {
        let mut state = self.state.lock().expect("action channel mutex poisoned");
        state.subscriber = None;
    }

    /// Close the channel and discard buffered items.
    #[cfg(any(test, not(target_arch = "wasm32")))]
    pub fn close(&self) {
        let mut state = self.state.lock().expect("action channel mutex poisoned");
        state.closed = true;
        state.subscriber = None;
        state.buffer.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;
    use futures::executor::block_on;

    fn channel() -> ActionChannel<String> {
        ActionChannel::new("closed")
    }

    #[test]
    fn buffered_actions_are_drained_in_fifo_order() {
        let channel = channel();
        channel.publish("first".to_string()).unwrap();
        channel.publish("second".to_string()).unwrap();

        let mut items = channel.subscribe::<truapi::latest::GenericError>();
        assert_eq!(block_on(items.next()), Some(Ok("first".to_string())));
        assert_eq!(block_on(items.next()), Some(Ok("second".to_string())));
    }

    #[test]
    fn full_startup_action_buffer_is_reported() {
        let channel = channel();
        for index in 0..ACTION_BUFFER_CAPACITY {
            channel.publish(index.to_string()).unwrap();
        }

        assert!(matches!(
            channel.publish("overflow".to_string()),
            Err(ProductRuntimeError::BufferFull)
        ));
    }

    #[test]
    fn detach_keeps_buffered_actions_for_the_next_subscriber() {
        let channel = channel();
        let mut first = channel.subscribe::<truapi::latest::GenericError>();
        channel.publish("live".to_string()).unwrap();
        assert_eq!(block_on(first.next()), Some(Ok("live".to_string())));

        channel.detach();
        assert_eq!(block_on(first.next()), None);
        channel.publish("buffered".to_string()).unwrap();

        let mut second = channel.subscribe::<truapi::latest::GenericError>();
        assert_eq!(block_on(second.next()), Some(Ok("buffered".to_string())));
    }

    #[test]
    fn closing_discards_buffered_actions() {
        let channel = channel();
        channel.publish("discard me".to_string()).unwrap();
        channel.close();

        // Subscribing to a closed channel interrupts, so a product can
        // tell it from a stream that ran and finished.
        let mut items = channel.subscribe::<truapi::latest::GenericError>();
        match block_on(items.next()) {
            Some(Err(CallError::HostFailure { reason })) => assert_eq!(reason, "closed"),
            other => panic!("a closed channel must interrupt, got {other:?}"),
        }
        assert!(matches!(
            channel.publish("too late".to_string()),
            Err(ProductRuntimeError::Closed)
        ));
    }

    #[test]
    fn separate_connections_cannot_observe_each_others_actions() {
        let first = channel();
        let second = channel();
        let mut first_items = first.subscribe::<truapi::latest::GenericError>();
        let mut second_items = second.subscribe::<truapi::latest::GenericError>();

        first.publish("first only".to_string()).unwrap();
        second.publish("second only".to_string()).unwrap();
        assert_eq!(
            block_on(first_items.next()),
            Some(Ok("first only".to_string()))
        );
        assert_eq!(
            block_on(second_items.next()),
            Some(Ok("second only".to_string()))
        );

        second.close();
        assert!(matches!(
            second.publish("closed".to_string()),
            Err(ProductRuntimeError::Closed)
        ));
    }
}
