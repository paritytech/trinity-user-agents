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
mod tests {
	use futures::{StreamExt, executor::block_on};
	use subxt::utils::H256;

	use super::*;
	use crate::{
		nft::{HeldItem, NftLedger, PurseTag, test_ledger},
		store::{Db, open_temporary_core_db},
	};

	fn item_key(byte: u8) -> ItemKey {
		ItemKey([byte; 32])
	}

	fn transfer_at(byte: u8, state: TransferState) -> TransferProgress {
		TransferProgress {
			consumed: item_key(byte),
			minted: item_key(byte + 100),
			tx_hash: Some(H256([byte + 200; 32])),
			state,
		}
	}

	fn waiting(byte: u8) -> TransferProgress {
		TransferProgress { tx_hash: None, ..transfer_at(byte, TransferState::Waiting) }
	}

	fn landed(byte: u8) -> TransferProgress {
		transfer_at(byte, TransferState::Landed { finalized: false })
	}

	fn finalized(byte: u8) -> TransferProgress {
		transfer_at(byte, TransferState::Landed { finalized: true })
	}

	fn failed(byte: u8) -> TransferProgress {
		transfer_at(byte, TransferState::Failed)
	}

	fn cleared(byte: u8, finalized: bool) -> NftPurseClearedItem {
		let (consumed, minted) = (NftPurseItemKey([byte; 32]), NftPurseItemKey([byte + 100; 32]));
		NftPurseClearedItem { consumed, minted, finalized }
	}

	#[test]
	fn nothing_landed_is_in_progress() {
		assert_eq!(fold_status(&[waiting(1), waiting(2)]), NftPurseStatus::InProgress);
	}

	#[test]
	fn a_landed_move_makes_the_operation_clearing_and_lists_every_landed_move() {
		let status = fold_status(&[finalized(1), landed(2), waiting(3)]);

		assert_eq!(
			status,
			NftPurseStatus::Clearing { cleared: vec![cleared(1, true), cleared(2, false)] }
		);
	}

	#[test]
	fn every_move_finalized_clears_the_operation() {
		let status = fold_status(&[finalized(2), finalized(1)]);

		let NftPurseStatus::Cleared { cleared: items, reference } = status else {
			panic!("{status:?}")
		};
		assert_eq!(items, vec![cleared(2, true), cleared(1, true)]);
		let leaf_items: Vec<_> = reference.leaves.iter().map(|(item, _)| item.0[0]).collect();
		assert_eq!(leaf_items, vec![1, 2]);
	}

	#[test]
	fn settled_moves_with_a_failure_are_incomplete_and_name_the_failed_items() {
		let status = fold_status(&[finalized(1), failed(2)]);

		let NftPurseStatus::Incomplete { cleared: items, failed, reference } = status else {
			panic!("{status:?}")
		};
		assert_eq!(items, vec![cleared(1, true)]);
		assert_eq!(failed, vec![NftPurseItemKey([2; 32])]);
		assert_eq!(reference.leaves, vec![(NftPurseItemKey([1; 32]), [201; 32])]);
	}

	#[test]
	fn a_failure_beside_a_move_still_landing_is_not_terminal() {
		assert!(matches!(fold_status(&[landed(1), failed(2)]), NftPurseStatus::Clearing { .. }));
	}

	/// Verifiers recompute the root with `binary-merkle-tree`, so it must match
	/// that crate's construction byte for byte.
	#[test]
	fn the_clearing_root_is_the_binary_merkle_root_of_the_encoded_leaves() {
		let transfers = [
			TransferProgress { tx_hash: Some(H256([4; 32])), ..finalized(3) },
			TransferProgress { tx_hash: Some(H256([2; 32])), ..finalized(1) },
		];

		let reference = clearing_reference(&transfers);

		let expected = "b4e4e38b8537fb079a236c1dce5c3b442dec0b44b42c04015ed0df9343aacf40";
		assert_eq!(hex::encode(reference.root), expected);
	}

	/// A product listens until the operation ends. Once every transfer is
	/// final nothing changes in the ledger again, so the stream must end on
	/// the terminal status itself rather than wait for another.
	#[test]
	fn the_status_stream_ends_with_its_terminal_status() {
		let (_dir, db) = open_temporary_core_db();
		let ledger = test_ledger(db.clone());
		let purses = NftPurses::new(ledger.clone());
		let caller: ProductId = "tickets.dot".parse().unwrap();
		let item = held_item_in_new_purses(&ledger);
		let key = NftPurseIdempotencyKey([1; 32]);
		let items = BTreeSet::from([item]);
		block_on(purses.rebalance(&caller, PurseId(0), PurseId(1), items, key)).unwrap();
		finalize_every_transfer(&db);

		let observed = block_on(async {
			let statuses = purses.observe_status(&caller, key).collect::<Vec<_>>();
			futures::future::select(Box::pin(statuses), futures_timer::Delay::new(TIMEOUT)).await
		});

		let futures::future::Either::Left((statuses, _)) = observed else {
			panic!("the stream did not end after its terminal status")
		};
		assert!(matches!(statuses[..], [Ok(Some(NftPurseStatus::Cleared { .. }))]), "{statuses:?}");
	}

	#[test]
	fn repeated_statuses_are_reported_once() {
		let mut last = LastStatus::default();

		let first = last.report_if_changed(Ok(Some(NftPurseStatus::InProgress)));
		let repeat = last.report_if_changed(Ok(Some(NftPurseStatus::InProgress)));

		assert_eq!(first.map(Result::unwrap), Some(Some(NftPurseStatus::InProgress)));
		assert!(repeat.is_none());
	}

	const TIMEOUT: core::time::Duration = core::time::Duration::from_secs(5);

	/// A key of purse 0, holding an NFT, with purse 1 beside it.
	fn held_item_in_new_purses(ledger: &NftLedger) -> ItemKey {
		for name in ["from", "to"] {
			block_on(ledger.create_purse(name.to_owned(), PurseTag::default())).unwrap();
		}
		let one = core::num::NonZeroU32::new(1).unwrap();
		let key = block_on(ledger.allocate_items(PurseId(0), one)).unwrap()[0];
		let held = HeldItem { collection: 1, item: 1, instance: 7 };
		block_on(ledger.record_holdings(vec![(key, Some(held))])).unwrap();
		key
	}

	fn finalize_every_transfer(db: &Db) {
		let finalize = |tx: &rusqlite::Transaction<'_>| {
			tx.execute(
				"UPDATE durable_tx SET status = 'FINALIZED_SUCCESS', tx_hash = ?1",
				[[9u8; 32]],
			)?;
			Ok(())
		};
		block_on(db.write(finalize)).unwrap();
	}
}
