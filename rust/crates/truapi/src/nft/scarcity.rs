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

pub use policy::{MOVE_POLICY, MoveParams};
pub use tracker::{HoldingTracker, HoldingTrackerDeps};

use self::{
	chain::ScarcityStorage,
	oracle::ScarcityMoveOracle,
	policy::{ScarcityMovePolicy, ScarcityMovePolicyDeps},
};
use super::{ItemSigner, operation::nft_domain};
use crate::{chain::WatchedStorage, durable::DurableRegistry, store::Db};

/// What NFT moves need to be decided and built.
pub struct ScarcityMovesDeps {
	/// The core database, for the key each move spends.
	pub db: Db,
	/// Storage of the chain that runs the pallet.
	pub storage: Arc<dyn WatchedStorage>,
	/// Genesis of that chain.
	pub genesis: H256,
	/// Signs with item keys.
	pub signer: Arc<dyn ItemSigner>,
}

/// `registry` with the NFT domain, whose moves the Scarcity oracle decides
/// and the move policy builds.
pub fn with_scarcity_moves(registry: DurableRegistry, deps: ScarcityMovesDeps) -> DurableRegistry {
	let ScarcityMovesDeps { db, storage, genesis, signer } = deps;
	let chain = Arc::new(ScarcityStorage::new(storage, genesis));
	let oracle = Arc::new(ScarcityMoveOracle::new(chain.clone(), genesis));
	let policy = ScarcityMovePolicy::new(ScarcityMovePolicyDeps { db, chain, signer });
	registry.with_domain(nft_domain(), oracle).with_policy(Arc::new(policy))
}
