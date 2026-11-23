use std::{sync::Arc, thread};

use futures::executor::block_on;

use super::*;
use crate::{
	durable::DurableTxFailureKind,
	pgas_claims::{
		ClaimBuildError, ClaimsHarness, FakeClaimBuilder, TEST_IDENTITY, UndeliveredClaim,
		destination, transient,
	},
	runtime::authority::AuthorityError,
	test_support::wait_until,
};

/// A request of [`TEST_IDENTITY`] crediting `destination(7)`.
fn request() -> DurablePgasClaim {
	DurablePgasClaim { target: destination(7), identity: TEST_IDENTITY }
}

/// A context whose caller has already gone.
fn cancelled_call() -> CallContext {
	let call = CallContext::default();
	call.cancel().cancel();
	call
}

/// Products are answered `Allocated` once the claim is in a best block.
#[test]
fn a_request_is_answered_once_its_claim_is_in_a_best_block() {
	let harness = Arc::new(ClaimsHarness::new(FakeClaimBuilder::building()));
	let events = harness.script_watch();
	let delivering = harness.clone();
	let delivery = thread::spawn(move || {
		block_on(request().deliver(delivering.claims(), &CallContext::default()))
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

	let answer = block_on(request().deliver(harness.claims(), &CallContext::default()));

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

	block_on(request().deliver(harness.claims(), &cancelled_call())).unwrap_err();

	assert_eq!(harness.claims_into(destination(7)), vec![live]);
}

/// A product that withdraws its request hears `Cancelled`, as from every
/// other withdrawn allocation, not that PGAS is unavailable.
#[test]
fn a_cancelled_request_answers_cancelled() {
	let harness = ClaimsHarness::new(FakeClaimBuilder::failing_with(transient));

	let answer = block_on(request().deliver(harness.claims(), &cancelled_call()));

	assert!(
		matches!(answer, Err(AllowanceAllocationError::Authority(AuthorityError::Cancelled(_)))),
		"{answer:?}"
	);
}
