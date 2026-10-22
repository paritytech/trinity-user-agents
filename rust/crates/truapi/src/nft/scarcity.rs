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

pub use policy::{TRANSFER_POLICY, TRANSFER_WAIT_LIMIT_SECS, TransferParams};
pub use tracker::{HoldingTracker, HoldingTrackerDeps};

use self::{
	chain::ScarcityStorage,
	oracle::ScarcityTransferOracle,
	policy::{ScarcityTransferPolicy, ScarcityTransferPolicyDeps},
};
use super::{ItemSigner, operation::nft_domain};
use crate::{
	chain::{ChainHeads, WatchedStorage},
	durable::DurableRegistry,
	store::Db,
};

/// What NFT transfers need to be decided and built.
pub struct ScarcityTransfersDeps {
	/// The core database, for the key each transfer spends.
	pub db: Db,
	/// Storage of the chain that runs the pallet.
	pub storage: Arc<dyn WatchedStorage>,
	/// Heads of that chain.
	pub heads: Arc<dyn ChainHeads>,
	/// Genesis of that chain.
	pub genesis: H256,
	/// Signs with item keys.
	pub signer: Arc<dyn ItemSigner>,
}

/// `registry` with the NFT domain, whose transfers the Scarcity oracle decides
/// and the transfer policy builds.
pub fn with_scarcity_transfers(
	registry: DurableRegistry,
	deps: ScarcityTransfersDeps,
) -> DurableRegistry {
	let ScarcityTransfersDeps { db, storage, heads, genesis, signer } = deps;
	let chain = Arc::new(ScarcityStorage::new(storage, heads, genesis));
	let oracle = Arc::new(ScarcityTransferOracle::new(chain.clone(), genesis));
	let policy = ScarcityTransferPolicy::new(ScarcityTransferPolicyDeps { db, chain, signer });
	registry.with_domain(nft_domain(), oracle).with_policy(Arc::new(policy))
}
