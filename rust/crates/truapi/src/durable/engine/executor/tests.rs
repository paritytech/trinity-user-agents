use core::time::Duration;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};

use futures::channel::oneshot;
use futures::executor::block_on;
use futures::future::{pending, ready};
use parking_lot::Mutex;

use super::*;
use crate::chain::{DispatchOutcome, MortalExtrinsic};
use crate::durable::dao;
use crate::durable::engine::{DurableRequest, DurableSubmission};
use crate::durable::model::{DomainId, DurableTxId, DurableTxStatus, GroupId, PolicyId, PolicyRef};
use crate::durable::oracle::{DurableRegistry, Unobservable};
use crate::durable::policy::{PolicyError, Preparation};
use crate::durable::testing::{
    FakeChain, GENESIS, ManualTimer, PrepareAnswer, PrepareScript, ScriptedPolicy, block_hash,
    engine_with, engine_with_spawner, extrinsic,
};
use crate::store::Db;
use crate::test_support::wait_until;

const TEST: DomainId = DomainId::from_static("test");
const OTHER: DomainId = DomainId::from_static("other");
const POLICY: PolicyId = PolicyId::from_static("rebuild");
const SECOND_POLICY: PolicyId = PolicyId::from_static("second");

struct Fixture {
    _dir: tempfile::TempDir,
    chain: Arc<FakeChain>,
    engine: Arc<DurableTxEngine>,
    timer: Arc<ManualTimer>,
}

impl Fixture {
    /// Schedules one transaction of `domain` in `group` for `policy`.
    fn schedule(&self, domain: DomainId, group: Option<&str>, policy: PolicyId) -> DurableTxId {
        self.schedule_together(domain, group, policy, 1)[0]
    }

    /// Schedules `count` transactions of `domain` in `group` for `policy`, in
    /// one write.
    fn schedule_together(
        &self,
        domain: DomainId,
        group: Option<&str>,
        policy: PolicyId,
        count: usize,
    ) -> Vec<DurableTxId> {
        let group = group.map(GroupId::new);
        let policy = PolicyRef {
            id: policy,
            params: vec![],
        };
        let request = DurableRequest::scheduled(domain, group, vec![policy; count]).unwrap();
        block_on(self.engine.execute(request)).unwrap()
    }

    fn status(&self, id: DurableTxId) -> DurableTxStatus {
        block_on(self.engine.status(id)).unwrap().unwrap()
    }

    fn wait_for(&self, id: DurableTxId, status: DurableTxStatus) {
        wait_until(
            || self.status(id) == status,
            "the transaction reaches its status",
        );
    }

    fn submitted(&self) -> Vec<crate::chain::EncodedExtrinsic> {
        self.chain.state().submitted.clone()
    }

