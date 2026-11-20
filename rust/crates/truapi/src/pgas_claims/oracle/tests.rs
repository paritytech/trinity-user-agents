use std::collections::{HashMap, HashSet};

use futures::{executor::block_on, stream::BoxStream};
use subxt::{client::OnlineClientAtBlock, config::SubstrateConfig};

use super::*;
use crate::{
	chain::{DecodableAtBlock, HashAndNumber, RawStorageChanges, StorageSnapshot, WatchedHead},
	durable::{DomainId, DurableTxStatus, SubmissionPlan, Window},
	nft::asset_hub_metadata,
	pgas_claims::testing::{TEST_DAY, TEST_IDENTITY, destination},
	runtime::statement_allowance::pgas::ClaimSlot,
	time::{ManualTimer, Timer},
};

const GENESIS: H256 = H256([0xab; 32]);
const FINALIZED: HashAndNumber = HashAndNumber { hash: H256([0x0f; 32]), number: 130 };
const BEST: HashAndNumber = HashAndNumber { hash: H256([0x0b; 32]), number: 140 };

/// `Pgas.ClaimedGasAliases` as recorded at each block, by raw key.
struct RecordedAliases(HashMap<H256, HashSet<Vec<u8>>>);

impl RecordedAliases {
	/// Records the alias of `attempt` at each block of `at`.
	fn of(attempt: ClaimAttempt, at: &[HashAndNumber]) -> Arc<Self> {
		let key = claimed_gas_alias_key(attempt.slot.day, &attempt.alias);
		let recorded = at.iter().map(|block| (block.hash, HashSet::from([key.clone()]))).collect();
		Arc::new(Self(recorded))
	}
}

#[async_trait::async_trait]
impl WatchedStorage for RecordedAliases {
	async fn client_at_finalized_head(
		&self,
		_genesis: H256,
	) -> Result<OnlineClientAtBlock<SubstrateConfig>, RuntimeFailure> {
		unreachable!("the oracle reads raw keys")
	}

	async fn client_at(
		&self,
		_genesis: H256,
		_at: HashAndNumber,
	) -> Result<OnlineClientAtBlock<SubstrateConfig>, RuntimeFailure> {
		unreachable!("the oracle reads raw keys")
	}

	async fn watch_raw_key_changes(
		&self,
		_genesis: H256,
		_head: WatchedHead,
		_keys: Vec<Vec<u8>>,
	) -> Result<BoxStream<'static, Result<RawStorageChanges, RuntimeFailure>>, RuntimeFailure> {
		unreachable!("the oracle reads at a block")
	}

	async fn read_raw_keys_at(
		&self,
		genesis: H256,
		at: H256,
		keys: Vec<Vec<u8>>,
	) -> Result<StorageSnapshot, RuntimeFailure> {
		assert_eq!(genesis, GENESIS);
		let recorded = self.0.get(&at).cloned().unwrap_or_default();
		let value = keys.iter().map(|key| recorded.contains(key).then(Vec::new)).collect();
		let at = HashAndNumber { hash: at, number: 0 };
		Ok(DecodableAtBlock { at, metadata: asset_hub_metadata(), value })
	}

	fn timer(&self) -> Arc<dyn Timer> {
		Arc::new(ManualTimer::default())
	}
}

/// An attempt in `slot_index`, with an alias distinct to that slot as the
/// person's aliases are.
fn attempt_in(slot_index: u32) -> ClaimAttempt {
	let alias = [u8::try_from(slot_index).expect("a test slot fits in u8"); 32];
	ClaimAttempt { slot: ClaimSlot { day: TEST_DAY, slot_index }, alias }
}

/// A pending claim whose live attempt is `attempt`.
fn claim_entry(attempt: Option<ClaimAttempt>) -> DurableTxEntry {
	let params =
		ClaimParams { target: destination(7), identity: TEST_IDENTITY, give_up_after: 0, attempt };
	DurableTxEntry {
		id: DurableTxId(1),
		domain: DomainId::new("pgas"),
		group: None,
		tx_hash: H256::zero(),
		window: Window { birth: 130, period: 64 },
		status: DurableTxStatus::Pending,
		scanned_to: None,
		plan: Some(SubmissionPlan { params: params.into() }),
	}
}

/// Whether `entry` is proven completed at the finalized and at the best
/// head, by an oracle over `storage`.
fn proven(storage: Arc<RecordedAliases>, entry: &DurableTxEntry) -> [bool; 2] {
	let oracle = PgasClaimOracle::new(storage, GENESIS);
	let heads = Heads { finalized: FINALIZED, best: BEST };
	let scope = block_on(oracle.open_pass(std::slice::from_ref(entry), &heads)).unwrap();
	[HeadKind::Finalized, HeadKind::Best].map(|head| scope.proven_completed(entry, head))
}

/// The pallet records the alias only when the claim succeeds, so the record
/// proves the claim.
#[test]
fn a_recorded_alias_proves_the_claim() {
	let entry = claim_entry(Some(attempt_in(3)));

	let proof = proven(RecordedAliases::of(attempt_in(3), &[FINALIZED, BEST]), &entry);

	assert_eq!(proof, [true, true]);
}

/// A claim in a best block that is not finalized yet is proven only there.
#[test]
fn an_alias_recorded_only_at_the_best_head_proves_the_claim_only_there() {
	let entry = claim_entry(Some(attempt_in(3)));

	let proof = proven(RecordedAliases::of(attempt_in(3), &[BEST]), &entry);

	assert_eq!(proof, [false, true]);
}

/// The alias of another slot of the same day was spent by another claim.
#[test]
fn the_alias_of_another_slot_proves_nothing() {
	let entry = claim_entry(Some(attempt_in(3)));

	let proof = proven(RecordedAliases::of(attempt_in(4), &[FINALIZED, BEST]), &entry);

	assert_eq!(proof, [false, false]);
}

#[test]
fn a_claim_never_built_proves_nothing() {
	let entry = claim_entry(None);

	let proof = proven(RecordedAliases::of(attempt_in(3), &[FINALIZED, BEST]), &entry);

	assert_eq!(proof, [false, false]);
}
