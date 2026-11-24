//! [`MoveBudget`]: the fee-less moves an NFT gets between paid moves, and
//! [`ScarcityMoveBudget`], which reads it from the runtime.

use std::sync::Arc;

use parity_scale_codec::Decode;
use subxt::{Metadata, utils::H256};

use crate::{
	chain::{ChainHeads, StorageWatchError, WatchedStorage},
	chain_runtime::RuntimeFailure,
};

/// `Scarcity::MaximumMoves`: a fee-less move is valid only while the NFT
/// used fewer moves than this since its last paid move.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MoveBudget {
	maximum_moves: u16,
}

impl MoveBudget {
	/// A budget of `maximum_moves` fee-less moves.
	pub fn new(maximum_moves: u16) -> Self {
		Self { maximum_moves }
	}

	/// Whether an NFT that used `moves` fee-less moves has none left.
	pub fn is_exhausted(&self, moves: u16) -> bool {
		moves >= self.maximum_moves
	}
}

/// Where the chain's move budget is read now. A trait because the live test
/// lowers the budget to move a fresh NFT paid; production has one
/// implementor, [`ScarcityMoveBudget`].
#[async_trait::async_trait]
pub trait MoveBudgetSource: Send + Sync {
	/// The budget the runtime at the best head declares.
	async fn current_budget(&self) -> Result<MoveBudget, StorageWatchError>;
}

/// Reads `Scarcity::MaximumMoves` from the runtime metadata at the best head
/// of the chain that runs the pallet.
pub struct ScarcityMoveBudget {
	storage: Arc<dyn WatchedStorage>,
	heads: Arc<dyn ChainHeads>,
	genesis: H256,
}

impl ScarcityMoveBudget {
	/// Reads the budget through `storage` and `heads` of the chain with
	/// `genesis`.
	pub fn new(
		storage: Arc<dyn WatchedStorage>,
		heads: Arc<dyn ChainHeads>,
		genesis: H256,
	) -> Self {
		Self { storage, heads, genesis }
	}
}

#[async_trait::async_trait]
impl MoveBudgetSource for ScarcityMoveBudget {
	async fn current_budget(&self) -> Result<MoveBudget, StorageWatchError> {
		let heads = self.heads.heads(self.genesis).await?;
		let client = self.storage.client_at(self.genesis, heads.best).await?;
		let maximum_moves = maximum_moves_in(&client.metadata())?;
		Ok(MoveBudget::new(maximum_moves))
	}
}

/// `Scarcity::MaximumMoves` as `metadata` declares it.
fn maximum_moves_in(metadata: &Metadata) -> Result<u16, StorageWatchError> {
	let constant = metadata
		.pallet_by_name("Scarcity")
		.and_then(|pallet| pallet.constant_by_name("MaximumMoves"))
		.ok_or_else(|| unreadable_budget("the metadata has no Scarcity::MaximumMoves".into()))?;
	u16::decode(&mut constant.value())
		.map_err(|error| unreadable_budget(format!("Scarcity::MaximumMoves is not a u16: {error}")))
}

/// The error for a runtime whose move budget cannot be read.
fn unreadable_budget(reason: String) -> StorageWatchError {
	StorageWatchError::Chain(RuntimeFailure::host_failure("scarcity_move_budget", reason))
}

#[cfg(test)]
mod tests;