    /// Lets queued work run, so a test can check that something did not
    /// happen.
    fn settle(&self) {
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// An engine serving the `test` and `other` domains with `policies`.
fn fixture(policies: Vec<Arc<ScriptedPolicy>>) -> Fixture {
    let chain = FakeChain::new(130, 140);
    let mut registry = DurableRegistry::new()
        .with_domain(TEST, Arc::new(Unobservable(GENESIS)))
        .with_domain(OTHER, Arc::new(Unobservable(GENESIS)));
    for policy in policies {
        registry = registry.with_policy(policy);
    }
    let (dir, engine, timer) = engine_with(&chain, registry);
    Fixture {
        _dir: dir,
        chain,
        engine,
        timer,
    }
}

/// Fresh signed bytes on every build, tagged from 100 up.
fn new_bytes(builds: &AtomicU8) -> MortalExtrinsic {
    extrinsic(100 + builds.fetch_add(1, Ordering::SeqCst), 100, 64)
}

/// Builds every transaction it is asked about, with new bytes each time.
fn builds_everything() -> PrepareScript {
    let builds = Arc::new(AtomicU8::new(0));
    Box::new(move |ids| {
        let answer: HashMap<DurableTxId, Preparation> = ids
            .iter()
            .map(|id| (*id, Preparation::Ready(new_bytes(&builds))))
            .collect();
        Box::pin(ready(Ok(answer)))
    })
}

/// Answers every batch with `answer`.
fn always(
    answer: impl Fn(&[DurableTxId]) -> PrepareAnswer + Send + Sync + 'static,
) -> PrepareScript {
    Box::new(move |ids| Box::pin(ready(answer(ids))))
}

fn unavailable() -> PrepareAnswer {
    Err(PolicyError {
        reason: "fee estimate unavailable".into(),
    })
}

fn every(ids: &[DurableTxId], preparation: Preparation) -> PrepareAnswer {
    Ok(ids.iter().map(|id| (*id, preparation.clone())).collect())
}

fn built_by(script: PrepareScript) -> Arc<ScriptedPolicy> {
    ScriptedPolicy::new(POLICY, true, script)
}

/// The rows that wait to be built, read straight from the ledger.
fn waiting(db: &Db) -> Vec<DurableTxId> {
    let read = block_on(db.read(|conn| dao::bucket_submissions(conn, &TEST, &POLICY, None)));
    read.unwrap().iter().map(|scheduled| scheduled.id).collect()
}

#[test]
fn a_scheduled_transaction_is_built_broadcast_and_watched() {
    let fixture = fixture(vec![built_by(builds_everything())]);

    let id = fixture.schedule(TEST, None, POLICY);

    fixture.wait_for(id, DurableTxStatus::Pending);
    wait_until(
        || fixture.submitted() == vec![extrinsic(100, 100, 64).extrinsic],
        "the built extrinsic is broadcast",
    );
    assert!(fixture.engine.ownership.is_owned(id));
}

/// A policy batches what it builds, so a batch must never mix domains or
/// groups: one domain's fee payer or nonce sequence is not another's.
#[test]
fn each_domain_policy_and_group_is_built_in_its_own_batch() {
    let first = built_by(always(|_| Ok(HashMap::new())));
    let second = ScriptedPolicy::new(SECOND_POLICY, true, always(|_| Ok(HashMap::new())));
    let fixture = fixture(vec![first.clone(), second.clone()]);

    let together = fixture.schedule_together(TEST, Some("g1"), POLICY, 2);
    let other_policy = fixture.schedule(TEST, Some("g1"), SECOND_POLICY);
    let other_group = fixture.schedule(TEST, Some("g2"), POLICY);
    let other_domain = fixture.schedule(OTHER, Some("g1"), POLICY);
    let batched_together =
        |policy: &ScriptedPolicy| policy.batches().iter().any(|batch| batch == &together);

    wait_until(|| batched_together(&first), "the shared batch is built");
    wait_until(
        || first.batches().contains(&vec![other_group]),
        "the other group is built alone",
    );
    wait_until(
        || first.batches().contains(&vec![other_domain]),
        "the other domain is built alone",
    );
    wait_until(
        || second.batches().contains(&vec![other_policy]),
        "the other policy is built alone",
    );
}

/// One group's build waits on something slow, such as a remote signer. The
/// other groups of the same policy must not wait behind it.
#[test]
fn a_batch_stuck_in_its_policy_does_not_hold_up_another_group() {
    let first_row = DurableTxId(1);
    let builds = Arc::new(AtomicU8::new(0));
    let script: PrepareScript = Box::new(move |ids| {
        if ids.contains(&first_row) {
            return Box::pin(pending());
        }
        let ready_now = every(ids, Preparation::Ready(new_bytes(&builds)));
        Box::pin(ready(ready_now))
    });
    let fixture = fixture(vec![built_by(script)]);
    let blocked = fixture.schedule(TEST, Some("g1"), POLICY);
    assert_eq!(blocked, first_row);

    let free = fixture.schedule(TEST, Some("g2"), POLICY);

    fixture.wait_for(free, DurableTxStatus::Pending);
    assert_eq!(fixture.status(blocked), DurableTxStatus::PendingSubmission);
}

/// A fee estimate cannot be read while the node is unreachable. That is no
/// verdict: the batch is asked again, later each time, and built once the
/// policy can answer.
#[test]
fn a_policy_that_cannot_answer_is_asked_again_after_a_growing_backoff() {
    let asked = Arc::new(AtomicU8::new(0));
    let calls = asked.clone();
    let policy = built_by(always(move |ids| {
        match calls.fetch_add(1, Ordering::SeqCst) {
            0 | 1 => unavailable(),
            _ => every(ids, Preparation::Ready(extrinsic(100, 100, 64))),
        }
    }));
    let fixture = fixture(vec![policy.clone()]);
    let id = fixture.schedule(TEST, None, POLICY);
    wait_until(|| fixture.timer.sleeps() == 1, "the bucket backs off");

    fixture.timer.advance(INITIAL_BACKOFF);
    wait_until(|| fixture.timer.sleeps() == 2, "the bucket backs off again");
    fixture.timer.advance(INITIAL_BACKOFF);
    fixture.settle();
    assert_eq!(policy.batches().len(), 2);
    fixture.timer.advance(INITIAL_BACKOFF);

    fixture.wait_for(id, DurableTxStatus::Pending);
    assert_eq!(policy.batches().len(), 3);
}

/// The policy has nothing to say yet, for example because the funds it
/// spends have not arrived. Waiting is not failing.
#[test]
fn a_transaction_the_policy_never_answers_keeps_waiting() {
    let policy = built_by(always(|_| Ok(HashMap::new())));
    let fixture = fixture(vec![policy.clone()]);
    let id = fixture.schedule(TEST, None, POLICY);

    for round in 1..=3 {
        wait_until(|| fixture.timer.sleeps() == round, "the bucket backs off");
        fixture.timer.advance(MAX_BACKOFF);
    }

    wait_until(|| policy.batches().len() == 4, "the batch is asked again");
    assert_eq!(fixture.status(id), DurableTxStatus::PendingSubmission);
}

#[test]
fn a_policy_that_gives_up_fails_the_transaction_without_broadcasting() {
    let fixture = fixture(vec![built_by(always(|ids| {
        every(ids, Preparation::GiveUp)
    }))]);

    let id = fixture.schedule(TEST, None, POLICY);

    fixture.wait_for(id, DurableTxStatus::Failure);
    assert_eq!(fixture.submitted(), vec![]);
}

/// An app update dropped the policy, or the domain, that rows from an earlier
/// run name. Nothing can build them, so they fail rather than wait forever.
#[test]
fn rows_nothing_can_build_are_abandoned() {
    let policy = built_by(builds_everything());
    let fixture = fixture(vec![policy.clone()]);
    let schedule_raw = |domain: DomainId, policy: PolicyId| {
        let policy = PolicyRef {
            id: policy,
            params: vec![],
        };
        let write = fixture
            .engine
            .db
            .write(move |tx| dao::schedule(tx, &domain, None, &policy));
        block_on(write).unwrap()
    };
    let unknown_policy = schedule_raw(TEST, PolicyId::from_static("removed"));
    let unknown_domain = schedule_raw(DomainId::from_static("removed"), POLICY);

    fixture.engine.ensure_submissions_started();

    fixture.wait_for(unknown_policy, DurableTxStatus::Failure);
    fixture.wait_for(unknown_domain, DurableTxStatus::Failure);
    assert_eq!(policy.batches(), Vec::<Vec<DurableTxId>>::new());
}

/// The policy hands back the exact bytes of an attempt that was already
/// released. Those bytes were decided once; broadcasting them again could
/// only confuse that decision, so the row keeps waiting for new ones.
#[test]
fn a_build_repeating_a_released_attempt_is_not_broadcast() {
    let policy = built_by(always(|ids| {
        every(ids, Preparation::Ready(extrinsic(100, 100, 64)))
    }));
    let fixture = fixture(vec![policy]);
    let next_id = DurableTxId(1);
    let repeated = extrinsic(100, 100, 64).extrinsic.hash();
    fixture.engine.ownership.release(next_id, repeated);

    let id = fixture.schedule(TEST, None, POLICY);

    wait_until(|| fixture.timer.sleeps() == 1, "the bucket backs off");
    assert_eq!(id, next_id);
    assert_eq!(fixture.status(id), DurableTxStatus::PendingSubmission);
    assert_eq!(fixture.submitted(), vec![]);
}

/// A transaction handed back to its policy is built again, but only after a
/// cooldown, so a policy whose builds keep failing cannot spin.
#[test]
fn a_transaction_handed_back_is_built_again_after_a_cooldown() {
    let policy = built_by(builds_everything());
    let fixture = fixture(vec![policy.clone()]);
    let id = fixture.schedule(TEST, None, POLICY);
    fixture.wait_for(id, DurableTxStatus::Pending);
    wait_until(
        || fixture.timer.sleeps() == 1,
        "the first attempt's watch waits",
    );
    let attempt = block_on(fixture.engine.db.read(move |conn| dao::entry(conn, id)))
        .unwrap()
        .unwrap();
    let back = crate::durable::model::Verdict {
        status: DurableTxStatus::PendingSubmission,
        success_detected_at: None,
        failure: None,
    };
    block_on(fixture.engine.write_verdict(&attempt, back)).unwrap();
    wait_until(|| fixture.timer.sleeps() == 2, "the rebuild cools down");

    fixture
        .timer
        .advance(REBUILD_COOLDOWN - Duration::from_millis(1));
    fixture.settle();
    assert_eq!(policy.batches().len(), 1);
    fixture.timer.advance(Duration::from_millis(1));

    let both = vec![
        extrinsic(100, 100, 64).extrinsic,
        extrinsic(101, 100, 64).extrinsic,
    ];
    wait_until(
        || fixture.submitted() == both,
        "the rebuilt bytes are broadcast",
    );
    assert_eq!(policy.batches(), vec![vec![id], vec![id]]);
}

/// A row scheduled while its bucket is mid-build must not be lost when that
/// build ends: it is built in the next round.
#[test]
fn a_transaction_scheduled_during_a_build_is_built_in_the_next_round() {
    let (release, released) = oneshot::channel::<()>();
    let gate = Mutex::new(Some(released));
    let builds = Arc::new(AtomicU8::new(0));
    let script: PrepareScript = Box::new(move |ids| {
        let answer = every(ids, Preparation::Ready(new_bytes(&builds)));
        let wait = gate.lock().take();
        Box::pin(async move {
            if let Some(released) = wait {
                let _ = released.await;
            }
            answer
        })
    });
    let policy = built_by(script);
    let fixture = fixture(vec![policy.clone()]);
    let first = fixture.schedule(TEST, None, POLICY);
    wait_until(|| policy.batches().len() == 1, "the first build starts");

    let second = fixture.schedule(TEST, None, POLICY);
    release.send(()).unwrap();

    fixture.wait_for(second, DurableTxStatus::Pending);
    assert_eq!(policy.batches(), vec![vec![first], vec![second]]);
}

/// Recovery and registration both start the executor. A second collector
/// would launch every bucket twice as often.
#[test]
fn starting_the_executor_twice_runs_one_collector() {
    let chain = FakeChain::new(130, 140);
    let spawned = Arc::new(AtomicUsize::new(0));
    let counter = spawned.clone();
    let inner = crate::test_support::test_spawner();
    let spawner: crate::subscription::Spawner = Arc::new(move |task| {
        counter.fetch_add(1, Ordering::SeqCst);
        inner(task)
    });
    let registry = DurableRegistry::new().with_domain(TEST, Arc::new(Unobservable(GENESIS)));
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
    let policy = built_by(Box::new(|_| Box::pin(pending())));
    let fixture = fixture(vec![policy.clone()]);
    let id = fixture.schedule(TEST, None, POLICY);
    wait_until(|| policy.batches().len() == 1, "the build starts");
    let Fixture { _dir, engine, .. } = fixture;
    let db = engine.db.clone();

    drop(engine);

    wait_until(
        || Arc::strong_count(&policy) == 1,
        "the build is abandoned with the engine",
    );
    assert_eq!(waiting(&db), vec![id]);
}

/// The row changed while its policy was building it, for example because a
/// second engine on the same database built it first. The build is not an
/// attempt, and the round made no progress.
#[test]
fn a_build_for_a_row_that_changed_meanwhile_starts_no_attempt() {
    let db_slot: Arc<Mutex<Option<Db>>> = Arc::default();
    let changed = db_slot.clone();
    let script: PrepareScript = Box::new(move |ids| {
        let db = changed.lock().clone().unwrap();
        let id = ids[0];
        let answer = every(ids, Preparation::Ready(extrinsic(100, 100, 64)));
        Box::pin(async move {
            db.write(move |tx| dao::abandon_submission(tx, id))
                .await
                .unwrap();
            answer
        })
    });
    let fixture = fixture(vec![built_by(script)]);
    *db_slot.lock() = Some(fixture.engine.db.clone());

    let id = fixture.schedule(TEST, None, POLICY);

    wait_until(|| fixture.timer.sleeps() == 1, "the bucket backs off");
    assert_eq!(fixture.status(id), DurableTxStatus::Failure);
    assert_eq!(fixture.submitted(), vec![]);
    assert!(!fixture.engine.ownership.is_owned(id));
}

/// The policy gives up on a row that has already moved on, so the abandon
/// writes nothing and the round made no progress.
#[test]
fn giving_up_on_a_row_that_changed_meanwhile_fails_nothing() {
    let db_slot: Arc<Mutex<Option<Db>>> = Arc::default();
    let changed = db_slot.clone();
    let script: PrepareScript = Box::new(move |ids| {
        let db = changed.lock().clone().unwrap();
        let id = ids[0];
        let answer = every(ids, Preparation::GiveUp);
        Box::pin(async move {
            let started = extrinsic(100, 100, 64);
            db.write(move |tx| dao::start_attempt(tx, id, &started))
                .await
                .unwrap();
            answer
        })
    });
    let fixture = fixture(vec![built_by(script)]);
    *db_slot.lock() = Some(fixture.engine.db.clone());

    let id = fixture.schedule(TEST, None, POLICY);

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
    let events = fixture.chain.script_watch();
    let presigned = extrinsic(1, 100, 64);
    let submission = DurableSubmission {
        extrinsic: presigned.clone(),
        policy: Some(PolicyRef {
            id: POLICY,
            params: vec![],
        }),
    };
    let request = DurableRequest::presigned(TEST, None, vec![submission]).unwrap();
    let id = block_on(fixture.engine.execute(request)).unwrap()[0];
    let presigned_hash = presigned.extrinsic.hash();
    fixture
        .chain
        .include(125, presigned_hash, DispatchOutcome::Failed);

    events
        .unbounded_send(crate::chain::WatchEvent::InFinalizedBlock(block_hash(125)))
        .unwrap();

    let both = vec![presigned.extrinsic, extrinsic(100, 100, 64).extrinsic];
    wait_until(
        || fixture.submitted() == both,
        "the rebuilt bytes are broadcast",
    );
    assert_eq!(policy.batches(), vec![vec![id]]);
    assert_eq!(fixture.status(id), DurableTxStatus::Pending);
}

#[test]
fn a_failure_without_a_policy_stays_final() {
    let policy = built_by(builds_everything());
    let fixture = fixture(vec![policy.clone()]);
    let events = fixture.chain.script_watch();
    let presigned = extrinsic(1, 100, 64);
    let submission = DurableSubmission {
        extrinsic: presigned.clone(),
        policy: None,
    };
    let request = DurableRequest::presigned(TEST, None, vec![submission]).unwrap();
    let id = block_on(fixture.engine.execute(request)).unwrap()[0];
    fixture
        .chain
        .include(125, presigned.extrinsic.hash(), DispatchOutcome::Failed);

    events
        .unbounded_send(crate::chain::WatchEvent::InFinalizedBlock(block_hash(125)))
        .unwrap();

    fixture.wait_for(id, DurableTxStatus::Failure);
    fixture.settle();
    assert_eq!(policy.batches(), Vec::<Vec<DurableTxId>>::new());
    assert_eq!(fixture.submitted(), vec![presigned.extrinsic]);
}

#[test]
fn the_backoff_doubles_from_two_seconds_up_to_two_minutes() {
    let waits: Vec<u64> = (1..=9).map(|rounds| backoff(rounds).as_secs()).collect();

    assert_eq!(waits, vec![2, 4, 8, 16, 32, 64, 120, 120, 120]);
    assert_eq!(backoff(u32::MAX), MAX_BACKOFF);
}

#[test]
fn the_rebuild_cooldown_doubles_from_five_seconds_up_to_ten_minutes() {
    let waits: Vec<u64> = (0..=9)
        .map(|rebuilds| cooldown(rebuilds).as_secs())
        .collect();

    assert_eq!(waits, vec![0, 5, 10, 20, 40, 80, 160, 320, 600, 600]);
    assert_eq!(cooldown(u32::MAX), MAX_REBUILD_COOLDOWN);
}
