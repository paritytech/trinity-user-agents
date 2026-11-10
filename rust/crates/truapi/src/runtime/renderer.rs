//! Connection-scoped Renderer policy shared by product and native entrypoints.

use crate::host_core::ProductRuntimeError;

/// Only a Worker execution draws bodies or receives their actions. No session
/// and no native adapter are required, so a signed-out host still renders.
pub fn renderer_access_for(
	execution_kind: crate::platform::ProductExecutionKind,
) -> Result<(), ProductRuntimeError> {
	if execution_kind != crate::platform::ProductExecutionKind::Worker {
		return Err(ProductRuntimeError::Denied);
	}
	Ok(())
}
