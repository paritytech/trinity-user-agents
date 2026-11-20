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
		model::{
			DomainId, DurableTxId, GroupId, PolicyParams, SubmissionPlan, TransactionDependency,
		},
		oracle::DurableRegistry,
		ownership::Ownership,
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
#[derive(Clone)]
pub struct DurableSubmission {
	/// Signed and ready to broadcast.
	pub extrinsic: MortalExtrinsic,
	/// How it is rebuilt, if it is.
	pub plan: Option<SubmissionPlan>,
}

/// One transaction left for its domain's policy to build, once every
/// transaction it depends on has released it.
pub struct DeferredSubmission {
	/// What the policy builds it from.
	pub params: PolicyParams,
	/// What it waits for. Empty lets it be built at once.
	pub depends_on: Vec<TransactionDependency>,
}

impl DeferredSubmission {
	/// A submission built from `params` that waits for nothing.
	pub fn independent(params: PolicyParams) -> Self {
		Self { params, depends_on: Vec::new() }
	}
}

/// Works out, inside the registration write, each transaction a request
/// defers, or refuses the registration.
type PlanInWrite<E> =
	Box<dyn FnOnce(&rusqlite::Transaction<'_>) -> Result<Vec<DeferredSubmission>, E> + Send>;

/// The transactions of a request, all of one kind.
enum RequestedTransactions<E> {
	/// Signed already: recorded as pending and broadcast once committed.
	Presigned(Vec<DurableSubmission>),
	/// Built later, each from its params.
	Deferred(Vec<DeferredSubmission>),
	/// Built later, worked out in the write.
	PlannedInWrite(PlanInWrite<E>),
}

/// Where the dependencies of a request may live: on the request's own
/// chain, in a domain the engine serves.
struct DependencyScope {
	registry: Arc<DurableRegistry>,
	genesis: H256,
}

/// A request named no transaction.
#[derive(Debug, Clone, thiserror::Error)]
#[error("a durable request needs at least one transaction, got none")]
pub struct EmptyRequest;

/// Why a registration was not recorded. Nothing was written or broadcast.
/// `E` is what the caller's hook refuses with.
#[derive(Debug, thiserror::Error)]
pub enum RegistrationError<E = DbError> {
	/// The domain is not registered, so nothing could decide or build its
	/// transactions.
	#[error("no durable domain {}: register it first", .0.as_str())]
	UnknownDomain(DomainId),
	/// The write failed.
	#[error(transparent)]
	Db(#[from] DbError),
	/// The caller's hook refused the registration.
	#[error("the registering domain refused: {0}")]
	Hook(E),
	/// A dependency names no registered transaction.
	#[error("dependency {0:?} names no registered durable transaction")]
	UnknownDependency(DurableTxId),
	/// A dependency lives on another chain than its dependent, where nothing
	/// orders the two.
	#[error(
		"dependency {dependency:?} lives on chain {chain:?}, expected its dependent's chain {expected:?}"
	)]
	DependencyOnOtherChain {
		/// The dependency.
		dependency: DurableTxId,
		/// Genesis hash of the chain it lives on.
		chain: H256,
		/// Genesis hash of the dependent's chain.
		expected: H256,
	},
	/// A dependency belongs to a domain the engine does not serve, so it
	/// could never be decided.
	#[error("dependency {dependency:?} belongs to durable domain {}, which is not registered", .domain.as_str())]
	DependencyInUnservedDomain {
		/// The dependency.
		dependency: DurableTxId,
		/// Its domain.
		domain: DomainId,
	},
}

impl<E> RequestedTransactions<E> {
	/// Whether it names no transaction. A plan worked out in the write is
	/// counted only there.
	fn is_empty(&self) -> bool {
		match self {
			Self::Presigned(submissions) => submissions.is_empty(),
			Self::Deferred(plans) => plans.is_empty(),
			Self::PlannedInWrite(_) => false,
		}
	}

