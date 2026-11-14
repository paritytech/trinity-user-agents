use std::sync::Mutex;

use futures::executor::block_on;

use super::*;
use crate::{
	chain::{DispatchOutcome, HashAndNumber, Heads},
	chain_runtime::RuntimeFailure,
	durable::{
		model::{DurableTxFailureKind, DurableTxStatus, HeadKind},
		oracle::{DurableRegistry, PassScope, Unobservable},
		testing::{
			FakeChain, ScriptedScope, ServesTestDomains, block, engine_with, extrinsic, insert,
		},
	},
	store::Db,
};

fn test_domain() -> DomainId {
	DomainId::new("test")
}

fn status(db: &Db, id: DurableTxId) -> DurableTxStatus {
	block_on(db.read(move |conn| dao::status(conn, id))).unwrap().unwrap()
}

/// An oracle that answers with a fixed scope and records the transactions
/// of every call.
struct ScriptedOracle<F> {
	scope: F,
	fails: bool,
	shown: Mutex<Vec<Vec<DurableTxId>>>,
}

impl<F> ScriptedOracle<F>
where
	F: Fn() -> Box<dyn PassScope> + Send + Sync,
{
	fn new(scope: F) -> Arc<Self> {
		Arc::new(Self { scope, fails: false, shown: Mutex::default() })
	}
}

fn says(
	scope: ScriptedScope,
) -> Arc<ScriptedOracle<impl Fn() -> Box<dyn PassScope> + Send + Sync>> {
	ScriptedOracle::new(move || Box::new(scope) as Box<dyn PassScope>)
}

#[async_trait::async_trait]
impl<F> CompletionOracle for ScriptedOracle<F>
where
	F: Fn() -> Box<dyn PassScope> + Send + Sync,
{
	async fn open_pass(
		&self,
		transactions: &[DurableTxEntry],
		_heads: &Heads,
	) -> Result<Box<dyn PassScope>, RuntimeFailure> {
		self.shown.lock().unwrap().push(transactions.iter().map(|tx| tx.id).collect());
		if self.fails {
			return Err(RuntimeFailure::host_failure("open_pass", "down"));
		}
		Ok((self.scope)())
	}
}

fn completed_at_finalized() -> ScriptedScope {
	ScriptedScope { completed_at_finalized: true, ..ScriptedScope::default() }
}

/// Recovery runs on every launch, and usually nothing is live; that must
/// cost no chain reads.
#[test]
fn a_settled_ledger_reads_no_chain() {
	let chain = FakeChain::new(150, 200);
	let registry = DurableRegistry::new().serving(test_domain(), says(completed_at_finalized()));
	let (_dir, engine, _timer) = engine_with(&chain, registry);

	block_on(engine.run_pass());

	assert_eq!(chain.state().heads_reads, 0);
}

#[test]
fn the_oracles_answer_is_written() {
	let chain = FakeChain::new(150, 200);
	let registry = DurableRegistry::new().serving(test_domain(), says(completed_at_finalized()));
	let (_dir, engine, _timer) = engine_with(&chain, registry);
	let id = insert(&engine.db, &test_domain(), extrinsic(1, 100, 64));

	block_on(engine.run_pass());

	assert_eq!(status(&engine.db, id), DurableTxStatus::FinalizedSuccess { at: None });
}

/// A watch is following the transaction right now and will decide it
/// faster; a pass deciding it too would race the watch's writes.
#[test]
fn a_transaction_a_watch_owns_gets_no_verdict() {
	let chain = FakeChain::new(150, 200);
	let registry = DurableRegistry::new().serving(test_domain(), says(completed_at_finalized()));
	let (_dir, engine, _timer) = engine_with(&chain, registry);
	let id = insert(&engine.db, &test_domain(), extrinsic(1, 100, 64));
	engine.ownership.acquire(id);

	block_on(engine.run_pass());

	assert_eq!(status(&engine.db, id), DurableTxStatus::Pending);
}

/// An app update removed a domain while its rows were live. Nothing can
/// decide them any more, but they must not be decided wrongly either.
#[test]
fn a_domain_without_an_oracle_is_left_alone() {
	let chain = FakeChain::new(150, 200);
	let (_dir, engine, _timer) = engine_with(&chain, DurableRegistry::new());
	let tx = extrinsic(1, 100, 64);
	chain.include(120, tx.extrinsic.hash(), DispatchOutcome::Succeeded);
	let id = insert(&engine.db, &DomainId::new("orphan"), tx);

	block_on(engine.run_pass());

	assert_eq!(status(&engine.db, id), DurableTxStatus::Pending);
	assert_eq!(chain.state().heads_reads, 0);
}

/// Proves completion at the finalized head for the listed transactions.
struct CompletedAtFinalized(Vec<DurableTxId>);

