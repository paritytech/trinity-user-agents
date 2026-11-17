use futures::{channel::mpsc, executor::block_on};

use super::*;
use crate::{
	chain::{DispatchOutcome, MortalExtrinsic},
	chain_runtime::RuntimeFailure,
	durable::{
		engine::DurableRequest,
		model::{DomainId, DurableTxEntry, DurableTxFailureKind, DurableTxStatus},
	},
	time::ManualTimer,
};
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::{
	Spawner,
	durable::{
		oracle::{DurableRegistry, Unobservable},
		testing::{
			FakeChain, GENESIS, block, block_hash, engine_with_spawner, extrinsic, without_plan,
		},
	},
	test_support::wait_until,
};
use subxt::tx::TransactionInvalid;

fn test_domain() -> DomainId {
	DomainId::new("test")
}

struct Fixture {
	_dir: tempfile::TempDir,
	chain: Arc<FakeChain>,
	engine: Arc<DurableTxEngine>,
	timer: Arc<ManualTimer>,
	nudges: mpsc::Receiver<()>,
	finished: Arc<AtomicUsize>,
	id: DurableTxId,
	tx: MortalExtrinsic,
}

impl Fixture {
	fn entry(&self) -> DurableTxEntry {
		let id = self.id;
		block_on(self.engine.db.read(move |conn| dao::entry(conn, id)))
			.unwrap()
			.unwrap()
	}

	fn wait_for(&self, status: DurableTxStatus) {
		wait_until(|| self.entry().status == status, "the watch writes its verdict");
	}

	/// Waits for the registration task and the watch it started to
	/// finish, the watch's release included.
	fn wait_released(&self) {
		wait_until(|| self.finished.load(Ordering::SeqCst) == 2, "the watch ends");
		assert!(!self.engine.ownership.is_owned(self.id));
	}

	fn nudged(&mut self) -> bool {
		self.nudges.try_recv().is_ok()
	}
}

/// Registers one transaction whose watch reports what `prepare` scripts.
fn register(
	prepare: impl FnOnce(&FakeChain) -> Option<mpsc::UnboundedSender<WatchEvent>>,
) -> (Fixture, Option<mpsc::UnboundedSender<WatchEvent>>) {
	let chain = FakeChain::new(130, 140);
	let events = prepare(&chain);
	let (spawner, finished) = Spawner::counting();
	let (dir, engine, timer) = engine_with_spawner(
		&chain,
		DurableRegistry::new().with_domain(test_domain(), Arc::new(Unobservable(GENESIS))),
		spawner,
	);
	let nudges = engine.recovery_wakes.subscribe();
	let tx = extrinsic(1, 100, 64);
	let id = block_on(engine.execute(
		DurableRequest::presigned(test_domain(), None, without_plan(vec![tx.clone()])).unwrap(),
	))
	.unwrap()[0];
	(Fixture { _dir: dir, chain, engine, timer, nudges, finished, id, tx }, events)
}

fn watched() -> (Fixture, mpsc::UnboundedSender<WatchEvent>) {
	let (fixture, events) = register(|chain| Some(chain.script_watch()));
	(fixture, events.unwrap())
}

#[test]
fn inclusion_in_a_best_block_with_success_records_that_block() {
	let (fixture, events) = watched();
	fixture
		.chain
		.include(135, fixture.tx.extrinsic.hash(), DispatchOutcome::Succeeded);

	events.unbounded_send(WatchEvent::InBestBlock(block_hash(135))).unwrap();

	fixture.wait_for(DurableTxStatus::PendingSuccess { at: block(135) });
}

/// Not finalized, so a failed dispatch there proposes nothing.
#[test]
fn a_failed_dispatch_in_a_best_block_proposes_nothing() {
	let (mut fixture, events) = watched();
	fixture
		.chain
		.include(135, fixture.tx.extrinsic.hash(), DispatchOutcome::Failed(String::new()));

	events.unbounded_send(WatchEvent::InBestBlock(block_hash(135))).unwrap();
	drop(events);

	fixture.wait_released();
	assert_eq!(fixture.entry().status, DurableTxStatus::Pending);
	assert!(fixture.nudged());
}

/// The watch decided the transaction itself, so starting recovery would
/// only re-derive an answer that exists.
#[test]
fn finality_with_success_finalizes_and_asks_for_no_recovery() {
	let (mut fixture, events) = watched();
	fixture
		.chain
		.include(125, fixture.tx.extrinsic.hash(), DispatchOutcome::Succeeded);

	events.unbounded_send(WatchEvent::InFinalizedBlock(block_hash(125))).unwrap();

	fixture.wait_for(DurableTxStatus::FinalizedSuccess { at: Some(block(125)) });
	fixture.wait_released();
	assert!(!fixture.nudged());
}

#[test]
fn finality_with_a_failed_dispatch_fails() {
	let (fixture, events) = watched();
	fixture
		.chain
		.include(125, fixture.tx.extrinsic.hash(), DispatchOutcome::Failed(String::new()));

	events.unbounded_send(WatchEvent::InFinalizedBlock(block_hash(125))).unwrap();

	fixture.wait_for(DurableTxStatus::Failure(DurableTxFailureKind::DispatchFailed));
}

