//! NFT operations: moves of items registered together under a caller's
//! idempotency key, and where each move stands.

use core::num::NonZeroU32;
use std::collections::BTreeSet;

use parity_scale_codec::Encode;
use rusqlite::Transaction;
use sp_crypto_hashing::blake2_256;
use subxt::utils::H256;

use super::{
	dao::{self, KeyStanding},
	item::{AllocateItemsError, Allocation, allocate_items_in},
	keys::{ItemKey, ItemKeyDeriver},
	purse::PurseId,
	scarcity::MoveParams,
};
use crate::{
	durable::{
		DomainId, DurableTxId, DurableTxState, DurableTxStatus, GroupId, PolicyParams, group_exists,
	},
	installation::InstallationId,
	store::DbError,
};

/// The durable domain NFT moves are registered under.
pub fn nft_domain() -> DomainId {
	DomainId::new("nft")
}

/// Names one operation: an idempotency key within the scope of the caller
/// that chose it. Layer 1 hashes the scope and never reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OperationKey(GroupId);

impl OperationKey {
	/// The operation `idempotency_key` names within `scope`.
	pub fn new(scope: &[u8], idempotency_key: [u8; 32]) -> Self {
		let digest = blake2_256(&(scope, idempotency_key).encode());
		Self(GroupId::new(format!("nft-op:{}", hex::encode(digest))))
	}

	/// The durable group the operation's moves belong to.
	pub fn group(&self) -> &GroupId {
		&self.0
	}
}

/// Why an operation was not registered. Nothing was written.
#[derive(Debug, thiserror::Error)]
pub enum StartOperationError {
	/// The operation names no item.
	#[error("an operation needs at least one item, got none")]
	NoItems,
	/// An operation is already registered under the same key.
	#[error("an operation with this key is already registered")]
	OperationExists,
	/// The item is not in the purse, or holds no NFT.
	#[error("item {} is not held in the purse", hex::encode(.0.0))]
	ItemNotFound(ItemKey),
	/// The item is handed off or spent by a live transaction.
	#[error("item {} is in use", hex::encode(.0.0))]
	ItemUnavailable(ItemKey),
	/// The destination keys could not be allocated.
	#[error(transparent)]
	Allocate(#[from] AllocateItemsError),
	/// The database failed.
	#[error(transparent)]
	Db(#[from] DbError),
}

/// Where the moves of an operation take their items.
pub enum MoveDestinations {
	/// Fresh keys allocated in this purse, one per item.
	AllocatedIn(PurseId),
	/// This key, outside every purse. Only one item can go there.
	Foreign(ItemKey),
}

/// Moves of `items`, each from its key in `from`, registered as one operation.
pub struct MoveRequest {
	/// The operation they form.
	pub operation: OperationKey,
	/// The purse every item is spent from.
	pub from: PurseId,
	/// The items, in the order their moves are registered.
	pub items: BTreeSet<ItemKey>,
	/// Where they go.
	pub destinations: MoveDestinations,
}

/// What allocating destination keys needs: the installation they are
/// derived under and the deriver.
pub struct DestinationKeys<'a> {
	/// The installation new keys are allocated under.
	pub installation: InstallationId,
	/// Derives new keys.
	pub deriver: &'a dyn ItemKeyDeriver,
}

/// Works out, inside the registration write, the params of each move of
/// `request`, or refuses: the operation exists, an item is not idle in
/// `from`, or destination keys cannot be allocated.
pub fn plan_moves(
	tx: &Transaction<'_>,
	request: &MoveRequest,
	keys: DestinationKeys<'_>,
) -> Result<Vec<PolicyParams>, StartOperationError> {
	if group_exists(tx, &nft_domain(), request.operation.group())? {
		return Err(StartOperationError::OperationExists);
	}
	let instances = idle_instances(tx, request.from, &request.items)?;
	let destinations = match request.destinations {
		MoveDestinations::AllocatedIn(purse) => allocate_destinations(tx, purse, &instances, keys)?,
		MoveDestinations::Foreign(destination) => vec![destination],
	};
	let params = instances.into_iter().zip(destinations);
	Ok(params
		.map(|(instance, destination)| MoveParams { instance, destination }.into())
		.collect())
}

/// One fresh key in `purse` for each of `instances`.
fn allocate_destinations(
	tx: &Transaction<'_>,
	purse: PurseId,
	instances: &[u64],
	keys: DestinationKeys<'_>,
) -> Result<Vec<ItemKey>, StartOperationError> {
	let count = u32::try_from(instances.len()).ok().and_then(NonZeroU32::new);
	let count = count.ok_or(StartOperationError::NoItems)?;
	let allocation = Allocation { purse, installation: keys.installation, count };
	Ok(allocate_items_in(tx, allocation, keys.deriver)??)
}

/// The instance each of `items` holds, in order, when every one is idle in
/// `purse`.
fn idle_instances(
	tx: &Transaction<'_>,
	purse: PurseId,
	items: &BTreeSet<ItemKey>,
) -> Result<Vec<u64>, StartOperationError> {
	let idle_instance = |item: &ItemKey| match dao::key_standing(tx, purse, item)? {
		KeyStanding::Idle { instance } => Ok(instance),
		KeyStanding::Spent => Err(StartOperationError::ItemUnavailable(*item)),
		KeyStanding::NotHeld => Err(StartOperationError::ItemNotFound(*item)),
	};
	items.iter().map(idle_instance).collect()
}

/// Records that the move with each id of `ids` spends the item at the same
/// position of `items`.
pub fn record_inputs(
	tx: &Transaction<'_>,
	items: &BTreeSet<ItemKey>,
	ids: &[DurableTxId],
) -> Result<(), StartOperationError> {
	let inputs: Vec<_> = items.iter().copied().zip(ids.iter().copied()).collect();
	Ok(dao::insert_inputs(tx, &inputs)?)
}

/// Where one move of an operation stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MoveProgress {
	/// Key the NFT leaves.
	pub consumed: ItemKey,
	/// Key the NFT goes to.
	pub minted: ItemKey,
	/// Hash of its latest attempt, once one was built.
	pub tx_hash: Option<H256>,
	/// Whether it waits, landed or failed.
	pub state: MoveState,
}

