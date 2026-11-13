use core::time::Duration;
use std::{
	collections::HashMap,
	sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};

use futures::executor::block_on;

use super::*;
use crate::{
	chain::{HeadEvent, Heads},
	chain_runtime::RuntimeFailure,
	durable::{
		model::{DomainId, DurableTxEntry, DurableTxFailureKind, DurableTxStatus, HeadKind},
		oracle::{CompletionOracle, DurableRegistry, LedgerView, PassScope},
		policy::Preparation,
		testing::{
			FakeChain, GENESIS, ScheduledTxHarness, ScriptedPolicy, answer_all_with, block,
			engine_with, extrinsic, insert,
		},
	},
	test_support::wait_until,
};

fn test_domain() -> DomainId {
	DomainId::new("test")
}

/// Counts the passes that reached the domain, and proves its
/// transactions completed from the `complete_from`th one on.
struct CountingOracle {
	chain: H256,
	opened: AtomicUsize,
	complete_from: usize,
}

impl CountingOracle {
	fn new(chain: H256, complete_from: usize) -> Arc<Self> {
		Arc::new(Self { chain, opened: AtomicUsize::new(0), complete_from })
	}
}

#[async_trait::async_trait]
impl CompletionOracle for CountingOracle {
	fn chain(&self) -> H256 {
		self.chain
	}

	async fn open_pass(
		&self,
		_transactions: &[DurableTxEntry],
		_ledger: &LedgerView,
		_heads: &Heads,
	) -> Result<Box<dyn PassScope>, RuntimeFailure> {
		let opened = self.opened.fetch_add(1, Ordering::SeqCst) + 1;
		Ok(Box::new(Completes(opened >= self.complete_from)))
	}
}

struct Completes(bool);

impl PassScope for Completes {
	fn proven_completed(&self, _tx: &DurableTxEntry, head: HeadKind) -> bool {
		self.0 && head == HeadKind::Finalized
	}
}

struct Fixture {
	_dir: tempfile::TempDir,
	engine: Arc<DurableTxEngine>,
	oracle: Arc<CountingOracle>,
	heads: mpsc::UnboundedSender<Result<HeadEvent, RuntimeFailure>>,
}

fn fixture(complete_from: usize) -> Fixture {
	let chain = FakeChain::new(130, 140);
	let heads = chain.script_heads();
	let oracle = CountingOracle::new(GENESIS, complete_from);
	let (dir, engine, _timer) =
		engine_with(&chain, DurableRegistry::new().with_domain(test_domain(), oracle.clone()));
	insert(&engine.db, &test_domain(), extrinsic(1, 100, 64));
	Fixture { _dir: dir, engine, oracle, heads }
}

fn run_in_background(
	engine: &Arc<DurableTxEngine>,
) -> std::thread::JoinHandle<Result<(), RecoveryError>> {
	let engine = engine.clone();
	std::thread::spawn(move || block_on(engine.run_until_settled()))
}

fn passes(fixture: &Fixture) -> usize {
	fixture.oracle.opened.load(Ordering::SeqCst)
}

/// The app was killed after its last transaction was finalized but before
/// its row was updated; the launch pass settles it and recovery stops
/// without waiting for a head.
#[test]
fn a_ledger_settled_by_the_first_pass_costs_one_pass() {
	let fixture = fixture(1);
	fixture.heads.unbounded_send(Ok(HeadEvent::Finalized(block(131)))).unwrap();

	block_on(fixture.engine.run_until_settled()).unwrap();

	assert_eq!(passes(&fixture), 1);
}

#[test]
fn one_pass_per_head_until_nothing_is_live() {
	let fixture = fixture(3);
	let running = run_in_background(&fixture.engine);
	wait_until(|| passes(&fixture) == 1, "the first pass runs at once");

	fixture.heads.unbounded_send(Ok(HeadEvent::Finalized(block(131)))).unwrap();
	wait_until(|| passes(&fixture) == 2, "a finalized head runs a pass");
	fixture.heads.unbounded_send(Ok(HeadEvent::Finalized(block(132)))).unwrap();

	assert_eq!(running.join().unwrap(), Ok(()));
	assert_eq!(passes(&fixture), 3);
}

/// Pre-finality success is read at the best head, so a new best block can
/// decide a transaction several blocks before finality does.
#[test]
fn a_best_head_drives_a_pass() {
	let fixture = fixture(2);
	let running = run_in_background(&fixture.engine);
	wait_until(|| passes(&fixture) == 1, "the first pass runs at once");

	fixture.heads.unbounded_send(Ok(HeadEvent::Best(block(141)))).unwrap();

	assert_eq!(running.join().unwrap(), Ok(()));
	assert_eq!(passes(&fixture), 2);
}

/// A released watch hands its transaction back without waiting a block.
#[test]
fn a_nudge_drives_a_pass() {
	let fixture = fixture(2);
	let running = run_in_background(&fixture.engine);
	wait_until(|| passes(&fixture) == 1, "the first pass runs at once");

	fixture.engine.recovery_wakes.wake();

	assert_eq!(running.join().unwrap(), Ok(()));
	assert_eq!(passes(&fixture), 2);
}

