//! [`ScarcityMovePolicy`]: builds each NFT move when the engine submits it,
//! from chain state read fresh at the finalized head.

use std::{collections::HashMap, sync::Arc};

use parity_scale_codec::{Decode, Encode};

use super::{
	chain::{KeyLocks, ScarcityChain, ScarcityNft},
	transfer::{MoveRuntime, NftMove, sign_move},
};
use crate::{
	chain::{HashAndNumber, MortalExtrinsic, Mortality},
	durable::{
		DurableTxEntry, DurableTxId, PolicyError, PolicyId, PolicyParams, Preparation,
		ScheduledDurableTx, SubmissionPolicy, VerdictFailureKind,
	},
	nft::{ItemKey, ItemSigner, ItemSlot, dao},
	store::Db,
};

/// The name rows store for this policy.
pub const MOVE_POLICY: &str = "nft.scarcity.move";

/// Attempts after which a move is given up, so a rejection that repeats
/// cannot loop forever.
const MAX_ATTEMPTS: u32 = 5;

/// Fee-less moves an instance allows between paid ones, the pallet's
/// `MaximumMoves`. Paid moves are not built yet.
const MAXIMUM_MOVES: u16 = 16;

/// Blocks a move stays valid for after its birth.
const MOVE_PERIOD: u64 = 64;

/// What the policy stores with each move: the instance it moves and the key
/// it goes to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Encode, Decode)]
pub struct MoveParams {
	/// The instance the source key holds when the move is registered.
	pub instance: u64,
	/// The key it goes to.
	pub destination: ItemKey,
}

impl From<MoveParams> for PolicyParams {
	fn from(params: MoveParams) -> Self {
		PolicyParams(params.encode())
	}
}

impl TryFrom<&PolicyParams> for MoveParams {
	type Error = parity_scale_codec::Error;

	fn try_from(params: &PolicyParams) -> Result<Self, Self::Error> {
		Self::decode(&mut &params.0[..])
	}
}

/// What the policy runs on.
pub struct ScarcityMovePolicyDeps {
	/// The core database, for the key each move spends.
	pub db: Db,
	/// Scarcity storage, read fresh for every batch.
	pub chain: Arc<dyn ScarcityChain>,
	/// Signs with the key each move spends.
	pub signer: Arc<dyn ItemSigner>,
}

/// Builds NFT moves: one fee-less `Scarcity::transfer` per spent key.
pub struct ScarcityMovePolicy {
	deps: ScarcityMovePolicyDeps,
}

impl ScarcityMovePolicy {
	/// A policy over `deps`.
	pub fn new(deps: ScarcityMovePolicyDeps) -> Self {
		Self { deps }
	}
}

/// One scheduled move with everything needed to judge and build it.
struct PlannedMove {
	id: DurableTxId,
	attempts: u32,
	params: MoveParams,
	source: ItemKey,
	slot: ItemSlot,
}

/// What the chain says about each planned move, in the same order.
struct ChainView {
	sources: Vec<Option<ScarcityNft>>,
	destinations: Vec<Option<ScarcityNft>>,
	locks: KeyLocks,
}

/// What to do with one move this time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MoveDecision {
	/// It can never land.
	GiveUp,
	/// Its key is locked. It waits and is asked about again.
	Wait,
	/// Build it, signing over this revision of the instance.
	Build { state_nonce: u64 },
}

#[async_trait::async_trait]
impl SubmissionPolicy for ScarcityMovePolicy {
	fn id(&self) -> PolicyId {
		PolicyId::new(MOVE_POLICY)
	}

	/// Every proven failure is built again: the next build reads the chain
	/// afresh and gives up once the move can never land.
	fn can_retry(&self, _: &DurableTxEntry, _: &PolicyParams, _: VerdictFailureKind) -> bool {
		true
	}

	async fn prepare(
		&self,
		batch: &[ScheduledDurableTx],
	) -> Result<HashMap<DurableTxId, Preparation>, PolicyError> {
		let (planned, unplanned) = self.read_planned_moves(batch).await?;
		let (head, runtime) = self.deps.chain.finalized_runtime().await.map_err(policy_error)?;
		let view = self.read_chain_view(head, &planned).await?;
		let mortality = Mortality::new(head, MOVE_PERIOD).map_err(policy_error)?;
		let mut prepared = self.prepare_moves(&planned, &view, &runtime, mortality)?;
		prepared.extend(unplanned.into_iter().map(|id| (id, Preparation::GiveUp)));
		Ok(prepared)
	}
}

