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
		model::{DomainId, DurableTxId, GroupId, PolicyId, SubmissionPlan},
		ownership::{Ownership, OwnershipClaim},
	},
	store::DbError,
};

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
		Ok(Self { domain, group, transactions })
	}

	/// Every plan in the request, to build or to rebuild.
	fn named_plans(&self) -> impl Iterator<Item = &SubmissionPlan> {
		let presigned = self.transactions.presigned().iter();
		let rebuilds = presigned.filter_map(|submission| submission.plan.as_ref());
		rebuilds.chain(self.transactions.scheduled())
	}

	/// Whether anything in the request is built or rebuilt by a policy.
	fn uses_submission_policy(&self) -> bool {
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
			RequestedTransactions::Scheduled(plans) => {
				plans.iter().map(|plan| dao::schedule(tx, &self.domain, group, plan)).collect()
			},
		}
	}
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
		request: DurableRequest,
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
		request: &DurableRequest,
	) -> Result<(), RegistrationError<E>> {
		let unknown = request
			.named_plans()
			.find(|plan| self.registry.submission_policy(&plan.policy).is_none());
		match unknown {
			Some(plan) => Err(RegistrationError::UnknownPolicy(plan.policy.clone())),
			None => Ok(()),
		}
	}

	/// Records the rows, then broadcasts what was committed.
	async fn register<T, E, F>(
		self: Arc<Self>,
		genesis: H256,
		request: DurableRequest,
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
		request: DurableRequest,
		on_register: F,
	) -> Result<Result<(Vec<DurableTxId>, T, Vec<OwnershipClaim>), E>, DbError>
	where
		F: FnOnce(&rusqlite::Transaction<'_>, &[DurableTxId]) -> Result<T, E> + Send + 'static,
		T: Send + 'static,
		E: Send + 'static,
	{
		let taken = Arc::new(Mutex::new(Vec::new()));
		let (ownership, claims) = (self.ownership.clone(), taken.clone());
		let written = self
			.db
			.write_unless_refused(move |tx| {
				let ids = request.insert_all(tx)?;
				claims.lock().extend(claim_presigned(&ownership, &ids, &request));
				Ok(on_register(tx, &ids).map(|value| (ids, value)))
			})
			.await;
		let claims = core::mem::take(&mut *taken.lock());
		if !matches!(written, Ok(Ok(_))) {
			claims.iter().for_each(|claim| self.ownership.release(*claim));
		}
		written.map(|recorded| recorded.map(|(ids, value)| (ids, value, claims)))
	}
}

/// Claims each new presigned row of `request`, in order.
fn claim_presigned(
	ownership: &Ownership,
	ids: &[DurableTxId],
	request: &DurableRequest,
) -> Vec<OwnershipClaim> {
	let presigned = request.transactions.presigned().len();
	ids.iter().take(presigned).map(|id| ownership.acquire(*id)).collect()
}

#[cfg(test)]
mod tests {
	use futures::{FutureExt, executor::block_on};

	use super::*;
	use crate::{
		durable::{
			model::{DurableTxState, DurableTxStatus, PolicyParams},
			testing::{
				FakeChain, ScheduledTxHarness, ScriptedPolicy, extrinsic, pending_presigned,
				test_engine, test_plan, without_plan,
			},
		},
		test_support::wait_until,
	};

	/// What a test domain refuses a registration with.
	#[derive(Debug, thiserror::Error)]
	enum DomainRefusal {
		#[error("an item is already spent")]
		ItemSpent,
		#[error(transparent)]
		Db(#[from] DbError),
	}

	impl From<rusqlite::Error> for DomainRefusal {
		fn from(error: rusqlite::Error) -> Self {
			Self::Db(error.into())
		}
	}

	fn domain() -> DomainId {
		DomainId::new("test")
	}

	fn request(extrinsics: Vec<MortalExtrinsic>) -> DurableRequest {
		DurableRequest::presigned(domain(), Some(GroupId::new("op")), without_plan(extrinsics))
			.unwrap()
	}

	#[test]
	fn a_request_with_no_extrinsic_cannot_be_built() {
		assert!(matches!(DurableRequest::presigned(domain(), None, vec![]), Err(EmptyRequest)));
	}

