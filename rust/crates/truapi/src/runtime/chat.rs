//! Connection-scoped Chat streams shared by product and native entrypoints.

#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
use std::{
	collections::HashMap,
	sync::{Arc, Mutex},
};

#[cfg(target_arch = "wasm32")]
use web_time::Instant;

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

/// A token bucket for each `(product, room)` pair. Each pair can make
/// `per_second` calls in one second, and it gets its tokens again at the same
/// rate.
pub struct ChatRateLimiter {
	per_second: u32,
	buckets: Mutex<HashMap<(String, String), ChatRateBucket>>,
}

struct ChatRateBucket {
	/// Tokens left, in thousandths of a token.
	milli_tokens: u64,
	/// The time of the last refill, on a clock that never moves back.
	refilled_at: Instant,
}

impl ChatRateLimiter {
	/// A limiter that accepts `per_second` calls for each room in one second.
	pub fn new(per_second: u32) -> Self {
		Self { per_second, buckets: Default::default() }
	}

	/// Take one token for `room_id` of `product_id` at `now`. Returns `false`
	/// when the room used all of its budget.
	pub fn try_acquire(&self, product_id: &str, room_id: &str, now: Instant) -> bool {
		let capacity = u64::from(self.per_second) * 1000;
		let mut buckets = self.buckets.lock().expect("chat rate limiter mutex poisoned");
		let bucket = buckets
			.entry((product_id.to_string(), room_id.to_string()))
			.or_insert(ChatRateBucket { milli_tokens: capacity, refilled_at: now });
		let elapsed_ms =
			u64::try_from(now.saturating_duration_since(bucket.refilled_at).as_millis())
				.unwrap_or(u64::MAX);
		bucket.milli_tokens = capacity.min(
			bucket
				.milli_tokens
				.saturating_add(elapsed_ms.saturating_mul(u64::from(self.per_second))),
		);
		bucket.refilled_at = bucket.refilled_at.max(now);
		if bucket.milli_tokens < 1000 {
			return false;
		}
		bucket.milli_tokens -= 1000;
		true
	}

	/// Remove the budget of a deleted room.
	pub fn forget(&self, product_id: &str, room_id: &str) {
		let mut buckets = self.buckets.lock().expect("chat rate limiter mutex poisoned");
		buckets.remove(&(product_id.to_string(), room_id.to_string()));
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	// The limiter refuses the 21st feed update in one second. The budget fills
	// again at the rate that the product spends it.
	#[test]
	fn the_rate_limiter_holds_each_room_to_its_budget() {
		let limiter = ChatRateLimiter::new(crate::platform::CHAT_FEED_UPDATES_PER_SECOND);
		let start = Instant::now();
		let after = |milliseconds| start + core::time::Duration::from_millis(milliseconds);
		for _ in 0..crate::platform::CHAT_FEED_UPDATES_PER_SECOND {
			assert!(limiter.try_acquire("chat.dot", "room", start));
		}
		assert!(!limiter.try_acquire("chat.dot", "room", start));
		// Another room and another product keep their own budgets.
		assert!(limiter.try_acquire("chat.dot", "other", start));
		assert!(limiter.try_acquire("other.dot", "room", start));
		// At 20 calls per second, the room gets one token again every 50 ms.
		assert!(!limiter.try_acquire("chat.dot", "room", after(49)));
		assert!(limiter.try_acquire("chat.dot", "room", after(50)));
		assert!(!limiter.try_acquire("chat.dot", "room", after(50)));

		let previews = ChatRateLimiter::new(crate::platform::CHAT_PREVIEW_UPDATES_PER_SECOND);
		for _ in 0..crate::platform::CHAT_PREVIEW_UPDATES_PER_SECOND {
			assert!(previews.try_acquire("chat.dot", "room", start));
		}
		assert!(!previews.try_acquire("chat.dot", "room", start));
		previews.forget("chat.dot", "room");
		assert!(previews.try_acquire("chat.dot", "room", start));
	}
}
