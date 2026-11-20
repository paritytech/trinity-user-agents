use futures::executor::block_on;

use super::*;
use crate::durable::{
	model::{
		DurableTxFailureKind, DurableTxId, DurableTxStatus, PolicyParams, TransactionDependency,
		VerdictFailureKind, VerdictStatus,
	},
	testing::{DeferredTxHarness, RetryQuestion, ScriptedPolicy, block, extrinsic, test_plan},
};

/// A submitted row the test policy rebuilds with params `[7]`.
fn rebuildable(harness: &DeferredTxHarness) -> DurableTxEntry {
	let id = harness.insert_unwatched(extrinsic(1, 100, 64), Some(test_plan(vec![7])));
	harness.entry(id)
}

fn failure(kind: VerdictFailureKind) -> Verdict {
	VerdictStatus::Failure(kind).into()
}

fn write(harness: &DeferredTxHarness, observed: &DurableTxEntry, verdict: Verdict) -> bool {
	block_on(write_verdict(harness.engine(), observed, verdict)).unwrap()
}

/// The nonce went stale while the transaction waited; its policy can sign
/// it again, so the failure goes back to be built instead of being final.
#[test]
fn a_failure_the_policy_retries_goes_back_to_be_built() {
	let harness = DeferredTxHarness::with_policy(ScriptedPolicy::never_answering());
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
	let harness = DeferredTxHarness::with_policy(ScriptedPolicy::never_answering());
	let id = harness.insert_unwatched(extrinsic(1, 100, 64), None);

	write(&harness, &harness.entry(id), failure(VerdictFailureKind::Expired));

	assert_eq!(harness.status(id), DurableTxStatus::Failure(DurableTxFailureKind::Expired));
	assert_eq!(harness.policy().retry_questions(), vec![]);
}

#[test]
fn a_failure_the_policy_declines_stays_final() {
	let declining = ScriptedPolicy::never_answering().declining_retries();
	let harness = DeferredTxHarness::with_policy(declining);
	let observed = rebuildable(&harness);

	write(&harness, &observed, failure(VerdictFailureKind::Rejected));

	let rejected = DurableTxStatus::Failure(DurableTxFailureKind::Rejected);
	assert_eq!(harness.status(observed.id), rejected);
}

#[test]
fn a_success_is_written_without_asking_the_policy() {
	let harness = DeferredTxHarness::with_policy(ScriptedPolicy::never_answering());
	let observed = rebuildable(&harness);
	let finalized = VerdictStatus::FinalizedSuccess { at: Some(block(120)) };

	write(&harness, &observed, finalized.into());

	assert_eq!(harness.status(observed.id), finalized.into());
	assert_eq!(harness.policy().retry_questions(), vec![]);
}

/// A submitted row the test policy rebuilds, waiting for `dependency`.
fn rebuildable_dependent(harness: &DeferredTxHarness, dependency: DurableTxId) -> DurableTxEntry {
	let id = harness.insert_unwatched(extrinsic(2, 100, 64), Some(test_plan(vec![7])));
	let waits = TransactionDependency::new(dependency);
	let write = harness.db().write(move |tx| dao::insert_dependency(tx, id, &waits));
	block_on(write).unwrap();
	harness.entry(id)
}

/// After a reorg drops the dependency, the dependent's attempt fails
/// because the dependency is missing, which says nothing about whether it
/// is worth building once the dependency lands again. So it waits again
/// without asking its policy, which would decline here.
#[test]
fn a_dependent_failing_before_its_dependency_settles_waits_again() {
	let declining = ScriptedPolicy::never_answering().declining_retries();
	let harness = DeferredTxHarness::with_policy(declining);
	let dependency = harness.insert_unwatched(extrinsic(1, 100, 64), None);
	let observed = rebuildable_dependent(&harness, dependency);

	write(&harness, &observed, failure(VerdictFailureKind::Expired));

	assert_eq!(harness.status(observed.id), DurableTxStatus::PendingSubmission);
	assert_eq!(harness.policy().retry_questions(), vec![]);
}

/// With its dependency in effect, the dependent failed for a reason of its
/// own, which only its policy can judge.
#[test]
fn a_dependent_failing_after_its_dependencies_settled_asks_its_policy() {
	let declining = ScriptedPolicy::never_answering().declining_retries();
	let harness = DeferredTxHarness::with_policy(declining);
	let dependency = harness.insert_unwatched(extrinsic(1, 100, 64), None);
	harness.record_status(dependency, DurableTxStatus::PendingSuccess { at: block(131) });
	let observed = rebuildable_dependent(&harness, dependency);

	write(&harness, &observed, failure(VerdictFailureKind::Expired));

	let expired = DurableTxStatus::Failure(DurableTxFailureKind::Expired);
	assert_eq!(harness.status(observed.id), expired);
	assert_eq!(harness.policy().retry_questions().len(), 1);
}
