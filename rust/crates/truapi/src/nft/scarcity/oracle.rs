//! [`ScarcityTransferOracle`]: proves an NFT transfer landed when its destination
//! holds the instance at the revision the transfer leaves it at.

use std::{collections::HashMap, sync::Arc};

use subxt::utils::H256;

use super::{
	chain::{ScarcityChain, ScarcityNft},
	policy::{TransferAttempt, TransferParams},
};
use crate::{
	chain::{HashAndNumber, Heads, StorageWatchError},
	chain_runtime::RuntimeFailure,
	durable::{CompletionOracle, DurableTxEntry, DurableTxId, HeadKind, LedgerView, PassScope},
	nft::ItemKey,
};

const OPEN_PASS: &str = "scarcity_transfer_oracle";

/// Decides NFT transfers from what their destinations hold. A transfer signs
/// revision N of its instance, and only the key holding it at N, or the
/// collection owner, can move it at N. A transfer that lands leaves the
/// instance at its destination at revision N + 1 with at least one move
/// counted. A failed dispatch leaves it at the source, and the owner's
/// `force_transfer` resets the moves to 0, so the destination holding the
/// instance at N + 1 with moves counted means this transfer landed, whatever
/// the destination held before. Any other revision proves nothing, since the
/// instance may have landed and moved on: the engine's block search decides.
pub struct ScarcityTransferOracle {
	chain: Arc<dyn ScarcityChain>,
	genesis: H256,
}

impl ScarcityTransferOracle {
	/// An oracle reading `chain`, the chain with `genesis`.
	pub fn new(chain: Arc<dyn ScarcityChain>, genesis: H256) -> Self {
		Self { chain, genesis }
	}

	/// Whether each of `transfers` landed by `at`.
	async fn landed_at(
		&self,
		transfers: &[BuiltTransfer],
		at: HashAndNumber,
	) -> Result<HashMap<DurableTxId, bool>, RuntimeFailure> {
		let destinations: Vec<ItemKey> =
			transfers.iter().map(|transfer| transfer.destination).collect();
		let held = self.chain.nfts_at(at.hash, &destinations).await.map_err(runtime_failure)?;
		Ok(transfers
			.iter()
			.zip(held)
			.map(|(transfer, held)| (transfer.id, transfer.left_as(held)))
			.collect())
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
		let transfers: Vec<_> = transactions.iter().filter_map(BuiltTransfer::of).collect();
		let (at_finalized, at_best) = futures::future::try_join(
			self.landed_at(&transfers, heads.finalized),
			self.landed_at(&transfers, heads.best),
		)
		.await?;
		Ok(Box::new(LandedTransfers { at_finalized, at_best }))
	}
}

/// A transfer whose live attempt was built, with the revision it signed.
struct BuiltTransfer {
	id: DurableTxId,
	instance: u64,
	destination: ItemKey,
	signed_state_nonce: u64,
}

impl BuiltTransfer {
	/// The transfer `entry` makes, once an attempt of it was built.
	fn of(entry: &DurableTxEntry) -> Option<Self> {
		let plan = entry.plan.as_ref()?;
		let params = TransferParams::try_from(&plan.params).ok()?;
		let TransferAttempt { instance, state_nonce: signed_state_nonce } =
			params.submitted_attempt?;
		Some(Self { id: entry.id, instance, destination: params.destination, signed_state_nonce })
	}

	/// Whether `held`, what the destination holds, is the instance as this
	/// transfer leaves it: one revision on, with moves counted.
	fn left_as(&self, held: Option<ScarcityNft>) -> bool {
		let next_revision = self.signed_state_nonce.checked_add(1);
		held.is_some_and(|nft| {
			nft.instance == self.instance && Some(nft.state_nonce) == next_revision && nft.moves > 0
		})
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

fn runtime_failure(error: StorageWatchError) -> RuntimeFailure {
	match error {
		StorageWatchError::Chain(failure) => failure,
		other => RuntimeFailure::host_failure(OPEN_PASS, other.to_string()),
	}
}

#[cfg(test)]
mod tests;
