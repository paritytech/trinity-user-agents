//! [`ScarcityTransferPolicy`]: builds each NFT transfer when the engine submits it,
//! from chain state read fresh at the best head.

use std::{collections::HashMap, sync::Arc};

use parity_scale_codec::{Decode, Encode};
use subxt::utils::H256;

use super::{
	chain::{KeyLocks, ScarcityChain, ScarcityNft, TransferContext},
	transfer::{NftTransfer, TRANSFER_PERIOD, sign_transfer},
};
use crate::{
	chain::{ExtrinsicService, MortalExtrinsic, Mortality},
	durable::{
		DurableTxEntry, DurableTxId, PolicyError, PolicyId, PolicyParams, Preparation,
		ScheduledDurableTx, SubmissionPolicy, VerdictFailureKind,
	},
	nft::{ItemKey, ItemSigner, ItemSlot, dao},
	store::{Db, DbError},
	unix_time::current_unix_secs,
};

/// The name rows store for this policy.
pub const TRANSFER_POLICY: &str = "nft.scarcity.transfer";

/// Attempts after which a transfer is given up, so a rejection that repeats
/// cannot loop forever.
const MAX_ATTEMPTS: u32 = 5;

/// How long a registered transfer may keep waiting for its NFT or for a lock to
/// end before it is given up, so its item never stays in use for good.
pub const TRANSFER_WAIT_LIMIT_SECS: u64 = 30 * 60;

/// What the policy stores with each transfer: the instance it transfers, the key it
/// goes to, and when it stops waiting.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Encode, Decode)]
pub struct TransferParams {
	/// The instance the source key holds when the transfer is registered.
	pub instance: u64,
	/// The key it goes to.
	pub destination: ItemKey,
	/// Whether `destination` was allocated for this transfer, so no instance
	/// was ever there before it. Only then does the instance at the
	/// destination prove the transfer landed.
	pub destination_is_fresh: bool,
	/// Unix time, in seconds of the host's clock, after which a transfer that
	/// still waits is given up. Fixed at registration, so a restart never
	/// extends it.
	pub give_up_after: u64,
}

impl From<TransferParams> for PolicyParams {
	fn from(params: TransferParams) -> Self {
		PolicyParams(params.encode())
	}
}

impl TryFrom<&PolicyParams> for TransferParams {
	type Error = parity_scale_codec::Error;

	fn try_from(params: &PolicyParams) -> Result<Self, Self::Error> {
		Self::decode(&mut &params.0[..])
	}
}

/// What the policy runs on.
pub struct ScarcityTransferPolicyDeps {
	/// The core database, for the key each transfer spends.
	pub db: Db,
	/// Scarcity storage, read fresh for every batch.
	pub chain: Arc<dyn ScarcityChain>,
	/// Signs with the key each transfer spends.
	pub signer: Arc<dyn ItemSigner>,
	/// Builds each transfer.
	pub extrinsics: Arc<ExtrinsicService>,
	/// Genesis of the chain that runs the pallet.
	pub genesis: H256,
}

/// Builds NFT transfers: one fee-less `Scarcity::transfer` per spent key.
pub struct ScarcityTransferPolicy {
	deps: ScarcityTransferPolicyDeps,
}

impl ScarcityTransferPolicy {
	/// A policy over `deps`.
	pub fn new(deps: ScarcityTransferPolicyDeps) -> Self {
		Self { deps }
	}
}

/// One scheduled transfer with everything needed to judge and build it.
struct PlannedTransfer {
	id: DurableTxId,
	attempts: u32,
	params: TransferParams,
	source: ItemKey,
	slot: ItemSlot,
}

/// What the chain says about one planned transfer at the best head.
#[derive(Clone, Copy)]
struct TransferView {
	/// What the source key holds.
	source: Option<ScarcityNft>,
	/// What the destination key holds.
	destination: Option<ScarcityNft>,
	/// Whether a failed transfer locked the source key.
	locked: bool,
}

/// What every transfer of one batch is built with.
struct BatchBuild {
	/// Builds every transfer of the batch born at the same finalized head.
	extrinsics: ExtrinsicService,
	/// The best head is so far past the finalized one that an era born there
	/// would die before it could land, so nothing is built until finality
	/// catches up.
	finality_stalled: bool,
	/// Unix time of the host's clock, which wait limits are measured in.
	now: u64,
}

