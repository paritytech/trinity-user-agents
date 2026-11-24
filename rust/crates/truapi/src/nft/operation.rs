//! NFT operations: transfers of items registered together under a caller's
//! idempotency key, and where each transfer stands.

use core::num::NonZeroU32;
use std::collections::BTreeSet;

use parity_scale_codec::Encode;
use rusqlite::{Connection, Transaction};
use sp_crypto_hashing::blake2_256;
use subxt::utils::H256;

use super::{
	dao,
	item::{AllocateItemsError, Allocation, allocate_items_in},
	keys::{ItemKey, ItemKeyDeriver},
	paid_moves::{MovePayments, PaidMoveError, UnplannedSource},
	purse::PurseId,
	scarcity::{TRANSFER_WAIT_LIMIT_SECS, TransferParams, TransferSigner},
	standing::{IdleItem, ItemRefusal, idle_items},
};
use crate::{
	durable::{
		DeferredSubmission, DomainId, DurableTxId, DurableTxState, DurableTxStatus, GroupId,
		group_exists,
	},
	installation::InstallationId,
	store::DbError,
	unix_time::current_unix_secs,
};

/// The durable domain NFT transfers are registered under.
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

	/// The durable group the operation's transfers belong to.
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
	/// No key was handed off under the tag.
	#[error("no key was handed off under this tag")]
	TagNotFound,
	/// No operation is registered under the import's key.
	#[error("no import is registered under this key")]
	ImportNotFound,
	/// A move of the import can still land or fail.
	#[error("the import has moves that are not final yet")]
	ImportUnsettled,
	/// Every item the import landed is consumed already.
	#[error("no item the import landed is idle")]
	NothingIdle,
	/// The destination keys could not be allocated.
	#[error(transparent)]
	Allocate(#[from] AllocateItemsError),
	/// The move budget could not be read, or a claim not registered.
	#[error(transparent)]
	PaidMoves(#[from] PaidMoveError),
	/// The database failed.
	#[error(transparent)]
	Db(#[from] DbError),
}

impl From<UnplannedSource> for StartOperationError {
	fn from(UnplannedSource(source): UnplannedSource) -> Self {
		Self::ItemUnavailable(source)
	}
}

impl From<ItemRefusal> for StartOperationError {
	fn from(refusal: ItemRefusal) -> Self {
		match refusal {
			ItemRefusal::NotFound(key) => Self::ItemNotFound(key),
			ItemRefusal::Unavailable(key) => Self::ItemUnavailable(key),
		}
	}
}

/// Which items an operation transfers and where they go.
#[derive(Clone)]
pub enum TransferType {
	/// Each of `inputs` into a fresh key allocated in `into`.
	Local {
		/// The items, in the order their transfers are registered.
		inputs: BTreeSet<ItemKey>,
		/// The purse the fresh keys are allocated in.
		into: PurseId,
	},
	/// `input` to `into`, a key outside every purse.
	Foreign {
		/// The item.
		input: ItemKey,
		/// The key it goes to.
		into: ItemKey,
	},
}

impl TransferType {
	/// The items the transfers spend, in the order they are registered.
	pub fn inputs(&self) -> BTreeSet<ItemKey> {
		match self {
			Self::Local { inputs, .. } => inputs.clone(),
			Self::Foreign { input, .. } => BTreeSet::from([*input]),
		}
	}
}

/// Transfers of items, each from its key in `from`, registered as one operation.
#[derive(Clone)]
pub struct TransferRequest {
	/// The operation they form.
	pub operation: OperationKey,
	/// The purse every item is spent from.
	pub from: PurseId,
	/// Which items go where.
	pub transfer: TransferType,
}

/// What allocating destination keys needs: the installation they are
/// derived under and the deriver.
pub struct DestinationKeys<'a> {
	/// The installation new keys are allocated under.
	pub installation: InstallationId,
	/// Derives new keys.
	pub deriver: &'a dyn ItemKeyDeriver,
}

/// Works out, inside the registration write, each transfer of `request`,
/// paid as `payments` says, or refuses: the operation exists, an item is not
/// idle in `from`, or destination keys cannot be allocated.
pub fn plan_transfers(
	tx: &Transaction<'_>,
	request: &TransferRequest,
	keys: DestinationKeys<'_>,
	payments: &MovePayments,
) -> Result<Vec<DeferredSubmission>, StartOperationError> {
	validate_operation_unused(tx, &request.operation)?;
	let sources = idle_items::<StartOperationError>(tx, request.from, &request.transfer.inputs())?;
	let destinations = match request.transfer {
		TransferType::Local { into, .. } => allocate_destinations(tx, into, sources.len(), keys)?,
		TransferType::Foreign { into, .. } => vec![into],
	};
	let give_up_after = current_unix_secs() + TRANSFER_WAIT_LIMIT_SECS;
	let submission = |(source, destination): (IdleItem, ItemKey)| {
		transfer_submission(&source, destination, give_up_after, payments)
	};
	let submissions = sources.into_iter().zip(destinations).map(submission);
	Ok(submissions.collect::<Result<_, UnplannedSource>>()?)
}

/// The transfer of `source` to `destination`, given up at `give_up_after`,
/// paid as `payments` says for its key.
fn transfer_submission(
	source: &IdleItem,
	destination: ItemKey,
	give_up_after: u64,
	payments: &MovePayments,
) -> Result<DeferredSubmission, UnplannedSource> {
	let payment = payments.payment_of(&source.key)?;
	let params = TransferParams {
		source: source.key,
		signer: TransferSigner::Derived(source.slot),
		destination,
		submitted_attempt: None,
		give_up_after,
		mode: payment.transfer_mode(),
	};
	Ok(payment.submission(params))
}

/// Each item of `request` with the fee-less moves its NFT used, read before
/// the registration write, or the refusal [`validate_can_start`] answers.
pub fn read_source_moves(
	conn: &Connection,
	request: &TransferRequest,
) -> Result<Vec<(ItemKey, Option<u16>)>, StartOperationError> {
	let sources = idle_items::<StartOperationError>(conn, request.from, &request.transfer.inputs())?;
	Ok(sources.into_iter().map(|source| (source.key, source.moves)).collect())
}

/// Refuses `request` when its operation is already registered or one of its
/// items is not idle in its purse.
pub fn validate_can_start(
	conn: &Connection,
	request: &TransferRequest,
) -> Result<(), StartOperationError> {
	validate_operation_unused(conn, &request.operation)?;
	idle_items::<StartOperationError>(conn, request.from, &request.transfer.inputs()).map(|_| ())
}

/// Refuses `operation` when it is already registered.
pub fn validate_operation_unused(
	conn: &Connection,
	operation: &OperationKey,
) -> Result<(), StartOperationError> {
	if group_exists(conn, &nft_domain(), operation.group())? {
		return Err(StartOperationError::OperationExists);
	}
	Ok(())
}

/// Refuses the first of `sources` that a live transaction already moves.
pub fn validate_not_moving(
	conn: &Connection,
	sources: &BTreeSet<ItemKey>,
) -> Result<(), StartOperationError> {
	let keys: Vec<ItemKey> = sources.iter().copied().collect();
	let moving = dao::moving_keys(conn, &keys)?;
	match moving.first() {
		Some(key) => Err(StartOperationError::ItemUnavailable(*key)),
		None => Ok(()),
	}
}

/// `item_count` fresh keys in `purse`.
pub fn allocate_destinations(
	tx: &Transaction<'_>,
	purse: PurseId,
	item_count: usize,
	keys: DestinationKeys<'_>,
) -> Result<Vec<ItemKey>, StartOperationError> {
	let count = u32::try_from(item_count).ok().and_then(NonZeroU32::new);
	let count = count.ok_or(StartOperationError::NoItems)?;
	let allocation = Allocation { purse, installation: keys.installation, count };
	Ok(allocate_items_in(tx, allocation, keys.deriver)??)
}

/// Records that the transfer with each id of `ids` spends the item at the same
/// position of `items`.
pub fn record_inputs(
	tx: &Transaction<'_>,
	items: impl IntoIterator<Item = ItemKey>,
	ids: &[DurableTxId],
) -> Result<(), StartOperationError> {
	let inputs: Vec<_> = items.into_iter().zip(ids.iter().copied()).collect();
	Ok(dao::insert_inputs(tx, &inputs)?)
}

/// Where one transfer of an operation stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransferProgress {
	/// Key the NFT leaves.
	pub consumed: ItemKey,
	/// Key the NFT goes to.
	pub minted: ItemKey,
	/// Hash of its latest attempt, once one was built.
	pub tx_hash: Option<H256>,
	/// Whether it waits, landed or failed.
	pub state: TransferState,
}

