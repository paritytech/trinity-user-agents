use futures::FutureExt;
use futures::executor::block_on;

use super::*;
use crate::durable::model::{DurableTxState, DurableTxStatus, PolicyParams};
use crate::durable::testing::{
    FakeChain, REBUILD_POLICY, RetryAnswer, ScriptedPolicy, engine_with_policies, extrinsic,
    rebuild_plan, test_engine, without_plan,
};
use crate::test_support::wait_until;

const DOMAIN: DomainId = DomainId::from_static("test");

fn request(extrinsics: Vec<MortalExtrinsic>) -> DurableRequest {
    DurableRequest::presigned(DOMAIN, Some(GroupId::new("op")), without_plan(extrinsics)).unwrap()
}

#[test]
fn a_request_with_no_extrinsic_cannot_be_built() {
    assert!(matches!(
        DurableRequest::presigned(DOMAIN, None, vec![]),
        Err(EmptyRequest)
    ));
}

#[test]
fn registered_ids_come_back_in_order_and_are_owned_by_their_watch() {
    let chain = FakeChain::new(130, 140);
    let (_dir, engine, _timer) = test_engine(&chain);

    let ids = block_on(engine.execute(request(vec![extrinsic(1, 100, 64), extrinsic(2, 100, 64)])))
        .unwrap();

    assert!(ids[0] < ids[1]);
    assert_eq!(
        block_on(engine.group(&DOMAIN, &GroupId::new("op"))).unwrap(),
        ids.iter()
            .map(|id| DurableTxState {
                id: *id,
                status: DurableTxStatus::Pending
            })
            .collect::<Vec<_>>()
    );
    assert!(ids.iter().all(|id| engine.ownership.is_owned(*id)));
}

/// Each extrinsic is submitted only after the row that tracks it commits.
#[test]
fn committed_extrinsics_are_broadcast() {
    let chain = FakeChain::new(130, 140);
    let (_dir, engine, _timer) = test_engine(&chain);

    block_on(engine.execute(request(vec![extrinsic(1, 100, 64)]))).unwrap();

    wait_until(
        || chain.state().submitted == vec![extrinsic(1, 100, 64).extrinsic],
        "the registered extrinsic is submitted",
    );
}

/// A caller can stop waiting once the write is under way, as a cancelled
/// FFI call does. The rows still commit, so their extrinsics must still
/// be broadcast: nothing else holds the signed bytes.
#[test]
fn a_registration_whose_caller_gives_up_still_broadcasts() {
    let chain = FakeChain::new(130, 140);
    let (_dir, engine, _timer) = test_engine(&chain);

    let mut registering = Box::pin(engine.execute(request(vec![extrinsic(1, 100, 64)])));
    assert!(registering.as_mut().now_or_never().is_none());
    drop(registering);

    wait_until(
        || chain.state().submitted.len() == 1,
        "the committed extrinsic is submitted",
    );
}

/// A recovery pass may run the moment the rows commit. Ownership is taken
/// inside the write, so the pass never evaluates a transaction its watch is
/// about to decide.
#[test]
fn a_registered_transaction_is_owned_before_its_rows_commit() {
    let chain = FakeChain::new(130, 140);
    let (_dir, engine, _timer) = test_engine(&chain);
    let probe = engine.clone();

    let (_, owned_in_write) = block_on(
        engine.execute_with(request(vec![extrinsic(1, 100, 64)]), move |_, ids| {
            Ok(probe.ownership.is_owned(ids[0]))
        }),
    )
    .unwrap();

    assert!(owned_in_write);
}

/// A domain locks the items a transaction spends in the same write as the
/// ledger row, so neither can exist without the other.
#[test]
fn the_hook_writes_its_rows_in_the_same_transaction_with_the_new_ids() {
    let chain = FakeChain::new(130, 140);
    let (_dir, engine, _timer) = test_engine(&chain);

    let (ids, hooked) = block_on(engine.execute_with(
        request(vec![extrinsic(1, 100, 64)]),
        |tx, ids| {
            tx.execute_batch("CREATE TABLE locks (tx_id INTEGER NOT NULL)")?;
            tx.execute("INSERT INTO locks (tx_id) VALUES (?1)", [ids[0].0])?;
            Ok(ids.to_vec())
        },
    ))
    .unwrap();

    let locked: i64 = block_on(
        engine
            .db
            .read(|conn| Ok(conn.query_row("SELECT tx_id FROM locks", [], |row| row.get(0))?)),
    )
    .unwrap();
    assert_eq!(hooked, ids.clone());
    assert_eq!(locked, ids[0].0);
}

/// The domain finds an item it meant to lock already spent and refuses the
/// registration: nothing may be recorded, owned or submitted.
#[test]
fn a_failing_hook_rolls_back_every_row_and_broadcasts_nothing() {
    let chain = FakeChain::new(130, 140);
    let (_dir, engine, _timer) = test_engine(&chain);

    let result = block_on(engine.execute_with(
        request(vec![extrinsic(1, 100, 64), extrinsic(2, 100, 64)]),
        |tx, _| -> Result<(), DbError> {
            tx.execute_batch("CREATE TABLE locks (tx_id INTEGER NOT NULL)")?;
            Err(DbError::Connection("domain refused".into()))
        },
    ));

    let tables: i64 = block_on(engine.db.read(|conn| {
        Ok(conn.query_row(
            "SELECT count(*) FROM sqlite_schema WHERE name = 'locks'",
            [],
            |row| row.get(0),
        )?)
    }))
    .unwrap();
    assert!(matches!(result, Err(RegistrationError::Db(_))));
    assert_eq!(
        block_on(engine.group(&DOMAIN, &GroupId::new("op"))).unwrap(),
        vec![]
    );
    assert_eq!(tables, 0);
    assert_eq!(chain.state().submitted.len(), 0);
    assert!(!engine.ownership.is_owned(DurableTxId(1)));
}