/// An error item is one missed head; the subscription goes on.
#[test]
fn a_failed_head_read_is_skipped() {
	let fixture = fixture(2);
	let running = run_in_background(&fixture.engine);
	wait_until(|| passes(&fixture) == 1, "the first pass runs at once");

	fixture
		.heads
		.unbounded_send(Err(RuntimeFailure::host_failure("head", "glitch")))
		.unwrap();
	fixture.heads.unbounded_send(Ok(HeadEvent::Finalized(block(131)))).unwrap();

	assert_eq!(running.join().unwrap(), Ok(()));
	assert_eq!(passes(&fixture), 2);
}

/// The app went to background and the node connection closed. Without heads
/// nothing would drive the next pass, so the loop fails and the host
/// retries it.
#[test]
fn a_head_subscription_that_ends_fails_the_loop() {
	let fixture = fixture(usize::MAX);
	let running = run_in_background(&fixture.engine);
	wait_until(|| passes(&fixture) == 1, "the first pass runs at once");

	drop(fixture.heads);

	assert!(matches!(
		running.join().unwrap(),
		Err(RecoveryError::HeadsLost { genesis, .. }) if genesis == GENESIS
	));
}

/// Sorts before [`GENESIS`], so the fake chain hands it the first
/// scripted head stream.
const OTHER_GENESIS: H256 = H256([0x01; 32]);

/// One chain's node connection closed while a transaction on another
/// chain was still waiting for its verdict. The healthy chain keeps
/// recovery running until it settles; the host then retries for the
/// lost one.
#[test]
fn a_lost_chain_does_not_stop_recovery_on_another_chain() {
	let chain = FakeChain::new(130, 140);
	let _other_heads = chain.script_heads();
	let lost_heads = chain.script_heads();
	let other_oracle = CountingOracle::new(OTHER_GENESIS, 2);
	let registry = DurableRegistry::new()
		.with_domain(DomainId::new("lost"), CountingOracle::new(GENESIS, usize::MAX))
		.with_domain(DomainId::new("other"), other_oracle.clone());
	let (_dir, engine, _timer) = engine_with(&chain, registry);
	insert(&engine.db, &DomainId::new("lost"), extrinsic(1, 100, 64));
	let other = insert(&engine.db, &DomainId::new("other"), extrinsic(2, 100, 64));
	let running = run_in_background(&engine);
	wait_until(|| other_oracle.opened.load(Ordering::SeqCst) == 1, "the first pass runs at once");

	drop(lost_heads);

	assert!(matches!(
		running.join().unwrap(),
		Err(RecoveryError::HeadsLost { genesis, .. }) if genesis == GENESIS
	));
	assert_eq!(
		block_on(engine.status(other)).unwrap(),
		Some(DurableTxStatus::FinalizedSuccess { at: None })
	);
}

/// A chain whose node connection closed has nothing waiting on it, so
/// recovery has nothing to retry there.
#[test]
fn a_lost_chain_with_nothing_live_does_not_fail_recovery() {
	let chain = FakeChain::new(130, 140);
	let _other_heads = chain.script_heads();
	let lost_heads = chain.script_heads();
	let other_oracle = CountingOracle::new(OTHER_GENESIS, 2);
	let registry = DurableRegistry::new()
		.with_domain(DomainId::new("lost"), CountingOracle::new(GENESIS, usize::MAX))
		.with_domain(DomainId::new("other"), other_oracle.clone());
	let (_dir, engine, _timer) = engine_with(&chain, registry);
	insert(&engine.db, &DomainId::new("other"), extrinsic(2, 100, 64));
	let running = run_in_background(&engine);
	wait_until(|| other_oracle.opened.load(Ordering::SeqCst) == 1, "the first pass runs at once");

	drop(lost_heads);

	assert_eq!(running.join().unwrap(), Ok(()));
}

#[test]
fn a_head_subscription_that_cannot_open_fails_the_loop() {
	let chain = FakeChain::new(130, 140);
	let (_dir, engine, _timer) = engine_with(
		&chain,
		DurableRegistry::new()
			.with_domain(test_domain(), Arc::new(crate::durable::oracle::Unobservable(GENESIS))),
	);
	insert(&engine.db, &test_domain(), extrinsic(1, 100, 64));

	assert!(matches!(block_on(engine.run_until_settled()), Err(RecoveryError::HeadsLost { .. })));
}

/// Abandoning live transactions because the ledger could not be read once
/// is far worse than one more pass.
#[test]
fn an_unreadable_ledger_counts_as_live() {
	let fixture = fixture(1);
	block_on(fixture.engine.db.close()).unwrap();

	assert_eq!(
		block_on(fixture.engine.after_pass(&LostChains::default())),
		ControlFlow::Continue(())
	);
}