/// Whether a move waits, landed or failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveState {
	/// Not built or not landed yet.
	Waiting,
	/// Landed at a block, finalized or not.
	Landed {
		/// Whether that block is finalized.
		finalized: bool,
	},
	/// It can no longer land.
	Failed,
}

impl From<DurableTxStatus> for MoveState {
	fn from(status: DurableTxStatus) -> Self {
		match status {
			DurableTxStatus::PendingSubmission | DurableTxStatus::Pending => Self::Waiting,
			DurableTxStatus::PendingSuccess { .. } => Self::Landed { finalized: false },
			DurableTxStatus::FinalizedSuccess { .. } => Self::Landed { finalized: true },
			DurableTxStatus::Failure(_) => Self::Failed,
		}
	}
}

/// Where each of `moves` stands. A move whose spent key or params cannot be
/// read is an error: the operation's rows are written together.
pub fn progress_of(
	conn: &rusqlite::Connection,
	moves: Vec<DurableTxState>,
) -> Result<Vec<MoveProgress>, DbError> {
	moves.into_iter().map(|state| move_progress(conn, state)).collect()
}

fn move_progress(
	conn: &rusqlite::Connection,
	state: DurableTxState,
) -> Result<MoveProgress, DbError> {
	let unreadable = |what: &str| DbError::Connection(format!("move {:?} has no {what}", state.id));
	let (consumed, _) = dao::input_of(conn, state.id)?.ok_or_else(|| unreadable("spent key"))?;
	let plan = state.plan.as_ref().ok_or_else(|| unreadable("plan"))?;
	let params = MoveParams::try_from(&plan.params).map_err(|_| unreadable("move params"))?;
	let state_of_move = MoveState::from(state.status);
	Ok(MoveProgress {
		consumed,
		minted: params.destination,
		tx_hash: state.tx_hash,
		state: state_of_move,
	})
}

#[cfg(test)]
mod tests;
