//! Connection-scoped, host-fed action streams buffered until the product subscribes.

use core::sync::atomic::{AtomicU64, Ordering};
use std::{collections::VecDeque, sync::Mutex};

use futures::{StreamExt, channel::mpsc};
use truapi::{CallError, Subscription};

use crate::host_core::ProductRuntimeError;

const ACTION_BUFFER_CAPACITY: usize = 64;

struct State<Item> {
	/// Every open subscription, beside the `core_instance` of the connection
	/// that opened it. A product may subscribe more than once (an SDK keeps one
	/// stream per surface it draws), and each one hears every item published
	/// while it is open. Items buffered while none was open go only to the next
	/// subscription.
	subscribers: Vec<(u64, mpsc::UnboundedSender<Item>)>,
	buffer: VecDeque<Item>,
	closed: bool,
}

impl<Item> Default for State<Item> {
	fn default() -> Self {
		Self { subscribers: Vec::new(), buffer: VecDeque::new(), closed: false }
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

impl<Item: Clone + Send + 'static> ActionChannel<Item> {
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

	/// One connection's stream of `Materials` nodes that fell back. Separate
	/// from the renderer's, whose items count as the user's taps.
	pub fn renderer_failures() -> Self {
		Self::new("renderer failures are closed for this product connection")
	}

	/// Open one more subscription for the product, draining buffered items into it first.
	///
	/// Buffered items go to this subscription only, so one opened later never hears them.
	/// `core_instance` names the connection that opens it, whose `detach` ends it.
	pub fn subscribe<E: Send + 'static>(
		&self,
		core_instance: u64,
	) -> Subscription<Item, CallError<E>> {
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
		state.subscribers.retain(|(_, subscriber)| !subscriber.is_closed());
		state.subscribers.push((core_instance, sender));
		Subscription::new(receiver.map(Ok))
	}

	/// Publish one item to every open subscription, or buffer it until the product subscribes.
	pub fn publish(&self, item: Item) -> Result<(), ProductRuntimeError> {
		let mut state = self.state.lock().expect("action channel mutex poisoned");
		if state.closed {
			return Err(ProductRuntimeError::Closed);
		}
		if !state.subscribers.is_empty() {
			self.note_published_now();
			state
				.subscribers
				.retain(|(_, subscriber)| subscriber.unbounded_send(item.clone()).is_ok());
			if !state.subscribers.is_empty() {
				return Ok(());
			}
		}
		if state.buffer.len() == ACTION_BUFFER_CAPACITY {
			return Err(ProductRuntimeError::BufferFull);
		}
		state.buffer.push_back(item);
		self.note_published_now();
		Ok(())
	}

	fn note_published_now(&self) {
		self.last_published_secs
			.store(crate::unix_time::current_unix_secs(), Ordering::Release);
	}

	/// Whether the host published an item within the last `window_secs`.
	pub fn published_within(&self, window_secs: u64) -> bool {
		let published_at = self.last_published_secs.load(Ordering::Acquire);
		published_at != 0 &&
			crate::unix_time::current_unix_secs().saturating_sub(published_at) <= window_secs
	}

	/// Backdate the last publish to `unix_secs`.
	#[cfg(test)]
	pub fn note_published_at(&self, unix_secs: u64) {
		self.last_published_secs.store(unix_secs, Ordering::Release);
	}

	/// End the streams of the connection `core_instance` while keeping buffered
	/// items for the next product connection that subscribes.
	///
	/// Other connections of the same execution keep their streams, so one that
	/// ends does not cut the taps off from one still open.
	pub fn detach(&self, core_instance: u64) {
		let mut state = self.state.lock().expect("action channel mutex poisoned");
		state.subscribers.retain(|(owner, _)| *owner != core_instance);
	}

	/// Close the channel and discard buffered items.
	#[cfg(any(test, not(target_arch = "wasm32")))]
	pub fn close(&self) {
		let mut state = self.state.lock().expect("action channel mutex poisoned");
		state.closed = true;
		state.subscribers.clear();
		state.buffer.clear();
	}
}

#[cfg(test)]
mod tests;
