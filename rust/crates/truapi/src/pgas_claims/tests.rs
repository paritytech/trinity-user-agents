use super::*;
use crate::{
	chain::{DispatchOutcome, WatchEvent},
	durable::{DurableTxFailureKind, block, block_hash, extrinsic},
	pgas_claims::testing::{ClaimsHarness, FakeClaimBuilder, TEST_IDENTITY, destination},
};

fn transient() -> ClaimBuildError {
	ClaimBuildError::Transient(anyhow::anyhow!("the ring revision has not arrived"))
}

/// Takes the claim built in slot 0 to a best block, as a watch reports it.
fn deliver_first_built_claim(harness: &ClaimsHarness, id: DurableTxId) {
	let events = harness.script_watch();
	harness.wait_for(id, DurableTxStatus::Pending);
	let tx_hash = extrinsic(0, 130, 64).extrinsic.hash();
	harness.chain().include(135, tx_hash, DispatchOutcome::Succeeded);
	events.unbounded_send(WatchEvent::InBestBlock(block_hash(135))).unwrap();
	harness.wait_for(id, DurableTxStatus::PendingSuccess { at: block(135) });
}

/// A caller finds its claim again by the destination alone, so the claim
/// lives in that destination's group.
#[test]
fn a_claim_waits_in_its_destinations_group() {
	let harness = ClaimsHarness::new(FakeClaimBuilder::failing_with(transient));

	let id = harness.claim_into(destination(7));

	let group = destination_group(&destination(7));
	let listed = futures::executor::block_on(harness.engine().group(&pgas_domain(), &group));
	let statuses: Vec<_> = listed.unwrap().into_iter().map(|row| (row.id, row.status)).collect();
	assert_eq!(statuses, vec![(id, DurableTxStatus::PendingSubmission)]);
}

/// Two requests for one destination share a claim instead of spending two
/// slots.
#[test]
fn a_second_claim_into_a_destination_with_an_active_one_is_refused_with_its_id() {
	let harness = ClaimsHarness::new(FakeClaimBuilder::failing_with(transient));
	let active = harness.claim_into(destination(7));

	let refused =
		futures::executor::block_on(harness.claims().claim(destination(7), TEST_IDENTITY));

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

	let other = futures::executor::block_on(harness.claims().claim(destination(8), TEST_IDENTITY));

	assert!(other.is_ok(), "{other:?}");
}

/// A request after the previous claim reached a block claims again, as a
/// request did before claims were durable.
#[test]
fn a_claim_seen_in_a_best_block_no_longer_refuses_a_new_one() {
	let harness = ClaimsHarness::new(FakeClaimBuilder::building());
	let delivered = harness.claim_into(destination(7));
	deliver_first_built_claim(&harness, delivered);

	let next = futures::executor::block_on(harness.claims().claim(destination(7), TEST_IDENTITY));

	assert!(next.is_ok_and(|id| id != delivered));
}

/// After an interruption, a caller re-attaches to the claim that is still
/// under way.
#[test]
fn the_active_claim_is_the_one_still_waiting_to_be_built() {
	let harness = ClaimsHarness::new(FakeClaimBuilder::failing_with(transient));
	let waiting = harness.claim_into(destination(7));

	let active = futures::executor::block_on(harness.claims().active_claim(destination(7)));

	assert_eq!(active.unwrap(), Some(waiting));
}

/// A failed claim is over, so it neither stands in for a new one nor
/// refuses it.
#[test]
fn a_failed_claim_is_not_active() {
	let harness =
		ClaimsHarness::new(FakeClaimBuilder::failing_with(|| ClaimBuildError::NoPersonhood));
	let failed = harness.claim_into(destination(7));
	harness.wait_for(failed, DurableTxStatus::Failure(DurableTxFailureKind::Abandoned));

	let active = futures::executor::block_on(harness.claims().active_claim(destination(7)));

	assert_eq!(active.unwrap(), None);
}
