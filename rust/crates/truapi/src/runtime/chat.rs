//! Connection-scoped Chat streams shared by product and native entrypoints.

use core::time::Duration;
use std::{
	collections::HashMap,
	sync::{Arc, Mutex},
};

use truapi::{CallError, Subscription, latest};

use crate::{
	host_core::ProductRuntimeError, runtime::ActionChannel, subscription::Spawner,
	unix_time::current_unix_millis,
};

/// Shortest time between two `Seen` events of one room.
const SEEN_WINDOW_MS: u64 = 1000;

/// Chat access policy shared by the wire runtime and the native entrypoints:
/// only a Chat-kind execution with an active session may use Chat, and the
/// host must have installed a native adapter.
pub fn chat_platform_for(
	execution_kind: crate::platform::ProductExecutionKind,
	has_session: bool,
	chat: Option<&Arc<dyn crate::platform::ChatPlatform>>,
) -> Result<Arc<dyn crate::platform::ChatPlatform>, ProductRuntimeError> {
	if execution_kind != crate::platform::ProductExecutionKind::Worker || !has_session {
		return Err(ProductRuntimeError::Denied);
	}
	chat.cloned().ok_or(ProductRuntimeError::Unsupported)
}

/// Call budgets the core holds each product room to, shared by every
/// connection of one host runtime.
pub struct ChatRateLimits {
	/// `update_feed` calls.
	pub feed: crate::platform::ChatRateLimiter,
	/// `set_room_preview` and `set_room_activity` calls.
	pub preview: crate::platform::ChatRateLimiter,
}

impl Default for ChatRateLimits {
	fn default() -> Self {
		Self {
			feed: crate::platform::ChatRateLimiter::new(
				crate::platform::CHAT_FEED_UPDATES_PER_SECOND,
			),
			preview: crate::platform::ChatRateLimiter::new(
				crate::platform::CHAT_PREVIEW_UPDATES_PER_SECOND,
			),
		}
	}
}

/// One product connection's stream of chat events, buffered until the
/// product subscribes.
///
/// A host reports `Seen` as the user scrolls, which can be many times a
/// second. The channel sends at most one `Seen` per room in each second, and
/// the newest one held back follows when the second ends.
pub struct ChatEventChannel {
	events: Arc<ActionChannel<latest::HostChatActionSubscribeItem>>,
	seen: Arc<Mutex<HashMap<String, SeenWindow>>>,
}

/// The `Seen` pacing state of one room.
struct SeenWindow {
	/// When the last `Seen` of the room went out, in unix milliseconds.
	sent_at_ms: u64,
	/// The newest `Seen` held back until the window ends.
	held: Option<latest::HostChatActionSubscribeItem>,
}

impl Default for ChatEventChannel {
	fn default() -> Self {
		Self {
			events: Arc::new(ActionChannel::new("chat is closed for this product connection")),
			seen: Arc::default(),
		}
	}
}

impl ChatEventChannel {
	/// Open the product's subscription and drain buffered events first.
	pub fn subscribe<E: Send + 'static>(
		&self,
	) -> Subscription<latest::HostChatActionSubscribeItem, CallError<E>> {
		self.events.subscribe()
	}

	/// Publish one event, buffering it until the product subscribes. A `Seen`
	/// that comes less than a second after the last `Seen` of its room waits
	/// on `spawner` for the end of that second, and a newer one replaces it.
	pub fn publish(
		&self,
		item: latest::HostChatActionSubscribeItem,
		spawner: &Spawner,
	) -> Result<(), ProductRuntimeError> {
		if !matches!(item.event, latest::ChatEvent::Seen { .. }) {
			return self.events.publish(item);
		}
		let now_ms = current_unix_millis();
		let mut seen = self.seen.lock().expect("chat seen mutex poisoned");
		let Some(window) = seen.get_mut(&item.room_id) else {
			seen.insert(item.room_id.clone(), SeenWindow { sent_at_ms: now_ms, held: None });
			drop(seen);
			return self.events.publish(item);
		};
		let elapsed_ms = now_ms.saturating_sub(window.sent_at_ms);
		if elapsed_ms >= SEEN_WINDOW_MS && window.held.is_none() {
			window.sent_at_ms = now_ms;
			drop(seen);
			return self.events.publish(item);
		}
		let room_id = item.room_id.clone();
		if window.held.replace(item).is_none() {
			let wait = Duration::from_millis(SEEN_WINDOW_MS.saturating_sub(elapsed_ms));
			let (events, seen) = (self.events.clone(), self.seen.clone());
			(spawner)(Box::pin(async move {
				futures_timer::Delay::new(wait).await;
				let held =
					seen.lock().expect("chat seen mutex poisoned").get_mut(&room_id).and_then(
						|window| {
							window.sent_at_ms = current_unix_millis();
							window.held.take()
						},
					);
				if let Some(item) = held {
					// The connection may have closed meanwhile; the event goes
					// with it.
					let _ = events.publish(item);
				}
			}));
		}
		Ok(())
	}

	/// End the current subscriber's stream while keeping buffered events for
	/// the next product connection that subscribes.
	pub fn detach(&self) {
		self.events.detach();
	}

	/// Close the channel and discard buffered and held events.
	#[cfg(any(test, not(target_arch = "wasm32")))]
	pub fn close(&self) {
		self.events.close();
		self.seen.lock().expect("chat seen mutex poisoned").clear();
	}
}
