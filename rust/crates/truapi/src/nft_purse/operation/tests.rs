use subxt::utils::H256;

use super::*;

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

#[test]
fn status_changes_drop_repeats_and_end_after_a_terminal_status() {
	let cleared_status = fold_status(&[finalized(1)]);
	let mut changes = StatusChanges::default();

	let seen: Vec<_> = [
		Some(NftPurseStatus::InProgress),
		Some(NftPurseStatus::InProgress),
		Some(cleared_status.clone()),
		Some(NftPurseStatus::InProgress),
	]
	.into_iter()
	.map(|status| changes.next(Ok(status)).map(|emitted| emitted.map(Result::unwrap)))
	.collect();

	let expected = vec![
		Some(Some(Some(NftPurseStatus::InProgress))),
		Some(None),
		Some(Some(Some(cleared_status))),
		None,
	];
	assert_eq!(seen, expected);
}
