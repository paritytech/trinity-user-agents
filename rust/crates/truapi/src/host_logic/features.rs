//! Feature-detection delegation.
//!
//! `feature_supported` and `supported_chains` are platform syscalls: each
//! host owns the set of chains it can service. This module forwards through
//! to [`crate::platform::Features`], except for the materials question, which
//! the core answers from what it reads and whether the host draws materials,
//! plus the in-core
//! RFC-0026 resolution that answers `get_chain_info` from the host's chain
//! set so per-request semantics (ordering, `NotSupported`) stay core-owned.

use crate::platform::{Features, HostChainSet};
use truapi::{
	latest::{
		ChainIdentifier, RemoteChainInfoError, RemoteChainInfoRequest, RemoteChainInfoResponse,
	},
	v01::{GenericError, HostFeatureSupportedRequest, HostFeatureSupportedResponse},
};

/// Forward a feature-support query to the platform implementation, except
/// the materials question, which `draws_materials` answers from the core.
pub async fn feature_supported<P: Features + ?Sized>(
	platform: &P,
	draws_materials: impl FnOnce(u32, &[String]) -> bool,
	request: HostFeatureSupportedRequest,
) -> Result<HostFeatureSupportedResponse, GenericError> {
	match request {
		HostFeatureSupportedRequest::Materials { format_version, features } => {
			Ok(HostFeatureSupportedResponse {
				supported: draws_materials(format_version, &features),
			})
		},
		request => platform.feature_supported(request).await,
	}
}

/// Fetch the host's chain set from the platform implementation.
pub async fn supported_chains<P: Features + ?Sized>(
	platform: &P,
) -> Result<HostChainSet, GenericError> {
	platform.supported_chains().await
}

/// The genesis hash the host serves for `chain`, or `None` when it serves no
/// such chain.
///
/// Runtime code that needs to talk to a chain by role resolves it here rather
/// than carrying a configured hash, so a chain the host does not serve is a
/// missing entry instead of a connection to the wrong chain.
pub fn genesis_for(set: &HostChainSet, chain: ChainIdentifier) -> Option<[u8; 32]> {
	set.chains
		.iter()
		.find(|entry| entry.identifier == chain)
		.map(|entry| entry.genesis_hash)
}

/// Resolve a `get_chain_info` request against the host's chain set: the
/// requested identifier's genesis hash plus the host's network, echoing the
/// identifier, or `NotSupported` when the host does not serve it.
pub fn chain_info(
	set: &HostChainSet,
	request: &RemoteChainInfoRequest,
) -> Result<RemoteChainInfoResponse, RemoteChainInfoError> {
	genesis_for(set, request.chain)
		.map(|genesis_hash| RemoteChainInfoResponse {
			network: set.network.clone(),
			chain: request.chain,
			genesis_hash,
		})
		.ok_or(RemoteChainInfoError::NotSupported)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod genesis_lookup_tests {
	use super::*;
	use crate::platform::HostChainEntry;

	fn set() -> HostChainSet {
		HostChainSet {
			network: "paseo".to_string(),
			chains: vec![HostChainEntry {
				identifier: ChainIdentifier::AssetHub,
				genesis_hash: [0xab; 32],
			}],
		}
	}

	/// A chain the host does not serve is absent, not a different chain's hash.
	#[test]
	fn only_served_chains_resolve() {
		assert_eq!(genesis_for(&set(), ChainIdentifier::AssetHub), Some([0xab; 32]));
		assert_eq!(genesis_for(&set(), ChainIdentifier::Bulletin), None);
	}

	/// `chain_info` answers from the same lookup, so the product-facing method and
	/// in-core callers cannot disagree about what the host serves.
	#[test]
	fn chain_info_agrees_with_the_lookup() {
		let set = set();
		for chain in [ChainIdentifier::AssetHub, ChainIdentifier::People] {
			let request = RemoteChainInfoRequest { chain };
			assert_eq!(
				chain_info(&set, &request).ok().map(|info| info.genesis_hash),
				genesis_for(&set, chain),
			);
		}
	}
}