impl PassScope for CompletedAtFinalized {
	fn proven_completed(&self, tx: &DurableTxEntry, head: HeadKind) -> bool {
		head == HeadKind::Finalized && self.0.contains(&tx.id)
	}
}

/// Records are never deleted, so a long-lived domain holds many decided
/// rows. A pass shows the oracle only the undecided ones, and asks it once:
/// a second call at the same heads would give the same answers, even after
/// the first answers changed the ledger.
#[test]
fn a_pass_asks_the_oracle_once_about_the_undecided_rows() {
	const PROVEN: DurableTxId = DurableTxId(4);
	const UNPROVEN: DurableTxId = DurableTxId(5);
	let chain = FakeChain::new(150, 200);
	let oracle =
		ScriptedOracle::new(|| Box::new(CompletedAtFinalized(vec![PROVEN])) as Box<dyn PassScope>);
	let registry = DurableRegistry::new().serving(test_domain(), oracle.clone());
	let (_dir, engine, _timer) = engine_with(&chain, registry);
	for tag in 1..=3 {
		let id = insert(&engine.db, &test_domain(), extrinsic(tag, 100, 64));
		finalize(&engine.db, id);
	}
	assert_eq!(
		[
			insert(&engine.db, &test_domain(), extrinsic(4, 100, 64)),
			insert(&engine.db, &test_domain(), extrinsic(5, 100, 64))
		],
		[PROVEN, UNPROVEN]
	);

	block_on(engine.run_pass());

	assert_eq!(*oracle.shown.lock().unwrap(), vec![vec![PROVEN, UNPROVEN]]);
	assert_eq!(
		[status(&engine.db, PROVEN), status(&engine.db, UNPROVEN)],
		[DurableTxStatus::FinalizedSuccess { at: None }, DurableTxStatus::Pending]
	);
}

#[test]
fn two_domains_on_one_chain_read_its_heads_once() {
	let chain = FakeChain::new(150, 200);
	let registry = DurableRegistry::new()
		.serving(DomainId::new("a"), Arc::new(Unobservable))
		.serving(DomainId::new("b"), Arc::new(Unobservable));
	let (_dir, engine, _timer) = engine_with(&chain, registry);
	insert(&engine.db, &DomainId::new("a"), extrinsic(1, 100, 64));
	insert(&engine.db, &DomainId::new("b"), extrinsic(2, 100, 64));

	block_on(engine.run_pass());

	assert_eq!(chain.state().heads_reads, 1);
}

/// The node is unreachable when the pass starts: without heads nothing can
/// be decided, so nothing is written.
#[test]
fn a_chain_whose_heads_cannot_be_read_gets_no_verdict() {
	let chain = FakeChain::new(150, 200);
	chain.state().heads_unavailable = true;
	let registry = DurableRegistry::new().serving(test_domain(), says(completed_at_finalized()));
	let (_dir, engine, _timer) = engine_with(&chain, registry);
	let id = insert(&engine.db, &test_domain(), extrinsic(1, 100, 64));

	block_on(engine.run_pass());

	assert_eq!(status(&engine.db, id), DurableTxStatus::Pending);
}

/// The domain's own chain read fails mid-pass. Even a transaction the
/// search could find waits for the next pass instead of being decided on
/// half the evidence.
#[test]
fn an_oracle_that_fails_to_open_leaves_its_domain_untouched() {
	let chain = FakeChain::new(150, 200);
	let tx = extrinsic(1, 100, 64);
	chain.include(120, tx.extrinsic.hash(), DispatchOutcome::Succeeded);
	let oracle = Arc::new(ScriptedOracle {
		scope: || Box::new(ScriptedScope::default()) as Box<dyn PassScope>,
		fails: true,
		shown: Mutex::default(),
	});
	let (_dir, engine, _timer) =
		engine_with(&chain, DurableRegistry::new().serving(test_domain(), oracle));
	let id = insert(&engine.db, &test_domain(), tx);

	block_on(engine.run_pass());

	assert_eq!(status(&engine.db, id), DurableTxStatus::Pending);
}

/// The watch saw the transaction in a best block, which a reorg then
/// replaced. With nothing else proving it ran, the success is withdrawn.
#[test]
fn a_record_whose_block_was_reorged_out_is_demoted_and_cleared() {
	let chain = FakeChain::new(150, 200);
	let (_dir, engine, _timer) =
		engine_with(&chain, DurableRegistry::new().serving(test_domain(), Arc::new(Unobservable)));
	let id = insert(&engine.db, &test_domain(), extrinsic(1, 100, 64));
	record_success(&engine.db, id, block(160));
	chain.state().reorged.insert(160, H256::repeat_byte(0x99));

	block_on(engine.run_pass());

	let entry = block_on(engine.db.read(move |conn| dao::entry(conn, id))).unwrap().unwrap();
	assert_eq!(entry.status, DurableTxStatus::Pending);
}

