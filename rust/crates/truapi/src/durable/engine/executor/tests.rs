mod batching;
mod pacing;
mod races;

use std::sync::{
	Arc,
	atomic::{AtomicU8, AtomicUsize, Ordering},
};

use futures::executor::block_on;

use super::*;
use crate::{
	durable::{
		model::{
			DurableTxFailureKind, DurableTxId, DurableTxStatus, PolicyId, PolicyParams,
			SubmissionPlan,
		},
		oracle::{DurableRegistry, Unobservable},
		policy::{PolicyError, Preparation},
		testing::{
			FakeChain, GENESIS, PrepareAnswer, ScheduledTxHarness, ScriptedPolicy, answer_all_with,
			engine_with_spawner, extrinsic, extrinsic_for_index, first_builds, other_domain,
			test_domain, test_plan, test_policy_id,
		},
	},
	store::Db,
	subscription::Spawner,
	test_support::{test_spawner, wait_until},
};

/// The rows of the test domain waiting for the test policy, read straight
/// from the ledger.
fn waiting_ids(db: &Db) -> Vec<DurableTxId> {
	let read =
		db.read(|conn| dao::bucket_submissions(conn, &test_domain(), &test_policy_id(), None));
	let rows = block_on(read).unwrap();
	rows.iter().map(|scheduled| scheduled.id).collect()
}

#[test]
fn a_scheduled_transaction_is_built_broadcast_and_watched() {
	let harness = ScheduledTxHarness::with_policy(ScriptedPolicy::building_everything());

	let id = harness.schedule_one();

	harness.wait_for(id, DurableTxStatus::Pending);
	wait_until(|| harness.broadcast() == first_builds(1), "the built extrinsic is broadcast");
	assert!(harness.engine().ownership.is_owned(id));
}

#[test]
fn a_policy_that_gives_up_fails_the_transaction_without_broadcasting() {
	let harness = ScheduledTxHarness::with_policy(ScriptedPolicy::giving_up());

	let id = harness.schedule_one();

	harness.wait_for(id, DurableTxStatus::Failure(DurableTxFailureKind::Abandoned));
	assert_eq!(harness.broadcast(), vec![]);
}

/// An app update dropped the policy, or the domain, that rows from an earlier
/// run name. Nothing can build them, so they fail rather than wait forever.
#[test]
fn rows_nothing_can_build_are_abandoned() {
	let harness = ScheduledTxHarness::with_policy(ScriptedPolicy::building_everything());
	let removed_policy =
		SubmissionPlan { policy: PolicyId::new("removed"), params: PolicyParams::default() };
	let unknown_policy = harness.leave_waiting(test_domain(), removed_policy);
	let unknown_domain = harness.leave_waiting(DomainId::new("removed"), test_plan(vec![]));

	harness.engine().ensure_submissions_started();

	let abandoned = DurableTxStatus::Failure(DurableTxFailureKind::Abandoned);
	harness.wait_for(unknown_policy, abandoned);
	harness.wait_for(unknown_domain, abandoned);
	assert_eq!(harness.policy().batches(), Vec::<Vec<DurableTxId>>::new());
}

/// Recovery and registration both start the executor. A second collector
/// would launch every bucket twice as often.
#[test]
fn starting_the_executor_twice_runs_one_collector() {
	let chain = FakeChain::new(130, 140);
	let spawned = Arc::new(AtomicUsize::new(0));
	let counter = spawned.clone();
	let inner = test_spawner();
	let spawner: Spawner = Arc::new(move |task| {
		counter.fetch_add(1, Ordering::SeqCst);
		inner(task)
	});
	let registry =
		DurableRegistry::new().with_domain(test_domain(), Arc::new(Unobservable(GENESIS)));
	let (_dir, engine, _timer) = engine_with_spawner(&chain, registry, spawner);

	engine.ensure_submissions_started();
	engine.ensure_submissions_started();

	assert_eq!(spawned.load(Ordering::SeqCst), 1);
}

/// The runtime is torn down while a policy is still building. The row keeps
/// waiting for the next process, and the build's task must not keep the
/// engine, and with it the database, alive.
#[test]
fn dropping_the_engine_mid_build_leaves_the_transaction_waiting() {
	let harness = ScheduledTxHarness::with_policy(ScriptedPolicy::never_answering());
	let id = harness.schedule_one();
	wait_until(|| harness.policy().batches().len() == 1, "the build starts");
	let db = harness.db().clone();

	let (_dir, policies) = harness.drop_engine();

	let released = || Arc::strong_count(&policies[0]) == 1;
	wait_until(released, "the build is abandoned with the engine");
	assert_eq!(waiting_ids(&db), vec![id]);
}
