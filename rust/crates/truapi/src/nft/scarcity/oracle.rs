//! [`ScarcityTransferOracle`]: proves an NFT transfer landed when its instance is
//! at its destination.

use std::{collections::HashMap, sync::Arc};

use subxt::utils::H256;

use super::{chain::ScarcityChain, policy::TransferParams};
use crate::{
	chain::{HashAndNumber, Heads, StorageWatchError},
	chain_runtime::RuntimeFailure,
	durable::{CompletionOracle, DurableTxEntry, DurableTxId, HeadKind, LedgerView, PassScope},
	nft::ItemKey,
};

const OPEN_PASS: &str = "scarcity_transfer_oracle";

/// Decides NFT transfers from where their instances are. Only the key that holds
/// an instance, or the collection owner, can transfer it, so the instance at a
/// transfer's destination means the transfer landed. An instance anywhere else
/// proves nothing, since it may have landed and transferred on: the engine's block
/// search decides those.
pub struct ScarcityTransferOracle {
	chain: Arc<dyn ScarcityChain>,
	genesis: H256,
}

impl ScarcityTransferOracle {
	/// An oracle reading `chain`, the chain with `genesis`.
	pub fn new(chain: Arc<dyn ScarcityChain>, genesis: H256) -> Self {
		Self { chain, genesis }
	}

	/// Whether each transfer's instance is at its destination at `at`.
	async fn landed_at(
		&self,
		transfers: &[(DurableTxId, TransferParams)],
		at: HashAndNumber,
	) -> Result<HashMap<DurableTxId, bool>, RuntimeFailure> {
		let instances: Vec<u64> = transfers.iter().map(|(_, params)| params.instance).collect();
		let holders = self.chain.holders_at(at.hash, &instances).await.map_err(runtime_failure)?;
		let landed = |((id, params), holder): (&(DurableTxId, TransferParams), Option<ItemKey>)| {
			(*id, holder == Some(params.destination))
		};
		Ok(transfers.iter().zip(holders).map(landed).collect())
	}
}

#[async_trait::async_trait]
impl CompletionOracle for ScarcityTransferOracle {
	fn chain(&self) -> H256 {
		self.genesis
	}

	async fn open_pass(
		&self,
		transactions: &[DurableTxEntry],
		_ledger: &LedgerView,
		heads: &Heads,
	) -> Result<Box<dyn PassScope>, RuntimeFailure> {
		let transfers: Vec<_> = transactions.iter().filter_map(transfer_of).collect();
		let (at_finalized, at_best) = futures::future::try_join(
			self.landed_at(&transfers, heads.finalized),
			self.landed_at(&transfers, heads.best),
		)
		.await?;
		Ok(Box::new(LandedTransfers { at_finalized, at_best }))
	}
}

/// Which transfers landed at each head of one pass.
struct LandedTransfers {
	at_finalized: HashMap<DurableTxId, bool>,
	at_best: HashMap<DurableTxId, bool>,
}

impl PassScope for LandedTransfers {
	fn proven_completed(&self, tx: &DurableTxEntry, head: HeadKind) -> bool {
		let landed = match head {
			HeadKind::Finalized => &self.at_finalized,
			HeadKind::Best => &self.at_best,
		};
		landed.get(&tx.id).copied().unwrap_or(false)
	}
}

/// The transfer `entry` makes, when its plan names one.
fn transfer_of(entry: &DurableTxEntry) -> Option<(DurableTxId, TransferParams)> {
	let plan = entry.plan.as_ref()?;
	TransferParams::try_from(&plan.params).ok().map(|params| (entry.id, params))
}

fn runtime_failure(error: StorageWatchError) -> RuntimeFailure {
	match error {
		StorageWatchError::Chain(failure) => failure,
		other => RuntimeFailure::host_failure(OPEN_PASS, other.to_string()),
	}
}

#[cfg(test)]
mod tests;
