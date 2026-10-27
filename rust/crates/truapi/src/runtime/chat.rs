//! Connection-scoped Chat streams shared by product and native entrypoints.

use std::sync::Arc;

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
	pub feed: crate::platform::ChatRateLimiter,
	/// `set_room_preview` calls.
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