/// Nothing could decide a domain without an oracle, so its rows must not
/// keep recovery running.
#[test]
fn live_rows_no_oracle_can_decide_do_not_keep_recovery_running() {
	let chain = FakeChain::new(130, 140);
	chain.script_heads();
	let (_dir, engine, _timer) = engine_with(
		&chain,
		DurableRegistry::new()
			.with_domain(test_domain(), Arc::new(crate::durable::oracle::Unobservable(GENESIS))),
	);
	insert(&engine.db, &DomainId::new("orphan"), extrinsic(1, 100, 64));

	let running = run_in_background(&engine);

	wait_until(|| running.is_finished(), "recovery returns");
	assert_eq!(running.join().unwrap(), Ok(()));
}

/// Nothing builds a row of an unregistered domain, so waiting for it would
/// keep recovery running forever.
#[test]
fn rows_of_an_unregistered_domain_waiting_to_be_built_do_not_keep_recovery_running() {
	let chain = FakeChain::new(130, 140);
	chain.script_heads();
	let (_dir, engine, _timer) = engine_with(
		&chain,
		DurableRegistry::new()
			.with_domain(test_domain(), Arc::new(crate::durable::oracle::Unobservable(GENESIS))),
	);
	let orphan = DomainId::new("orphan");
	let plan = crate::durable::testing::test_plan(vec![1]);
	block_on(engine.db.write(move |tx| dao::schedule(tx, &orphan, None, &plan))).unwrap();

	let running = run_in_background(&engine);

	wait_until(|| running.is_finished(), "recovery returns");
	assert_eq!(running.join().unwrap(), Ok(()));
}

/// The device is offline at launch with nothing live. Recovery must finish
/// rather than fail on a head subscription it never needed, or the host
/// would retry it forever.
#[test]
fn a_settled_ledger_needs_no_head_subscription() {
	let chain = FakeChain::new(130, 140);
	let (_dir, engine, _timer) = engine_with(
		&chain,
		DurableRegistry::new()
			.with_domain(test_domain(), Arc::new(crate::durable::oracle::Unobservable(GENESIS))),
	);

	assert_eq!(block_on(engine.run_until_settled()), Ok(()));
}

/// A policy with nothing to say about any row until `give_up` is set,
/// and giving up on every row from then on.
fn giving_up_once_set(give_up: Arc<AtomicBool>) -> ScriptedPolicy {
	ScriptedPolicy::answering_with(move |ids| match give_up.load(Ordering::SeqCst) {
		true => answer_all_with(ids, Preparation::GiveUp),
		false => Ok(HashMap::new()),
	})
}

/// The app was killed before a scheduled transaction was built. Recovery
/// is all that runs on the next launch, so it must build it.
#[test]
fn recovery_builds_what_a_previous_process_left_waiting() {
	let harness = ScheduledTxHarness::with_policy(ScriptedPolicy::answering_nothing());
	let id = harness.leave_waiting_from_previous_process();

	let _running = run_in_background(harness.engine());

	wait_until(|| harness.policy().batches() == vec![vec![id]], "recovery builds the row");
}

/// Nothing on chain can decide a transaction that was never submitted,
/// but it is still work: the host must keep recovery running until it is
/// built or given up. Once it is, recovery ends without waiting for a
/// head, which may never come once the node connection dropped.
#[test]
fn recovery_runs_until_waiting_transactions_are_decided() {
	let give_up = Arc::new(AtomicBool::new(false));
	let mut harness = ScheduledTxHarness::with_policy(giving_up_once_set(give_up.clone()));
	let id = harness.leave_waiting_from_previous_process();
	let running = run_in_background(harness.engine());
	wait_until(|| harness.policy().batches().len() == 1, "the row is asked about");

	harness.finalize(131);
	harness.end_heads();
	harness.settle();
	assert!(!running.is_finished());
	give_up.store(true, Ordering::SeqCst);
	wait_until(|| harness.timer().sleeps() == 1, "the bucket backs off");
	harness.timer().advance(Duration::from_secs(2));
	let abandoned = DurableTxStatus::Failure(DurableTxFailureKind::Abandoned);
	harness.wait_for(id, abandoned);

	wait_until(|| running.is_finished(), "recovery returns");
	assert_eq!(running.join().unwrap(), Ok(()));
}

/// A transaction a previous run broadcast is decided only by passes its
/// chain's heads drive. Once those heads are lost, the host must hear of
/// it to resubscribe, however long another row waits to be built.
#[test]
fn a_lost_chain_fails_recovery_while_a_row_waits_to_be_built() {
	let mut harness = ScheduledTxHarness::with_policy(ScriptedPolicy::answering_nothing());
	harness.insert_unwatched(extrinsic(1, 100, 64), None);
	harness.leave_waiting_from_previous_process();
	let running = run_in_background(harness.engine());
	wait_until(|| harness.policy().batches().len() == 1, "the row is asked about");

	harness.end_heads();

	wait_until(|| running.is_finished(), "recovery returns");
	assert!(matches!(
		running.join().unwrap(),
		Err(RecoveryError::HeadsLost { genesis, .. }) if genesis == GENESIS
	));
}
