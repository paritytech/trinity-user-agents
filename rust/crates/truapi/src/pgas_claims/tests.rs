use super::*;
use crate::durable::DurableTxFailureKind;

mod live;
mod testing;

pub use live::LivePerson;
pub use testing::{BuildCall, FakeClaimBuilder, TEST_DAY, destination};
use testing::{ClaimsHarness, transient};

/// A caller finds its claim again by the destination alone, so the claim
/// lives in that destination's group.
#[test]
fn a_claim_waits_in_its_destinations_group() {
	let harness = ClaimsHarness::new(FakeClaimBuilder::failing_with(transient));

	let id = harness.claim_into(destination(7));

	let group = destination_group(&destination(7));
	let listed = futures::executor::block_on(harness.engine().group(&pgas_domain(), &group));
	let rows = listed.unwrap();
	let ids: Vec<_> = rows.iter().map(|row| row.id).collect();
	assert_eq!(ids, vec![id]);
	assert_eq!(rows[0].status, DurableTxStatus::PendingSubmission);
}

/// Two requests for one destination share a claim instead of spending two
/// slots.
#[test]
fn a_second_claim_into_a_destination_with_an_active_one_is_refused_with_its_id() {
	let harness = ClaimsHarness::new(FakeClaimBuilder::failing_with(transient));
	let active = harness.claim_into(destination(7));

	let refused =
		futures::executor::block_on(harness.claims().claim(destination(7)));

	assert!(
		matches!(refused, Err(RegistrationError::Hook(PgasClaimError::AlreadyClaiming(id))) if id == active),
		"{refused:?}"
	);
}

/// Only the destination's own claim stands in the way.
#[test]
fn an_active_claim_into_another_destination_does_not_refuse_a_claim() {
	let harness = ClaimsHarness::new(FakeClaimBuilder::failing_with(transient));
	harness.claim_into(destination(7));

	let other = futures::executor::block_on(harness.claims().claim(destination(8)));

	assert!(other.is_ok(), "{other:?}");
}

/// Once a claim has reached a block, the next request for the destination
/// claims another slot.
#[test]
fn a_claim_seen_in_a_best_block_no_longer_refuses_a_new_one() {
	let harness = ClaimsHarness::new(FakeClaimBuilder::building());
	let delivered = harness.claim_seen_in_best_block(destination(7));

	let next = futures::executor::block_on(harness.claims().claim(destination(7)));

	assert!(next.is_ok_and(|id| id != delivered));
}

/// A failed claim is over, so it does not refuse a new one.
#[test]
fn a_failed_claim_no_longer_refuses_a_new_one() {
	let harness =
		ClaimsHarness::new(FakeClaimBuilder::failing_with(|| ClaimBuildError::NoPersonhood));
	let failed = harness.claim_into(destination(7));
	harness.wait_for(failed, DurableTxStatus::Failure(DurableTxFailureKind::Abandoned));

	let next = futures::executor::block_on(harness.claims().claim(destination(7)));

	assert!(next.is_ok_and(|id| id != failed));
}
