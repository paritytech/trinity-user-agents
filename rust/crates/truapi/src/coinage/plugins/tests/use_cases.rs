//! What `prepare` builds for top-ups, claims and chat sends, as the
//! products' use cases expect it.

use super::*;
use crate::{
	coinage::{
		CoinSecret, Exponent,
		ledger::{BlockRef, CoinOnChain, Facts},
		testing::{funding::{funding, register_records}, set_status},
	},
	installation::InstallationId,
};

/// What `prepare` answered for one record.
#[derive(Debug, PartialEq, Eq)]
enum Answer {
	Built,
	GivenUp,
}

struct Setup {
	wallet: TestWallet,
	engine: Arc<crate::durable::DurableTxEngine>,
	chain: Arc<FakeChain>,
	running: Running,
}

fn setup() -> Setup {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let engine = unbuilt_engine(wallet.db.clone());
	let running = Running::new();
	running.open(RunningState { params: chain_params(), check_failed: false, funding: HashMap::new() });
	Setup { wallet, engine, chain: Arc::new(FakeChain::default()), running }
}

fn key(id: u8) -> OpKey {
	OpKey { scope: b"product".to_vec(), id: [id; 32] }
}

impl Setup {
	fn allocate(&self, kind: AssetKind, exponent: u8) -> Asset {
		let outputs = [Exponent::new(exponent).unwrap()];
		block_on(self.wallet.db.write(move |tx| ledger::allocate(tx, PurseId::MAIN, kind, &outputs))).unwrap()[0]
	}

	/// A top-up of one entry of `exponent` under `operation`, paid by `from`.
	fn load(&self, operation: &OpKey, from: [u8; 32], exponent: u8, retry: Retry) -> (DurableTxId, RecordParams) {
		let entry = self.allocate(AssetKind::Entry, exponent);
		let params = RecordParams { call: Call::Load(from), retry };
		let rows = Rows { inputs: Vec::new(), outputs: vec![entry], tokens: Vec::new() };
		(register_records(&self.engine, operation, Admission::Ordinary, vec![(rows, params)]).unwrap()[0], params)
	}

	/// A claim of the peer coin `secret` controls, under `operation`, with
	/// its secret kept as an import keeps it.
	fn claim(&self, operation: &OpKey, secret: CoinSecret, retry: Retry) -> (DurableTxId, RecordParams) {
		let account = crate::coinage::planning::account_of(&secret).unwrap();
		block_on(self.wallet.db.write(move |tx| ledger::keep_secrets(tx, &[(account, secret)]))).unwrap();
		let output = self.allocate(AssetKind::Coin, 2);
		let params = RecordParams { call: Call::Transfer, retry };
		let rows = Rows { inputs: vec![Asset::PeerCoin(account)], outputs: vec![output], tokens: Vec::new() };
		(register_records(&self.engine, operation, Admission::Import, vec![(rows, params)]).unwrap()[0], params)
	}

	/// F1's read of the peer coin `secret` controls at block `number`.
	fn observe_peer(&self, secret: &CoinSecret, number: u32, present: bool) {
		let account = crate::coinage::planning::account_of(secret).unwrap();
		let now = present.then(|| CoinOnChain { exponent: Exponent::new(2).unwrap(), age: 0, locked_until: None });
		let reads = vec![(Subject::Asset(Asset::PeerCoin(account)), ledger::Read::Coin { now, emptied_by: None })];
		let facts = Facts { at: BlockRef { number, hash: [number as u8; 32] }, reads };
		block_on(self.wallet.db.write(move |tx| ledger::observe(tx, &facts, NOW, 255))).unwrap();
	}

	fn fund(&self, operation: &OpKey, funding: Arc<dyn Funding>) {
		self.running.update(|state| {
			state.funding.insert(operation.clone(), funding);
		});
	}

	fn prepare(&self, records: &[(DurableTxId, RecordParams)]) -> HashMap<DurableTxId, Answer> {
		self.prepare_at(records, NOW)
	}

	fn prepare_at(&self, records: &[(DurableTxId, RecordParams)], now: u64) -> HashMap<DurableTxId, Answer> {
		let mut deps = deps(&self.wallet, self.chain.clone(), self.running.clone());
		deps.clock = Arc::new(move || now);
		let batch: Vec<DeferredDurableTx> = records
			.iter()
			.map(|(record, params)| DeferredDurableTx {
				id: *record,
				domain: coinage_domain(),
				group: None,
				plan: SubmissionPlan { params: params.to_params() },
				attempts: 0,
			})
			.collect();
		let answers = block_on(CoinagePolicy(Arc::new(deps)).prepare(&batch)).unwrap();
		answers
			.into_iter()
			.map(|(record, answer)| match answer {
				Preparation::Ready { .. } => (record, Answer::Built),
				Preparation::GiveUp => (record, Answer::GivenUp),
			})
			.collect()
	}
}

