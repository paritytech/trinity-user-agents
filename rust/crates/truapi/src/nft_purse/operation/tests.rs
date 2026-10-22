use futures::{StreamExt, executor::block_on};
use subxt::utils::H256;

use super::*;
use crate::{
	nft::{HeldItem, NftLedger, PurseTag, test_ledger},
	store::{Db, open_temporary_core_db},
};

fn key(byte: u8) -> ItemKey {
	ItemKey([byte; 32])
}

fn progress(byte: u8, state: TransferState) -> TransferProgress {
	TransferProgress {
		consumed: key(byte),
		minted: key(byte + 100),
		tx_hash: Some(H256([byte + 200; 32])),
		state,
	}
}

fn waiting(byte: u8) -> TransferProgress {
	TransferProgress { tx_hash: None, ..progress(byte, TransferState::Waiting) }
}

fn landed(byte: u8) -> TransferProgress {
	progress(byte, TransferState::Landed { finalized: false })
}

fn finalized(byte: u8) -> TransferProgress {
	progress(byte, TransferState::Landed { finalized: true })
}

fn failed(byte: u8) -> TransferProgress {
	progress(byte, TransferState::Failed)
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
	block_on(purses.rebalance(&caller, PurseId(0), PurseId(1), &[item], key)).unwrap();
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
fn held_item_in_new_purses(ledger: &NftLedger) -> NftPurseItemKey {
	for name in ["from", "to"] {
		block_on(ledger.create_purse(name.to_owned(), PurseTag::default())).unwrap();
	}
	let one = core::num::NonZeroU32::new(1).unwrap();
	let key = block_on(ledger.allocate_items(PurseId(0), one)).unwrap()[0];
	let held = HeldItem { collection: 1, item: 1, instance: 7 };
	block_on(ledger.record_holdings(vec![(key, Some(held))])).unwrap();
	NftPurseItemKey(key.0)
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
