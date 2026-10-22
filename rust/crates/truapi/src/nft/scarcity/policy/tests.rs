use core::num::NonZeroU32;

use futures::executor::block_on;
use subxt::utils::H256;
use tempfile::TempDir;

use super::*;
use crate::{
	chain::{HashAndNumber, StorageWatchError},
	durable::{DomainId, SubmissionPlan},
	nft::{
		NftLedger, PurseId, PurseTag,
		scarcity::{chain::TransferContext, testing::asset_hub_metadata},
		testing::{FixedEntropy, TEST_ENTROPY, test_ledger},
	},
	runtime::AuthorityError,
	store::open_temporary_core_db,
	unix_time::current_unix_secs,
};

const FINALIZED: HashAndNumber = HashAndNumber { hash: H256([0x0f; 32]), number: 2_138_796 };
/// The best head, ahead of [`FINALIZED`]: an NFT minted there is held only here.
const BEST: HashAndNumber = HashAndNumber { hash: H256([0x0b; 32]), number: 2_138_800 };
const DESTINATION: ItemKey = ItemKey([0x08; 32]);
const INSTANCE: u64 = 330;
const NOW: u64 = 1_700_000_000_000;

/// Scarcity storage a test writes by hand, held at [`BEST`] only.
#[derive(Default)]
struct ScriptedScarcity {
	held: HashMap<ItemKey, ScarcityNft>,
	locked_until: HashMap<ItemKey, u64>,
	/// How many blocks the best head runs past [`BEST`], as when finality
	/// stalls.
	best_ahead_by: u64,
}

#[async_trait::async_trait]
impl ScarcityChain for ScriptedScarcity {
	async fn nfts_at(
		&self,
		at: H256,
		keys: &[ItemKey],
	) -> Result<Vec<Option<ScarcityNft>>, StorageWatchError> {
		let held = |key: &ItemKey| self.held.get(key).copied().filter(|_| at == BEST.hash);
		Ok(keys.iter().map(held).collect())
	}

	async fn holders_at(
		&self,
		_at: H256,
		_instances: &[u64],
	) -> Result<Vec<Option<ItemKey>>, StorageWatchError> {
		unreachable!("the policy never asks who holds an instance")
	}

	async fn locks_at(&self, at: H256, keys: &[ItemKey]) -> Result<KeyLocks, StorageWatchError> {
		assert_eq!(at, BEST.hash);
		let until = keys.iter().map(|key| self.locked_until.get(key).copied()).collect();
		Ok(KeyLocks { now: NOW, until })
	}

	async fn transfer_context(&self) -> Result<TransferContext, StorageWatchError> {
		let runtime = TransferRuntime {
			genesis: H256([0xab; 32]),
			spec_version: 3_004_000,
			transaction_version: 22,
			metadata: asset_hub_metadata(),
		};
		let best = HashAndNumber { number: BEST.number + self.best_ahead_by, ..BEST };
		Ok(TransferContext { birth: FINALIZED, runtime, best })
	}
}

/// What the source key holds when the transfer is still possible.
fn held_nft(moves: u16) -> ScarcityNft {
	ScarcityNft {
		instance: INSTANCE,
		collection: 27,
		item: 13,
		minted_at: 0,
		last_moved: 0,
		state_nonce: 3,
		moves,
	}
}

/// A scheduled transfer of one allocated key to [`DESTINATION`], as the ledger
/// registers it.
struct RegisteredTransfer {
	_dir: TempDir,
	db: Db,
	source: ItemKey,
	scheduled: ScheduledDurableTx,
}

fn registered_transfer(attempts: u32) -> RegisteredTransfer {
	registered_transfer_until(attempts, current_unix_secs() + TRANSFER_WAIT_LIMIT_SECS)
}

