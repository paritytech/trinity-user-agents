//! Unified [`Share`] trait.

use crate::{
	CallContext, CallError,
	versioned::share::{HostShareError, HostShareRequest, HostShareResponse},
	wasm_env, wire, wire_trait,
};

/// Text shared through the host's own share sheet.
///
/// The host draws the sheet and the user chooses where the text goes: another
/// application, a chat, the clipboard. The product gets back only whether the
/// user shared it, never where.
#[wasm_env]
#[wire_trait(id = 26)]
#[crate::async_trait]
pub trait Share: Send + Sync {
	/// Ask the host to let the user share one text.
	///
	/// A host with no share sheet answers `Unsupported`, and cancelling the
	/// call closes the sheet.
	///
	/// ```ts
	/// const result = await truapi.share.share({
	///   text: "Add me: https://greenmarket.dot.li/r/42",
	/// });
	/// assert(result.isOk(), "share.share failed:", result);
	/// console.log("share ended:", result.value.outcome);
	/// ```
	#[wire(id = 0)]
	async fn share(
		&self,
		_cx: &CallContext,
		_request: HostShareRequest,
	) -> Result<HostShareResponse, CallError<HostShareError>> {
		Err(CallError::unavailable())
	}
}
