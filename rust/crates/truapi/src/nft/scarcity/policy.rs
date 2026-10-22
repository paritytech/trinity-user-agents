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
	store::Db,
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

/// What the chain says about each planned transfer, in the same order.
struct ChainView {
	sources: Vec<Option<ScarcityNft>>,
	destinations: Vec<Option<ScarcityNft>>,
	locks: KeyLocks,
}

/// What to do with one transfer this time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TransferDecision {
	/// It can never land.
	GiveUp,
	/// Its key is locked. It waits and is asked about again.
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
		let TransferContext { birth, runtime, best } =
			self.deps.chain.transfer_context().await.map_err(policy_error)?;
		let view = self.read_chain_view(best, &planned).await?;
		let mortality = Mortality::new(birth, TRANSFER_PERIOD).map_err(policy_error)?;
		let mut prepared = self.prepare_transfers(&planned, &view, &runtime, mortality)?;
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
		let read = move |conn: &rusqlite::Connection| {
			let mut planned = Vec::new();
			let mut unplanned = Vec::new();
			for scheduled in batch {
				match planned_transfer(conn, &scheduled)? {
					Some(planned_transfer) => planned.push(planned_transfer),
					None => unplanned.push(scheduled.id),
				}
			}
			Ok((planned, unplanned))
		};
		self.deps.db.read(read).await.map_err(policy_error)
	}

	/// The sources, destinations and source locks of `planned` at `at`.
	async fn read_chain_view(
		&self,
		at: H256,
		planned: &[PlannedTransfer],
	) -> Result<ChainView, PolicyError> {
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
		Ok(ChainView { sources: sources_held, destinations: destinations_held, locks })
	}

	/// What to do with each of `planned`. A transfer that waits is left out.
	fn prepare_transfers(
		&self,
		planned: &[PlannedTransfer],
		view: &ChainView,
		runtime: &TransferRuntime,
		mortality: Mortality,
	) -> Result<HashMap<DurableTxId, Preparation>, PolicyError> {
		let mut prepared = HashMap::new();
		let now = current_unix_secs();
		for (index, planned_transfer) in planned.iter().enumerate() {
			let preparation = match decide_transfer(planned_transfer, view, index, now) {
				TransferDecision::GiveUp => Preparation::GiveUp,
				TransferDecision::Wait => continue,
				TransferDecision::Build { state_nonce } => {
					let signed = self.build(planned_transfer, state_nonce, runtime, mortality)?;
					Preparation::Ready(signed)
				},
			};
			prepared.insert(planned_transfer.id, preparation);
		}
		Ok(prepared)
	}

	/// `planned_transfer` signed by its source key. Fails without a session.
	fn build(
		&self,
		planned_transfer: &PlannedTransfer,
		state_nonce: u64,
		runtime: &TransferRuntime,
		mortality: Mortality,
	) -> Result<MortalExtrinsic, PolicyError> {
		let signer = self.deps.signer.item_signer(planned_transfer.slot).map_err(policy_error)?;
		let TransferParams { instance, destination, .. } = planned_transfer.params;
		let nft_transfer = NftTransfer { instance, state_nonce, destination };
		sign_transfer(&signer, runtime, &nft_transfer, mortality).map_err(policy_error)
	}
}

/// `scheduled` with the key it spends, or `None` when it names no transfer: its
/// params do not decode or it spends no key.
fn planned_transfer(
	conn: &rusqlite::Connection,
	scheduled: &ScheduledDurableTx,
) -> Result<Option<PlannedTransfer>, crate::store::DbError> {
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

/// What to do with the transfer at `index` of `view` at unix time `now`: what the
/// chain says, except that a transfer waiting past its limit is given up.
fn decide_transfer(
	planned_transfer: &PlannedTransfer,
	view: &ChainView,
	index: usize,
	now: u64,
) -> TransferDecision {
	let decision = decide_from_chain(planned_transfer, view, index);
	let waited_too_long = now >= planned_transfer.params.give_up_after;
	if decision == TransferDecision::Wait && waited_too_long {
		return TransferDecision::GiveUp;
	}
	decision
}

/// What the chain says about the transfer at `index` of `view`. A source that
/// does not hold the instance waits, since a reorg can put it back; an
/// occupied destination or a used-up move budget never changes.
fn decide_from_chain(
	planned_transfer: &PlannedTransfer,
	view: &ChainView,
	index: usize,
) -> TransferDecision {
	if planned_transfer.attempts >= MAX_ATTEMPTS || view.destinations[index].is_some() {
		return TransferDecision::GiveUp;
	}
	let held = view.sources[index].filter(|held| held.instance == planned_transfer.params.instance);
	let Some(held) = held else { return TransferDecision::Wait };
	if held.moves >= MAXIMUM_MOVES {
		return TransferDecision::GiveUp;
	}
	let locked = view.locks.until[index].is_some_and(|until| until > view.locks.now);
	if locked {
		return TransferDecision::Wait;
	}
	TransferDecision::Build { state_nonce: held.state_nonce }
}

fn policy_error(error: impl core::fmt::Display) -> PolicyError {
	PolicyError { reason: error.to_string() }
}

#[cfg(test)]
mod tests;
