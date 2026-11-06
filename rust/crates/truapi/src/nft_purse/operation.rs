//! The RFC's chain operations over Layer 1 transfers: registering a rebalance or
//! a transfer under a product's idempotency key, and the status a product
//! observes for it.

use std::collections::BTreeSet;

use futures::{StreamExt, stream::BoxStream};
use parity_scale_codec::Encode;
use sp_crypto_hashing::blake2_256;
use truapi::latest::{
	NftPurseChequeId, NftPurseClearedItem, NftPurseClearingReference, NftPurseIdempotencyKey,
	NftPurseItemKey, NftPurseOwner, NftPurseStatus,
};

use super::NftPurses;
use crate::{
	durable::RegistrationError,
	nft::{
		ItemKey, NftLedger, OperationKey, PurseId, StartOperationError, TransferProgress,
		TransferRequest, TransferState, TransferType,
	},
	platform::ProductId,
	store::DbError,
	stream_extensions::StreamExtensions,
};

/// An operation nothing refused when it was prepared: its key was unused and
/// its items idle. Starting it checks both again.
pub struct PreparedOperation {
	ledger: NftLedger,
	request: TransferRequest,
}

impl PreparedOperation {
	/// Registers the operation's transfers.
	pub async fn start(self) -> Result<(), RegistrationError<StartOperationError>> {
		self.ledger.start_transfers(self.request).await
	}
}

impl NftPurses {
	/// Prepares transferring `items` from `from` into fresh keys of `to`, as
	/// the operation `caller` names `idempotency_key`. The caller asks for
	/// [`drain_needs_confirmation`] before starting it.
	pub async fn prepare_rebalance(
		&self,
		caller: &ProductId,
		from: PurseId,
		to: PurseId,
		items: BTreeSet<ItemKey>,
		idempotency_key: NftPurseIdempotencyKey,
	) -> Result<PreparedOperation, RegistrationError<StartOperationError>> {
		let operation = operation_key(caller, idempotency_key);
		let transfer = TransferType::Local { inputs: items, into: to };
		self.prepare_unless_refused(TransferRequest { operation, from, transfer }).await
	}

	/// Prepares transferring `item`, held in `from`, to `destination` outside the
	/// purses, as the operation `caller` names `idempotency_key`.
	pub async fn prepare_transfer(
		&self,
		caller: &ProductId,
		from: PurseId,
		item: NftPurseItemKey,
		destination: NftPurseItemKey,
		idempotency_key: NftPurseIdempotencyKey,
	) -> Result<PreparedOperation, RegistrationError<StartOperationError>> {
		let operation = operation_key(caller, idempotency_key);
		let transfer =
			TransferType::Foreign { input: ItemKey(item.0), into: ItemKey(destination.0) };
		self.prepare_unless_refused(TransferRequest { operation, from, transfer }).await
	}

	/// `request` as an operation to start, unless it is already refused.
	async fn prepare_unless_refused(
		&self,
		request: TransferRequest,
	) -> Result<PreparedOperation, RegistrationError<StartOperationError>> {
		self.ledger
			.check_transfers_can_start(&request)
			.await
			.map_err(RegistrationError::Hook)?;
		Ok(PreparedOperation { ledger: self.ledger.clone(), request })
	}

	/// The purse `item` belongs to, if it is one of ours.
	pub async fn purse_of(&self, item: NftPurseItemKey) -> Result<Option<PurseId>, DbError> {
		self.ledger.purse_of(ItemKey(item.0)).await
	}

	/// The status of the operation `caller` registered as `idempotency_key`,
	/// now and after every change, without repeats, ending after a terminal
	/// one. `None` while `caller` registered no such operation.
	pub fn observe_status(
		&self,
		caller: &ProductId,
		idempotency_key: NftPurseIdempotencyKey,
	) -> BoxStream<'static, Result<Option<NftPurseStatus>, DbError>> {
		let operation = operation_key(caller, idempotency_key);
		self.ledger
			.observe_operation(&operation)
			.map(|transfers| transfers.map(status_of))
			.distinct_until_changed()
			.take_until_inclusive(|read| read.as_ref().is_ok_and(is_terminal))
	}
}