	/// The presigned submissions, in order. Empty for a deferred request.
	fn presigned(&self) -> &[DurableSubmission] {
		match self {
			Self::Presigned(submissions) => submissions,
			Self::Deferred(_) | Self::PlannedInWrite(_) => &[],
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

	/// One transaction per submission, recorded as waiting and built from
	/// its params once committed and once its dependencies released it.
	pub fn deferred(
		domain: DomainId,
		group: Option<GroupId>,
		submissions: Vec<DeferredSubmission>,
	) -> Result<Self, EmptyRequest> {
		Self::new(domain, group, RequestedTransactions::Deferred(submissions))
	}

	/// One transaction per submission `plan` works out inside the
	/// registration write, each recorded and built as [`Self::deferred`]
	/// does. An `Err` from `plan` refuses the registration as
	/// [`RegistrationError::Hook`].
	pub fn deferred_in(
		domain: DomainId,
		group: Option<GroupId>,
		plan: impl FnOnce(&rusqlite::Transaction<'_>) -> Result<Vec<DeferredSubmission>, E>
		+ Send
		+ 'static,
	) -> Self {
		Self { domain, group, transactions: RequestedTransactions::PlannedInWrite(Box::new(plan)) }
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

	/// Records every transaction as a row, in order, or returns why the
	/// registration is refused: the plan's refusal, a dependency `scope`
	/// rules out, or a failed write.
	fn insert_all(
		self,
		tx: &rusqlite::Transaction<'_>,
		scope: &DependencyScope,
	) -> Result<Vec<DurableTxId>, RegistrationError<E>> {
		let Self { domain, group, transactions } = self;
		let submissions = match transactions {
			RequestedTransactions::Presigned(submissions) => {
				return Ok(insert_presigned(tx, &domain, group.as_ref(), &submissions)?);
			},
			RequestedTransactions::Deferred(submissions) => submissions,
			RequestedTransactions::PlannedInWrite(plan) => {
				plan(tx).map_err(RegistrationError::Hook)?
			},
		};
		scope.check_dependencies(tx, &submissions)?;
		Ok(defer_all(tx, &domain, group.as_ref(), submissions)?)
	}
}

impl DependencyScope {
	/// Refuses the first dependency of `submissions` that names no row, or a
	/// row on another chain or in a domain the engine does not serve.
	fn check_dependencies<E>(
		&self,
		tx: &rusqlite::Transaction<'_>,
		submissions: &[DeferredSubmission],
	) -> Result<(), RegistrationError<E>> {
		submissions
			.iter()
			.flat_map(|submission| &submission.depends_on)
			.try_for_each(|dependency| self.check_dependency(tx, dependency.id))
	}

	/// Refuses dependency `id` when it names no row, or a row on another
	/// chain or in a domain the engine does not serve.
	fn check_dependency<E>(
		&self,
		tx: &rusqlite::Transaction<'_>,
		id: DurableTxId,
	) -> Result<(), RegistrationError<E>> {
		let Some(domain) = dao::registered_domain(tx, id)? else {
			return Err(RegistrationError::UnknownDependency(id));
		};
		match self.registry.chain(&domain) {
			None => Err(RegistrationError::DependencyInUnservedDomain { dependency: id, domain }),
			Some(chain) if chain != self.genesis => {
				Err(RegistrationError::DependencyOnOtherChain {
					dependency: id,
					chain,
					expected: self.genesis,
				})
			},
			Some(_) => Ok(()),
		}
	}
}

/// Records each presigned submission as a pending row, in order.
fn insert_presigned(
	tx: &rusqlite::Transaction<'_>,
	domain: &DomainId,
	group: Option<&GroupId>,
	submissions: &[DurableSubmission],
) -> Result<Vec<DurableTxId>, DbError> {
	submissions
		.iter()
		.map(|submission| {
			dao::insert(tx, domain, group, &submission.extrinsic, submission.plan.as_ref())
		})
		.collect()
}

/// Records each submission as a waiting row with its dependencies, in
/// order.
fn defer_all(
	tx: &rusqlite::Transaction<'_>,
	domain: &DomainId,
	group: Option<&GroupId>,
	submissions: Vec<DeferredSubmission>,
) -> Result<Vec<DurableTxId>, DbError> {
	submissions
		.into_iter()
		.map(|submission| defer_one(tx, domain, group, submission))
		.collect()
}

/// Records `submission` as a waiting row that waits for each of its
/// dependencies.
fn defer_one(
	tx: &rusqlite::Transaction<'_>,
	domain: &DomainId,
	group: Option<&GroupId>,
	submission: DeferredSubmission,
) -> Result<DurableTxId, DbError> {
	let DeferredSubmission { params, depends_on } = submission;
	let id = dao::defer(tx, domain, group, &SubmissionPlan { params })?;
	depends_on
		.iter()
		.try_for_each(|dependency| dao::insert_dependency(tx, id, dependency))?;
	Ok(id)
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
		// Detached from the caller: once queued, the write commits on the
		// database thread even if this future is dropped (a cancelled FFI
		// call, a select), and this task holds the only copy of the signed
		// bytes that must then be broadcast.
		let (done, outcome) = oneshot::channel();
		let engine = self.clone();
		self.spawner.spawn(async move {
			let _ = done.send(engine.register(genesis, request, on_register).await);
		});
		outcome.await.unwrap_or(Err(RegistrationError::Db(DbError::Closed)))
	}

	/// The genesis of the chain `domain` lives on, when it is registered.
	fn registered_chain<E>(&self, domain: &DomainId) -> Result<H256, RegistrationError<E>> {
		self.chain_of(domain)
			.ok_or_else(|| RegistrationError::UnknownDomain(domain.clone()))
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
		let scope = DependencyScope { registry: self.registry.clone(), genesis };
		let (ids, value, claims) = self.record(request, scope, on_register).await??;
		for (id, DurableSubmission { extrinsic, .. }) in claims.into_iter().zip(submissions) {
			spawn_watch(&self, Attempt::new(id, genesis, &extrinsic), extrinsic.extrinsic);
		}
		Ok((ids, value))
	}

	/// Writes the rows and the hook's rows in one transaction, taking
	/// ownership of each attempt inside it so a recovery pass never reaches
	/// a committed row before its watch. Returns the claimed ids, the
	/// presigned rows in order, or why the registration was refused: a
	/// dependency `scope` rules out, or the plan's or the hook's refusal. A
	/// rolled-back write gives the claims up again.
	async fn record<T, E, F>(
		&self,
		request: DurableRequest<E>,
		scope: DependencyScope,
		on_register: F,
	) -> Result<Result<(Vec<DurableTxId>, T, Vec<DurableTxId>), RegistrationError<E>>, DbError>
	where
		F: FnOnce(&rusqlite::Transaction<'_>, &[DurableTxId]) -> Result<T, E> + Send + 'static,
		T: Send + 'static,
		E: Send + 'static,
	{
		let taken = Arc::new(Mutex::new(Vec::new()));
		let (ownership, claims) = (self.ownership.clone(), taken.clone());
		let write_rows_and_hook = move |tx: &rusqlite::Transaction<'_>| {
			let presigned = request.transactions.presigned().len();
			let ids = match request.insert_all(tx, &scope) {
				Ok(ids) => ids,
				Err(refusal) => return Ok(Err(refusal)),
			};
			claims.lock().extend(claim_presigned(&ownership, &ids, presigned));
			let hooked = on_register(tx, &ids).map_err(RegistrationError::Hook);
			Ok(hooked.map(|value| (ids, value)))
		};
		let written = self.db.write_unless_refused(write_rows_and_hook).await;
		let claims = core::mem::take(&mut *taken.lock());
		if !matches!(written, Ok(Ok(_))) {
			claims.iter().for_each(|id| self.ownership.release(*id));
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
) -> Vec<DurableTxId> {
	let claimed: Vec<DurableTxId> = ids.iter().take(presigned).copied().collect();
	claimed.iter().for_each(|id| ownership.acquire(*id));
	claimed
}

#[cfg(test)]
mod tests;
