//! RFC-6 top-up statuses as the products' use cases follow them.

use super::*;
use crate::coinage::status::Failure;

/// One cent in the payment asset's units.
const UNIT: u128 = 10_000;

fn settling(settled: u64, in_flight: u64, provisional: u64) -> StatusItem {
	StatusItem { status: Status::Settling, progress: Progress { settled, in_flight, provisional } }
}

fn claimed(finalized: bool) -> HostPaymentTopUpStatusSubscribeItem {
	HostPaymentTopUpStatusSubscribeItem::Claimed { finalized }
}

fn top_up(items: &[StatusItem], amount: u64) -> Vec<HostPaymentTopUpStatusSubscribeItem> {
	items.iter().map(|item| top_up_status(*item, amount, UNIT)).collect()
}

#[test]
fn a_top_up_reports_detecting_while_it_waits_for_its_source() {
	assert_eq!(top_up(&[item(Status::Waiting, 0, 8)], 8), [HostPaymentTopUpStatusSubscribeItem::Detecting]);
}

#[test]
fn a_top_up_is_claimed_finally_once_layer_one_is_done_at_the_amount() {
	let items = [item(Status::Submitted, 0, 8), item(Status::Done, 8, 0)];
	assert_eq!(top_up(&items, 8), [HostPaymentTopUpStatusSubscribeItem::Claiming, claimed(true)]);
}

#[test]
fn a_top_up_reports_its_inclusion_before_its_finality() {
	let items = [settling(0, 8, 8), item(Status::Done, 8, 0)];
	assert_eq!(top_up(&items, 8), [claimed(false), claimed(true)]);
}

/// One load or claim that succeeded on the best chain is not the amount:
/// its sibling may still fail.
#[test]
fn a_top_up_whose_value_is_merely_in_flight_is_still_claiming() {
	assert_eq!(top_up(&[settling(0, 10, 8)], 10), [HostPaymentTopUpStatusSubscribeItem::Claiming]);
}

#[test]
fn a_shortfall_that_has_not_finalized_is_still_claiming() {
	assert_eq!(top_up(&[settling(0, 8, 8)], 10), [HostPaymentTopUpStatusSubscribeItem::Claiming], "a sibling worth 2 cents failed");
}

#[test]
fn a_claim_held_up_part_way_through_still_reports_claiming() {
	assert_eq!(top_up(&[item(Status::Submitted, 4, 4)], 8), [HostPaymentTopUpStatusSubscribeItem::Claiming]);
}

#[test]
fn a_finalized_shortfall_is_reported_as_partial() {
	assert_eq!(top_up(&[item(Status::Done, 6, 0)], 8), [HostPaymentTopUpStatusSubscribeItem::ClaimedPartially { actual_claimed: 6 * UNIT }]);
}

#[test]
fn a_top_up_that_ended_short_is_reported_as_partial() {
	assert_eq!(top_up(&[item(Status::PartiallyDone, 4, 0)], 8), [HostPaymentTopUpStatusSubscribeItem::ClaimedPartially { actual_claimed: 4 * UNIT }]);
}

#[test]
fn a_top_up_that_never_landed_is_reported_as_not_claimed() {
	let items = [item(Status::Failed(Failure::NothingMoved), 0, 0), item(Status::Failed(Failure::GivenUp), 0, 0)];
	assert_eq!(top_up(&items, 8), [HostPaymentTopUpStatusSubscribeItem::NotClaimed, HostPaymentTopUpStatusSubscribeItem::NotClaimed]);
}

/// Layer 1 shows a reorganisation (G2 invariant 3), and so does the mapping.
#[test]
fn a_claim_a_fork_retracts_stops_being_reported_as_claimed() {
	let items = [item(Status::Waiting, 0, 8), settling(0, 8, 8), item(Status::Submitted, 0, 8)];
	assert_eq!(top_up(&items, 8), [HostPaymentTopUpStatusSubscribeItem::Detecting, claimed(false), HostPaymentTopUpStatusSubscribeItem::Claiming]);
}
