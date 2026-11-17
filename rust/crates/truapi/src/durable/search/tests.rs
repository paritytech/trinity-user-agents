use futures::executor::block_on;

use super::*;
use crate::{
	chain::DispatchOutcome,
	durable::testing::{FakeChain, GENESIS, block, block_hash},
};

const TX: H256 = H256([7; 32]);

fn search(chain: &FakeChain, from: u64, to: u64) -> SearchResult {
	block_on(search_range(chain, GENESIS, from, to, TX))
}

/// A reorg can reorder extrinsics, so the outcome is read from the block
/// the extrinsic was actually found in.
#[test]
fn a_hit_reports_its_block_and_the_outcome_read_there() {
	let chain = FakeChain::new(130, 140);
	chain.include(110, TX, DispatchOutcome::Failed(String::new()));

	assert_eq!(
		search(&chain, 100, 130),
		SearchResult::Found {
			block: block(110),
			outcome: Some(DispatchOutcome::Failed(String::new()))
		}
	);
}

#[test]
fn absence_over_a_fully_read_range_is_complete() {
	let chain = FakeChain::new(130, 140);

	assert_eq!(search(&chain, 100, 130), SearchResult::NotFound { read_advanced_to: Some(130) });
}

#[test]
fn an_unreadable_block_hash_ends_the_proven_absence_before_it() {
	let chain = FakeChain::new(130, 140);
	chain.state().unreadable_heights.insert(120);

	assert_eq!(search(&chain, 100, 130), SearchResult::NotFound { read_advanced_to: Some(119) });
}

#[test]
fn an_unreadable_body_ends_the_proven_absence_before_it() {
	let chain = FakeChain::new(130, 140);
	chain.state().unreadable_bodies.insert(block_hash(101));

	assert_eq!(search(&chain, 100, 130), SearchResult::NotFound { read_advanced_to: Some(100) });
}

/// A hit is positive evidence whatever could not be read before it.
#[test]
fn a_hit_past_an_unreadable_block_is_still_found() {
	let chain = FakeChain::new(130, 140);
	chain.state().unreadable_heights.insert(105);
	chain.include(110, TX, DispatchOutcome::Succeeded);

	assert_eq!(
		search(&chain, 100, 130),
		SearchResult::Found { block: block(110), outcome: Some(DispatchOutcome::Succeeded) }
	);
}

/// The body is still kept but the events are pruned: inclusion alone is not
/// success.
#[test]
fn a_hit_whose_events_cannot_be_read_has_no_outcome() {
	let chain = FakeChain::new(130, 140);
	chain.include(110, TX, DispatchOutcome::Succeeded);
	chain.state().unreadable_outcomes.insert(block_hash(110));

	assert_eq!(search(&chain, 100, 130), SearchResult::Found { block: block(110), outcome: None });
}

/// The search ends at its first hit rather than reading the whole range.
#[test]
fn the_search_stops_reading_bodies_past_a_hit() {
	let chain = FakeChain::new(1000, 1000);
	chain.include(1, TX, DispatchOutcome::Succeeded);

	search(&chain, 0, 1000);

	assert!(chain.state().body_reads < 100);
}

/// A reorg between passes can change any block of the range, so nothing
/// read earlier is reused.
#[test]
fn every_search_reads_the_range_again() {
	let chain = FakeChain::new(130, 140);

	search(&chain, 100, 102);
	search(&chain, 100, 102);

	assert_eq!(chain.state().body_reads, 6);
}
