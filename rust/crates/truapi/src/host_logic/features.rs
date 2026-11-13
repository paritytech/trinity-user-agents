//! Feature-detection delegation.
//!
//! `feature_supported` is a platform syscall: each host owns the set of chains
//! it can service. This module forwards it to [`crate::platform::Features`],
//! except for the materials question, which the core answers itself.
//! `get_chain_info` is answered by the core from its [`crate::platform::Stack`]
//! instead.

use crate::platform::Features;
use truapi::latest::{GenericError, HostFeatureSupportedRequest, HostFeatureSupportedResponse};

/// Forward a feature-support query to the platform implementation. The core
/// draws no materials yet, so it answers that question itself.
pub async fn feature_supported<P: Features + ?Sized>(
	platform: &P,
	request: HostFeatureSupportedRequest,
) -> Result<HostFeatureSupportedResponse, GenericError> {
	match request {
		HostFeatureSupportedRequest::Materials { .. } => {
			Ok(HostFeatureSupportedResponse { supported: false })
		},
		request => platform.feature_supported(request).await,
	}
}

#[cfg(test)]
mod tests;