/// The secret of a peer's coin.
fn peer_coin(index: u32) -> CoinSecret {
	coin_key(&[5; 16], PurseId::MAIN, InstallationId([1; 32]), index).unwrap().0
}

/// A load of 8 cents at the fixture's unit of 10 000, keeping the minimum
/// balance of 1.
const LOAD_COST: u128 = 8 * 10_000 + 1;

/// A load the funding account cannot pay would fail on chain; it waits
/// for the funds instead, and is built once they land.
#[test]
fn a_load_waits_while_its_funding_account_cannot_cover_it_and_is_built_once_it_can() {
	let setup = setup();
	let paying = funding(1);
	setup.fund(&key(1), paying.clone());
	let load = setup.load(&key(1), paying.account(), 3, Retry::Until(NOW + 3600));
	assert_eq!(setup.prepare(&[load]), HashMap::from([]), "an empty account pays nothing");
	setup.chain.funds.lock().insert(paying.account(), LOAD_COST - 1);
	assert_eq!(setup.prepare(&[load]), HashMap::from([]), "nor one that would fall below its minimum balance");
	setup.chain.funds.lock().insert(paying.account(), LOAD_COST);
	assert_eq!(setup.prepare(&[load]), HashMap::from([(load.0, Answer::Built)]));
}

/// Two loads of one account read one nonce unless the second waits for the
/// first to be stable (E2 invariant 4).
#[test]
fn the_loads_of_one_funding_account_are_built_one_at_a_time_in_registration_order() {
	let setup = setup();
	let paying = funding(1);
	setup.chain.funds.lock().insert(paying.account(), 2 * LOAD_COST);
	setup.fund(&key(1), paying.clone());
	setup.fund(&key(2), paying.clone());
	let first = setup.load(&key(1), paying.account(), 3, Retry::Until(NOW + 3600));
	let second = setup.load(&key(2), paying.account(), 3, Retry::Until(NOW + 3600));
	assert_eq!(setup.prepare(&[second, first]), HashMap::from([(first.0, Answer::Built)]));
	set_status(&setup.wallet.db, first.0, "PENDING_SUCCESS", None);
	assert_eq!(setup.prepare(&[second]), HashMap::from([]), "a success the best chain may still retract");
	set_status(&setup.wallet.db, first.0, "FINALIZED_SUCCESS", None);
	assert_eq!(setup.prepare(&[second]), HashMap::from([(second.0, Answer::Built)]));
}

#[test]
fn a_load_whose_window_has_closed_is_given_up_even_before_its_first_attempt() {
	let setup = setup();
	let paying = funding(1);
	setup.chain.funds.lock().insert(paying.account(), LOAD_COST);
	setup.fund(&key(1), paying.clone());
	let load = setup.load(&key(1), paying.account(), 3, Retry::Until(NOW - 1));
	assert_eq!(setup.prepare(&[load]), HashMap::from([(load.0, Answer::GivenUp)]));
}

/// Nothing builds before the start sequence opens `Running`; a window that
/// closed in the meantime is still enforced once it does.
#[test]
fn a_load_whose_window_closed_before_the_layer_ran_is_given_up_once_it_runs() {
	let setup = setup();
	let paying = funding(1);
	let closed = Running::new();
	let setup = Setup { running: closed.clone(), ..setup };
	let load = setup.load(&key(1), paying.account(), 3, Retry::Until(NOW + 10));
	assert_eq!(setup.prepare(&[load]), HashMap::from([]), "nothing is built before the layer starts");
	closed.open(RunningState { params: chain_params(), check_failed: false, funding: HashMap::new() });
	setup.fund(&key(1), paying);
	assert_eq!(setup.prepare_at(&[load], NOW + 11), HashMap::from([(load.0, Answer::GivenUp)]));
}

/// The funding capability does not survive a restart (G1 invariant 6).
#[test]
fn a_load_whose_funding_is_gone_is_given_up() {
	let setup = setup();
	let load = setup.load(&key(1), funding(1).account(), 3, Retry::Until(NOW + 3600));
	assert_eq!(setup.prepare(&[load]), HashMap::from([(load.0, Answer::GivenUp)]));
}

