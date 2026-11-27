//! Fixtures shared by the PGAS claim tests: a builder that claims the first
//! slot it is not told to skip, and an engine whose claims it builds.

use std::{collections::BTreeSet, sync::Arc};

use futures::channel::mpsc::UnboundedSender;
use parking_lot::Mutex;
use subxt::utils::AccountId32;
use tempfile::TempDir;

use crate::{
	chain::{DispatchOutcome, WatchEvent},
	durable::{
		DurableRegistry, DurableTxEngine, DurableTxId, DurableTxStatus, FakeChain, GENESIS,
		Unobservable, block, block_hash, engine_with, extrinsic,
	},
	pgas_claims::{
		PgasClaims,
		builder::{ClaimBuildError, PgasClaimBuilder},
		claim_extrinsic::{BuiltClaim, ClaimSlot},
		pgas_domain,
		policy::PgasClaimPolicy,
	},
	test_support::wait_until,
};

/// The day every fake claim is built for.
pub const TEST_DAY: u32 = 20_412;

/// A build failure the policy asks about again later.
pub fn transient() -> ClaimBuildError {
	ClaimBuildError::Transient(anyhow::anyhow!("the ring revision has not arrived"))
}

/// A destination with a distinct byte.
pub fn destination(byte: u8) -> AccountId32 {
	AccountId32([byte; 32])
}

/// One call of [`FakeClaimBuilder`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuildCall {
	/// The account the claim mints into.
	pub target: AccountId32,
	/// The slots it was told to skip.
	pub excluded: BTreeSet<ClaimSlot>,
}

/// Builds each claim in the first slot of [`TEST_DAY`] that it is not told
/// to skip, as a real scan would on an empty day, or fails every build.
#[derive(Default)]
pub struct FakeClaimBuilder {
	failure: Option<fn() -> ClaimBuildError>,
	calls: Mutex<Vec<BuildCall>>,
}

impl FakeClaimBuilder {
	/// A builder whose every build succeeds.
	pub fn building() -> Arc<Self> {
		Arc::default()
	}

	/// A builder whose every build fails with `failure`.
	pub fn failing_with(failure: fn() -> ClaimBuildError) -> Arc<Self> {
		Arc::new(Self { failure: Some(failure), calls: Mutex::default() })
	}

	/// Every build asked for, in order.
	pub fn calls(&self) -> Vec<BuildCall> {
		self.calls.lock().clone()
	}
}

#[async_trait::async_trait]
impl PgasClaimBuilder for FakeClaimBuilder {
	async fn build_claim(
		&self,
		target: AccountId32,
		excluded: &BTreeSet<ClaimSlot>,
	) -> Result<BuiltClaim, ClaimBuildError> {
		let excluded = excluded.clone();
		self.calls
			.lock()
			.push(BuildCall { target, excluded: excluded.clone() });
		if let Some(failure) = self.failure {
			return Err(failure());
		}
		Ok(claim_in_first_free_slot(&excluded))
	}
}

/// A claim in the first slot of [`TEST_DAY`] that `excluded` does not list,
/// with an extrinsic distinct to that slot.
fn claim_in_first_free_slot(excluded: &BTreeSet<ClaimSlot>) -> BuiltClaim {
	let slot = (0..)
		.map(|slot_index| ClaimSlot { day: TEST_DAY, slot_index })
		.find(|slot| !excluded.contains(slot))
		.expect("a test day has a free slot");
	let tag = u8::try_from(slot.slot_index).expect("a test slot fits in u8");
	BuiltClaim { extrinsic: extrinsic(tag, 130, 64), slot }
}

/// An engine serving the PGAS domain over a [`FakeChain`], whose claims the
/// real policy builds through a [`FakeClaimBuilder`] and nothing on chain
/// decides.
pub struct ClaimsHarness {
	_dir: TempDir,
	chain: Arc<FakeChain>,
	engine: Arc<DurableTxEngine>,
	claims: PgasClaims,
}

impl ClaimsHarness {
	/// A harness whose claims `builder` builds.
	pub fn new(builder: Arc<FakeClaimBuilder>) -> Self {
		let chain = FakeChain::new(130, 140);
		let policy = Arc::new(PgasClaimPolicy::new(builder));
		let registry = DurableRegistry::new().with_domain(
			pgas_domain(),
			GENESIS,
			Arc::new(Unobservable),
			policy,
		);
		let (dir, engine, _timer) = engine_with(&chain, registry);
		let claims = PgasClaims::new(engine.clone());
		Self { _dir: dir, chain, engine, claims }
	}

	/// The claims under test.
	pub fn claims(&self) -> &PgasClaims {
		&self.claims
	}

	/// The engine the claims are registered with.
	pub fn engine(&self) -> &Arc<DurableTxEngine> {
		&self.engine
	}

	/// Registers a claim into `dest`, which must be accepted.
	pub fn claim_into(&self, dest: AccountId32) -> DurableTxId {
		futures::executor::block_on(self.claims.claim(dest)).unwrap()
	}

	/// Where `id` stands.
	pub fn status(&self, id: DurableTxId) -> DurableTxStatus {
		futures::executor::block_on(self.engine.status(id)).unwrap().unwrap()
	}

	/// Waits until `id` reaches `status`.
	pub fn wait_for(&self, id: DurableTxId, status: DurableTxStatus) {
		wait_until(|| self.status(id) == status, "the claim reaches its status");
	}

	/// Registers a claim into `dest` and takes it to a best block. The
	/// builder must build.
	pub fn claim_seen_in_best_block(&self, dest: AccountId32) -> DurableTxId {
		let events = self.script_watch();
		let id = self.claim_into(dest);
		self.take_to_best_block(id, &events);
		id
	}

	/// The events the next submitted claim reports, sent by the test.
	pub fn script_watch(&self) -> UnboundedSender<WatchEvent> {
		self.chain.script_watch()
	}

	/// Takes claim `id`, whose attempt reports on `events`, to a best block
	/// once it is submitted.
	pub fn take_to_best_block(&self, id: DurableTxId, events: &UnboundedSender<WatchEvent>) {
		self.wait_for(id, DurableTxStatus::Pending);
		let submitted =
			self.chain.state().submitted.last().expect("the claim was submitted").hash();
		self.chain.include(135, submitted, DispatchOutcome::Succeeded);
		events.unbounded_send(WatchEvent::InBestBlock(block_hash(135))).unwrap();
		self.wait_for(id, DurableTxStatus::PendingSuccess { at: block(135) });
	}
}
