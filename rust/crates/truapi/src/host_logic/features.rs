//! Feature-detection delegation.
//!
//! `feature_supported` is a platform syscall: each host owns the set of chains
//! it can service. This module forwards it to [`crate::platform::Features`],
//! except for the materials and touch questions, which the core answers
//! itself.
//! `get_chain_info` is answered by the core from its [`crate::platform::Stack`]
//! instead.

use crate::platform::Features;
use truapi::latest::{GenericError, HostFeatureSupportedRequest, HostFeatureSupportedResponse};

/// Forward a feature-support query to the platform implementation, except
/// the materials question, which `draws_materials` answers from the core,
/// and the touch question, which `draws_touch` answers.
pub async fn feature_supported<P: Features + ?Sized>(
	platform: &P,
	draws_materials: impl FnOnce(u32, &[String]) -> bool,
	draws_touch: bool,
	request: HostFeatureSupportedRequest,
) -> Result<HostFeatureSupportedResponse, GenericError> {
	match request {
		HostFeatureSupportedRequest::Materials { format_version, features } => {
			Ok(HostFeatureSupportedResponse {
				supported: draws_materials(format_version, &features),
			})
		},
		HostFeatureSupportedRequest::RendererTouch => {
			Ok(HostFeatureSupportedResponse { supported: draws_touch })
		},
		request => platform.feature_supported(request).await,
	}
}

#[cfg(test)]
mod tests;
