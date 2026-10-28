//! Connection-scoped Chat streams shared by product and native entrypoints.

use std::{
	collections::HashMap,
	sync::{Arc, Mutex},
};

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

/// Call budgets the core holds each product room to, shared by every
/// connection of one host runtime.
pub struct ChatRateLimits {
	/// `update_feed` calls.
	pub feed: ChatRateLimiter,
	/// `set_room_preview` calls.
	pub preview: ChatRateLimiter,
}

impl Default for ChatRateLimits {
	fn default() -> Self {
		Self {
			feed: ChatRateLimiter::new(crate::platform::CHAT_FEED_UPDATES_PER_SECOND),
			preview: ChatRateLimiter::new(crate::platform::CHAT_PREVIEW_UPDATES_PER_SECOND),
		}
	}
}

/// Token bucket for each `(product, room)` pair: at most `per_second` calls in
/// one second, refilled at the same rate.
pub struct ChatRateLimiter {
	per_second: u32,
	buckets: Mutex<HashMap<(String, String), ChatRateBucket>>,
}

struct ChatRateBucket {
	/// Tokens left, in thousandths of a token.
	milli_tokens: u64,
	/// Milliseconds since the Unix epoch of the last refill.
	refilled_at_ms: u64,
}

impl ChatRateLimiter {
	/// A limiter that accepts `per_second` calls for each room in one second.
	pub fn new(per_second: u32) -> Self {
		Self { per_second, buckets: Default::default() }
	}

	/// Take one token for `room_id` of `product_id` at `now_ms`, milliseconds
	/// since the Unix epoch. `false` when the room used its budget.
	pub fn try_acquire(&self, product_id: &str, room_id: &str, now_ms: u64) -> bool {
		let capacity = u64::from(self.per_second) * 1000;
		let mut buckets = self.buckets.lock().expect("chat rate limiter mutex poisoned");
		let bucket = buckets
			.entry((product_id.to_string(), room_id.to_string()))
			.or_insert(ChatRateBucket { milli_tokens: capacity, refilled_at_ms: now_ms });
		let elapsed_ms = now_ms.saturating_sub(bucket.refilled_at_ms);
		bucket.milli_tokens = capacity
			.min(bucket.milli_tokens.saturating_add(elapsed_ms * u64::from(self.per_second)));
		bucket.refilled_at_ms = bucket.refilled_at_ms.max(now_ms);
		if bucket.milli_tokens < 1000 {
			return false;
		}
		bucket.milli_tokens -= 1000;
		true
	}

	/// Drop the budget of a deleted room.
	pub fn forget(&self, product_id: &str, room_id: &str) {
		let mut buckets = self.buckets.lock().expect("chat rate limiter mutex poisoned");
		buckets.remove(&(product_id.to_string(), room_id.to_string()));
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	// The 21st feed update in one second is refused, and the budget comes back
	// at the rate it is spent.
	#[test]
	fn the_rate_limiter_holds_each_room_to_its_budget() {
		let limiter = ChatRateLimiter::new(crate::platform::CHAT_FEED_UPDATES_PER_SECOND);
		let start = 1_000_000;
		for _ in 0..crate::platform::CHAT_FEED_UPDATES_PER_SECOND {
			assert!(limiter.try_acquire("chat.dot", "room", start));
		}
		assert!(!limiter.try_acquire("chat.dot", "room", start));
		// Another room and another product keep their own budgets.
		assert!(limiter.try_acquire("chat.dot", "other", start));
		assert!(limiter.try_acquire("other.dot", "room", start));
		// At 20 per second one token comes back every 50 ms.
		assert!(!limiter.try_acquire("chat.dot", "room", start + 49));
		assert!(limiter.try_acquire("chat.dot", "room", start + 50));
		assert!(!limiter.try_acquire("chat.dot", "room", start + 50));

		let previews = ChatRateLimiter::new(crate::platform::CHAT_PREVIEW_UPDATES_PER_SECOND);
		for _ in 0..crate::platform::CHAT_PREVIEW_UPDATES_PER_SECOND {
			assert!(previews.try_acquire("chat.dot", "room", start));
		}
		assert!(!previews.try_acquire("chat.dot", "room", start));
		previews.forget("chat.dot", "room");
		assert!(previews.try_acquire("chat.dot", "room", start));
	}
}
