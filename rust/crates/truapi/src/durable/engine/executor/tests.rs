mod batching;
mod pacing;
mod races;

use std::sync::{
	Arc,
	atomic::{AtomicU8, AtomicUsize, Ordering},
};

use futures::{StreamExt, executor::block_on};

use super::*;
use crate::{
	Spawner,
	durable::{
		model::{DurableTxFailureKind, DurableTxId, DurableTxStatus, PolicyParams, SubmissionPlan},
		oracle::{DurableRegistry, Unobservable},
		policy::{PolicyError, Preparation},
		testing::{
			FakeChain, PrepareAnswer, ScheduledTxHarness, ScriptedPolicy, ServesTestDomains,
			answer_all_with, engine_with_spawner, extrinsic, extrinsic_for_index, first_builds,
			other_domain, test_domain, test_plan,
		},
	},
	store::Db,
	test_support::wait_until,
};

/// The rows of the test domain waiting to be built, read straight from the
/// ledger.
fn waiting_ids(db: &Db) -> Vec<DurableTxId> {
	let read = db.read(|conn| dao::bucket_submissions(conn, &test_domain(), None));
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

/// The domain that rows from an earlier run name is not registered on this
/// start, perhaps only for now. Nothing builds them, and they keep waiting
/// rather than reach a final status; they are not counted as live work
/// either, or the host would keep recovery running for nothing.
#[test]
fn rows_of_an_unregistered_domain_keep_waiting() {
	let harness = ScheduledTxHarness::with_policy(ScriptedPolicy::building_everything());
	let unknown_domain = harness.leave_waiting(DomainId::new("removed"), test_plan(vec![]));
	let mut live = harness.engine().live_work();
	assert!(!block_on(live.next()).unwrap().unwrap());

	harness.engine().ensure_submissions_started();
	let built = harness.schedule_one();

	harness.wait_for(built, DurableTxStatus::Pending);
	assert_eq!(harness.status(unknown_domain), DurableTxStatus::PendingSubmission);
	assert_eq!(harness.policy().batches(), vec![vec![built]]);
}

/// Recovery and registration both start the executor. A second builder
/// would start every bucket twice as often.
#[test]
fn starting_the_executor_twice_runs_one_builder() {
	let chain = FakeChain::new(130, 140);
	let spawned = Arc::new(AtomicUsize::new(0));
	let counter = spawned.clone();
	let inner = Spawner::for_test();
	let spawner = Spawner::with_test_scheduler(move |task| {
		counter.fetch_add(1, Ordering::SeqCst);
		inner.spawn(task)
	});
	let registry = DurableRegistry::new().serving(test_domain(), Arc::new(Unobservable));
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

	let (_dir, policy) = harness.drop_engine();

	let released = || Arc::strong_count(&policy) == 1;
	wait_until(released, "the build is abandoned with the engine");
	assert_eq!(waiting_ids(&db), vec![id]);
}

/// The waiting rows stop being observable, for example when subscribing
/// fails on a transient read error, while the engine lives on. Recovery
/// calls `ensure_submissions_started` again, and it must start a new builder
/// rather than see the ended one as running.
#[test]
fn a_builder_whose_waiting_rows_end_is_started_again() {
	let harness = ScheduledTxHarness::with_policy(ScriptedPolicy::building_everything());
	harness.engine().executor.abort_all();
	let (stale, _registration) = AbortHandle::new_pair();
	*harness.engine().executor.builder.lock() = Some(stale);

	let ended = futures::stream::empty().boxed();
	block_on(Builder::new(Arc::downgrade(harness.engine())).run(ended));
	harness.engine().ensure_submissions_started();

	let id = harness.schedule_one();
	harness.wait_for(id, DurableTxStatus::Pending);
}