/// A rolled-back id is handed out again, so its ownership must be given
/// up rather than marked released: the same bytes registered again under
/// the reused id are still watched.
#[test]
fn the_same_extrinsic_registered_again_after_a_rollback_is_broadcast() {
    let chain = FakeChain::new(130, 140);
    let (_dir, engine, _timer) = test_engine(&chain);
    let refused = block_on(
        engine.execute_with(request(vec![extrinsic(1, 100, 64)]), |_, _| {
            Err::<(), _>(DbError::Connection("domain refused".into()))
        }),
    );
    assert!(refused.is_err());

    block_on(engine.execute(request(vec![extrinsic(1, 100, 64)]))).unwrap();

    wait_until(
        || chain.state().submitted.len() == 1,
        "the registered extrinsic is submitted",
    );
}

#[test]
fn a_domain_without_an_oracle_cannot_register() {
    let chain = FakeChain::new(130, 140);
    let (_dir, engine, _timer) = test_engine(&chain);
    let orphan = DurableRequest::presigned(
        DomainId::from_static("orphan"),
        None,
        without_plan(vec![extrinsic(1, 100, 64)]),
    )
    .unwrap();

    assert!(matches!(
        block_on(engine.execute(orphan)),
        Err(RegistrationError::UnknownDomain(domain)) if domain == DomainId::from_static("orphan")
    ));
}

/// An engine serving the `test` domain with the `rebuild` policy, which
/// never builds anything.
fn engine_with_policy(chain: &Arc<FakeChain>) -> (tempfile::TempDir, Arc<DurableTxEngine>) {
    let policy = ScriptedPolicy::never_building(REBUILD_POLICY, RetryAnswer::Final);
    let (dir, engine, _timer) = engine_with_policies(chain, &[DOMAIN], vec![policy]);
    (dir, engine)
}

fn scheduled(plans: Vec<SubmissionPlan>) -> DurableRequest {
    DurableRequest::scheduled(DOMAIN, Some(GroupId::new("op")), plans).unwrap()
}

#[test]
fn an_empty_schedule_is_refused() {
    assert!(matches!(
        DurableRequest::scheduled(DOMAIN, None, vec![]),
        Err(EmptyRequest)
    ));
}

/// Nothing is signed yet, so there is nothing to broadcast or watch: the
/// row waits for its policy.
#[test]
fn a_scheduled_transaction_waits_for_its_policy_and_broadcasts_nothing() {
    let chain = FakeChain::new(130, 140);
    let (_dir, engine) = engine_with_policy(&chain);

    let ids = block_on(engine.execute(scheduled(vec![rebuild_plan(vec![1]), rebuild_plan(vec![1])]))).unwrap();

    assert_eq!(
        block_on(engine.group(&DOMAIN, &GroupId::new("op"))).unwrap(),
        ids.iter()
            .map(|id| DurableTxState {
                id: *id,
                status: DurableTxStatus::PendingSubmission
            })
            .collect::<Vec<_>>()
    );
    assert_eq!(chain.state().submitted.len(), 0);
    assert!(ids.iter().all(|id| !engine.ownership.is_owned(*id)));
}

/// A presigned transaction that names a policy is rebuilt by it after a
/// failure, so the row must remember which policy that is.
#[test]
fn a_presigned_transaction_keeps_the_policy_that_rebuilds_it() {
    let chain = FakeChain::new(130, 140);
    let (_dir, engine) = engine_with_policy(&chain);
    let submission = DurableSubmission {
        extrinsic: extrinsic(1, 100, 64),
        plan: Some(rebuild_plan(vec![1])),
    };
    let request = DurableRequest::presigned(DOMAIN, None, vec![submission]).unwrap();

    let ids = block_on(engine.execute(request)).unwrap();

    let id = ids[0];
    let plan = block_on(engine.db.read(move |conn| dao::submission_plan(conn, id)));
    assert_eq!(plan.unwrap(), Some(rebuild_plan(vec![1])));
    wait_until(
        || chain.state().submitted.len() == 1,
        "the presigned extrinsic is broadcast",
    );
}

/// A policy missing from the registry is a wiring bug: refused at the
/// call site instead of surfacing as a failed transaction later.
#[test]
fn a_request_naming_an_unregistered_policy_writes_nothing() {
    let chain = FakeChain::new(130, 140);
    let (_dir, engine) = engine_with_policy(&chain);
    let unknown = SubmissionPlan {
        policy: PolicyId::from_static("unknown"),
        params: PolicyParams::default(),
    };

    let result = block_on(engine.execute(scheduled(vec![rebuild_plan(vec![1]), unknown])));

    assert!(matches!(
        result,
        Err(RegistrationError::UnknownPolicy(policy)) if policy == PolicyId::from_static("unknown")
    ));
    assert_eq!(
        block_on(engine.group(&DOMAIN, &GroupId::new("op"))).unwrap(),
        vec![]
    );
}

/// The domain locks what the scheduled transaction will spend in the same
/// write. When that fails, the schedule must roll back with it, or a row
/// would be built that spends nothing locked.
#[test]
fn a_schedule_whose_hook_fails_rolls_back_with_it() {
    let chain = FakeChain::new(130, 140);
    let (_dir, engine) = engine_with_policy(&chain);

    let result = block_on(engine.execute_with(scheduled(vec![rebuild_plan(vec![1])]), |_, _| {
        Err::<(), _>(DbError::Connection("domain refused".into()))
    }));

    assert!(matches!(result, Err(RegistrationError::Db(_))));
    assert_eq!(
        block_on(engine.group(&DOMAIN, &GroupId::new("op"))).unwrap(),
        vec![]
    );
}
