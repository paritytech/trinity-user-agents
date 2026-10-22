//! The RFC's chain operations over Layer 1 transfers: registering a rebalance or
//! a transfer under a product's idempotency key, and the status a product
//! observes for it.

use core::future::ready;
use std::collections::BTreeSet;

use futures::{StreamExt, stream::BoxStream};
use parity_scale_codec::Encode;
use sp_crypto_hashing::blake2_256;
use truapi::latest::{
	NftPurseClearedItem, NftPurseClearingReference, NftPurseIdempotencyKey, NftPurseItemKey,
	NftPurseStatus,
};

use super::{NftPurses, PurseOwner};
use crate::{
	chain::take_until_inclusive,
	durable::RegistrationError,
	nft::{
		ItemKey, OperationKey, PurseId, StartOperationError, TransferDestinations,
		TransferProgress, TransferRequest, TransferState,
	},
	platform::ProductId,
	store::DbError,
};

impl NftPurses {
	/// Registers transferring `items` from `from` into fresh keys of `to`, as
	/// the operation `caller` names `idempotency_key`. The caller asks for
	/// [`drain_needs_confirmation`] first.
	pub async fn rebalance(
		&self,
		caller: &ProductId,
		from: PurseId,
		to: PurseId,
		items: BTreeSet<ItemKey>,
		idempotency_key: NftPurseIdempotencyKey,
	) -> Result<(), RegistrationError<StartOperationError>> {
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
	) -> Result<(), RegistrationError<StartOperationError>> {
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
		let changes = statuses
			.scan(LastStatus::default(), |last, read| ready(Some(last.report_if_changed(read))))
			.filter_map(ready)
			.boxed();
		take_until_inclusive(changes, |read| read.as_ref().is_ok_and(is_terminal))
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

/// The last status a product was shown, so a repeat is not shown again.
#[derive(Default)]
struct LastStatus(Option<Option<NftPurseStatus>>);

impl LastStatus {
	/// Returns `read` unless it repeats the last status shown, and remembers
	/// it. A failed read is always returned.
	fn report_if_changed(
		&mut self,
		read: Result<Option<NftPurseStatus>, DbError>,
	) -> Option<Result<Option<NftPurseStatus>, DbError>> {
		let Ok(status) = read else { return Some(read) };
		if self.0.as_ref() == Some(&status) {
			return None;
		}
		self.0 = Some(status.clone());
		Some(Ok(status))
	}
}

/// Whether `status` is the operation's last: `Cleared` or `Incomplete`.
fn is_terminal(status: &Option<NftPurseStatus>) -> bool {
	matches!(status, Some(NftPurseStatus::Cleared { .. } | NftPurseStatus::Incomplete { .. }))
}

#[cfg(test)]
mod tests;
