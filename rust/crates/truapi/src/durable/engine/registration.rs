//! Registration: records presigned extrinsics and hands each to a
//! submission watch once its row is committed, or records transactions a
//! policy builds later.

use std::sync::Arc;

use futures::channel::oneshot;
use parking_lot::Mutex;
use subxt::utils::H256;

use super::{
	DurableTxEngine,
	tracker::{Attempt, spawn_watch},
};
use crate::{
	chain::MortalExtrinsic,
	durable::{
		dao,
		model::{DomainId, DurableTxId, GroupId, PolicyId, PolicyParams, SubmissionPlan},
		ownership::{Ownership, OwnershipClaim},
	},
	store::DbError,
};

/// What to register: transactions of one domain, optionally under a group,
/// either signed already or left for a policy to build. `E` is what the
/// caller refuses the registration with, from inside its write.
pub struct DurableRequest<E = DbError> {
	domain: DomainId,
	group: Option<GroupId>,
	transactions: RequestedTransactions<E>,
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

/// Works out, inside the registration write, the params of each transaction
/// a request schedules, or refuses the registration.
type PlanInWrite<E> =
	Box<dyn FnOnce(&rusqlite::Transaction<'_>) -> Result<Vec<PolicyParams>, E> + Send>;

/// The transactions of a request, all of one kind.
enum RequestedTransactions<E> {
	/// Signed already: recorded as pending and broadcast once committed.
	Presigned(Vec<DurableSubmission>),
	/// Built later, each through its plan.
	Scheduled(Vec<SubmissionPlan>),
	/// Built later by `policy`, with params worked out in the write.
	PlannedInWrite {
		/// The policy that builds every one of them.
		policy: PolicyId,
		/// Their params, one transaction each.
		plan: PlanInWrite<E>,
	},
}

/// A request named no transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("a durable request needs at least one transaction, got none")]
pub struct EmptyRequest;

/// Why a registration was not recorded. Nothing was written or broadcast.
/// `E` is what the caller's hook refuses with.
#[derive(Debug, thiserror::Error)]
pub enum RegistrationError<E = DbError> {
	/// No oracle is registered for the domain, so nothing could decide its
	/// transactions.
	#[error("no durable domain {}: register an oracle for it first", .0.as_str())]
	UnknownDomain(DomainId),
	/// The request names a submission policy the registry does not have, so
	/// nothing could build or rebuild its transaction.
	#[error("no submission policy {}: register it first", .0.as_str())]
	UnknownPolicy(PolicyId),
	/// The write failed.
	#[error(transparent)]
	Db(#[from] DbError),
	/// The caller's hook refused the registration.
	#[error("the registering domain refused: {0}")]
	Hook(E),
}

impl<E> RequestedTransactions<E> {
	/// Whether it names no transaction. A plan worked out in the write is
	/// counted only there.
	fn is_empty(&self) -> bool {
		match self {
			Self::Presigned(submissions) => submissions.is_empty(),
			Self::Scheduled(plans) => plans.is_empty(),
			Self::PlannedInWrite { .. } => false,
		}
	}

	/// The presigned submissions, in order. Empty for a schedule.
	fn presigned(&self) -> &[DurableSubmission] {
		match self {
			Self::Presigned(submissions) => submissions,
			Self::Scheduled(_) | Self::PlannedInWrite { .. } => &[],
		}
	}

	/// Every policy named to build or rebuild a transaction.
	fn named_policies(&self) -> Vec<&PolicyId> {
		match self {
			Self::Presigned(submissions) => submissions
				.iter()
				.filter_map(|submission| submission.plan.as_ref())
				.map(|plan| &plan.policy)
				.collect(),
			Self::Scheduled(plans) => plans.iter().map(|plan| &plan.policy).collect(),
			Self::PlannedInWrite { policy, .. } => vec![policy],
		}
	}
}

impl<E> DurableRequest<E> {
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

	/// One transaction per params `plan` works out inside the registration
	/// write, each recorded as waiting and built by `policy` once committed.
	/// An `Err` from `plan` refuses the registration as
	/// [`RegistrationError::Hook`].
	pub fn scheduled_in(
		domain: DomainId,
		group: Option<GroupId>,
		policy: PolicyId,
		plan: impl FnOnce(&rusqlite::Transaction<'_>) -> Result<Vec<PolicyParams>, E> + Send + 'static,
	) -> Self {
		let plan = Box::new(plan);
		Self { domain, group, transactions: RequestedTransactions::PlannedInWrite { policy, plan } }
	}

	fn new(
		domain: DomainId,
		group: Option<GroupId>,
		transactions: RequestedTransactions<E>,
	) -> Result<Self, EmptyRequest> {
		if transactions.is_empty() {
			return Err(EmptyRequest);
		}
		Ok(Self { domain, group, transactions })
	}

	/// Whether anything in the request is built or rebuilt by a policy.
	fn uses_submission_policy(&self) -> bool {
		!self.transactions.named_policies().is_empty()
	}

	/// Records every transaction as a row, in order, or returns the plan's
	/// refusal.
	fn insert_all(
		self,
		tx: &rusqlite::Transaction<'_>,
	) -> Result<Result<Vec<DurableTxId>, E>, DbError> {
		let Self { domain, group, transactions } = self;
		let plans = match transactions {
			RequestedTransactions::Presigned(submissions) => {
				return insert_presigned(tx, &domain, group.as_ref(), &submissions).map(Ok);
			},
			RequestedTransactions::Scheduled(plans) => plans,
			RequestedTransactions::PlannedInWrite { policy, plan } => match plan(tx) {
				Ok(params) => plans_of(&policy, params),
				Err(refusal) => return Ok(Err(refusal)),
			},
		};
		schedule_all(tx, &domain, group.as_ref(), &plans).map(Ok)
	}
}

/// Records each presigned submission as a pending row, in order.
fn insert_presigned(
	tx: &rusqlite::Transaction<'_>,
	domain: &DomainId,
	group: Option<&GroupId>,
	submissions: &[DurableSubmission],
) -> Result<Vec<DurableTxId>, DbError> {
	let insert = |submission: &DurableSubmission| {
		dao::insert(tx, domain, group, &submission.extrinsic, submission.plan.as_ref())
	};
	submissions.iter().map(insert).collect()
}

/// Records each plan as a waiting row, in order.
fn schedule_all(
	tx: &rusqlite::Transaction<'_>,
	domain: &DomainId,
	group: Option<&GroupId>,
	plans: &[SubmissionPlan],
) -> Result<Vec<DurableTxId>, DbError> {
	plans.iter().map(|plan| dao::schedule(tx, domain, group, plan)).collect()
}

/// One plan of `policy` per params.
fn plans_of(policy: &PolicyId, params: Vec<PolicyParams>) -> Vec<SubmissionPlan> {
	let plan = |params| SubmissionPlan { policy: policy.clone(), params };
	params.into_iter().map(plan).collect()
}

impl DurableTxEngine {
	/// Records `request` in its own write and broadcasts it once committed.
	pub async fn execute(
		self: &Arc<Self>,
		request: DurableRequest,
	) -> Result<Vec<DurableTxId>, RegistrationError> {
		let (ids, ()) = self.execute_with(request, |_, _| Ok::<_, DbError>(())).await?;
		Ok(ids)
	}

	/// Records `request` and runs `on_register` with the new ids in the same
	/// write, so the domain's own rows commit or roll back with the ledger's.
	/// An error from `on_register` rolls everything back, broadcasts nothing
	/// and comes back as [`RegistrationError::Hook`]. The registration
	/// finishes even when the caller stops waiting, because committed rows are
	/// useless without their broadcast.
	pub async fn execute_with<T, E, F>(
		self: &Arc<Self>,
		request: DurableRequest<E>,
		on_register: F,
	) -> Result<(Vec<DurableTxId>, T), RegistrationError<E>>
	where
		F: FnOnce(&rusqlite::Transaction<'_>, &[DurableTxId]) -> Result<T, E> + Send + 'static,
		T: Send + 'static,
		E: Send + 'static,
	{
		let genesis = self.registered_chain(&request.domain)?;
		self.check_policies_registered(&request)?;
		if request.uses_submission_policy() {
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
		outcome.await.unwrap_or(Err(RegistrationError::Db(DbError::Closed)))
	}

	/// The genesis of the chain `domain` lives on, when it is registered.
	fn registered_chain<E>(&self, domain: &DomainId) -> Result<H256, RegistrationError<E>> {
		self.chain_of(domain)
			.ok_or_else(|| RegistrationError::UnknownDomain(domain.clone()))
	}

	/// Refuses a request naming a policy that is not registered, which
	/// nothing could ever build.
	fn check_policies_registered<E>(
		&self,
		request: &DurableRequest<E>,
	) -> Result<(), RegistrationError<E>> {
		let named = request.transactions.named_policies();
		let unknown = named.into_iter().find(|id| self.registry.submission_policy(id).is_none());
		match unknown {
			Some(policy) => Err(RegistrationError::UnknownPolicy(policy.clone())),
			None => Ok(()),
		}
	}

	/// Records the rows, then broadcasts what was committed.
	async fn register<T, E, F>(
		self: Arc<Self>,
		genesis: H256,
		request: DurableRequest<E>,
		on_register: F,
	) -> Result<(Vec<DurableTxId>, T), RegistrationError<E>>
	where
		F: FnOnce(&rusqlite::Transaction<'_>, &[DurableTxId]) -> Result<T, E> + Send + 'static,
		T: Send + 'static,
		E: Send + 'static,
	{
		let submissions = request.transactions.presigned().to_vec();
		let recorded = self.record(request, on_register).await?;
		let (ids, value, claims) = recorded.map_err(RegistrationError::Hook)?;
		for (claim, DurableSubmission { extrinsic, .. }) in claims.into_iter().zip(submissions) {
			spawn_watch(&self, Attempt::new(claim, genesis, &extrinsic), extrinsic.extrinsic);
		}
		Ok((ids, value))
	}

	/// Writes the rows and the hook's rows in one transaction, taking
	/// ownership of each attempt inside it so a recovery pass never reaches
	/// a committed row before its watch. Returns the claims, one per
	/// presigned row in order, or the hook's refusal. A rolled-back write
	/// gives the claims up again.
	async fn record<T, E, F>(
		&self,
		request: DurableRequest<E>,
		on_register: F,
	) -> Result<Result<(Vec<DurableTxId>, T, Vec<OwnershipClaim>), E>, DbError>
	where
		F: FnOnce(&rusqlite::Transaction<'_>, &[DurableTxId]) -> Result<T, E> + Send + 'static,
		T: Send + 'static,
		E: Send + 'static,
	{
		let taken = Arc::new(Mutex::new(Vec::new()));
		let (ownership, claims) = (self.ownership.clone(), taken.clone());
		let write_rows_and_hook = move |tx: &rusqlite::Transaction<'_>| {
			let presigned = request.transactions.presigned().len();
			let ids = match request.insert_all(tx)? {
				Ok(ids) => ids,
				Err(refusal) => return Ok(Err(refusal)),
			};
			claims.lock().extend(claim_presigned(&ownership, &ids, presigned));
			Ok(on_register(tx, &ids).map(|value| (ids, value)))
		};
		let written = self.db.write_unless_refused(write_rows_and_hook).await;
		let claims = core::mem::take(&mut *taken.lock());
		if !matches!(written, Ok(Ok(_))) {
			claims.iter().for_each(|claim| self.ownership.release(*claim));
		}
		written.map(|recorded| recorded.map(|(ids, value)| (ids, value, claims)))
	}
}

/// Claims the first `presigned` of the new rows, the presigned ones, in
/// order.
fn claim_presigned(
	ownership: &Ownership,
	ids: &[DurableTxId],
	presigned: usize,
) -> Vec<OwnershipClaim> {
	ids.iter().take(presigned).map(|id| ownership.acquire(*id)).collect()
}

#[cfg(test)]
mod tests;
