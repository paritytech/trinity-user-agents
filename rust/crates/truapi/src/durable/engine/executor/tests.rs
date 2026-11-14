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
	durable::{
		model::{DurableTxFailureKind, DurableTxId, DurableTxStatus, PolicyParams, SubmissionPlan},
		policy::{PolicyError, Preparation},
		testing::{
			DeferredTxHarness, PrepareAnswer, ScriptedPolicy, answer_all_with, extrinsic,
			extrinsic_for_index, first_builds, other_domain, test_domain, test_plan,
		},
	},
	store::Db,
	test_support::wait_until,
};

/// The rows of the test domain waiting to be built, read straight from the
/// ledger.
fn waiting_ids(db: &Db) -> Vec<DurableTxId> {
	let read = db.read(|conn| dao::domain_submissions(conn, &test_domain()));
	let rows = block_on(read).unwrap();
	rows.iter().map(|deferred| deferred.id).collect()
}

#[test]
fn a_deferred_transaction_is_built_broadcast_and_watched() {
	let harness = DeferredTxHarness::with_policy(ScriptedPolicy::building_everything());

	let id = harness.defer_one();

	harness.wait_for(id, DurableTxStatus::Pending);
	wait_until(|| harness.broadcast() == first_builds(1), "the built extrinsic is broadcast");
	assert!(harness.engine().ownership.is_owned(id));
}

#[test]
fn a_policy_that_gives_up_fails_the_transaction_without_broadcasting() {
	let harness = DeferredTxHarness::with_policy(ScriptedPolicy::giving_up());

	let id = harness.defer_one();

	harness.wait_for(id, DurableTxStatus::Failure(DurableTxFailureKind::Abandoned));
	assert_eq!(harness.broadcast(), vec![]);
}

/// The domain that rows from an earlier run name is not registered on this
/// start, perhaps only for now. Nothing builds them, and they keep waiting
/// rather than reach a final status; they are not counted as live work
/// either, or the host would keep recovery running for nothing.
#[test]
fn rows_of_an_unregistered_domain_keep_waiting() {
	let harness = DeferredTxHarness::with_policy(ScriptedPolicy::building_everything());
	let unknown_domain = harness.leave_waiting(DomainId::new("removed"), test_plan(vec![]));
	let mut live = harness.engine().live_work();
	assert!(!block_on(live.next()).unwrap().unwrap());

	let built = harness.defer_one();

	harness.wait_for(built, DurableTxStatus::Pending);
	assert_eq!(harness.status(unknown_domain), DurableTxStatus::PendingSubmission);
	assert_eq!(harness.policy().batches(), vec![vec![built]]);
}

/// The runtime is torn down while a policy is still building. The row keeps
/// waiting for the next process, and the build's task must not keep the
/// engine, and with it the database, alive.
#[test]
fn dropping_the_engine_mid_build_leaves_the_transaction_waiting() {
	let harness = DeferredTxHarness::with_policy(ScriptedPolicy::never_answering());
	let id = harness.defer_one();
	wait_until(|| harness.policy().batches().len() == 1, "the build starts");
	let db = harness.db().clone();

	let (_dir, policy) = harness.drop_engine();

	let released = || Arc::strong_count(&policy) == 1;
	wait_until(released, "the build is abandoned with the engine");
	assert_eq!(waiting_ids(&db), vec![id]);
}
