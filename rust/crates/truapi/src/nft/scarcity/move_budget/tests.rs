use super::*;
use crate::nft::scarcity::testing::asset_hub_metadata;

/// The budget comes from the runtime the node runs, so it must decode from
/// real Asset Hub metadata.
#[test]
fn the_budget_decodes_from_asset_hub_metadata() {
	let metadata = asset_hub_metadata();

	assert_eq!(maximum_moves_in(&metadata).unwrap(), 16);
}

/// A fee-less move is valid only below the budget, so the last move it
/// allows leaves the NFT exhausted.
#[test]
fn an_nft_is_exhausted_once_it_used_the_whole_budget() {
	let budget = MoveBudget::new(16);

	assert!(!budget.is_exhausted(15));
	assert!(budget.is_exhausted(16));
}

/// A holder pays for its own later moves, so a move to it is paid only once
/// the NFT has no fee-less move left.
#[test]
fn a_move_to_a_holder_is_paid_once_no_fee_less_move_is_left() {
	let budget = MoveBudget::new(16);

	assert_eq!(budget.transfer_mode(15, DestinationKind::Holder), TransferMode::FeeLess);
	assert_eq!(budget.transfer_mode(16, DestinationKind::Holder), TransferMode::Paid);
}

/// The sender reclaims from a cheque key fee-less, so a move to it is paid
/// once it would arrive with no fee-less move left.
#[test]
fn a_move_to_a_cheque_key_is_paid_once_it_would_arrive_exhausted() {
	let budget = MoveBudget::new(16);

	assert_eq!(budget.transfer_mode(14, DestinationKind::ChequeKey), TransferMode::FeeLess);
	assert_eq!(budget.transfer_mode(15, DestinationKind::ChequeKey), TransferMode::Paid);
}
