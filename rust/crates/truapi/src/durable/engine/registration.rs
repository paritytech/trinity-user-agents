//! Registration: records presigned extrinsics and hands each to a
//! submission watch once its row is committed, or records transactions a
//! policy builds later.

use std::sync::Arc;

use futures::channel::oneshot;
use parking_lot::Mutex;
use subxt::utils::H256;

use super::DurableTxEngine;
use super::tracker::{Attempt, spawn_watch};
use crate::chain::MortalExtrinsic;
use crate::durable::dao;
use crate::durable::model::{DomainId, DurableTxId, GroupId, PolicyId, SubmissionPlan};
use crate::durable::ownership::Ownership;
use crate::store::DbError;

/// What to register: transactions of one domain, optionally under a group,
/// either signed already or left for a policy to build.
pub struct DurableRequest {
    domain: DomainId,
    group: Option<GroupId>,
    transactions: RequestedTransactions,
}

/// One presigned extrinsic, and the plan that builds it again after a
/// failure its policy accepts to retry. `None` makes every proven failure
/// final.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DurableSubmission {
    /// Signed and ready to broadcast.
    pub extrinsic: MortalExtrinsic,
    /// How it is rebuilt, if it is.
    pub plan: Option<SubmissionPlan>,
}

/// The transactions of a request, all of one kind.
enum RequestedTransactions {
    /// Signed already: recorded as pending and broadcast once committed.
    Presigned(Vec<DurableSubmission>),
    /// Built later, each through its plan.
    Scheduled(Vec<SubmissionPlan>),
}

/// A request named no transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("a durable request needs at least one transaction, got none")]
pub struct EmptyRequest;

/// Why a registration was not recorded. Nothing was written or broadcast.
#[derive(Debug, thiserror::Error)]
pub enum RegistrationError {
    /// No oracle is registered for the domain, so nothing could decide its
    /// transactions.
    #[error("no durable domain {}: register an oracle for it first", .0.as_str())]
    UnknownDomain(DomainId),
    /// The request names a submission policy the registry does not have, so
    /// nothing could build or rebuild its transaction.
    #[error("no submission policy {}: register it first", .0.as_str())]
    UnknownPolicy(PolicyId),
    /// The write failed or the caller's hook refused it.
    #[error(transparent)]
    Db(#[from] DbError),
}

impl RequestedTransactions {
    fn is_empty(&self) -> bool {
        match self {
            Self::Presigned(submissions) => submissions.is_empty(),
            Self::Scheduled(plans) => plans.is_empty(),
        }
    }

    /// The presigned submissions, in order. Empty for a schedule.
    fn presigned(&self) -> &[DurableSubmission] {
        match self {
            Self::Presigned(submissions) => submissions,
            Self::Scheduled(_) => &[],
        }
    }

    /// The scheduled transactions' plans, in order. Empty for presigned
    /// ones.
    fn scheduled(&self) -> &[SubmissionPlan] {
        match self {
            Self::Presigned(_) => &[],
            Self::Scheduled(plans) => plans,
        }
    }
}

impl DurableRequest {
    /// Presigned `submissions`, recorded as pending and broadcast once
    /// committed.
    pub fn presigned(
        domain: DomainId,
        group: Option<GroupId>,
        submissions: Vec<DurableSubmission>,
    ) -> Result<Self, EmptyRequest> {
        Self::new(domain, group, RequestedTransactions::Presigned(submissions))
    }

    /// One transaction per plan in `plans`, recorded as waiting and built
    /// through its plan once committed.
    pub fn scheduled(
        domain: DomainId,
        group: Option<GroupId>,
        plans: Vec<SubmissionPlan>,
    ) -> Result<Self, EmptyRequest> {
        Self::new(domain, group, RequestedTransactions::Scheduled(plans))
    }

    fn new(
        domain: DomainId,
        group: Option<GroupId>,
        transactions: RequestedTransactions,
    ) -> Result<Self, EmptyRequest> {
        if transactions.is_empty() {
            return Err(EmptyRequest);
        }
        Ok(Self {
            domain,
            group,
            transactions,
        })
    }

    /// Every plan in the request, to build or to rebuild.
    fn named_plans(&self) -> impl Iterator<Item = &SubmissionPlan> {
        let presigned = self.transactions.presigned().iter();
        let rebuilds = presigned.filter_map(|submission| submission.plan.as_ref());
        rebuilds.chain(self.transactions.scheduled())
    }

