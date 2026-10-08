//! Rows that change under a build, and failures that send a row back to be
//! built.

use futures::channel::mpsc;
use parking_lot::Mutex;

use super::*;
use crate::chain::{DispatchOutcome, WatchEvent};
use crate::durable::engine::DurableSubmission;
use crate::durable::testing::{block_hash, rebuild_plan};
use crate::store::DbError;

/// A fixture whose policy, asked about a row, first changes it with `race`
/// and then answers `preparation` for it.
fn fixture_racing(
    preparation: Preparation,
    race: fn(&rusqlite::Transaction<'_>, DurableTxId) -> Result<bool, DbError>,
) -> Fixture {
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
    let fixture = fixture(vec![built_by(script)]);
    *db_slot.lock() = Some(fixture.engine.db.clone());
    fixture
}

/// Registers `extrinsic`, rebuilt through `plan`, and returns its id and
/// the sender of its watch's events.
fn register_watched(
    fixture: &Fixture,
    extrinsic: MortalExtrinsic,
    plan: Option<SubmissionPlan>,
) -> (DurableTxId, mpsc::UnboundedSender<WatchEvent>) {
    let events = fixture.chain.script_watch();
    let submission = DurableSubmission { extrinsic, plan };
    let request = DurableRequest::presigned(TEST, None, vec![submission]).unwrap();
    let ids = block_on(fixture.engine.execute(request)).unwrap();
    (ids[0], events)
}

/// Finalizes `extrinsic` in block 125 with a failed dispatch.
fn fail_on_chain(
    fixture: &Fixture,
    events: &mpsc::UnboundedSender<WatchEvent>,
    extrinsic: &MortalExtrinsic,
) {
    let hash = extrinsic.extrinsic.hash();
    fixture.chain.include(125, hash, DispatchOutcome::Failed);
    let finalized = WatchEvent::InFinalizedBlock(block_hash(125));
    events.unbounded_send(finalized).unwrap();
}

/// The row changed while its policy was building it, for example because a
/// second engine on the same database built it first. The build is not an
/// attempt, and the round made no progress.
#[test]
fn a_build_for_a_row_that_changed_meanwhile_starts_no_attempt() {
    let ready_now = Preparation::Ready(extrinsic(100, 100, 64));
    let fixture = fixture_racing(ready_now, dao::abandon_submission);

    let id = fixture.schedule_one(TEST, None, REBUILD_POLICY);

    wait_until(|| fixture.timer.sleeps() == 1, "the bucket backs off");
    assert_eq!(fixture.status(id), DurableTxStatus::Failure);
    assert_eq!(fixture.submitted(), vec![]);
    assert!(!fixture.engine.ownership.is_owned(id));
}

/// The policy gives up on a row that has already moved on, so the abandon
/// writes nothing and the round made no progress.
#[test]
fn giving_up_on_a_row_that_changed_meanwhile_fails_nothing() {
    let started_elsewhere =
        |tx: &rusqlite::Transaction<'_>, id| dao::start_attempt(tx, id, &extrinsic(100, 100, 64));
    let fixture = fixture_racing(Preparation::GiveUp, started_elsewhere);

    let id = fixture.schedule_one(TEST, None, REBUILD_POLICY);

    wait_until(|| fixture.timer.sleeps() == 1, "the bucket backs off");
    assert_eq!(fixture.status(id), DurableTxStatus::Pending);
}

/// The node includes the extrinsic but its dispatch fails, say on a fee the
/// policy can raise. The watch's verdict sends it back to the policy, which
/// builds and broadcasts new bytes instead of failing the transaction.
#[test]
fn a_failure_the_watch_proves_goes_back_to_the_policy_and_is_rebuilt() {
    let policy = built_by(builds_everything());
    let fixture = fixture(vec![policy.clone()]);
    let presigned = extrinsic(1, 100, 64);
    let rebuilt_by = Some(rebuild_plan(vec![]));
    let (id, events) = register_watched(&fixture, presigned.clone(), rebuilt_by);

    fail_on_chain(&fixture, &events, &presigned);

    let both = vec![presigned.extrinsic, extrinsic(100, 100, 64).extrinsic];
    wait_until(
        || fixture.submitted() == both,
        "the rebuilt bytes are broadcast",
    );
    assert_eq!(policy.batches(), vec![vec![id]]);
    assert_eq!(fixture.status(id), DurableTxStatus::Pending);
}

#[test]
fn a_watched_failure_without_a_policy_is_never_rebuilt() {
    let policy = built_by(builds_everything());
    let fixture = fixture(vec![policy.clone()]);
    let presigned = extrinsic(1, 100, 64);
    let (id, events) = register_watched(&fixture, presigned.clone(), None);

    fail_on_chain(&fixture, &events, &presigned);

    fixture.wait_for(id, DurableTxStatus::Failure);
    fixture.settle();
    assert_eq!(policy.batches(), Vec::<Vec<DurableTxId>>::new());
    assert_eq!(fixture.submitted(), vec![presigned.extrinsic]);
}