	#[test]
	fn registered_ids_come_back_in_order_and_are_owned_by_their_watch() {
		let chain = FakeChain::new(130, 140);
		let (_dir, engine, _timer) = test_engine(&chain);

		let ids =
			block_on(engine.execute(request(vec![extrinsic(1, 100, 64), extrinsic(2, 100, 64)])))
				.unwrap();

		assert!(ids[0] < ids[1]);
		assert_eq!(
			block_on(engine.group(&domain(), &GroupId::new("op"))).unwrap(),
			vec![pending_presigned(ids[0], 1), pending_presigned(ids[1], 2)]
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

		wait_until(|| chain.state().submitted.len() == 1, "the committed extrinsic is submitted");
	}

	/// A recovery pass may run the moment the rows commit. Ownership is taken
	/// inside the write, so the pass never evaluates a transaction its watch is
	/// about to decide.
	#[test]
	fn a_registered_transaction_is_owned_before_its_rows_commit() {
		let chain = FakeChain::new(130, 140);
		let (_dir, engine, _timer) = test_engine(&chain);
		let probe = engine.clone();

		let (_, owned_in_write) =
			block_on(engine.execute_with(request(vec![extrinsic(1, 100, 64)]), move |_, ids| {
				Ok::<_, DbError>(probe.ownership.is_owned(ids[0]))
			}))
			.unwrap();

		assert!(owned_in_write);
	}

	/// A domain locks the items a transaction spends in the same write as the
	/// ledger row, so neither can exist without the other.
	#[test]
	fn the_hook_writes_its_rows_in_the_same_transaction_with_the_new_ids() {
		let chain = FakeChain::new(130, 140);
		let (_dir, engine, _timer) = test_engine(&chain);

		let (ids, hooked) =
			block_on(engine.execute_with(request(vec![extrinsic(1, 100, 64)]), |tx, ids| {
				tx.execute_batch("CREATE TABLE locks (tx_id INTEGER NOT NULL)")?;
				tx.execute("INSERT INTO locks (tx_id) VALUES (?1)", [ids[0].0])?;
				Ok::<_, DbError>(ids.to_vec())
			}))
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
			|tx, _| -> Result<(), DomainRefusal> {
				tx.execute_batch("CREATE TABLE locks (tx_id INTEGER NOT NULL)")?;
				Err(DomainRefusal::ItemSpent)
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
		assert!(matches!(result, Err(RegistrationError::Hook(DomainRefusal::ItemSpent))));
		assert_eq!(block_on(engine.group(&domain(), &GroupId::new("op"))).unwrap(), vec![]);
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
		let refused =
			block_on(engine.execute_with(request(vec![extrinsic(1, 100, 64)]), |_, _| {
				Err::<(), _>(DbError::Connection("domain refused".into()))
			}));
		assert!(refused.is_err());

		block_on(engine.execute(request(vec![extrinsic(1, 100, 64)]))).unwrap();

		wait_until(|| chain.state().submitted.len() == 1, "the registered extrinsic is submitted");
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

	/// Schedules `plans` in the `op` group of the test domain.
	fn scheduled_in_op(plans: Vec<SubmissionPlan>) -> DurableRequest {
		DurableRequest::scheduled(domain(), Some(GroupId::new("op")), plans).unwrap()
	}

	fn op_group(engine: &DurableTxEngine) -> Vec<DurableTxState> {
		block_on(engine.group(&domain(), &GroupId::new("op"))).unwrap()
	}

	#[test]
	fn an_empty_schedule_is_refused() {
		assert!(matches!(DurableRequest::scheduled(domain(), None, vec![]), Err(EmptyRequest)));
	}

	/// Nothing is signed yet, so there is nothing to broadcast or watch: the
	/// row waits for its policy.
	#[test]
	fn a_scheduled_transaction_waits_for_its_policy_and_broadcasts_nothing() {
		let harness = ScheduledTxHarness::with_policy(ScriptedPolicy::never_answering());
		let engine = harness.engine();

		let ids = harness.schedule(scheduled_in_op(vec![test_plan(vec![1]), test_plan(vec![1])]));

		let waiting = |id: &DurableTxId| DurableTxState {
			id: *id,
			status: DurableTxStatus::PendingSubmission,
			tx_hash: None,
			plan: Some(test_plan(vec![1])),
		};
		assert_eq!(op_group(engine), ids.iter().map(waiting).collect::<Vec<_>>());
		assert_eq!(harness.broadcast(), vec![]);
		assert!(ids.iter().all(|id| !engine.ownership.is_owned(*id)));
	}

	/// A presigned transaction that names a policy is rebuilt by it after a
	/// failure, so the row must remember which policy that is.
	#[test]
	fn a_presigned_transaction_keeps_the_policy_that_rebuilds_it() {
		let harness = ScheduledTxHarness::with_policy(ScriptedPolicy::never_answering());
		let submission =
			DurableSubmission { extrinsic: extrinsic(1, 100, 64), plan: Some(test_plan(vec![1])) };
		let request = DurableRequest::presigned(domain(), None, vec![submission]).unwrap();

		let id = harness.schedule(request)[0];

		let plan = block_on(harness.engine().db.read(move |conn| dao::submission_plan(conn, id)));
		assert_eq!(plan.unwrap(), Some(test_plan(vec![1])));
		let presigned_sent = || harness.broadcast() == vec![extrinsic(1, 100, 64).extrinsic];
		wait_until(presigned_sent, "the presigned extrinsic is broadcast");
	}

	/// A policy missing from the registry is a wiring bug: refused at the
	/// call site instead of surfacing as a failed transaction later.
	#[test]
	fn a_request_naming_an_unregistered_policy_writes_nothing() {
		let harness = ScheduledTxHarness::with_policy(ScriptedPolicy::never_answering());
		let engine = harness.engine();
		let unknown =
			SubmissionPlan { policy: PolicyId::new("unknown"), params: PolicyParams::default() };

		let result = block_on(engine.execute(scheduled_in_op(vec![test_plan(vec![1]), unknown])));

		assert!(matches!(
			result,
			Err(RegistrationError::UnknownPolicy(policy)) if policy == PolicyId::new("unknown")
		));
		assert_eq!(op_group(engine), vec![]);
	}

	/// The domain locks what the scheduled transaction will spend in the same
	/// write. When that fails, the schedule must roll back with it, or a row
	/// would be built that spends nothing locked.
	#[test]
	fn a_schedule_rolls_back_when_locking_what_it_spends_fails() {
		let harness = ScheduledTxHarness::with_policy(ScriptedPolicy::never_answering());
		let engine = harness.engine();
		let request = scheduled_in_op(vec![test_plan(vec![1])]);

		let result = block_on(engine.execute_with(request, |_, _| {
			Err::<(), _>(DomainRefusal::ItemSpent)
		}));

		assert!(matches!(result, Err(RegistrationError::Hook(DomainRefusal::ItemSpent))));
		assert_eq!(op_group(engine), vec![]);
	}
}
