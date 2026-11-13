use futures::executor::block_on;

use super::*;
use crate::durable::{
	model::{
		DurableTxFailureKind, DurableTxStatus, PolicyParams, VerdictFailureKind, VerdictStatus,
	},
	testing::{RetryQuestion, ScheduledTxHarness, ScriptedPolicy, block, extrinsic, test_plan},
};

/// A submitted row the test policy rebuilds with params `[7]`.
fn rebuildable(harness: &ScheduledTxHarness) -> DurableTxEntry {
	let id = harness.insert_unwatched(extrinsic(1, 100, 64), Some(test_plan(vec![7])));
	harness.entry(id)
}

fn failure(kind: VerdictFailureKind) -> Verdict {
	VerdictStatus::Failure(kind).into()
}

fn write(harness: &ScheduledTxHarness, observed: &DurableTxEntry, verdict: Verdict) -> bool {
	block_on(write_verdict(harness.engine(), observed, verdict)).unwrap()
}

/// The nonce went stale while the transaction waited; its policy can sign
/// it again, so the failure goes back to be built instead of being final.
#[test]
fn a_failure_the_policy_retries_goes_back_to_be_built() {
	let harness = ScheduledTxHarness::with_policy(ScriptedPolicy::never_answering());
	let observed = rebuildable(&harness);

	assert!(write(&harness, &observed, failure(VerdictFailureKind::DispatchFailed)));

	assert_eq!(harness.status(observed.id), DurableTxStatus::PendingSubmission);
	assert_eq!(
		harness.policy().retry_questions(),
		vec![RetryQuestion {
			entry: observed,
			params: PolicyParams(vec![7]),
			failure: VerdictFailureKind::DispatchFailed,
		}]
	);
}

#[test]
fn a_failure_without_a_policy_stays_final() {
	let harness = ScheduledTxHarness::with_policy(ScriptedPolicy::never_answering());
	let id = harness.insert_unwatched(extrinsic(1, 100, 64), None);

	write(&harness, &harness.entry(id), failure(VerdictFailureKind::Expired));

	assert_eq!(harness.status(id), DurableTxStatus::Failure(DurableTxFailureKind::Expired));
	assert_eq!(harness.policy().retry_questions(), vec![]);
}

#[test]
fn a_failure_the_policy_declines_stays_final() {
	let declining = ScriptedPolicy::never_answering().declining_retries();
	let harness = ScheduledTxHarness::with_policy(declining);
	let observed = rebuildable(&harness);

	write(&harness, &observed, failure(VerdictFailureKind::Rejected));

	let rejected = DurableTxStatus::Failure(DurableTxFailureKind::Rejected);
	assert_eq!(harness.status(observed.id), rejected);
}

#[test]
fn a_success_is_written_without_asking_the_policy() {
	let harness = ScheduledTxHarness::with_policy(ScriptedPolicy::never_answering());
	let observed = rebuildable(&harness);
	let finalized = VerdictStatus::FinalizedSuccess { at: Some(block(120)) };

	write(&harness, &observed, finalized.into());

	assert_eq!(harness.status(observed.id), finalized.into());
	assert_eq!(harness.policy().retry_questions(), vec![]);
}

/// Whether the transaction may be retried is unknown, so neither answer
/// may be written: the write fails and the next pass asks again.
#[test]
fn a_policy_that_cannot_be_read_writes_nothing() {
	let harness = ScheduledTxHarness::with_policy(ScriptedPolicy::never_answering());
	let observed = rebuildable(&harness);
	let id = observed.id;
	block_on(harness.engine().db.write(move |tx| {
		tx.execute("UPDATE durable_tx SET policy_params = 5 WHERE id = ?1", [id.0])?;
		Ok(())
	}))
	.unwrap();

	let written =
		block_on(write_verdict(harness.engine(), &observed, failure(VerdictFailureKind::Expired)));

	assert!(written.is_err());
	assert_eq!(harness.status(id), DurableTxStatus::Pending);
}