/// The node rejects the extrinsic during validation, for example because
/// its nonce is stale. Nothing can ever include these bytes, so the
/// transaction fails without waiting for its era.
#[test]
fn a_refusal_before_submission_fails_at_once_and_sends_nothing() {
	let (mut fixture, _) = register(|chain| {
		chain.state().validation = Ok(ValidationResult::Invalid(TransactionInvalid::Stale));
		None
	});

	fixture.wait_for(DurableTxStatus::Failure(DurableTxFailureKind::Rejected));
	fixture.wait_released();
	assert_eq!(fixture.chain.state().submitted.len(), 0);
	assert!(!fixture.nudged());
}

/// The pool drops the extrinsic. It may still have been included, so the
/// watch decides nothing and recovery takes over.
#[test]
fn a_dropped_transaction_is_handed_to_recovery() {
	let (mut fixture, events) = watched();

	events.unbounded_send(WatchEvent::Dropped("pool full".into())).unwrap();

	fixture.wait_released();
	assert_eq!(fixture.entry().status, DurableTxStatus::Pending);
	assert!(fixture.nudged());
}

/// The block is announced but its events cannot be read yet. Inclusion is
/// not success, so nothing is recorded.
#[test]
fn inclusion_whose_outcome_cannot_be_read_records_nothing() {
	let (fixture, events) = watched();
	fixture
		.chain
		.include(135, fixture.tx.extrinsic.hash(), DispatchOutcome::Succeeded);
	fixture.chain.state().unreadable_outcomes.insert(block_hash(135));

	events.unbounded_send(WatchEvent::InBestBlock(block_hash(135))).unwrap();
	drop(events);

	fixture.wait_released();
	assert_eq!(fixture.entry().status, DurableTxStatus::Pending);
}

/// A reorg retracts the best block the success was recorded at; a success
/// resting on a block that is gone is no evidence.
#[test]
fn a_retraction_of_the_recorded_block_clears_the_record() {
	let (fixture, events) = watched();
	fixture
		.chain
		.include(135, fixture.tx.extrinsic.hash(), DispatchOutcome::Succeeded);
	events.unbounded_send(WatchEvent::InBestBlock(block_hash(135))).unwrap();
	fixture.wait_for(DurableTxStatus::PendingSuccess { at: block(135) });

	events.unbounded_send(WatchEvent::NoLongerInBestBlock).unwrap();

	fixture.wait_for(DurableTxStatus::Pending);
}

/// The extrinsic moves to a block whose outcome cannot be read, and that
/// block is retracted. The earlier recorded success is not what was
/// retracted.
#[test]
fn a_retraction_of_a_block_that_was_not_recorded_leaves_the_record() {
	let (fixture, events) = watched();
	fixture
		.chain
		.include(135, fixture.tx.extrinsic.hash(), DispatchOutcome::Succeeded);
	events.unbounded_send(WatchEvent::InBestBlock(block_hash(135))).unwrap();
	fixture.wait_for(DurableTxStatus::PendingSuccess { at: block(135) });
	fixture.chain.state().unreadable_outcomes.insert(block_hash(136));

	events.unbounded_send(WatchEvent::InBestBlock(block_hash(136))).unwrap();
	events.unbounded_send(WatchEvent::NoLongerInBestBlock).unwrap();
	drop(events);

	fixture.wait_released();
	assert_eq!(fixture.entry().status, DurableTxStatus::PendingSuccess { at: block(135) });
}

/// The connection closes mid-watch. The transaction stays pending with its
/// locks and recovery decides it.
#[test]
fn a_watch_that_ends_without_a_verdict_is_handed_to_recovery() {
	let (mut fixture, events) = watched();

	drop(events);

	fixture.wait_released();
	assert_eq!(fixture.entry().status, DurableTxStatus::Pending);
	assert!(fixture.nudged());
}

#[test]
fn a_submission_that_cannot_open_is_handed_to_recovery() {
	let (mut fixture, _) = register(|chain| {
		chain.state().submit_fails = true;
		None
	});

	fixture.wait_released();
	assert_eq!(fixture.entry().status, DurableTxStatus::Pending);
	assert!(fixture.nudged());
}

/// The bytes exist only in memory, so a validation call that fails must
/// not stop them being submitted: the node decides when it receives them.
#[test]
fn a_validation_the_node_cannot_answer_still_submits() {
	let (fixture, _) = register(|chain| {
		chain.state().validation = Err(RuntimeFailure::host_failure("validate", "down"));
		None
	});

	wait_until(|| fixture.chain.state().submitted.len() == 1, "the extrinsic is submitted");
}

/// The node stops reporting without closing the subscription. Recovery
/// takes over long before the era could end.
#[test]
fn a_silent_watch_releases_after_the_timeout() {
	let (mut fixture, _events) = watched();
	wait_until(|| fixture.timer.sleeps() > 0, "the watch waits for an event");
	assert!(fixture.engine.ownership.is_owned(fixture.id));

	fixture.timer.advance(SILENCE_TIMEOUT);

	fixture.wait_released();
	assert!(fixture.nudged());
}
