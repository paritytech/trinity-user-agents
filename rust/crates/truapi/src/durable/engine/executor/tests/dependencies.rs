//! A dependent is handed to its policy only once its dependencies let it
//! go.

use crate::{
	durable::{
		model::{
			DependencyStatus, DependencyStatusRecord, DurableTxFailureKind, DurableTxId,
			DurableTxStatus, TransactionDependency,
		},
		testing::{
			DeferredTxHarness, ScriptedPolicy, block, cascading_dependency, extrinsic,
			first_build_gated, insert, other_domain,
		},
	},
	test_support::wait_until,
};

/// One test-domain transaction waiting for each of `dependencies`, whose
/// failure it ignores.
fn defer_dependent_ignoring_failures(
	harness: &DeferredTxHarness,
	dependencies: &[DurableTxId],
) -> DurableTxId {
	let ignoring = dependencies.iter().copied().map(TransactionDependency::new).collect();
	harness.defer_with_dependencies(ignoring)
}

/// A submitted transaction of the other domain that nothing watches or
/// decides, so only the test moves it.
fn unwatched_dependency(harness: &DeferredTxHarness, tag: u8) -> DurableTxId {
	insert(harness.db(), &other_domain(), extrinsic(tag, 100, 64))
}

/// Whether the policy was ever asked about `id`.
fn asked_about(harness: &DeferredTxHarness, id: DurableTxId) -> bool {
	harness.policy().batches().iter().any(|batch| batch.contains(&id))
}

/// A dependency still waiting to be built has not taken effect, so a
/// dependent built now would be invalid on chain.
#[test]
fn a_dependent_waits_while_its_dependency_waits_to_be_built() {
	let harness = DeferredTxHarness::with_policy(ScriptedPolicy::answering_nothing());
	let dependency = harness.defer_one();
	let dependent = defer_dependent_ignoring_failures(&harness, &[dependency]);
	wait_until(|| asked_about(&harness, dependency), "the dependency is asked about");

	harness.settle();

	assert!(!asked_about(&harness, dependent));
}

/// Built before its dependency takes effect, the dependent would be invalid
/// on chain. Once the dependency is in a best block, it is valid there.
#[test]
fn a_dependent_waits_until_its_dependency_succeeds_at_the_best_block() {
	let harness = DeferredTxHarness::with_policy(ScriptedPolicy::answering_nothing());
	let dependency = unwatched_dependency(&harness, 1);
	let dependent = defer_dependent_ignoring_failures(&harness, &[dependency]);

	harness.settle();
	assert!(!asked_about(&harness, dependent));

	harness.record_status(dependency, DurableTxStatus::PendingSuccess { at: block(131) });
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
	let dependent = defer_dependent_ignoring_failures(&harness, &[dependency]);
	harness.settle();

	harness.record_status(dependency, DurableTxStatus::PendingSuccess { at: block(131) });

	wait_until(|| asked_about(&harness, dependent), "the worker hears the dependency");
}

/// The policy decides from how each dependency released the dependent,
/// for example giving up when the one it needed failed.
#[test]
fn the_policy_sees_how_each_dependency_released_the_dependent() {
	let harness = DeferredTxHarness::with_policy(ScriptedPolicy::answering_nothing());
	let in_best_block = unwatched_dependency(&harness, 1);
	let finalized = unwatched_dependency(&harness, 2);
	let failed = unwatched_dependency(&harness, 3);
	let dependent =
		defer_dependent_ignoring_failures(&harness, &[in_best_block, finalized, failed]);

	harness.record_status(in_best_block, DurableTxStatus::PendingSuccess { at: block(131) });
	harness.record_status(finalized, DurableTxStatus::FinalizedSuccess { at: None });
	let expired = DurableTxStatus::Failure(DurableTxFailureKind::Expired);
	harness.record_status(failed, expired);
	wait_until(|| asked_about(&harness, dependent), "the released dependent is handed over");

	let handed_over = harness.policy().rows_asked();
	let dependencies =
		handed_over.iter().find(|row| row.id == dependent).unwrap().dependencies.clone();
	assert_eq!(
		dependencies,
		vec![
			DependencyStatusRecord { id: in_best_block, status: DependencyStatus::Success },
			DependencyStatusRecord { id: finalized, status: DependencyStatus::Success },
			DependencyStatusRecord { id: failed, status: DependencyStatus::IgnoredFailure },
		]
	);
}

/// A policy may take seconds to build an attempt. A dependency that fails
/// meanwhile must still stop it, or the dependent lands although the
/// transaction it needed failed.
#[test]
fn a_dependency_failing_while_the_dependent_is_built_stops_the_attempt() {
	let (release, script) = first_build_gated();
	let harness = DeferredTxHarness::with_policy(ScriptedPolicy::from_script(script));
	let dependency = unwatched_dependency(&harness, 1);
	let dependent = harness.defer_with_dependencies(vec![cascading_dependency(dependency)]);
	harness.record_status(dependency, DurableTxStatus::PendingSuccess { at: block(131) });
	wait_until(|| asked_about(&harness, dependent), "the dependent is being built");

	let expired = DurableTxStatus::Failure(DurableTxFailureKind::Expired);
	harness.record_status(dependency, expired);
	release.send(()).unwrap();
	harness.settle();

	assert_eq!(harness.status(dependent), DurableTxStatus::PendingSubmission);
	assert_eq!(harness.broadcast(), vec![]);
}
