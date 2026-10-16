//! Rows that change under a build, and failures that send a row back to be
//! built.

use futures::channel::mpsc;
use parking_lot::Mutex;

use super::*;
use crate::{
	chain::{DispatchOutcome, MortalExtrinsic, WatchEvent},
	durable::{
		engine::{DurableRequest, DurableSubmission},
		testing::{PrepareScript, block_hash},
	},
	store::DbError,
};

/// What a racing write does to a row while its policy builds it.
type RaceWrite = fn(&rusqlite::Transaction<'_>, DurableTxId) -> Result<bool, DbError>;

/// A harness whose policy, asked about a row, first changes it with `race`
/// and then answers `preparation` for it.
fn racing_harness(preparation: Preparation, race: RaceWrite) -> ScheduledTxHarness {
	let db_slot: Arc<Mutex<Option<Db>>> = Arc::default();
	let racing_db = db_slot.clone();
	let script: PrepareScript = Box::new(move |ids| {
		let db = racing_db.lock().clone().unwrap();
		let id = ids[0];
		let answer = answer_all_with(ids, preparation.clone());
		Box::pin(async move {
			db.write(move |tx| race(tx, id)).await.unwrap();
			answer
		})
	});
	let harness = ScheduledTxHarness::with_policy(ScriptedPolicy::from_script(script));
	*db_slot.lock() = Some(harness.db().clone());
	harness
}

/// Registers `extrinsic`, rebuilt through `plan`, and returns its id and
/// the sender of its watch's events.
fn register_watched(
	harness: &ScheduledTxHarness,
	extrinsic: MortalExtrinsic,
	plan: Option<SubmissionPlan>,
) -> (DurableTxId, mpsc::UnboundedSender<WatchEvent>) {
	let events = harness.chain().script_watch();
	let submission = DurableSubmission { extrinsic, plan };
	let request = DurableRequest::presigned(test_domain(), None, vec![submission]).unwrap();
	(harness.schedule(request)[0], events)
}

/// Finalizes `extrinsic` in block 125 with a failed dispatch.
fn fail_on_chain(
	harness: &ScheduledTxHarness,
	events: &mpsc::UnboundedSender<WatchEvent>,
	extrinsic: &MortalExtrinsic,
) {
	harness
		.chain()
		.include(125, extrinsic.extrinsic.hash(), DispatchOutcome::Failed);
	let finalized = WatchEvent::InFinalizedBlock(block_hash(125));
	events.unbounded_send(finalized).unwrap();
}

/// The row changed while its policy was building it, for example because a
/// second engine on the same database gave up on it first. The build is not
/// an attempt, and the round made no progress.
#[test]
fn a_build_for_a_row_that_changed_meanwhile_starts_no_attempt() {
	let ready_now = Preparation::Ready(nth_build(0));
	let harness = racing_harness(ready_now, dao::abandon_submission);

	let id = harness.schedule_one();

	wait_until(|| harness.timer().sleeps() == 1, "the bucket backs off");
	assert_eq!(harness.status(id), DurableTxStatus::Failure(DurableTxFailureKind::Abandoned));
	assert_eq!(harness.broadcast(), vec![]);
	assert!(!harness.engine().ownership.is_owned(id));
}

/// The policy gives up on a row that has already moved on, so the abandon
/// writes nothing and the round made no progress.
#[test]
fn giving_up_on_a_row_that_changed_meanwhile_fails_nothing() {
	let started_elsewhere: RaceWrite =
		|tx, id| dao::start_attempt(tx, id, &extrinsic(100, 100, 64));
	let harness = racing_harness(Preparation::GiveUp, started_elsewhere);

	let id = harness.schedule_one();

	wait_until(|| harness.timer().sleeps() == 1, "the bucket backs off");
	assert_eq!(harness.status(id), DurableTxStatus::Pending);
}

/// The node includes the extrinsic but its dispatch fails, say on a fee the
/// policy can raise. The watch's verdict sends it back to the policy, which
/// builds and broadcasts new bytes instead of failing the transaction.
#[test]
fn a_failure_the_watch_proves_goes_back_to_the_policy_and_is_rebuilt() {
	let harness = ScheduledTxHarness::with_policy(ScriptedPolicy::building_everything());
	let presigned = extrinsic(1, 100, 64);
	let (id, events) = register_watched(&harness, presigned.clone(), Some(test_plan(vec![])));

	fail_on_chain(&harness, &events, &presigned);

	let both = vec![presigned.extrinsic, nth_build(0).extrinsic];
	wait_until(|| harness.broadcast() == both, "the rebuilt bytes are broadcast");
	assert_eq!(harness.policy().batches(), vec![vec![id]]);
	assert_eq!(harness.status(id), DurableTxStatus::Pending);
}

#[test]
fn a_watched_failure_without_a_policy_is_never_rebuilt() {
	let harness = ScheduledTxHarness::with_policy(ScriptedPolicy::building_everything());
	let presigned = extrinsic(1, 100, 64);
	let (id, events) = register_watched(&harness, presigned.clone(), None);

	fail_on_chain(&harness, &events, &presigned);

	harness.wait_for(id, DurableTxStatus::Failure(DurableTxFailureKind::DispatchFailed));
	harness.settle();
	assert_eq!(harness.policy().batches(), Vec::<Vec<DurableTxId>>::new());
	assert_eq!(harness.broadcast(), vec![presigned.extrinsic]);
}
