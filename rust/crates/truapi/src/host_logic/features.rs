//! Feature-detection delegation.
//!
//! `feature_supported` is a platform syscall: each host owns the set of chains
//! it can service. This module is a thin shim that forwards through to
//! [`crate::platform::Features`]. `get_chain_info` is answered by the core
//! from its [`crate::platform::Stack`] instead.

use crate::platform::Features;
use truapi::latest::{GenericError, HostFeatureSupportedRequest, HostFeatureSupportedResponse};

/// Forward a feature-support query to the platform implementation.
pub async fn feature_supported<P: Features + ?Sized>(
	platform: &P,
	request: HostFeatureSupportedRequest,
) -> Result<HostFeatureSupportedResponse, GenericError> {
	platform.feature_supported(request).await
}

#[cfg(test)]
mod tests;
