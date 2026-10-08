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
mod tests;