impl BatchBuild {
	/// The build of transfers born at the finalized head of `context`, through
	/// `extrinsics` with the runtime there cached.
	async fn born_at(
		extrinsics: &ExtrinsicService,
		genesis: H256,
		context: TransferContext,
	) -> Result<Self, PolicyError> {
		let TransferContext { birth, best } = context;
		let mortality = Mortality::new(birth, TRANSFER_PERIOD).map_err(policy_error)?;
		let finality_stalled = best.number + TRANSFER_PERIOD / 2 >= mortality.death();
		let extrinsics = extrinsics.cache_runtime_at(genesis, birth).await.map_err(policy_error)?;
		Ok(Self { extrinsics, finality_stalled, now: current_unix_secs() })
	}
}

/// What the chain says to do with one transfer this time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TransferDecision {
	/// It can never land.
	GiveUp,
	/// Its NFT is not at the source, or the source is locked. It waits and
	/// is asked about again.
	Wait,
	/// Build it, signing over this revision of the instance.
	Build { state_nonce: u64 },
}

#[async_trait::async_trait]
impl SubmissionPolicy for ScarcityTransferPolicy {
	fn id(&self) -> PolicyId {
		PolicyId::new(TRANSFER_POLICY)
	}

	/// Every proven failure is built again: the next build reads the chain
	/// afresh and gives up once the transfer can never land.
	fn can_retry(&self, _: &DurableTxEntry, _: &PolicyParams, _: VerdictFailureKind) -> bool {
		true
	}

	async fn prepare(
		&self,
		batch: &[ScheduledDurableTx],
	) -> Result<HashMap<DurableTxId, Preparation>, PolicyError> {
		let (planned, unplanned) = self.read_planned_transfers(batch).await?;
		let context = self.deps.chain.transfer_context().await.map_err(policy_error)?;
		let views = self.read_transfer_views(context.best.hash, &planned).await?;
		let build = BatchBuild::born_at(&self.deps.extrinsics, self.deps.genesis, context).await?;
		let mut prepared = HashMap::new();
		for (transfer, view) in planned.iter().zip(views) {
			if let Some(answer) = self.preparation_of(transfer, view, &build).await {
				prepared.insert(transfer.id, answer);
			}
		}
		prepared.extend(unplanned.into_iter().map(|id| (id, Preparation::GiveUp)));
		Ok(prepared)
	}
}

impl ScarcityTransferPolicy {
	/// The transfers of `batch` with their spent key, and the ids of rows that
	/// name no transfer this policy can build, which can only be given up.
	async fn read_planned_transfers(
		&self,
		batch: &[ScheduledDurableTx],
	) -> Result<(Vec<PlannedTransfer>, Vec<DurableTxId>), PolicyError> {
		let batch = batch.to_vec();
		let read = move |conn: &rusqlite::Connection| split_planned(conn, batch);
		self.deps.db.read(read).await.map_err(policy_error)
	}

	/// What the chain holds at `at` for each of `planned`, in order.
	async fn read_transfer_views(
		&self,
		at: H256,
		planned: &[PlannedTransfer],
	) -> Result<Vec<TransferView>, PolicyError> {
		let sources: Vec<ItemKey> = planned.iter().map(|planned| planned.source).collect();
		let destinations: Vec<ItemKey> =
			planned.iter().map(|planned| planned.params.destination).collect();
		let chain = &self.deps.chain;
		let (sources_held, destinations_held, locks) = futures::future::try_join3(
			chain.nfts_at(at, &sources),
			chain.nfts_at(at, &destinations),
			chain.locks_at(at, &sources),
		)
		.await
		.map_err(policy_error)?;
		Ok(zip_views(sources_held, destinations_held, locks))
	}

	/// The answer for `transfer`, or `None` while it waits. A transfer that
	/// cannot be signed yet, as without a session, waits like any other, so
	/// its wait limit still applies.
	async fn preparation_of(
		&self,
		transfer: &PlannedTransfer,
		view: TransferView,
		build: &BatchBuild,
	) -> Option<Preparation> {
		let ready = match decide_from_chain(transfer, view) {
			TransferDecision::GiveUp => return Some(Preparation::GiveUp),
			TransferDecision::Wait => None,
			TransferDecision::Build { state_nonce } => {
				self.ready_to_submit(transfer, state_nonce, build).await
			},
		};
		ready.or_else(|| give_up_after_limit(transfer, build.now))
	}