/// Whether `caller` must confirm draining a purse `owner` holds: one owned by
/// Root or by another product.
pub fn drain_needs_confirmation(caller: &ProductId, owner: &NftPurseOwner) -> bool {
	!matches!(owner, NftPurseOwner::Product(id) if id == caller)
}

/// The idempotency key of the operation `domain` names for cheque `id`, as
/// products derive it to observe a revoke or a refund.
pub fn cheque_idempotency_key(domain: &[u8], id: &NftPurseChequeId) -> NftPurseIdempotencyKey {
	let preimage = [domain, &id.0].concat();
	NftPurseIdempotencyKey(blake2_256(&preimage))
}

/// The operation `caller` names `idempotency_key`, scoped to `caller`.
pub fn operation_key(caller: &ProductId, idempotency_key: NftPurseIdempotencyKey) -> OperationKey {
	OperationKey::new(caller.as_str().as_bytes(), idempotency_key.0)
}

/// The status of an operation whose transfers stand as `transfers`, or `None` when it
/// has none.
fn status_of(transfers: Vec<TransferProgress>) -> Option<NftPurseStatus> {
	(!transfers.is_empty()).then(|| fold_status(&transfers))
}

/// The RFC status of an operation whose transfers stand as `transfers`.
fn fold_status(transfers: &[TransferProgress]) -> NftPurseStatus {
	let cleared: Vec<_> = transfers.iter().filter_map(cleared_item).collect();
	let failed = failed_items(transfers);
	let settled = transfers.iter().all(is_settled);
	match (settled, failed.is_empty(), cleared.is_empty()) {
		(true, true, _) => {
			NftPurseStatus::Cleared { reference: clearing_reference(transfers), cleared }
		},
		(true, false, _) => {
			NftPurseStatus::Incomplete { reference: clearing_reference(transfers), cleared, failed }
		},
		(false, _, false) => NftPurseStatus::Clearing { cleared },
		(false, _, true) => NftPurseStatus::InProgress,
	}
}

/// The source keys of the transfers that failed.
fn failed_items(transfers: &[TransferProgress]) -> Vec<NftPurseItemKey> {
	let failed = transfers.iter().filter(|progress| progress.state == TransferState::Failed);
	failed.map(|progress| NftPurseItemKey(progress.consumed.0)).collect()
}

/// Whether nothing will change `progress` again: it is finalized or failed.
fn is_settled(progress: &TransferProgress) -> bool {
	matches!(progress.state, TransferState::Landed { finalized: true } | TransferState::Failed)
}

/// `progress` as a cleared item, when its transfer landed.
fn cleared_item(progress: &TransferProgress) -> Option<NftPurseClearedItem> {
	let TransferState::Landed { finalized } = progress.state else { return None };
	let consumed = NftPurseItemKey(progress.consumed.0);
	Some(NftPurseClearedItem { consumed, minted: NftPurseItemKey(progress.minted.0), finalized })
}

/// The landed transfers of `transfers`, each with the transaction that transferred it,
/// sorted by item, and their binary Merkle root.
fn clearing_reference(transfers: &[TransferProgress]) -> NftPurseClearingReference {
	let mut leaves: Vec<_> = transfers
		.iter()
		.filter(|progress| matches!(progress.state, TransferState::Landed { .. }))
		.filter_map(|progress| Some((NftPurseItemKey(progress.consumed.0), progress.tx_hash?.0)))
		.collect();
	leaves.sort_by_key(|(item, _)| item.0);
	let encoded = leaves.iter().map(|(item, tx_hash)| (item.0, *tx_hash).encode());
	let root = binary_merkle_tree::merkle_root::<ClearingHasher, _>(encoded);
	NftPurseClearingReference { root, leaves }
}

/// Blake2-256 as the hasher `binary_merkle_tree` builds the root with.
struct ClearingHasher;

impl hash_db::Hasher for ClearingHasher {
	type Out = [u8; 32];
	type StdHasher = std::hash::DefaultHasher;
	const LENGTH: usize = 32;

	fn hash(bytes: &[u8]) -> [u8; 32] {
		blake2_256(bytes)
	}
}

/// Whether `status` is the operation's last: `Cleared` or `Incomplete`.
fn is_terminal(status: &Option<NftPurseStatus>) -> bool {
	matches!(status, Some(NftPurseStatus::Cleared { .. } | NftPurseStatus::Incomplete { .. }))
}

#[cfg(test)]
mod tests;