/// Whether a transfer waits, landed or failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransferState {
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

impl From<DurableTxStatus> for TransferState {
	fn from(status: DurableTxStatus) -> Self {
		match status {
			DurableTxStatus::PendingSubmission | DurableTxStatus::Pending => Self::Waiting,
			DurableTxStatus::PendingSuccess { .. } => Self::Landed { finalized: false },
			DurableTxStatus::FinalizedSuccess { .. } => Self::Landed { finalized: true },
			DurableTxStatus::Failure(_) => Self::Failed,
		}
	}
}

/// Where each of `transfers` stands. A transfer whose params cannot be read
/// is an error: registration writes them with every transfer.
pub fn progress_of(transfers: Vec<DurableTxState>) -> Result<Vec<TransferProgress>, DbError> {
	transfers.into_iter().map(transfer_progress).collect()
}

/// The latest of `transfers` for each key they consume, in the order those
/// latest moves were registered. A later move of a key replaces an earlier
/// one, as when a repeated revoke retries a reclaim that failed.
pub fn latest_per_source(transfers: Vec<TransferProgress>) -> Vec<TransferProgress> {
	let mut seen = BTreeSet::new();
	let mut latest: Vec<_> = transfers
		.into_iter()
		.rev()
		.filter(|transfer| seen.insert(transfer.consumed))
		.collect();
	latest.reverse();
	latest
}

fn transfer_progress(state: DurableTxState) -> Result<TransferProgress, DbError> {
	let unreadable = |what: String| {
		DbError::Connection(format!("transfer {:?} has no readable {what}", state.id))
	};
	let plan = state.plan.as_ref().ok_or_else(|| unreadable("plan".into()))?;
	let params = TransferParams::try_from(&plan.params)
		.map_err(|error| unreadable(format!("params: {error}")))?;
	Ok(TransferProgress {
		consumed: params.source,
		minted: params.destination,
		tx_hash: state.tx_hash,
		state: TransferState::from(state.status),
	})
}

#[cfg(test)]
mod tests;