#[test]
fn a_failed_load_is_built_again_until_its_deadline_whatever_the_failure() {
	let load = RecordParams { call: Call::Load([1; 32]), retry: Retry::Until(NOW + 3600) };
	let failures = [VerdictFailureKind::Rejected, VerdictFailureKind::Expired, VerdictFailureKind::DispatchFailed];
	assert_eq!(failures.map(|failure| retries(load, failure, NOW)), [true; 3]);
}

#[test]
fn a_claim_waits_while_its_coin_is_not_on_chain_and_is_built_once_it_is() {
	let setup = setup();
	let claim = setup.claim(&key(1), peer_coin(1), Retry::Until(NOW + 3600));
	assert_eq!(setup.prepare(&[claim]), HashMap::from([]), "never observed");
	setup.observe_peer(&peer_coin(1), 2, false);
	assert_eq!(setup.prepare(&[claim]), HashMap::from([]), "observed absent");
	setup.observe_peer(&peer_coin(1), 3, true);
	assert_eq!(setup.prepare(&[claim]), HashMap::from([(claim.0, Answer::Built)]));
}

/// Each record is decided from itself: the coins that arrived are claimed
/// without waiting for the rest (C2 invariant 5).
#[test]
fn only_the_coins_that_arrived_are_claimed() {
	let setup = setup();
	let arrived = setup.claim(&key(1), peer_coin(1), Retry::Until(NOW + 3600));
	let missing = setup.claim(&key(1), peer_coin(2), Retry::Until(NOW + 3600));
	setup.observe_peer(&peer_coin(1), 2, true);
	assert_eq!(setup.prepare(&[arrived, missing]), HashMap::from([(arrived.0, Answer::Built)]));
}

/// A later read that shows the account empty replaces the earlier one: a
/// fork took the coin away.
#[test]
fn a_coin_a_later_read_shows_gone_is_not_claimed_on_the_strength_of_an_older_one() {
	let setup = setup();
	let claim = setup.claim(&key(1), peer_coin(1), Retry::Until(NOW + 3600));
	setup.observe_peer(&peer_coin(1), 2, true);
	setup.observe_peer(&peer_coin(1), 3, false);
	assert_eq!(setup.prepare(&[claim]), HashMap::from([]));
}

/// Past the claim deadline the coin belongs to the payer's reclaim (K2
/// invariant 3).
#[test]
fn a_claim_whose_coin_never_arrived_is_given_up_once_its_window_closes() {
	let setup = setup();
	let claim = setup.claim(&key(1), peer_coin(1), Retry::Until(NOW + 10));
	assert_eq!(setup.prepare(&[claim]), HashMap::from([]), "the window is open");
	assert_eq!(setup.prepare_at(&[claim], NOW + 11), HashMap::from([]), "nothing is given up without a look");
	setup.observe_peer(&peer_coin(1), 2, false);
	assert_eq!(setup.prepare_at(&[claim], NOW + 11), HashMap::from([(claim.0, Answer::GivenUp)]));
}

/// No minting is built before its handoff is committed (G1 invariant 3).
#[test]
fn a_chat_send_builds_nothing_before_its_handoff_is_committed() {
	let setup = setup();
	let input = setup.wallet.coin(3);
	let output = setup.allocate(AssetKind::Coin, 3);
	let params = RecordParams { call: Call::Transfer, retry: Retry::ExpiryOnly };
	let rows = Rows { inputs: vec![input], outputs: vec![output], tokens: Vec::new() };
	let mint = register_records(&setup.engine, &key(1), Admission::Ordinary, vec![(rows, params)]).unwrap()[0];
	block_on(setup.wallet.db.write(move |tx| ledger::mark_disclosed(tx, &[output]))).unwrap().unwrap();
	assert_eq!(setup.prepare(&[(mint, params)]), HashMap::from([]));
	block_on(setup.wallet.db.write(move |tx| ledger::commit_disclosed(tx, &[output]))).unwrap();
	assert_eq!(setup.prepare(&[(mint, params)]), HashMap::from([(mint, Answer::Built)]));
}

/// One record that cannot be built is answered without holding up the
/// others in its batch.
#[test]
fn a_top_up_that_cannot_resume_does_not_stop_the_rest_being_built() {
	let setup = setup();
	let orphaned = setup.load(&key(1), funding(1).account(), 3, Retry::Until(NOW + 3600));
	let claim = setup.claim(&key(2), peer_coin(1), Retry::Until(NOW + 3600));
	setup.observe_peer(&peer_coin(1), 2, true);
	assert_eq!(setup.prepare(&[orphaned, claim]), HashMap::from([(orphaned.0, Answer::GivenUp), (claim.0, Answer::Built)]));
}
