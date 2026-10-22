//! [`ScarcityTransferPolicy`]: builds each NFT transfer when the engine submits it,
//! from chain state read fresh at the best head.

use std::{collections::HashMap, sync::Arc};

use parity_scale_codec::{Decode, Encode};
use subxt::utils::H256;

use super::{
	chain::{KeyLocks, ScarcityChain, ScarcityNft, TransferContext},
	transfer::{NftTransfer, TransferRuntime, sign_transfer},
};
use crate::{
	chain::{MortalExtrinsic, Mortality},
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

/// Fee-less moves an instance allows between paid ones, the pallet's
/// `MaximumMoves`. Paid moves are not built yet.
const MAXIMUM_MOVES: u16 = 16;

/// Blocks a transfer stays valid for after its birth.
const TRANSFER_PERIOD: u64 = 64;

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
	runtime: TransferRuntime,
	mortality: Mortality,
	/// The best head is so far past the finalized one that an era born there
	/// would die before it could land, so nothing is built until finality
	/// catches up.
	finality_stalled: bool,
	/// Unix time of the host's clock, which wait limits are measured in.
	now: u64,
}

impl BatchBuild {
	/// The build of transfers born at the finalized head of `context`.
	fn born_at(context: TransferContext) -> Result<Self, PolicyError> {
		let TransferContext { birth, runtime, best } = context;
		let mortality = Mortality::new(birth, TRANSFER_PERIOD).map_err(policy_error)?;
		let finality_stalled = best.number + TRANSFER_PERIOD / 2 >= mortality.death();
		Ok(Self { runtime, mortality, finality_stalled, now: current_unix_secs() })
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
		let build = BatchBuild::born_at(context)?;
		let prepare_one = |(transfer, view): (&PlannedTransfer, TransferView)| {
			self.preparation_of(transfer, view, &build).map(|answer| (transfer.id, answer))
		};
		let mut prepared: HashMap<_, _> =
			planned.iter().zip(views).filter_map(prepare_one).collect();
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
	fn preparation_of(
		&self,
		transfer: &PlannedTransfer,
		view: TransferView,
		build: &BatchBuild,
	) -> Option<Preparation> {
		let ready = match decide_from_chain(transfer, view) {
			TransferDecision::GiveUp => return Some(Preparation::GiveUp),
			TransferDecision::Wait => None,
			TransferDecision::Build { state_nonce } => {
				self.ready_to_submit(transfer, state_nonce, build)
			},
		};
		ready.or_else(|| give_up_after_limit(transfer, build.now))
	}

	/// `transfer` signed and ready, or `None` when finality stalled or it
	/// could not be signed.
	fn ready_to_submit(
		&self,
		transfer: &PlannedTransfer,
		state_nonce: u64,
		build: &BatchBuild,
	) -> Option<Preparation> {
		if build.finality_stalled {
			return None;
		}
		let signed = self.sign_planned_transfer(transfer, state_nonce, build);
		let signed = signed.inspect_err(|error| tracing::warn!(%error, "NFT transfer not signed"));
		signed.ok().map(Preparation::Ready)
	}

	/// `transfer` signed by its source key. Fails without a session.
	fn sign_planned_transfer(
		&self,
		transfer: &PlannedTransfer,
		state_nonce: u64,
		build: &BatchBuild,
	) -> Result<MortalExtrinsic, PolicyError> {
		let signer = self.deps.signer.item_signer(transfer.slot).map_err(policy_error)?;
		let TransferParams { instance, destination, .. } = transfer.params;
		let nft_transfer = NftTransfer { instance, state_nonce, destination };
		sign_transfer(&signer, &build.runtime, &nft_transfer, build.mortality).map_err(policy_error)
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
	let view = |((source, destination), locked)| TransferView { source, destination, locked };
	sources.into_iter().zip(destinations).zip(locked).map(view).collect()
}

/// [`Preparation::GiveUp`] once `transfer` waited past its limit, at unix
/// time `now`.
fn give_up_after_limit(transfer: &PlannedTransfer, now: u64) -> Option<Preparation> {
	(now >= transfer.params.give_up_after).then_some(Preparation::GiveUp)
}

/// What the chain says about `transfer`. A source that does not hold the
/// instance waits, since a reorg can put it back. An occupied destination
/// fails at once, and so does a used-up move budget, which never refills.
fn decide_from_chain(transfer: &PlannedTransfer, view: TransferView) -> TransferDecision {
	if transfer.attempts >= MAX_ATTEMPTS || view.destination.is_some() {
		return TransferDecision::GiveUp;
	}
	let held = view.source.filter(|held| held.instance == transfer.params.instance);
	let Some(held) = held else { return TransferDecision::Wait };
	if held.moves >= MAXIMUM_MOVES {
		return TransferDecision::GiveUp;
	}
	if view.locked {
		return TransferDecision::Wait;
	}
	TransferDecision::Build { state_nonce: held.state_nonce }
}

fn policy_error(error: impl core::fmt::Display) -> PolicyError {
	PolicyError { reason: error.to_string() }
}

#[cfg(test)]
mod tests;
