mod batching;
mod pacing;
mod races;

use core::time::Duration;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};

use futures::executor::block_on;
use futures::future::{pending, ready};

use super::*;
use crate::chain::{EncodedExtrinsic, MortalExtrinsic};
use crate::durable::engine::DurableRequest;
use crate::durable::model::{DomainId, DurableTxId, DurableTxStatus, GroupId, PolicyId, PolicyParams,
    SubmissionPlan,
};
use crate::durable::oracle::{DurableRegistry, Unobservable};
use crate::durable::policy::{PolicyError, Preparation};
use crate::durable::testing::{
    FakeChain, GENESIS, ManualTimer, PrepareAnswer, PrepareScript, REBUILD_POLICY, RetryAnswer,
    ScriptedPolicy, engine_with_policies, engine_with_spawner, extrinsic, leave_waiting, rebuild_plan,
};
use crate::store::Db;
use crate::subscription::Spawner;
use crate::test_support::{test_spawner, wait_until};

const TEST: DomainId = DomainId::from_static("test");
const OTHER: DomainId = DomainId::from_static("other");

struct Fixture {
    _dir: tempfile::TempDir,
    chain: Arc<FakeChain>,
    engine: Arc<DurableTxEngine>,
    timer: Arc<ManualTimer>,
}

impl Fixture {
    /// Schedules one transaction of `domain` in `group` for `policy`.
    fn schedule_one(&self, domain: DomainId, group: Option<&str>, policy: PolicyId) -> DurableTxId {
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
        let plan = SubmissionPlan {
            policy,
            params: PolicyParams::default(),
        };
        let request = DurableRequest::scheduled(domain, group, vec![plan; count]).unwrap();
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

    fn submitted(&self) -> Vec<EncodedExtrinsic> {
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
    let (dir, engine, timer) = engine_with_policies(&chain, &[TEST, OTHER], policies);
    Fixture {
        _dir: dir,
        chain,
        engine,
        timer,
    }
}

/// Fresh signed bytes on every build, tagged from 100 up.
fn new_bytes(builds: &AtomicU8) -> MortalExtrinsic {
    let tag = 100 + builds.fetch_add(1, Ordering::SeqCst);
    extrinsic(tag, 100, 64)
}

/// The bytes [`new_bytes`] builds under `tags`, as the chain receives them.
fn broadcast_bytes(tags: &[u8]) -> Vec<EncodedExtrinsic> {
    tags.iter()
        .map(|tag| extrinsic(*tag, 100, 64).extrinsic)
        .collect()
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

/// Answers every batch with what `answer` gives for its ids.
fn answer_every_batch(
    answer: impl Fn(&[DurableTxId]) -> PrepareAnswer + Send + Sync + 'static,
) -> PrepareScript {
    Box::new(move |ids| Box::pin(ready(answer(ids))))
}

fn unavailable() -> PrepareAnswer {
    Err(PolicyError {
        reason: "fee estimate unavailable".into(),
    })
}

fn answer_all_with(ids: &[DurableTxId], preparation: Preparation) -> PrepareAnswer {
    Ok(ids.iter().map(|id| (*id, preparation.clone())).collect())
}

/// The `rebuild` policy, retrying every failure and building with `script`.
fn built_by(script: PrepareScript) -> Arc<ScriptedPolicy> {
    ScriptedPolicy::new(REBUILD_POLICY, RetryAnswer::Retry, script)
}

/// The rows that wait to be built, read straight from the ledger.
fn waiting_ids(db: &Db) -> Vec<DurableTxId> {
    let read = db.read(|conn| dao::bucket_submissions(conn, &TEST, &REBUILD_POLICY, None));
    let rows = block_on(read).unwrap();
    rows.iter().map(|scheduled| scheduled.id).collect()
}

#[test]
fn a_scheduled_transaction_is_built_broadcast_and_watched() {
    let fixture = fixture(vec![built_by(builds_everything())]);

    let id = fixture.schedule_one(TEST, None, REBUILD_POLICY);

    fixture.wait_for(id, DurableTxStatus::Pending);
    let built = broadcast_bytes(&[100]);
    wait_until(
        || fixture.submitted() == built,
        "the built extrinsic is broadcast",
    );
    assert!(fixture.engine.ownership.is_owned(id));
}

#[test]
fn a_policy_that_gives_up_fails_the_transaction_without_broadcasting() {
    let gives_up = answer_every_batch(|ids| answer_all_with(ids, Preparation::GiveUp));
    let fixture = fixture(vec![built_by(gives_up)]);

    let id = fixture.schedule_one(TEST, None, REBUILD_POLICY);

    fixture.wait_for(id, DurableTxStatus::Failure);
    assert_eq!(fixture.submitted(), vec![]);
}

/// An app update dropped the policy, or the domain, that rows from an earlier
/// run name. Nothing can build them, so they fail rather than wait forever.
#[test]
fn rows_nothing_can_build_are_abandoned() {
    let policy = built_by(builds_everything());
    let fixture = fixture(vec![policy.clone()]);
    let db = &fixture.engine.db;
    let removed = SubmissionPlan {
        policy: PolicyId::from_static("removed"),
        params: PolicyParams::default(),
    };
    let unknown_policy = leave_waiting(db, TEST, removed);
    let rebuild = rebuild_plan(vec![]);
    let unknown_domain = leave_waiting(db, DomainId::from_static("removed"), rebuild);

    fixture.engine.ensure_submissions_started();

    fixture.wait_for(unknown_policy, DurableTxStatus::Failure);
    fixture.wait_for(unknown_domain, DurableTxStatus::Failure);
    assert_eq!(policy.batches(), Vec::<Vec<DurableTxId>>::new());
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
    let id = fixture.schedule_one(TEST, None, REBUILD_POLICY);
    wait_until(|| policy.batches().len() == 1, "the build starts");
    let Fixture { _dir, engine, .. } = fixture;
    let db = engine.db.clone();

    drop(engine);

    wait_until(
        || Arc::strong_count(&policy) == 1,
        "the build is abandoned with the engine",
    );
    assert_eq!(waiting_ids(&db), vec![id]);
}
