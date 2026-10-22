//! Connection-scoped Chat streams shared by product and native entrypoints.

use std::{
	collections::{HashMap, VecDeque},
	sync::{Arc, Mutex},
};

use futures::{StreamExt, channel::mpsc};
use truapi::{CallError, Subscription, latest};

use crate::host_core::ProductRuntimeError;

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

/// The declaration of a room that lets the core ask a product a query.
#[derive(Clone, Copy)]
pub enum ChatHandler {
	/// A flag of the room configuration.
	Config(fn(&latest::ChatRoomConfig) -> bool),
}

/// Call budgets the core holds each product room to, shared by every
/// connection of one host runtime.
pub struct ChatRateLimits {
	/// `update_feed` calls.
	pub feed: crate::platform::ChatRateLimiter,
	/// `set_room_preview` calls.
	pub preview: crate::platform::ChatRateLimiter,
	/// `set_room_contact` calls.
	pub contact: crate::platform::ChatRateLimiter,
	/// `set_room_call` calls.
	pub call: crate::platform::ChatRateLimiter,
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
			contact: crate::platform::ChatRateLimiter::new(
				crate::platform::CHAT_PREVIEW_UPDATES_PER_SECOND,
			),
			call: crate::platform::ChatRateLimiter::new(
				crate::platform::CHAT_CALL_UPDATES_PER_SECOND,
			),
		}
	}
}

/// The room events of each product, shared by every connection of one host
/// runtime. A host publishes an event for a product whether or not its worker
/// runs. The core keeps the newest [`CHAT_ROOM_EVENTS_QUEUED_MAX`] events of a
/// product that has no subscriber and drops the oldest.
///
/// [`CHAT_ROOM_EVENTS_QUEUED_MAX`]: crate::platform::CHAT_ROOM_EVENTS_QUEUED_MAX
#[derive(Default)]
pub struct ChatRoomEventHub {
	products: Mutex<HashMap<String, RoomEventQueue>>,
	/// When the user last pressed a call button or accepted a call, by
	/// product and room, in Unix seconds.
	call_consents: Mutex<HashMap<(String, String), u64>>,
}

/// The subscriber and the queued events of one product.
#[derive(Default)]
struct RoomEventQueue {
	subscriber: Option<mpsc::UnboundedSender<latest::HostChatRoomEventsSubscribeItem>>,
	queued: VecDeque<latest::HostChatRoomEventsSubscribeItem>,
}

impl ChatRoomEventHub {
	/// Send one event to the subscriber of `product_id`, or keep it until the
	/// product subscribes.
	pub fn publish(&self, product_id: &str, mut item: latest::HostChatRoomEventsSubscribeItem) {
		if matches!(
			item.event,
			latest::ChatRoomEvent::CallRequested { .. } |
				latest::ChatRoomEvent::CallAnswered { accept: true, .. }
		) {
			self.call_consents.lock().expect("chat call consent mutex poisoned").insert(
				(product_id.to_string(), item.room_id.clone()),
				crate::unix_time::current_unix_secs(),
			);
		}
		let mut products = self.products.lock().expect("chat room event mutex poisoned");
		let queue = products.entry(product_id.to_string()).or_default();
		if let Some(subscriber) = queue.subscriber.as_ref() {
			match subscriber.unbounded_send(item) {
				Ok(()) => return,
				Err(error) => item = error.into_inner(),
			}
			queue.subscriber = None;
		}
		if queue.queued.len() == crate::platform::CHAT_ROOM_EVENTS_QUEUED_MAX {
			queue.queued.pop_front();
		}
		queue.queued.push_back(item);
	}

	/// Whether the user pressed a call button or accepted a call in `room_id`
	/// of `product_id` within the last `window_secs`.
	pub fn call_consented_within(&self, product_id: &str, room_id: &str, window_secs: u64) -> bool {
		self.call_consents
			.lock()
			.expect("chat call consent mutex poisoned")
			.get(&(product_id.to_string(), room_id.to_string()))
			.is_some_and(|at| {
				crate::unix_time::current_unix_secs().saturating_sub(*at) <= window_secs
			})
	}

	/// Open the subscription of `product_id`: first the queued events, then
	/// live ones. A new subscription ends the previous one.
	pub fn subscribe<E: Send + 'static>(
		&self,
		product_id: &str,
	) -> Subscription<latest::HostChatRoomEventsSubscribeItem, CallError<E>> {
		let (sender, receiver) = mpsc::unbounded();
		let mut products = self.products.lock().expect("chat room event mutex poisoned");
		let queue = products.entry(product_id.to_string()).or_default();
		for item in queue.queued.drain(..) {
			let _ = sender.unbounded_send(item);
		}
		queue.subscriber = Some(sender);
		Subscription::new(receiver.map(Ok))
	}
}