/// The node cannot serve the hash at the recorded height, for example after
/// a restart. A transport failure must not withdraw a success.
#[test]
fn an_unreadable_record_height_keeps_the_success() {
	let chain = FakeChain::new(150, 200);
	let (_dir, engine, _timer) =
		engine_with(&chain, DurableRegistry::new().serving(test_domain(), Arc::new(Unobservable)));
	let id = insert(&engine.db, &test_domain(), extrinsic(1, 100, 64));
	record_success(&engine.db, id, block(160));
	chain.state().unreadable_heights.insert(160);

	block_on(engine.run_pass());

	assert_eq!(status(&engine.db, id), DurableTxStatus::PendingSuccess { at: block(160) });
}

/// A reorg replaced the branch with a shorter one that does not reach the
/// recorded block at all.
#[test]
fn a_chain_shorter_than_the_record_clears_it() {
	let chain = FakeChain::new(150, 155);
	let (_dir, engine, _timer) =
		engine_with(&chain, DurableRegistry::new().serving(test_domain(), Arc::new(Unobservable)));
	let id = insert(&engine.db, &test_domain(), extrinsic(1, 100, 64));
	record_success(&engine.db, id, block(160));

	block_on(engine.run_pass());

	assert_eq!(status(&engine.db, id), DurableTxStatus::Pending);
}

/// The search decides a transaction no oracle can see, end to end over
/// the chain's own blocks.
#[test]
fn a_transaction_found_in_a_finalized_block_finalizes() {
	let chain = FakeChain::new(150, 200);
	let tx = extrinsic(1, 100, 64);
	chain.include(120, tx.extrinsic.hash(), DispatchOutcome::Succeeded);
	let (_dir, engine, _timer) =
		engine_with(&chain, DurableRegistry::new().serving(test_domain(), Arc::new(Unobservable)));
	let id = insert(&engine.db, &test_domain(), tx);

	block_on(engine.run_pass());

	let entry = block_on(engine.db.read(move |conn| dao::entry(conn, id))).unwrap().unwrap();
	assert_eq!(entry.status, DurableTxStatus::FinalizedSuccess { at: Some(block(120)) });
}

/// Finality stalls while best blocks keep coming. The era has passed on the
/// best chain, but only finalized facts may fail a transaction.
#[test]
fn best_height_alone_never_expires_a_transaction() {
	let chain = FakeChain::new(150, 400);
	let (_dir, engine, _timer) =
		engine_with(&chain, DurableRegistry::new().serving(test_domain(), Arc::new(Unobservable)));
	let id = insert(&engine.db, &test_domain(), extrinsic(1, 100, 64));

	block_on(engine.run_pass());

	assert_eq!(status(&engine.db, id), DurableTxStatus::Pending);
}

#[test]
fn absence_over_a_closed_window_fails_the_transaction() {
	let chain = FakeChain::new(200, 210);
	let (_dir, engine, _timer) =
		engine_with(&chain, DurableRegistry::new().serving(test_domain(), Arc::new(Unobservable)));
	let id = insert(&engine.db, &test_domain(), extrinsic(1, 100, 64));

	block_on(engine.run_pass());

	assert_eq!(status(&engine.db, id), DurableTxStatus::Failure(DurableTxFailureKind::Expired));
}

/// Recovery runs on every finalized head while a transaction stays
/// pending. Blocks searched on an earlier pass are finalized and cannot
/// change, so the next pass reads only the ones finalized since.
#[test]
fn a_later_pass_reads_only_the_blocks_finalized_since() {
	let chain = FakeChain::new(120, 140);
	let (_dir, engine, _timer) =
		engine_with(&chain, DurableRegistry::new().serving(test_domain(), Arc::new(Unobservable)));
	insert(&engine.db, &test_domain(), extrinsic(1, 100, 64));
	block_on(engine.run_pass());
	chain.state().finalized = 123;
	let reads_before = chain.state().body_reads;

	block_on(engine.run_pass());

	assert_eq!(chain.state().body_reads - reads_before, 3);
}

fn finalize(db: &Db, id: DurableTxId) {
	block_on(db.write(move |tx| {
		let observed = dao::entry(tx, id)?.unwrap();
		dao::compare_and_set(tx, &observed, DurableTxStatus::FinalizedSuccess { at: None }, None)?;
		Ok(())
	}))
	.unwrap();
}

fn record_success(db: &Db, id: DurableTxId, at: HashAndNumber) {
	block_on(db.write(move |tx| {
		let observed = dao::entry(tx, id)?.unwrap();
		dao::compare_and_set(tx, &observed, DurableTxStatus::PendingSuccess { at }, None)?;
		Ok(())
	}))
	.unwrap();
}