/// [`registered_transfer`] that stops waiting at unix time `give_up_after`.
fn registered_transfer_until(attempts: u32, give_up_after: u64) -> RegisteredTransfer {
	let (dir, db) = open_temporary_core_db();
	let source = allocate_one_key(&test_ledger(db.clone()));
	let id = block_on(db.write(move |tx| {
		tx.execute(
			"INSERT INTO durable_tx (domain, status) VALUES ('nft', 'PENDING_SUBMISSION')",
			[],
		)?;
		let id = DurableTxId(tx.last_insert_rowid());
		dao::insert_inputs(tx, &[(source, id)])?;
		Ok(id)
	}))
	.unwrap();
	let params = TransferParams {
		instance: INSTANCE,
		destination: DESTINATION,
		destination_is_fresh: true,
		give_up_after,
	};
	let plan = SubmissionPlan { policy: PolicyId::new(TRANSFER_POLICY), params: params.into() };
	let scheduled =
		ScheduledDurableTx { id, domain: DomainId::new("nft"), group: None, plan, attempts };
	RegisteredTransfer { _dir: dir, db, source, scheduled }
}

fn allocate_one_key(ledger: &NftLedger) -> ItemKey {
	block_on(ledger.create_purse("tickets".to_owned(), PurseTag::default())).unwrap();
	let one = NonZeroU32::new(1).unwrap();
	block_on(ledger.allocate_items(PurseId(0), one)).unwrap()[0]
}

fn policy(registered: &RegisteredTransfer, chain: ScriptedScarcity) -> ScarcityTransferPolicy {
	ScarcityTransferPolicy::new(ScarcityTransferPolicyDeps {
		db: registered.db.clone(),
		chain: Arc::new(chain),
		signer: Arc::new(FixedEntropy(TEST_ENTROPY.to_vec())),
	})
}

fn holding(registered: &RegisteredTransfer, nft: ScarcityNft) -> ScriptedScarcity {
	let held = HashMap::from([(registered.source, nft)]);
	ScriptedScarcity { held, ..ScriptedScarcity::default() }
}

fn prepare_one(registered: &RegisteredTransfer, chain: ScriptedScarcity) -> Option<Preparation> {
	let batch = [registered.scheduled.clone()];
	let mut prepared = block_on(policy(registered, chain).prepare(&batch)).unwrap();
	prepared.remove(&registered.scheduled.id)
}

/// An item is available once the best block holds it, so a transfer is judged
/// there: an NFT minted at the best head and not finalized yet is built, its
/// era born at the finalized head.
#[test]
fn an_nft_held_at_the_best_head_is_built_born_at_the_finalized_head() {
	let registered = registered_transfer(0);

	let prepared = prepare_one(&registered, holding(&registered, held_nft(0)));

	let Some(Preparation::Ready(signed)) = prepared else { panic!("built: {prepared:?}") };
	assert_eq!(signed.mortality, Mortality::new(FINALIZED, TRANSFER_PERIOD).unwrap());
}

/// A reorg can briefly show the source key without its NFT, so the transfer
/// waits for it to come back rather than failing the operation.
#[test]
fn a_key_that_does_not_hold_the_instance_waits_for_it() {
	let registered = registered_transfer(0);
	let moved_on = ScarcityNft { instance: INSTANCE + 1, ..held_nft(0) };

	let gone = prepare_one(&registered, ScriptedScarcity::default());
	let replaced = prepare_one(&registered, holding(&registered, moved_on));

	assert_eq!(gone, None);
	assert_eq!(replaced, None);
}

/// Waiting is bounded, so an NFT that is gone for good does not keep its
/// item in use forever.
#[test]
fn a_move_still_waiting_past_its_limit_is_given_up() {
	let expired = registered_transfer_until(0, current_unix_secs() - 1);
	let mut locked = holding(&expired, held_nft(0));
	locked.locked_until.insert(expired.source, NOW + 1);

	let without_nft = prepare_one(&expired, ScriptedScarcity::default());
	let while_locked = prepare_one(&expired, locked);

	assert_eq!(without_nft, Some(Preparation::GiveUp));
	assert_eq!(while_locked, Some(Preparation::GiveUp));
}

