//! A dependent is handed to its policy only once its dependencies let it
//! go.

use futures::executor::block_on;

use crate::{
	durable::{
		dao,
		engine::{DeferredSubmission, DurableRequest},
		model::{
			DependencyStatus, DependencyStatusRecord, DurableTxFailureKind, DurableTxId,
			DurableTxStatus, PolicyParams, TransactionDependency,
		},
		testing::{
			DeferredTxHarness, ScriptedPolicy, block, extrinsic, insert, other_domain, test_domain,
		},
	},
	test_support::wait_until,
};

/// One test-domain transaction waiting for each of `dependencies`, whose
/// failure it ignores.
fn defer_waiting_for(harness: &DeferredTxHarness, dependencies: &[DurableTxId]) -> DurableTxId {
	let depends_on = dependencies.iter().copied().map(TransactionDependency::new).collect();
	let submission = DeferredSubmission { params: PolicyParams(vec![1]), depends_on };
	harness.defer(DurableRequest::deferred(test_domain(), None, vec![submission]).unwrap())[0]
}

/// A submitted transaction of the other domain that nothing watches or
/// decides, so only the test moves it.
fn unwatched_dependency(harness: &DeferredTxHarness, tag: u8) -> DurableTxId {
	insert(harness.db(), &other_domain(), extrinsic(tag, 100, 64))
}

/// Writes `status` onto `id`, as a watch or a pass would.
fn record_status(harness: &DeferredTxHarness, id: DurableTxId, status: DurableTxStatus) {
	let observed = harness.entry(id);
	let write = harness.db().write(move |tx| dao::compare_and_set(tx, &observed, status, None));
	assert!(block_on(write).unwrap());
}

/// Whether the policy was ever asked about `id`.
fn asked_about(harness: &DeferredTxHarness, id: DurableTxId) -> bool {
	harness.policy().batches().iter().any(|batch| batch.contains(&id))
}

/// Built before its dependency takes effect, the dependent would be invalid
/// on chain. Once the dependency is in a best block, it is valid there.
#[test]
fn a_dependent_waits_until_its_dependency_succeeds_at_the_best_block() {
	let harness = DeferredTxHarness::with_policy(ScriptedPolicy::answering_nothing());
	let dependency = unwatched_dependency(&harness, 1);
	let dependent = defer_waiting_for(&harness, &[dependency]);

	harness.settle();
	assert!(!asked_about(&harness, dependent));

	record_status(&harness, dependency, DurableTxStatus::PendingSuccess { at: block(131) });
	wait_until(|| asked_about(&harness, dependent), "the released dependent is handed over");
}

/// A worker already running for the dependent's domain hears no change in
/// its own rows when a dependency of another domain succeeds. Without the
/// dependency in its change signal it would wait until it asks again.
#[test]
fn a_dependency_of_another_domain_wakes_the_running_worker() {
	let harness = DeferredTxHarness::with_policy(ScriptedPolicy::answering_nothing());
	let keeps_worker_running = harness.defer_one();
	wait_until(|| asked_about(&harness, keeps_worker_running), "the worker runs");
	let dependency = unwatched_dependency(&harness, 1);
	let dependent = defer_waiting_for(&harness, &[dependency]);
	harness.settle();

	record_status(&harness, dependency, DurableTxStatus::PendingSuccess { at: block(131) });

	wait_until(|| asked_about(&harness, dependent), "the worker hears the dependency");
}

/// The policy decides from how each dependency let the dependent go, for
/// example giving up when the one it needed failed.
#[test]
fn the_policy_sees_how_each_dependency_let_the_dependent_go() {
	let harness = DeferredTxHarness::with_policy(ScriptedPolicy::answering_nothing());
	let in_best_block = unwatched_dependency(&harness, 1);
	let finalized = unwatched_dependency(&harness, 2);
	let failed = unwatched_dependency(&harness, 3);
	let dependent = defer_waiting_for(&harness, &[in_best_block, finalized, failed]);

	record_status(&harness, in_best_block, DurableTxStatus::PendingSuccess { at: block(131) });
	record_status(&harness, finalized, DurableTxStatus::FinalizedSuccess { at: None });
	let expired = DurableTxStatus::Failure(DurableTxFailureKind::Expired);
	record_status(&harness, failed, expired);
	wait_until(|| asked_about(&harness, dependent), "the released dependent is handed over");

	let handed_over = harness.policy().prepared_rows();
	let dependencies = handed_over.iter().find(|row| row.id == dependent).unwrap().dependencies.clone();
	assert_eq!(
		dependencies,
		vec![
			DependencyStatusRecord { id: in_best_block, status: DependencyStatus::Success },
			DependencyStatusRecord { id: finalized, status: DependencyStatus::Success },
			DependencyStatusRecord { id: failed, status: DependencyStatus::IgnoredFailure },
		]
	);
}
