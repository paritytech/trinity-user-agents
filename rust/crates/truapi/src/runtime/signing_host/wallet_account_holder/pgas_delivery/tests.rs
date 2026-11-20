use std::{sync::Arc, thread};

use futures::executor::block_on;

use super::*;
use crate::{
	durable::{DurableTxFailureKind, DurableTxId},
	pgas_claims::{
		ClaimBuildError, ClaimsHarness, FakeClaimBuilder, TEST_IDENTITY, UndeliveredClaim,
		destination,
	},
	test_support::wait_until,
};

fn transient() -> ClaimBuildError {
	ClaimBuildError::Transient(anyhow::anyhow!("the ring revision has not arrived"))
}

/// A request of [`TEST_IDENTITY`] crediting `destination(7)`.
fn request() -> DurablePgasClaim {
	DurablePgasClaim { target: destination(7), identity: TEST_IDENTITY }
}

/// Delivers `request` with its caller already gone, so the answer names the
/// claim it waited on without waiting for it.
fn claim_waited_on(harness: &ClaimsHarness, request: DurablePgasClaim) -> DurableTxId {
	let cancel = CancellationToken::default();
	cancel.cancel();
	match block_on(request.deliver(harness.claims(), &cancel)) {
		Err(AllowanceAllocationError::PgasClaimNotDelivered { claim, .. }) => claim,
		other => panic!("the request waited on no claim: {other:?}"),
	}
}

/// Products are answered `Allocated` once the claim is in a best block, and
/// a request after an earlier claim reached a block claims again.
#[test]
fn a_request_is_answered_once_its_claim_is_in_a_best_block() {
	let harness = Arc::new(ClaimsHarness::new(FakeClaimBuilder::building()));
	let earlier = harness.claim_seen_in_best_block(destination(7));
	let events = harness.script_watch();
	let delivering = harness.clone();
	let delivery = thread::spawn(move || {
		block_on(request().deliver(delivering.claims(), &CancellationToken::default()))
	});
	wait_until(|| harness.claims_into(destination(7)).len() == 2, "a new claim is registered");
	let claim = harness.claims_into(destination(7))[1];

	harness.take_to_best_block(claim, &events);

	let answer = delivery.join().unwrap();
	assert!(answer.is_ok(), "{answer:?}");
	assert_ne!(claim, earlier);
}

/// A failed claim reaches the product as `NotAvailable`, through an error
/// that is no authority error.
#[test]
fn a_request_whose_claim_fails_is_not_delivered() {
	let harness =
		ClaimsHarness::new(FakeClaimBuilder::failing_with(|| ClaimBuildError::NoPersonhood));

	let answer = block_on(request().deliver(harness.claims(), &CancellationToken::default()));

	assert!(
		matches!(
			answer,
			Err(AllowanceAllocationError::PgasClaimNotDelivered {
				reason: UndeliveredClaim::Failed(DurableTxFailureKind::Abandoned),
				..
			})
		),
		"{answer:?}"
	);
}

/// A retried request does not spend a second slot on the claim already
/// under way.
#[test]
fn a_request_waits_on_the_live_claim_and_registers_none() {
	let harness = ClaimsHarness::new(FakeClaimBuilder::failing_with(transient));
	let live = harness.claim_into(destination(7));

	let waited_on = claim_waited_on(&harness, request());

	assert_eq!(waited_on, live);
	assert_eq!(harness.claims_into(destination(7)), vec![live]);
}

/// The claim is built as the account whose session asked.
#[test]
fn a_request_with_no_live_claim_registers_one_as_its_identity() {
	let builder = FakeClaimBuilder::failing_with(transient);
	let harness = ClaimsHarness::new(builder.clone());

	let waited_on = claim_waited_on(&harness, request());

	assert_eq!(harness.claims_into(destination(7)), vec![waited_on]);
	wait_until(|| !builder.calls().is_empty(), "the claim is built");
	assert_eq!(builder.calls()[0].identity, TEST_IDENTITY);
}
