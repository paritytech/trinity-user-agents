use futures::executor::block_on;

use super::*;
use crate::{
	chain::Mortality,
	durable::{DomainId, DurableTxStatus, PolicyId, SubmissionPlan},
	nft::scarcity::{
		chain::{KeyLocks, MoveContext, ScarcityNft},
		policy::MOVE_POLICY,
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
		assert_eq!(instances, [INSTANCE]);
		let holder = if at == FINALIZED.hash { self.at_finalized } else { self.at_best };
		Ok(vec![holder])
	}

	async fn locks_at(&self, _: H256, _: &[ItemKey]) -> Result<KeyLocks, StorageWatchError> {
		unreachable!("the oracle only asks who holds an instance")
	}

	async fn move_context(&self) -> Result<MoveContext, StorageWatchError> {
		unreachable!("the oracle only asks who holds an instance")
	}
}

fn move_entry() -> DurableTxEntry {
	let params = MoveParams { instance: INSTANCE, destination: DESTINATION };
	DurableTxEntry {
		id: DurableTxId(1),
		domain: DomainId::new("nft"),
		group: None,
		tx_hash: H256([0x01; 32]),
		mortality: Mortality::new(FINALIZED, 64).unwrap(),
		status: DurableTxStatus::Pending,
		scanned_to: None,
		plan: Some(SubmissionPlan { policy: PolicyId::new(MOVE_POLICY), params: params.into() }),
	}
}

/// Whether the move is proven at the finalized head and at the best head.
fn proven_at_both_heads(holders: InstanceHolders) -> (bool, bool) {
	let oracle = ScarcityMoveOracle::new(Arc::new(holders), H256([0xab; 32]));
	let entry = move_entry();
	let heads = Heads { finalized: FINALIZED, best: BEST };
	let view = LedgerView::new(vec![entry.clone()]);
	let scope = block_on(oracle.open_pass(core::slice::from_ref(&entry), &view, &heads)).unwrap();
	let at_finalized = scope.proven_completed(&entry, HeadKind::Finalized);
	(at_finalized, scope.proven_completed(&entry, HeadKind::Best))
}

#[test]
fn a_move_is_proven_at_each_head_where_its_instance_is_at_the_destination() {
	let landed_at_best = InstanceHolders { at_finalized: None, at_best: Some(DESTINATION) };
	let finalized = InstanceHolders { at_finalized: Some(DESTINATION), at_best: Some(DESTINATION) };

	assert_eq!(proven_at_both_heads(landed_at_best), (false, true));
	assert_eq!(proven_at_both_heads(finalized), (true, true));
}

#[test]
fn an_instance_anywhere_else_proves_nothing_either_way() {
	let elsewhere = InstanceHolders { at_finalized: Some(ELSEWHERE), at_best: Some(ELSEWHERE) };
	let entry = move_entry();
	let oracle = ScarcityMoveOracle::new(Arc::new(elsewhere), H256([0xab; 32]));
	let heads = Heads { finalized: FINALIZED, best: BEST };
	let view = LedgerView::new(vec![entry.clone()]);

	let scope = block_on(oracle.open_pass(core::slice::from_ref(&entry), &view, &heads)).unwrap();

	assert!(!scope.proven_completed(&entry, HeadKind::Best));
	assert!(!scope.proven_not_completed(&entry, HeadKind::Finalized));
}