	/// `transfer` signed and ready, or `None` when finality stalled or it
	/// could not be signed.
	async fn ready_to_submit(
		&self,
		transfer: &PlannedTransfer,
		state_nonce: u64,
		build: &BatchBuild,
	) -> Option<Preparation> {
		if build.finality_stalled {
			return None;
		}
		let signed = self.sign_planned_transfer(transfer, state_nonce, build).await;
		let signed = signed.inspect_err(|error| tracing::warn!(%error, "NFT transfer not signed"));
		signed.ok().map(Preparation::Ready)
	}

	/// `transfer` signed by its source key. Fails without a session.
	async fn sign_planned_transfer(
		&self,
		transfer: &PlannedTransfer,
		state_nonce: u64,
		build: &BatchBuild,
	) -> Result<MortalExtrinsic, PolicyError> {
		let signer = self.deps.signer.item_signer(transfer.slot).map_err(policy_error)?;
		let TransferParams { instance, destination, .. } = transfer.params;
		let nft_transfer = NftTransfer { instance, state_nonce, destination };
		let signed =
			sign_transfer(&build.extrinsics, self.deps.genesis, &signer, &nft_transfer).await;
		signed.map_err(policy_error)
	}
}

/// The transfers of `batch` with the key each spends, and the ids of rows
/// that name no transfer.
fn split_planned(
	conn: &rusqlite::Connection,
	batch: Vec<ScheduledDurableTx>,
) -> Result<(Vec<PlannedTransfer>, Vec<DurableTxId>), DbError> {
	let mut planned = Vec::new();
	let mut unplanned = Vec::new();
	for scheduled in batch {
		match planned_transfer(conn, &scheduled)? {
			Some(transfer) => planned.push(transfer),
			None => unplanned.push(scheduled.id),
		}
	}
	Ok((planned, unplanned))
}

/// `scheduled` with the key it spends, or `None` when it names no transfer: its
/// params do not decode or it spends no key.
fn planned_transfer(
	conn: &rusqlite::Connection,
	scheduled: &ScheduledDurableTx,
) -> Result<Option<PlannedTransfer>, DbError> {
	let Ok(params) = TransferParams::try_from(&scheduled.plan.params) else { return Ok(None) };
	let input = dao::input_of(conn, scheduled.id)?;
	Ok(input.map(|(source, slot)| PlannedTransfer {
		id: scheduled.id,
		attempts: scheduled.attempts,
		params,
		source,
		slot,
	}))
}

/// One view per transfer from the three reads, which answer in the same order.
fn zip_views(
	sources: Vec<Option<ScarcityNft>>,
	destinations: Vec<Option<ScarcityNft>>,
	locks: KeyLocks,
) -> Vec<TransferView> {
	let KeyLocks { now, until } = locks;
	let locked = until.into_iter().map(move |until| until.is_some_and(|until| until > now));
	sources
		.into_iter()
		.zip(destinations)
		.zip(locked)
		.map(|((source, destination), locked)| TransferView { source, destination, locked })
		.collect()
}

/// [`Preparation::GiveUp`] once `transfer` waited past its limit, at unix
/// time `now`.
fn give_up_after_limit(transfer: &PlannedTransfer, now: u64) -> Option<Preparation> {
	(now >= transfer.params.give_up_after).then_some(Preparation::GiveUp)
}

/// What the chain says about `transfer`. A source that does not hold the
/// instance waits, since a reorg can put it back. An occupied destination
/// fails at once.
fn decide_from_chain(transfer: &PlannedTransfer, view: TransferView) -> TransferDecision {
	if transfer.attempts >= MAX_ATTEMPTS || view.destination.is_some() {
		return TransferDecision::GiveUp;
	}
	let held = view.source.filter(|held| held.instance == transfer.params.instance);
	let Some(held) = held else { return TransferDecision::Wait };
	if view.locked {
		return TransferDecision::Wait;
	}
	TransferDecision::Build { state_nonce: held.state_nonce }
}

