//! [`ScarcityMoveOracle`]: proves an NFT move landed when its instance is
//! at its destination.

use std::{collections::HashMap, sync::Arc};

use subxt::utils::H256;

use super::{chain::ScarcityChain, policy::MoveParams};
use crate::{
	chain::{HashAndNumber, Heads, StorageWatchError},
	chain_runtime::RuntimeFailure,
	durable::{CompletionOracle, DurableTxEntry, DurableTxId, HeadKind, LedgerView, PassScope},
	nft::ItemKey,
};

const OPEN_PASS: &str = "scarcity_move_oracle";

/// Decides NFT moves from where their instances are. Only the key that holds
/// an instance, or the collection owner, can move it, so the instance at a
/// move's destination means the move landed. An instance anywhere else
/// proves nothing, since it may have landed and moved on: the engine's block
/// search decides those.
pub struct ScarcityMoveOracle {
	chain: Arc<dyn ScarcityChain>,
	genesis: H256,
}

impl ScarcityMoveOracle {
	/// An oracle reading `chain`, the chain with `genesis`.
	pub fn new(chain: Arc<dyn ScarcityChain>, genesis: H256) -> Self {
		Self { chain, genesis }
	}

	/// Whether each move's instance is at its destination at `at`.
	async fn landed_at(
		&self,
		moves: &[(DurableTxId, MoveParams)],
		at: HashAndNumber,
	) -> Result<HashMap<DurableTxId, bool>, RuntimeFailure> {
		let instances: Vec<u64> = moves.iter().map(|(_, params)| params.instance).collect();
		let holders = self.chain.holders_at(at.hash, &instances).await.map_err(runtime_failure)?;
		let landed = |((id, params), holder): (&(DurableTxId, MoveParams), Option<ItemKey>)| {
			(*id, holder == Some(params.destination))
		};
		Ok(moves.iter().zip(holders).map(landed).collect())
	}
}

#[async_trait::async_trait]
impl CompletionOracle for ScarcityMoveOracle {
	fn chain(&self) -> H256 {
		self.genesis
	}

	async fn open_pass(
		&self,
		transactions: &[DurableTxEntry],
		_ledger: &LedgerView,
		heads: &Heads,
	) -> Result<Box<dyn PassScope>, RuntimeFailure> {
		let moves: Vec<_> = transactions.iter().filter_map(move_of).collect();
		let (at_finalized, at_best) = futures::future::try_join(
			self.landed_at(&moves, heads.finalized),
			self.landed_at(&moves, heads.best),
		)
		.await?;
		Ok(Box::new(LandedMoves { at_finalized, at_best }))
	}
}

/// Which moves landed at each head of one pass.
struct LandedMoves {
	at_finalized: HashMap<DurableTxId, bool>,
	at_best: HashMap<DurableTxId, bool>,
}

impl PassScope for LandedMoves {
	fn proven_completed(&self, tx: &DurableTxEntry, head: HeadKind) -> bool {
		let landed = match head {
			HeadKind::Finalized => &self.at_finalized,
			HeadKind::Best => &self.at_best,
		};
		landed.get(&tx.id).copied().unwrap_or(false)
	}
}

/// The move `entry` makes, when its plan names one.
fn move_of(entry: &DurableTxEntry) -> Option<(DurableTxId, MoveParams)> {
	let plan = entry.plan.as_ref()?;
	MoveParams::try_from(&plan.params).ok().map(|params| (entry.id, params))
}

fn runtime_failure(error: StorageWatchError) -> RuntimeFailure {
	match error {
		StorageWatchError::Chain(failure) => failure,
		other => RuntimeFailure::host_failure(OPEN_PASS, other.to_string()),
	}
}

#[cfg(test)]
mod tests;