impl ScarcityMovePolicy {
	/// The moves of `batch` with their spent key, and the ids of rows that
	/// name no move this policy can build, which can only be given up.
	async fn read_planned_moves(
		&self,
		batch: &[ScheduledDurableTx],
	) -> Result<(Vec<PlannedMove>, Vec<DurableTxId>), PolicyError> {
		let batch = batch.to_vec();
		let read = move |conn: &rusqlite::Connection| {
			let mut planned = Vec::new();
			let mut unplanned = Vec::new();
			for scheduled in batch {
				match planned_move(conn, &scheduled)? {
					Some(planned_move) => planned.push(planned_move),
					None => unplanned.push(scheduled.id),
				}
			}
			Ok((planned, unplanned))
		};
		self.deps.db.read(read).await.map_err(policy_error)
	}

	/// The sources, destinations and source locks of `planned` at `head`.
	async fn read_chain_view(
		&self,
		head: HashAndNumber,
		planned: &[PlannedMove],
	) -> Result<ChainView, PolicyError> {
		let sources: Vec<ItemKey> = planned.iter().map(|planned| planned.source).collect();
		let destinations: Vec<ItemKey> =
			planned.iter().map(|planned| planned.params.destination).collect();
		let chain = &self.deps.chain;
		let (sources_held, destinations_held, locks) = futures::future::try_join3(
			chain.nfts_at(head.hash, &sources),
			chain.nfts_at(head.hash, &destinations),
			chain.locks_at(head.hash, &sources),
		)
		.await
		.map_err(policy_error)?;
		Ok(ChainView { sources: sources_held, destinations: destinations_held, locks })
	}

	/// What to do with each of `planned`. A move that waits is left out.
	fn prepare_moves(
		&self,
		planned: &[PlannedMove],
		view: &ChainView,
		runtime: &MoveRuntime,
		mortality: Mortality,
	) -> Result<HashMap<DurableTxId, Preparation>, PolicyError> {
		let mut prepared = HashMap::new();
		for (index, planned_move) in planned.iter().enumerate() {
			let preparation = match decide_move(planned_move, view, index) {
				MoveDecision::GiveUp => Preparation::GiveUp,
				MoveDecision::Wait => continue,
				MoveDecision::Build { state_nonce } => {
					let signed = self.build(planned_move, state_nonce, runtime, mortality)?;
					Preparation::Ready(signed)
				},
			};
			prepared.insert(planned_move.id, preparation);
		}
		Ok(prepared)
	}

	/// `planned_move` signed by its source key. Fails without a session.
	fn build(
		&self,
		planned_move: &PlannedMove,
		state_nonce: u64,
		runtime: &MoveRuntime,
		mortality: Mortality,
	) -> Result<MortalExtrinsic, PolicyError> {
		let signer = self.deps.signer.item_signer(planned_move.slot).map_err(policy_error)?;
		let MoveParams { instance, destination } = planned_move.params;
		let nft_move = NftMove { instance, state_nonce, destination };
		sign_move(&signer, runtime, &nft_move, mortality).map_err(policy_error)
	}
}

/// `scheduled` with the key it spends, or `None` when it names no move: its
/// params do not decode or it spends no key.
fn planned_move(
	conn: &rusqlite::Connection,
	scheduled: &ScheduledDurableTx,
) -> Result<Option<PlannedMove>, crate::store::DbError> {
	let Ok(params) = MoveParams::try_from(&scheduled.plan.params) else { return Ok(None) };
	let input = dao::input_of(conn, scheduled.id)?;
	Ok(input.map(|(source, slot)| PlannedMove {
		id: scheduled.id,
		attempts: scheduled.attempts,
		params,
		source,
		slot,
	}))
}

/// What to do with the move at `index` of `view`.
fn decide_move(planned_move: &PlannedMove, view: &ChainView, index: usize) -> MoveDecision {
	if planned_move.attempts >= MAX_ATTEMPTS || view.destinations[index].is_some() {
		return MoveDecision::GiveUp;
	}
	let Some(held) = view.sources[index] else { return MoveDecision::GiveUp };
	if held.instance != planned_move.params.instance || held.moves >= MAXIMUM_MOVES {
		return MoveDecision::GiveUp;
	}
	let locked = view.locks.until[index].is_some_and(|until| until > view.locks.now);
	if locked {
		return MoveDecision::Wait;
	}
	MoveDecision::Build { state_nonce: held.state_nonce }
}

fn policy_error(error: impl core::fmt::Display) -> PolicyError {
	PolicyError { reason: error.to_string() }
}

#[cfg(test)]
mod tests;