fn policy_error(error: impl core::fmt::Display) -> PolicyError {
	PolicyError { reason: error.to_string() }
}

#[cfg(test)]
mod tests {
	use core::num::NonZeroU32;

	use futures::executor::block_on;
	use subxt::utils::H256;
	use tempfile::TempDir;

	use super::*;
	use crate::{
		chain::{HashAndNumber, StorageWatchError, extrinsic_service_serving},
		durable::{DomainId, SubmissionPlan},
		nft::{
			NftLedger, PurseId, PurseTag,
			scarcity::{chain::TransferContext, testing::ASSET_HUB_METADATA_BYTES},
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
	/// The genesis the scripted node's chain is configured with.
	const GENESIS: H256 = H256([0xab; 32]);

	/// Builds transfers over a scripted node serving the Asset Hub fixture.
	fn transfer_extrinsics() -> Arc<ExtrinsicService> {
		Arc::new(extrinsic_service_serving(ASSET_HUB_METADATA_BYTES, 1))
	}

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

		async fn locks_at(
			&self,
			at: H256,
			keys: &[ItemKey],
		) -> Result<KeyLocks, StorageWatchError> {
			assert_eq!(at, BEST.hash);
			let until = keys.iter().map(|key| self.locked_until.get(key).copied()).collect();
			Ok(KeyLocks { now: NOW, until })
		}

		async fn transfer_context(&self) -> Result<TransferContext, StorageWatchError> {
			let best = HashAndNumber { number: BEST.number + self.best_ahead_by, ..BEST };
			Ok(TransferContext { birth: FINALIZED, best })
		}
	}

	/// What the source key holds when the transfer is still possible.
	fn held_nft() -> ScarcityNft {
		ScarcityNft {
			instance: INSTANCE,
			collection: 27,
			item: 13,
			minted_at: 0,
			last_moved: 0,
			state_nonce: 3,
			moves: 0,
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
			extrinsics: transfer_extrinsics(),
			genesis: GENESIS,
		})
	}

	fn holding(registered: &RegisteredTransfer, nft: ScarcityNft) -> ScriptedScarcity {
		let held = HashMap::from([(registered.source, nft)]);
		ScriptedScarcity { held, ..ScriptedScarcity::default() }
	}

	fn prepare_one(
		registered: &RegisteredTransfer,
		chain: ScriptedScarcity,
	) -> Option<Preparation> {
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

		let prepared = prepare_one(&registered, holding(&registered, held_nft()));

		let Some(Preparation::Ready(signed)) = prepared else { panic!("built: {prepared:?}") };
		assert_eq!(signed.mortality, Mortality::new(FINALIZED, TRANSFER_PERIOD).unwrap());
	}

	/// A reorg can briefly show the source key without its NFT, so the transfer
	/// waits for it to come back rather than failing the operation.
	#[test]
	fn a_key_that_does_not_hold_the_instance_waits_for_it() {
		let registered = registered_transfer(0);
		let moved_on = ScarcityNft { instance: INSTANCE + 1, ..held_nft() };

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
		let mut locked = holding(&expired, held_nft());
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

		let prepared = prepare_one(&expired, holding(&expired, held_nft()));

		assert!(matches!(prepared, Some(Preparation::Ready(_))));
	}

	#[test]
	fn an_occupied_destination_is_given_up() {
		let registered = registered_transfer(0);
		let mut chain = holding(&registered, held_nft());
		chain.held.insert(DESTINATION, held_nft());

		let prepared = prepare_one(&registered, chain);

		assert_eq!(prepared, Some(Preparation::GiveUp));
	}

	#[test]
	fn a_locked_key_waits_until_its_lock_ends() {
		let registered = registered_transfer(0);
		let locked = |until| {
			let mut chain = holding(&registered, held_nft());
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

		let prepared = prepare_one(&registered, holding(&registered, held_nft()));

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
				chain: Arc::new(holding(registered, held_nft())),
				signer: Arc::new(SignedOut),
				extrinsics: transfer_extrinsics(),
				genesis: GENESIS,
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
			|blocks| ScriptedScarcity { best_ahead_by: blocks, ..holding(&registered, held_nft()) };
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
}
