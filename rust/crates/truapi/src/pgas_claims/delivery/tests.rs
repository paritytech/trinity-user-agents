use std::{sync::Arc, thread};

use futures::{executor::block_on, future::pending};

use super::*;
use crate::{
	CancellationToken,
	pgas_claims::{
		ClaimBuildError,
		testing::{ClaimsHarness, FakeClaimBuilder, TEST_IDENTITY, destination, transient},
	},
	test_support::wait_until,
};

/// Waits for `id` with no cancellation.
fn wait_uncancelled(harness: &ClaimsHarness, id: DurableTxId) -> Result<(), UndeliveredClaim> {
	block_on(harness.claims().wait_until_delivered(id, pending()))
}

/// A product is answered as soon as its claim is in a best block, without
/// waiting for finality.
#[test]
fn a_claim_seen_in_a_best_block_is_delivered() {
	let harness = ClaimsHarness::new(FakeClaimBuilder::building());
	let id = harness.claim_seen_in_best_block(destination(7));

	let delivered = wait_uncancelled(&harness, id);

	assert!(delivered.is_ok(), "{delivered:?}");
}

#[test]
fn a_failed_claim_is_not_delivered() {
	let harness =
		ClaimsHarness::new(FakeClaimBuilder::failing_with(|| ClaimBuildError::NoPersonhood));
	let id = harness.claim_into(destination(7));

	let delivered = wait_uncancelled(&harness, id);

	assert!(
		matches!(delivered, Err(UndeliveredClaim::Failed(DurableTxFailureKind::Abandoned))),
		"{delivered:?}"
	);
}

/// A caller stops waiting, as a direct claim gave up on inclusion, while the
/// claim carries on in the ledger.
#[test]
fn a_caller_stops_waiting_after_three_minutes_and_the_claim_carries_on() {
	let harness = Arc::new(ClaimsHarness::new(FakeClaimBuilder::failing_with(transient)));
	let id = harness.claim_into(destination(7));
	let waiting = harness.clone();
	let wait = thread::spawn(move || wait_uncancelled(&waiting, id));
	wait_until(|| harness.delivery_timer().sleeps() == 1, "the caller starts waiting");

	harness.delivery_timer().advance(Duration::from_secs(180));

	let delivered = wait.join().unwrap();
	assert!(matches!(delivered, Err(UndeliveredClaim::TimedOut)), "{delivered:?}");
	assert_eq!(harness.status(id), DurableTxStatus::PendingSubmission);
}

#[test]
fn a_cancelled_caller_stops_waiting() {
	let harness = ClaimsHarness::new(FakeClaimBuilder::failing_with(transient));
	let id = harness.claim_into(destination(7));
	let cancel = CancellationToken::default();
	cancel.cancel();

	let delivered = block_on(harness.claims().wait_until_delivered(id, cancel.cancelled()));

	assert!(
		matches!(delivered, Err(UndeliveredClaim::Cancelled(CancellationReason::Cancelled))),
		"{delivered:?}"
	);
}

/// Concurrent requests for one destination wait on one claim.
#[test]
fn a_claim_into_a_destination_with_an_active_one_joins_it() {
	let harness = ClaimsHarness::new(FakeClaimBuilder::failing_with(transient));
	let active = harness.claim_into(destination(7));

	let joined = block_on(harness.claims().claim_or_join(destination(7), TEST_IDENTITY));

	assert_eq!(joined.unwrap(), active);
}
