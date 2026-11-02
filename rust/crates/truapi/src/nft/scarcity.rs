//! What the Scarcity pallet holds for each item key. Everything that knows
//! the pallet's storage layout stays in here.

mod chain;
mod oracle;
mod policy;
#[cfg(test)]
mod testing;
mod tracker;
mod transfer;

use std::sync::Arc;

use subxt::utils::H256;

pub use policy::{TRANSFER_POLICY, TRANSFER_WAIT_LIMIT_SECS, TransferParams, TransferSigner};
#[cfg(test)]
pub use testing::ASSET_HUB_PIPELINE;
pub use tracker::{HoldingTracker, HoldingTrackerDeps};

use self::{
	chain::ScarcityStorage,
	oracle::ScarcityTransferOracle,
	policy::{ScarcityTransferPolicy, ScarcityTransferPolicyDeps},
};
use super::{ImportedSecretStore, ItemSigner, operation::nft_domain};
use crate::{
	chain::{ChainHeads, ExtrinsicService, WatchedStorage},
	durable::DurableRegistry,
};

/// What NFT transfers need to be decided and built.
pub struct ScarcityTransfersDeps {
	/// Storage of the chain that runs the pallet.
	pub storage: Arc<dyn WatchedStorage>,
	/// Heads of that chain.
	pub heads: Arc<dyn ChainHeads>,
	/// Genesis of that chain.
	pub genesis: H256,
	/// Signs with the item keys the host derives.
	pub signer: Arc<dyn ItemSigner>,
	/// Holds the secrets of imported item keys.
	pub imported: ImportedSecretStore,
	/// Builds the transfers.
	pub extrinsics: Arc<ExtrinsicService>,
}

/// `registry` with the NFT domain, whose transfers the Scarcity oracle decides
/// and the transfer policy builds.
pub fn with_scarcity_transfers(
	registry: DurableRegistry,
	deps: ScarcityTransfersDeps,
) -> DurableRegistry {
	let ScarcityTransfersDeps { storage, heads, genesis, signer, imported, extrinsics } = deps;
	let chain = Arc::new(ScarcityStorage::new(storage, heads, genesis));
	let oracle = Arc::new(ScarcityTransferOracle::new(chain.clone(), genesis));
	let policy_deps = ScarcityTransferPolicyDeps { chain, signer, imported, extrinsics, genesis };
	let policy = ScarcityTransferPolicy::new(policy_deps);
	registry.with_domain(nft_domain(), oracle).with_policy(Arc::new(policy))
}
