//! [`DeferredTxHarness`]: an engine whose transactions scripted policies
//! build, and what a test steers and reads it with.

use core::time::Duration;
use std::sync::Arc;

use futures::{channel::mpsc, executor::block_on};
use tempfile::TempDir;

use super::{FakeChain, GENESIS, ScriptedPolicy, block, insert_with_plan, open_db, test_plan};
use crate::{
	chain::{EncodedExtrinsic, HeadEvent, MortalExtrinsic},
	chain_runtime::RuntimeFailure,
	durable::{
		dao,
		engine::{DurableDeps, DurableRequest, DurableTxEngine},
		model::{DomainId, DurableTxEntry, DurableTxId, DurableTxStatus, SubmissionPlan},
		oracle::{DurableRegistry, Unobservable},
	},
	store::Db,
	test_support::wait_until,
	time::ManualTimer,
};

/// The domain a test defers into unless it needs a second one.
pub fn test_domain() -> DomainId {
	DomainId::new("test")
}

/// A second domain the harness serves.
pub fn other_domain() -> DomainId {
	DomainId::new("other")
}

/// An engine serving [`test_domain`] and [`other_domain`], which nothing on
/// chain decides, built by one scripted policy, with a [`ManualTimer`] and scripted
/// finalized heads.
pub struct DeferredTxHarness {
	_dir: TempDir,
	chain: Arc<FakeChain>,
	db: Db,
	engine: Arc<DurableTxEngine>,
	timer: Arc<ManualTimer>,
	heads: mpsc::UnboundedSender<Result<HeadEvent, RuntimeFailure>>,
	policy: Arc<ScriptedPolicy>,
}

impl DeferredTxHarness {
	/// A harness whose domains are both built by `policy`.
	pub fn with_policy(policy: ScriptedPolicy) -> Self {
		let chain = FakeChain::new(130, 140);
		let heads = chain.script_heads();
		let policy = Arc::new(policy);
		let registry = [test_domain(), other_domain()].into_iter().fold(
			DurableRegistry::new(),
			|registry, domain| {
				registry.with_domain(domain, GENESIS, Arc::new(Unobservable), policy.clone())
			},
		);
		let (dir, db) = open_db();
		let timer = Arc::new(ManualTimer::default());
		let engine = engine_over(&chain, db.clone(), registry, timer.clone());
		Self { _dir: dir, chain, db, engine, timer, heads, policy }
	}

	/// The engine's ledger.
	pub fn db(&self) -> &Db {
		&self.db
	}

	/// The engine under test.
	pub fn engine(&self) -> &Arc<DurableTxEngine> {
		&self.engine
	}

	/// The chain the engine broadcasts to.
	pub fn chain(&self) -> &Arc<FakeChain> {
		&self.chain
	}

	/// The engine's clock.
	pub fn timer(&self) -> &ManualTimer {
		&self.timer
	}

	/// The policy that builds both domains.
	pub fn policy(&self) -> &Arc<ScriptedPolicy> {
		&self.policy
	}

	/// Defers one transaction of the test domain, in no group.
	pub fn defer_one(&self) -> DurableTxId {
		self.defer_repeated(1)[0]
	}

	/// Defers `count` transactions of the test domain, in no group, in one
	/// write.
	pub fn defer_repeated(&self, count: usize) -> Vec<DurableTxId> {
		let plans = vec![test_plan(vec![]); count];
		let request = DurableRequest::deferred(test_domain(), None, plans).unwrap();
		self.defer(request)
	}

	/// Registers `request` and returns its ids.
	pub fn defer(&self, request: DurableRequest) -> Vec<DurableTxId> {
		block_on(self.engine.execute(request)).unwrap()
	}

	/// A test-domain row waiting to be built, written as a previous
	/// process would have left it: no executor hears of it.
	pub fn leave_waiting_from_previous_process(&self) -> DurableTxId {
		self.leave_waiting(test_domain(), test_plan(vec![]))
	}

	/// A row of `domain` waiting to be built through `plan`, written as a
	/// previous process would have left it.
	pub fn leave_waiting(&self, domain: DomainId, plan: SubmissionPlan) -> DurableTxId {
		let write = self.db.write(move |tx| dao::defer(tx, &domain, None, &plan));
		block_on(write).unwrap()
	}

	/// Records `extrinsic` as a pending test-domain row rebuilt through
	/// `plan`, without broadcasting or watching it.
	pub fn insert_unwatched(
		&self,
		extrinsic: MortalExtrinsic,
		plan: Option<SubmissionPlan>,
	) -> DurableTxId {
		insert_with_plan(&self.db, &test_domain(), extrinsic, plan)
	}

	/// The ledger row of `id`, which has an attempt.
	pub fn entry(&self, id: DurableTxId) -> DurableTxEntry {
		block_on(self.db.read(move |conn| dao::entry(conn, id))).unwrap().unwrap()
	}

	/// Where `id` stands.
	pub fn status(&self, id: DurableTxId) -> DurableTxStatus {
		block_on(self.engine.status(id)).unwrap().unwrap()
	}

	/// Waits until `id` reaches `status`.
	pub fn wait_for(&self, id: DurableTxId, status: DurableTxStatus) {
		wait_until(|| self.status(id) == status, "the transaction reaches its status");
	}

	/// Everything broadcast so far, in order.
	pub fn broadcast(&self) -> Vec<EncodedExtrinsic> {
		self.chain.state().submitted.clone()
	}

	/// Announces finalized block `number`.
	pub fn finalize(&self, number: u64) {
		self.heads.unbounded_send(Ok(HeadEvent::Finalized(block(number)))).unwrap();
	}

	/// Ends the chain's head stream, as a closed node connection does.
	pub fn end_heads(&mut self) {
		let (disconnected, _) = mpsc::unbounded();
		self.heads = disconnected;
	}

	/// Lets queued work run, so a test can check that something did not
	/// happen.
	pub fn settle(&self) {
		std::thread::sleep(Duration::from_millis(50));
	}

	/// Drops the engine, keeping its database directory and the policy so a
	/// test can check what outlives it.
	pub fn drop_engine(self) -> (TempDir, Arc<ScriptedPolicy>) {
		(self._dir, self.policy)
	}
}

/// An engine over `chain` and `db`, waiting on `timer`.
fn engine_over(
	chain: &Arc<FakeChain>,
	db: Db,
	registry: DurableRegistry,
	timer: Arc<ManualTimer>,
) -> Arc<DurableTxEngine> {
	DurableTxEngine::new(DurableDeps {
		db,
		registry,
		heads: chain.clone(),
		blocks: chain.clone(),
		validator: chain.clone(),
		submitter: chain.clone(),
		timer,
		spawner: crate::Spawner::for_test(),
	})
}
