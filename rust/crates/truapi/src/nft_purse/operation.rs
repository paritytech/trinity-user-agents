//! The RFC's chain operations over Layer 1 transfers: registering a rebalance or
//! a transfer under a product's idempotency key, and the status a product
//! observes for it.

use core::future::ready;

use futures::{StreamExt, stream::BoxStream};
use parity_scale_codec::Encode;
use sp_crypto_hashing::blake2_256;
use truapi::latest::{
	NftPurseClearedItem, NftPurseClearingReference, NftPurseIdempotencyKey, NftPurseItemKey,
	NftPurseStatus,
};

use super::{NftPurses, PurseOwner};
use crate::{
	durable::RegistrationError,
	nft::{
		ItemKey, OperationKey, PurseId, StartOperationError, TransferDestinations,
		TransferProgress, TransferRequest, TransferState,
	},
	platform::ProductId,
	store::DbError,
};

/// Why an operation was not registered.
pub type StartError = RegistrationError<StartOperationError>;

impl NftPurses {
	/// Registers transferring `keys` from `from` into fresh keys of `to`, as the
	/// operation `caller` names `idempotency_key`. The caller asks for
	/// [`drain_needs_confirmation`] first.
	pub async fn rebalance(
		&self,
		caller: &ProductId,
		from: PurseId,
		to: PurseId,
		keys: &[NftPurseItemKey],
		idempotency_key: NftPurseIdempotencyKey,
	) -> Result<(), StartError> {
		let items = keys.iter().map(|key| ItemKey(key.0)).collect();
		let operation = operation_key(caller, idempotency_key);
		let destinations = TransferDestinations::AllocatedIn(to);
		self.ledger
			.start_transfers(TransferRequest { operation, from, items, destinations })
			.await
	}

	/// Registers transferring `item`, held in `from`, to `destination` outside the
	/// purses, as the operation `caller` names `idempotency_key`.
	pub async fn transfer(
		&self,
		caller: &ProductId,
		from: PurseId,
		item: NftPurseItemKey,
		destination: NftPurseItemKey,
		idempotency_key: NftPurseIdempotencyKey,
	) -> Result<(), StartError> {
		let items = [ItemKey(item.0)].into_iter().collect();
		let operation = operation_key(caller, idempotency_key);
		let destinations = TransferDestinations::Foreign(ItemKey(destination.0));
		self.ledger
			.start_transfers(TransferRequest { operation, from, items, destinations })
			.await
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
		let statuses = self
			.ledger
			.observe_operation(&operation)
			.map(|transfers| transfers.map(status_of));
		statuses
			.scan(StatusChanges::default(), |changes, status| ready(changes.next(status)))
			.filter_map(ready)
			.boxed()
	}
}

/// Whether `caller` must confirm draining a purse `owner` holds: one owned by
/// Root or by another product.
pub fn drain_needs_confirmation(caller: &ProductId, owner: &PurseOwner) -> bool {
	!matches!(owner, PurseOwner::Product(id) if id == caller)
}

/// The operation `caller` names `idempotency_key`, scoped to `caller`.
fn operation_key(caller: &ProductId, idempotency_key: NftPurseIdempotencyKey) -> OperationKey {
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
	let failed: Vec<_> = transfers
		.iter()
		.filter(|progress| progress.state == TransferState::Failed)
		.map(|progress| NftPurseItemKey(progress.consumed.0))
		.collect();
	let finalized =
		|progress: &TransferProgress| progress.state == TransferState::Landed { finalized: true };
	let settled = transfers
		.iter()
		.all(|progress| finalized(progress) || progress.state == TransferState::Failed);
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

/// The statuses a product sees: each change once, and nothing after the
/// first terminal one.
#[derive(Default)]
struct StatusChanges {
	last: Option<Option<NftPurseStatus>>,
	ended: bool,
}

impl StatusChanges {
	/// `None` ends the stream, `Some(None)` skips a repeat, and
	/// `Some(Some(_))` passes `read` on.
	fn next(
		&mut self,
		read: Result<Option<NftPurseStatus>, DbError>,
	) -> Option<Option<Result<Option<NftPurseStatus>, DbError>>> {
		if self.ended {
			return None;
		}
		let Ok(status) = read else { return Some(Some(read)) };
		if self.last.as_ref() == Some(&status) {
			return Some(None);
		}
		self.ended = status.as_ref().is_some_and(is_terminal);
		self.last = Some(status.clone());
		Some(Some(Ok(status)))
	}
}

fn is_terminal(status: &NftPurseStatus) -> bool {
	matches!(status, NftPurseStatus::Cleared { .. } | NftPurseStatus::Incomplete { .. })
}

#[cfg(test)]
mod tests;
