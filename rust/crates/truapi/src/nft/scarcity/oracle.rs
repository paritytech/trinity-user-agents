//! [`ScarcityTransferOracle`]: proves an NFT transfer landed when its destination
//! holds the instance at the revision the transfer leaves it at.

use std::{collections::HashMap, sync::Arc};

use subxt::utils::H256;

use super::{
	chain::{ScarcityChain, ScarcityNft},
	policy::TransferParams,
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
		let (instance, signed_state_nonce) = params.instance.zip(params.signed_state_nonce)?;
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
mod tests {
	use futures::executor::block_on;

	use super::*;
	use crate::{
		chain::Mortality,
		durable::{DomainId, DurableTxStatus, PolicyId, SubmissionPlan},
		nft::{
			ItemSlot, PurseId, TEST_INSTALLATION,
			keys::ItemIndex,
			scarcity::{
				chain::{KeyLocks, TransferContext},
				policy::{TRANSFER_POLICY, TransferSigner},
			},
		},
	};

	const FINALIZED: HashAndNumber = HashAndNumber { hash: H256([0x0f; 32]), number: 130 };
	const BEST: HashAndNumber = HashAndNumber { hash: H256([0x0b; 32]), number: 140 };
	const SOURCE: ItemKey = ItemKey([0x05; 32]);
	const DESTINATION: ItemKey = ItemKey([0x08; 32]);
	const INSTANCE: u64 = 330;
	/// The revision the transfer's attempt signed.
	const SIGNED: u64 = 7;

	/// What [`DESTINATION`] holds at each head.
	struct DestinationHolds {
		at_finalized: Option<ScarcityNft>,
		at_best: Option<ScarcityNft>,
	}

	#[async_trait::async_trait]
	impl ScarcityChain for DestinationHolds {
		async fn nfts_at(
			&self,
			at: H256,
			keys: &[ItemKey],
		) -> Result<Vec<Option<ScarcityNft>>, StorageWatchError> {
			let held = if at == FINALIZED.hash { self.at_finalized } else { self.at_best };
			Ok(keys.iter().map(|key| held.filter(|_| *key == DESTINATION)).collect())
		}

		async fn locks_at(&self, _: H256, _: &[ItemKey]) -> Result<KeyLocks, StorageWatchError> {
			unreachable!("the oracle only reads what destinations hold")
		}

		async fn transfer_context(&self) -> Result<TransferContext, StorageWatchError> {
			unreachable!("the oracle only reads what destinations hold")
		}
	}

	/// [`INSTANCE`] at `state_nonce` after `moves` moves.
	fn instance_at(state_nonce: u64, moves: u16) -> ScarcityNft {
		ScarcityNft {
			instance: INSTANCE,
			collection: 1,
			item: 1,
			minted_at: 0,
			last_moved: 0,
			state_nonce,
			moves,
		}
	}

	/// The instance as a landed transfer leaves it.
	fn landed() -> Option<ScarcityNft> {
		Some(instance_at(SIGNED + 1, 1))
	}

	/// A transfer of [`INSTANCE`] to [`DESTINATION`] whose attempt signed
	/// `signed_state_nonce`, or none was built yet.
	fn transfer_entry(signed_state_nonce: Option<u64>) -> DurableTxEntry {
		let params = TransferParams {
			source: SOURCE,
			signer: TransferSigner::Derived(ItemSlot {
				purse: PurseId(0),
				installation: TEST_INSTALLATION,
				index: ItemIndex(0),
			}),
			instance: Some(INSTANCE),
			destination: DESTINATION,
			signed_state_nonce,
			give_up_after: 0,
		};
		DurableTxEntry {
			id: DurableTxId(1),
			domain: DomainId::new("nft"),
			group: None,
			tx_hash: H256([0x01; 32]),
			mortality: Mortality::new(FINALIZED, 64).unwrap(),
			status: DurableTxStatus::Pending,
			scanned_to: None,
			plan: Some(SubmissionPlan {
				policy: PolicyId::new(TRANSFER_POLICY),
				params: params.into(),
			}),
		}
	}

	/// Whether `entry` is proven at the finalized head and at the best head.
	fn proven_at_both_heads(entry: DurableTxEntry, holds: DestinationHolds) -> (bool, bool) {
		let oracle = ScarcityTransferOracle::new(Arc::new(holds), H256([0xab; 32]));
		let heads = Heads { finalized: FINALIZED, best: BEST };
		let view = LedgerView::new(vec![entry.clone()]);
		let scope =
			block_on(oracle.open_pass(core::slice::from_ref(&entry), &view, &heads)).unwrap();
		let at_finalized = scope.proven_completed(&entry, HeadKind::Finalized);
		(at_finalized, scope.proven_completed(&entry, HeadKind::Best))
	}

	/// A destination need not be fresh: its revision proves the transfer, so
	/// a transfer to an outside key is decided at the best head too.
	#[test]
	fn a_transfer_is_proven_at_each_head_where_the_destination_holds_the_next_revision() {
		let landed_at_best = DestinationHolds { at_finalized: None, at_best: landed() };
		let finalized = DestinationHolds { at_finalized: landed(), at_best: landed() };

		assert_eq!(
			proven_at_both_heads(transfer_entry(Some(SIGNED)), landed_at_best),
			(false, true)
		);
		assert_eq!(proven_at_both_heads(transfer_entry(Some(SIGNED)), finalized), (true, true));
	}

	/// The destination may have held the instance before, or got it back after
	/// it moved on: any revision but the next one proves nothing.
	#[test]
	fn the_instance_at_another_revision_proves_nothing() {
		let held_before = Some(instance_at(SIGNED, 1));
		let moved_on_and_back = Some(instance_at(SIGNED + 3, 4));

		for held in [held_before, moved_on_and_back] {
			let holds = DestinationHolds { at_finalized: held, at_best: held };
			assert_eq!(proven_at_both_heads(transfer_entry(Some(SIGNED)), holds), (false, false));
		}
	}

	/// The collection owner can force the instance to any key at the next
	/// revision too, but that resets its moves.
	#[test]
	fn a_forced_move_to_the_destination_proves_nothing() {
		let forced = Some(instance_at(SIGNED + 1, 0));
		let holds = DestinationHolds { at_finalized: forced, at_best: forced };

		assert_eq!(proven_at_both_heads(transfer_entry(Some(SIGNED)), holds), (false, false));
	}

	/// Until an attempt is built there is no revision to check against.
	#[test]
	fn a_transfer_never_built_proves_nothing() {
		let holds = DestinationHolds { at_finalized: landed(), at_best: landed() };

		assert_eq!(proven_at_both_heads(transfer_entry(None), holds), (false, false));
	}
}
