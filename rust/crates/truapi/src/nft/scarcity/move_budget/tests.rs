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