    /// Whether anything in the request is built or rebuilt by a policy.
    fn names_a_policy(&self) -> bool {
        self.named_plans().next().is_some()
    }

    /// Records every transaction as a row, in order.
    fn insert_all(&self, tx: &rusqlite::Transaction<'_>) -> Result<Vec<DurableTxId>, DbError> {
        let group = self.group.as_ref();
        match &self.transactions {
            RequestedTransactions::Presigned(submissions) => submissions
                .iter()
                .map(|submission| {
                    let plan = submission.plan.as_ref();
                    dao::insert(tx, &self.domain, group, &submission.extrinsic, plan)
                })
                .collect(),
            RequestedTransactions::Scheduled(plans) => plans
                .iter()
                .map(|plan| dao::schedule(tx, &self.domain, group, plan))
                .collect(),
        }
    }
}

impl DurableTxEngine {
    /// Records `request` in its own write and broadcasts it once committed.
    pub async fn execute(
        self: &Arc<Self>,
        request: DurableRequest,
    ) -> Result<Vec<DurableTxId>, RegistrationError> {
        let (ids, ()) = self.execute_with(request, |_, _| Ok(())).await?;
        Ok(ids)
    }

    /// Records `request` and runs `on_register` with the new ids in the same
    /// write, so the domain's own rows commit or roll back with the ledger's.
    /// An error from `on_register` rolls everything back and broadcasts
    /// nothing. The registration finishes even when the caller stops
    /// waiting, because committed rows are useless without their broadcast.
    pub async fn execute_with<T, F>(
        self: &Arc<Self>,
        request: DurableRequest,
        on_register: F,
    ) -> Result<(Vec<DurableTxId>, T), RegistrationError>
    where
        F: FnOnce(&rusqlite::Transaction<'_>, &[DurableTxId]) -> Result<T, DbError>
            + Send
            + 'static,
        T: Send + 'static,
    {
        let genesis = self.genesis_if_registered(&request)?;
        if request.names_a_policy() {
            self.ensure_submissions_started();
        }
        // Detached from the caller: once queued, the write commits on the
        // database thread even if this future is dropped (a cancelled FFI
        // call, a select), and this task holds the only copy of the signed
        // bytes that must then be broadcast.
        let (done, outcome) = oneshot::channel();
        let engine = self.clone();
        (self.spawner)(Box::pin(async move {
            let _ = done.send(engine.register(genesis, request, on_register).await);
        }));
        outcome
            .await
            .unwrap_or(Err(RegistrationError::Db(DbError::Closed)))
    }

    /// The genesis of the request's chain, once every domain and policy it
    /// names is registered.
    fn genesis_if_registered(&self, request: &DurableRequest) -> Result<H256, RegistrationError> {
        let genesis = self
            .chain_of(&request.domain)
            .ok_or_else(|| RegistrationError::UnknownDomain(request.domain.clone()))?;
        let unknown_policy = request
            .named_plans()
            .find(|plan| self.registry.submission_policy(&plan.policy).is_none());
        match unknown_policy {
            Some(plan) => Err(RegistrationError::UnknownPolicy(plan.policy.clone())),
            None => Ok(genesis),
        }
    }

    /// Records the rows, then broadcasts what was committed.
    async fn register<T, F>(
        self: Arc<Self>,
        genesis: H256,
        request: DurableRequest,
        on_register: F,
    ) -> Result<(Vec<DurableTxId>, T), RegistrationError>
    where
        F: FnOnce(&rusqlite::Transaction<'_>, &[DurableTxId]) -> Result<T, DbError>
            + Send
            + 'static,
        T: Send + 'static,
    {
        let submissions = request.transactions.presigned().to_vec();
        let (ids, value) = self.record(request, on_register).await?;
        // A watch only runs for an attempt this engine owns, so it never
        // races a recovery pass deciding the same row.
        for (id, DurableSubmission { extrinsic, .. }) in ids.iter().zip(submissions) {
            if self.ownership.is_owned(*id) {
                spawn_watch(
                    &self,
                    Attempt::new(*id, genesis, &extrinsic),
                    extrinsic.extrinsic,
                );
            }
        }
        Ok((ids, value))
    }

    /// Writes the rows and the hook's rows in one transaction, taking
    /// ownership of each attempt inside it so a recovery pass never reaches
    /// a committed row before its watch. A rolled-back write gives the
    /// ownership up again.
    async fn record<T, F>(
        &self,
        request: DurableRequest,
        on_register: F,
    ) -> Result<(Vec<DurableTxId>, T), DbError>
    where
        F: FnOnce(&rusqlite::Transaction<'_>, &[DurableTxId]) -> Result<T, DbError>
            + Send
            + 'static,
        T: Send + 'static,
    {
        let taken = Arc::new(Mutex::new(Vec::new()));
        let (ownership, claims) = (self.ownership.clone(), taken.clone());
        let written = self
            .db
            .write(move |tx| {
                let ids = request.insert_all(tx)?;
                let owned = own(&ownership, &ids, request.transactions.presigned());
                claims.lock().extend(owned);
                Ok((ids.clone(), on_register(tx, &ids)?))
            })
            .await;
        if written.is_err() {
            self.ownership.abandon_all(&taken.lock());
        }
        written
    }
}

/// Takes ownership of each new presigned row's attempt. Returns what was
/// taken.
fn own(
    ownership: &Ownership,
    ids: &[DurableTxId],
    submissions: &[DurableSubmission],
) -> Vec<(DurableTxId, H256)> {
    ids.iter()
        .zip(submissions)
        .map(|(id, submission)| (*id, submission.extrinsic.extrinsic.hash()))
        .filter(|(id, hash)| ownership.acquire(*id, *hash))
        .collect()
}

#[cfg(test)]
mod tests {
    use futures::FutureExt;
    use futures::executor::block_on;

    use super::*;
    use crate::durable::model::{DurableTxState, DurableTxStatus, PolicyParams};
    use crate::durable::testing::{
        FakeChain, REBUILD_POLICY, RetryAnswer, ScriptedPolicy, engine_with_policies, extrinsic,
        rebuild_plan, test_engine, without_plan,
    };
    use crate::test_support::wait_until;

    fn domain() -> DomainId {
        DomainId::new("test")
    }

    fn request(extrinsics: Vec<MortalExtrinsic>) -> DurableRequest {
        DurableRequest::presigned(domain(), Some(GroupId::new("op")), without_plan(extrinsics))
            .unwrap()
    }

    #[test]
    fn a_request_with_no_extrinsic_cannot_be_built() {
        assert!(matches!(
            DurableRequest::presigned(domain(), None, vec![]),
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
            block_on(engine.group(&domain(), &GroupId::new("op"))).unwrap(),
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
            block_on(engine.group(&domain(), &GroupId::new("op"))).unwrap(),
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
            DomainId::new("orphan"),
            None,
            without_plan(vec![extrinsic(1, 100, 64)]),
        )
        .unwrap();

        assert!(matches!(
            block_on(engine.execute(orphan)),
            Err(RegistrationError::UnknownDomain(domain)) if domain == DomainId::new("orphan")
        ));
    }

    /// An engine serving the `test` domain with the `rebuild` policy, which
    /// never builds anything.
    fn engine_with_policy(chain: &Arc<FakeChain>) -> (tempfile::TempDir, Arc<DurableTxEngine>) {
        let policy = ScriptedPolicy::never_building(REBUILD_POLICY, RetryAnswer::Final);
        let (dir, engine, _timer) = engine_with_policies(chain, &[domain()], vec![policy]);
        (dir, engine)
    }

    fn scheduled(plans: Vec<SubmissionPlan>) -> DurableRequest {
        DurableRequest::scheduled(domain(), Some(GroupId::new("op")), plans).unwrap()
    }

    #[test]
    fn an_empty_schedule_is_refused() {
        assert!(matches!(
            DurableRequest::scheduled(domain(), None, vec![]),
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
            block_on(engine.group(&domain(), &GroupId::new("op"))).unwrap(),
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
        let request = DurableRequest::presigned(domain(), None, vec![submission]).unwrap();

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
            block_on(engine.group(&domain(), &GroupId::new("op"))).unwrap(),
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
            block_on(engine.group(&domain(), &GroupId::new("op"))).unwrap(),
            vec![]
        );
    }
}
