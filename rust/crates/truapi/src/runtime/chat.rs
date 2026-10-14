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