/// The limit only bounds waiting: a transfer the chain allows is still built.
#[test]
fn a_move_past_its_limit_is_still_built_when_nothing_holds_it_back() {
	let expired = registered_transfer_until(0, current_unix_secs() - 1);

	let prepared = prepare_one(&expired, holding(&expired, held_nft(0)));

	assert!(matches!(prepared, Some(Preparation::Ready(_))));
}

#[test]
fn an_nft_without_free_moves_left_is_given_up() {
	let registered = registered_transfer(0);

	let prepared = prepare_one(&registered, holding(&registered, held_nft(MAXIMUM_MOVES)));

	assert_eq!(prepared, Some(Preparation::GiveUp));
}

#[test]
fn an_occupied_destination_is_given_up() {
	let registered = registered_transfer(0);
	let mut chain = holding(&registered, held_nft(0));
	chain.held.insert(DESTINATION, held_nft(0));

	let prepared = prepare_one(&registered, chain);

	assert_eq!(prepared, Some(Preparation::GiveUp));
}

#[test]
fn a_locked_key_waits_until_its_lock_ends() {
	let registered = registered_transfer(0);
	let locked = |until| {
		let mut chain = holding(&registered, held_nft(0));
		chain.locked_until.insert(registered.source, until);
		chain
	};

	let while_locked = prepare_one(&registered, locked(NOW + 1));
	let after_lock = prepare_one(&registered, locked(NOW));

	assert_eq!(while_locked, None);
	assert!(matches!(after_lock, Some(Preparation::Ready(_))));
}

#[test]
fn a_move_is_given_up_after_its_last_attempt() {
	let registered = registered_transfer(MAX_ATTEMPTS);

	let prepared = prepare_one(&registered, holding(&registered, held_nft(0)));

	assert_eq!(prepared, Some(Preparation::GiveUp));
}

/// Without a session nothing can be signed, so the transfer waits like any
/// other, and its wait limit still frees the item if the user never returns.
#[test]
fn without_a_session_a_transfer_waits_until_its_limit() {
	let waiting = registered_transfer(0);
	let expired = registered_transfer_until(0, current_unix_secs() - 1);
	let signed_out = |registered: &RegisteredTransfer| {
		ScarcityTransferPolicy::new(ScarcityTransferPolicyDeps {
			db: registered.db.clone(),
			chain: Arc::new(holding(registered, held_nft(0))),
			signer: Arc::new(SignedOut),
		})
	};
	let prepare = |registered: &RegisteredTransfer| {
		let batch = core::slice::from_ref(&registered.scheduled);
		block_on(signed_out(registered).prepare(batch))
			.unwrap()
			.remove(&registered.scheduled.id)
	};

	assert_eq!(prepare(&waiting), None);
	assert_eq!(prepare(&expired), Some(Preparation::GiveUp));
}

/// An era born at a finalized head that best has nearly outrun would die
/// before landing and use up an attempt, so nothing is built until finality
/// catches up.
#[test]
fn a_transfer_waits_while_finality_lags_too_far_behind() {
	let registered = registered_transfer(0);
	let behind =
		|blocks| ScriptedScarcity { best_ahead_by: blocks, ..holding(&registered, held_nft(0)) };
	let lag_to_death = FINALIZED.number + TRANSFER_PERIOD / 2 - BEST.number;

	let stalled = prepare_one(&registered, behind(lag_to_death));
	let lagging = prepare_one(&registered, behind(lag_to_death - 1));

	assert_eq!(stalled, None);
	assert!(matches!(lagging, Some(Preparation::Ready(_))));
}

/// A signer with no session.
struct SignedOut;

impl ItemSigner for SignedOut {
	fn item_signer(
		&self,
		_: ItemSlot,
	) -> Result<crate::host_internal::extrinsic::Sr25519Signer, AuthorityError> {
		Err(AuthorityError::Disconnected)
	}
}
