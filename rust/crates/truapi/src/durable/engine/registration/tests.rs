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

fn request<E>(extrinsics: Vec<MortalExtrinsic>) -> DurableRequest<E> {
	DurableRequest::presigned(domain(), Some(GroupId::new("op")), without_plan(extrinsics)).unwrap()
}

#[test]
fn a_request_with_no_extrinsic_cannot_be_built() {
	assert!(matches!(
		DurableRequest::<DbError>::presigned(domain(), None, vec![]),
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
	let refused = block_on(engine.execute_with(request(vec![extrinsic(1, 100, 64)]), |_, _| {
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
fn scheduled_in_op<E>(plans: Vec<SubmissionPlan>) -> DurableRequest<E> {
	DurableRequest::scheduled(domain(), Some(GroupId::new("op")), plans).unwrap()
}

fn op_group(engine: &DurableTxEngine) -> Vec<DurableTxState> {
	block_on(engine.group(&domain(), &GroupId::new("op"))).unwrap()
}

#[test]
fn an_empty_schedule_is_refused() {
	assert!(matches!(
		DurableRequest::<DbError>::scheduled(domain(), None, vec![]),
		Err(EmptyRequest)
	));
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

	let result =
		block_on(engine.execute_with(request, |_, _| Err::<(), _>(DomainRefusal::ItemSpent)));

	assert!(matches!(result, Err(RegistrationError::Hook(DomainRefusal::ItemSpent))));
	assert_eq!(op_group(engine), vec![]);
}

fn planned_in_op<E: Send + 'static>(
	plan: impl FnOnce(&rusqlite::Transaction<'_>) -> Result<Vec<PolicyParams>, E> + Send + 'static,
) -> DurableRequest<E> {
	let group = Some(GroupId::new("op"));
	DurableRequest::scheduled_in(domain(), group, test_plan(vec![]).policy, plan)
}

/// A domain that works its params out from rows it reads in the same
/// write gets one waiting row per params, built by the policy it named.
#[test]
fn a_schedule_planned_in_the_write_records_one_waiting_row_per_params() {
	let harness = ScheduledTxHarness::with_policy(ScriptedPolicy::never_answering());
	let engine = harness.engine();
	let request =
		planned_in_op(|_| Ok::<_, DbError>(vec![PolicyParams(vec![7]), PolicyParams(vec![8])]));

	let (ids, ()) = block_on(engine.execute_with(request, |_, _| Ok::<_, DbError>(()))).unwrap();

	let waiting = |(id, params): (&DurableTxId, u8)| DurableTxState {
		id: *id,
		status: DurableTxStatus::PendingSubmission,
		tx_hash: None,
		plan: Some(test_plan(vec![params])),
	};
	let expected: Vec<_> = ids.iter().zip([7, 8]).map(waiting).collect();
	assert_eq!(op_group(engine), expected);
}

/// The domain finds what it would spend already spent while planning, so
/// nothing may be recorded and the hook never runs.
#[test]
fn a_plan_that_refuses_records_nothing() {
	let harness = ScheduledTxHarness::with_policy(ScriptedPolicy::never_answering());
	let engine = harness.engine();
	let request = planned_in_op(|_| Err(DomainRefusal::ItemSpent));

	let result = block_on(engine.execute_with(request, |_, _| -> Result<(), DomainRefusal> {
		panic!("the hook runs only after the rows are recorded")
	}));

	assert!(matches!(result, Err(RegistrationError::Hook(DomainRefusal::ItemSpent))));
	assert_eq!(op_group(engine), vec![]);
}

/// A planned schedule names its policy up front, so an unknown one is
/// refused before the write and the plan never runs.
#[test]
fn a_planned_schedule_naming_an_unknown_policy_is_refused_before_planning() {
	let harness = ScheduledTxHarness::with_policy(ScriptedPolicy::never_answering());
	let engine = harness.engine();
	let group = Some(GroupId::new("op"));
	let request = DurableRequest::scheduled_in(domain(), group, PolicyId::new("unknown"), |_| {
		panic!("an unknown policy is refused before planning")
	});

	let result = block_on(engine.execute_with(request, |_, _| Ok::<_, DbError>(())));

	assert!(matches!(result, Err(RegistrationError::UnknownPolicy(_))));
}
