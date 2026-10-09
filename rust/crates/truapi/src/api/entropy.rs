//! Unified [`Entropy`] trait.

use crate::{
	CallContext, CallError,
	versioned::entropy::{
		HostDeriveEntropyError, HostDeriveEntropyRequest, HostDeriveEntropyResponse,
	},
	wire, wire_trait,
};

/// Deterministic entropy derivation.
#[wire_trait(id = 6)]
#[crate::async_trait]
pub trait Entropy: Send + Sync {
	/// Derive deterministic entropy.
	///
	/// ```ts
	/// const result = await truapi.entropy.derive({
	///   context: "0x70726f647563742d6b6579",
	/// });
	/// assert(result.isOk(), "derive failed:", result);
	/// console.log("entropy derived:", result.value);
	/// ```
	#[wire(id = 0)]
	async fn derive(
		&self,
		_cx: &CallContext,
		_request: HostDeriveEntropyRequest,
	) -> Result<HostDeriveEntropyResponse, CallError<HostDeriveEntropyError>> {
		Err(CallError::unavailable())
	}
}
