use futures::executor::block_on;

use super::*;
use crate::{
	chain::Mortality,
	durable::{DomainId, DurableTxStatus, PolicyId, SubmissionPlan},
	nft::scarcity::{
		chain::{KeyLocks, ScarcityNft, TransferContext},
		policy::TRANSFER_POLICY,
	},
};

const FINALIZED: HashAndNumber = HashAndNumber { hash: H256([0x0f; 32]), number: 130 };
const BEST: HashAndNumber = HashAndNumber { hash: H256([0x0b; 32]), number: 140 };
const DESTINATION: ItemKey = ItemKey([0x08; 32]);
const ELSEWHERE: ItemKey = ItemKey([0x09; 32]);
const INSTANCE: u64 = 330;

/// Who holds [`INSTANCE`] at each head.
struct InstanceHolders {
	at_finalized: Option<ItemKey>,
	at_best: Option<ItemKey>,
}

#[async_trait::async_trait]
impl ScarcityChain for InstanceHolders {
	async fn nfts_at(
		&self,
		_: H256,
		_: &[ItemKey],
	) -> Result<Vec<Option<ScarcityNft>>, StorageWatchError> {
		unreachable!("the oracle only asks who holds an instance")
	}

	async fn holders_at(
		&self,
		at: H256,
		instances: &[u64],
	) -> Result<Vec<Option<ItemKey>>, StorageWatchError> {
		let holder = if at == FINALIZED.hash { self.at_finalized } else { self.at_best };
		Ok(instances.iter().map(|_| holder).collect())
	}

	async fn locks_at(&self, _: H256, _: &[ItemKey]) -> Result<KeyLocks, StorageWatchError> {
		unreachable!("the oracle only asks who holds an instance")
	}

	async fn transfer_context(&self) -> Result<TransferContext, StorageWatchError> {
		unreachable!("the oracle only asks who holds an instance")
	}
}

fn transfer_entry() -> DurableTxEntry {
	transfer_entry_into(true)
}

/// A transfer of [`INSTANCE`] to [`DESTINATION`], a key allocated for it or
/// an outside one.
fn transfer_entry_into(destination_is_fresh: bool) -> DurableTxEntry {
	let params = TransferParams {
		instance: INSTANCE,
		destination: DESTINATION,
		destination_is_fresh,
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

/// Whether the transfer is proven at the finalized head and at the best head.
fn proven_at_both_heads(holders: InstanceHolders) -> (bool, bool) {
	let oracle = ScarcityTransferOracle::new(Arc::new(holders), H256([0xab; 32]));
	let entry = transfer_entry();
	let heads = Heads { finalized: FINALIZED, best: BEST };
	let view = LedgerView::new(vec![entry.clone()]);
	let scope = block_on(oracle.open_pass(core::slice::from_ref(&entry), &view, &heads)).unwrap();
	let at_finalized = scope.proven_completed(&entry, HeadKind::Finalized);
	(at_finalized, scope.proven_completed(&entry, HeadKind::Best))
}

#[test]
fn a_transfer_is_proven_at_each_head_where_its_instance_is_at_the_destination() {
	let landed_at_best = InstanceHolders { at_finalized: None, at_best: Some(DESTINATION) };
	let finalized = InstanceHolders { at_finalized: Some(DESTINATION), at_best: Some(DESTINATION) };

	assert_eq!(proven_at_both_heads(landed_at_best), (false, true));
	assert_eq!(proven_at_both_heads(finalized), (true, true));
}

#[test]
fn an_instance_anywhere_else_proves_nothing_either_way() {
	let elsewhere = InstanceHolders { at_finalized: Some(ELSEWHERE), at_best: Some(ELSEWHERE) };
	let entry = transfer_entry();
	let oracle = ScarcityTransferOracle::new(Arc::new(elsewhere), H256([0xab; 32]));
	let heads = Heads { finalized: FINALIZED, best: BEST };
	let view = LedgerView::new(vec![entry.clone()]);

	let scope = block_on(oracle.open_pass(core::slice::from_ref(&entry), &view, &heads)).unwrap();

	assert!(!scope.proven_completed(&entry, HeadKind::Best));
	assert!(!scope.proven_not_completed(&entry, HeadKind::Finalized));
}

/// An outside key may have held the instance before the transfer, as when a
/// product returns an NFT to whoever sent it, so the instance there proves
/// nothing and the block search decides.
#[test]
fn the_instance_at_an_outside_destination_proves_nothing() {
	let at_destination =
		InstanceHolders { at_finalized: Some(DESTINATION), at_best: Some(DESTINATION) };
	let entry = transfer_entry_into(false);
	let oracle = ScarcityTransferOracle::new(Arc::new(at_destination), H256([0xab; 32]));
	let heads = Heads { finalized: FINALIZED, best: BEST };
	let view = LedgerView::new(vec![entry.clone()]);

	let scope = block_on(oracle.open_pass(core::slice::from_ref(&entry), &view, &heads)).unwrap();

	assert!(!scope.proven_completed(&entry, HeadKind::Finalized));
	assert!(!scope.proven_completed(&entry, HeadKind::Best));
}
