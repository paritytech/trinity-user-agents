use std::{sync::Arc, thread};

use futures::executor::block_on;

use super::*;
use crate::{
	durable::{DurableTxFailureKind, DurableTxId},
	pgas_claims::{
		ClaimBuildError, ClaimsHarness, FakeClaimBuilder, TEST_IDENTITY, UndeliveredClaim,
		destination, transient,
	},
	test_support::wait_until,
};

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

/// Products are answered `Allocated` once the claim is in a best block.
#[test]
fn a_request_is_answered_once_its_claim_is_in_a_best_block() {
	let harness = Arc::new(ClaimsHarness::new(FakeClaimBuilder::building()));
	let events = harness.script_watch();
	let delivering = harness.clone();
	let delivery = thread::spawn(move || {
		block_on(request().deliver(delivering.claims(), &CancellationToken::default()))
	});
	wait_until(|| harness.claims_into(destination(7)).len() == 1, "a claim is registered");
	let claim = harness.claims_into(destination(7))[0];

	harness.take_to_best_block(claim, &events);

	let answer = delivery.join().unwrap();
	assert!(answer.is_ok(), "{answer:?}");
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
