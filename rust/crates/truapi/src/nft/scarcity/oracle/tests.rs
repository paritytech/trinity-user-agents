use futures::executor::block_on;
use subxt::utils::H256;

use super::*;
use crate::{
	chain::Mortality,
	durable::{DomainId, DurableTxStatus, SubmissionPlan},
	nft::{
		ItemSlot, PurseId, TEST_INSTALLATION,
		keys::ItemIndex,
		scarcity::{
			chain::{KeyLocks, TransferContext},
			policy::TransferSigner,
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
	let submitted_attempt =
		signed_state_nonce.map(|state_nonce| TransferAttempt { instance: INSTANCE, state_nonce });
	let params = TransferParams {
		source: SOURCE,
		signer: TransferSigner::Derived(ItemSlot {
			purse: PurseId(0),
			installation: TEST_INSTALLATION,
			index: ItemIndex(0),
		}),
		destination: DESTINATION,
		submitted_attempt,
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
		plan: Some(SubmissionPlan { params: params.into() }),
	}
}

/// Whether `entry` is proven at the finalized head and at the best head.
fn proven_at_both_heads(entry: DurableTxEntry, holds: DestinationHolds) -> (bool, bool) {
	let oracle = ScarcityTransferOracle::new(Arc::new(holds));
	let heads = Heads { finalized: FINALIZED, best: BEST };
	let scope = block_on(oracle.open_pass(core::slice::from_ref(&entry), &heads)).unwrap();
	let at_finalized = scope.proven_completed(&entry, HeadKind::Finalized);
	(at_finalized, scope.proven_completed(&entry, HeadKind::Best))
}

/// A destination need not be fresh: its revision proves the transfer, so
/// a transfer to an outside key is decided at the best head too.
#[test]
fn a_transfer_is_proven_at_each_head_where_the_destination_holds_the_next_revision() {
	let landed_at_best = DestinationHolds { at_finalized: None, at_best: landed() };
	let finalized = DestinationHolds { at_finalized: landed(), at_best: landed() };

	assert_eq!(proven_at_both_heads(transfer_entry(Some(SIGNED)), landed_at_best), (false, true));
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
