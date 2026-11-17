//! The `coinage` rebuild policy: what a record is built from, when it waits,
//! and when it is given up (C2 invariant 5).

use core::sync::atomic::{AtomicU64, Ordering};
use std::{collections::HashMap, sync::Arc};

use futures::executor::block_on;
use subxt::utils::H256;

use crate::{
	chain::extrinsic_service_serving,
	coinage::{
		Asset, Exponent, PrivacyPolicy, PurseId,
		assembly::Transaction,
		ledger::{self, AssetKind, BlockRef, CoinOnChain, EntryOnChain, Facts, Read, Subject},
		namespace::coin_key,
		operation::{OpKey, Retry},
		planning::Call,
		plugins::{CoinageDeps, CoinagePolicy, Running, RunningState, buildable, retries, transaction},
		registration::{Admission, Rows, coinage_domain},
		start::RecordParams,
		testing::{FixedSeed, NOW, NoPerson, PEOPLE_METADATA_BYTES, TestWallet, chain_params, register_rows, unbuilt_engine, world::WorldChain},
	},
	durable::{
		DeferredDurableTx, DurableTxEntry, DurableTxId, DurableTxStatus, PolicyParams, Preparation, SubmissionPlan,
		SubmissionPolicy, VerdictFailureKind, Window,
	},
	installation::InstallationId,
};

/// The deadline of every `Until` record here.
const DEADLINE: u64 = NOW + 600;

/// What the policy answered for one record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Answer {
	/// An attempt.
	Built,
	/// The record fails.
	GaveUp,
	/// No answer: the record keeps waiting.
	Waits,
}

/// The policy over a test wallet and a [`WorldChain`], at a clock the test
/// moves.
struct Bench {
	wallet: TestWallet,
	chain: Arc<WorldChain>,
	clock: Arc<AtomicU64>,
	policy: CoinagePolicy,
	engine: Arc<crate::durable::DurableTxEngine>,
	operations: u8,
	blocks: u32,
}

impl Bench {
	fn new() -> Self {
		let wallet = TestWallet::new(PrivacyPolicy::fastest());
		let chain = WorldChain::new();
		let clock = Arc::new(AtomicU64::new(NOW));
		let running = Running::new();
		running.open(RunningState { params: chain_params(), check_failed: false, funding: HashMap::new() });
		let time = clock.clone();
		let deps = CoinageDeps {
			store: wallet.db.clone(),
			seed: Arc::new(FixedSeed),
			clock: Arc::new(move || time.load(Ordering::SeqCst)),
			people: chain.clone(),
			people_genesis: H256([0xab; 32]),
			asset_hub_genesis: H256([0xcd; 32]),
			extrinsics: Arc::new(extrinsic_service_serving(PEOPLE_METADATA_BYTES, 0)),
			personhood: Arc::new(NoPerson),
			running,
		};
		let engine = unbuilt_engine(wallet.db.clone());
		Self { wallet, chain, clock, policy: CoinagePolicy(Arc::new(deps)), engine, operations: 0, blocks: 1 }
	}

	/// An own coin of `exponent`, never observed.
	fn coin(&self, exponent: u8) -> Asset {
		let exponents = [Exponent::new(exponent).unwrap()];
		block_on(self.wallet.db.write(move |tx| ledger::allocate(tx, PurseId::MAIN, AssetKind::Coin, &exponents))).unwrap()[0]
	}

	/// A peer's coin whose secret the wallet keeps.
	fn peer_coin(&self, n: u32) -> Asset {
		let (secret, account) = coin_key(&[5; 16], PurseId::MAIN, InstallationId([9; 32]), n).unwrap();
		block_on(self.wallet.db.write(move |tx| ledger::keep_secrets(tx, &[(account, secret)]))).unwrap();
		Asset::PeerCoin(account)
	}

	/// F1 reads `read` for `asset` at the next block.
	fn observe(&mut self, asset: Asset, read: Read) {
		self.blocks += 1;
		let facts = Facts { at: BlockRef { number: self.blocks, hash: [1; 32] }, reads: vec![(Subject::Asset(asset), read)] };
		block_on(self.wallet.db.write(move |tx| ledger::observe(tx, &facts, NOW, 255))).unwrap();
	}

	/// One record registered under a fresh operation.
	fn record(&mut self, inputs: &[Asset], outputs: &[Asset]) -> DurableTxId {
		self.operations += 1;
		let rows = Rows { inputs: inputs.to_vec(), outputs: outputs.to_vec(), tokens: Vec::new() };
		let admission = if inputs.iter().any(|input| matches!(input, Asset::PeerCoin(_))) { Admission::Import } else { Admission::Ordinary };
		register_rows(&self.engine, &OpKey::own([self.operations; 32]), admission, vec![rows]).unwrap()[0]
	}

	/// An import of the peer's coin `n` into a fresh coin.
	fn import(&mut self, n: u32) -> (DurableTxId, Asset) {
		let peer = self.peer_coin(n);
		let output = self.coin(2);
		(self.record(&[peer], &[output]), peer)
	}

	/// The policy's answer for each of `batch`, waiting with its params and
	/// build count.
	fn answers(&self, batch: &[(DurableTxId, PolicyParams, u32)]) -> Vec<Answer> {
		let waiting: Vec<DeferredDurableTx> = batch
			.iter()
			.map(|(id, params, attempts)| DeferredDurableTx {
				id: *id,
				domain: coinage_domain(),
				group: None,
				plan: SubmissionPlan { params: params.clone() },
				attempts: *attempts,
			})
			.collect();
		let mut answers = block_on(self.policy.prepare(&waiting)).unwrap();
		waiting
			.iter()
			.map(|row| match answers.remove(&row.id) {
				Some(Preparation::Ready { .. }) => Answer::Built,
				Some(Preparation::GiveUp) => Answer::GaveUp,
				None => Answer::Waits,
			})
			.collect()
	}

	/// The policy's answer for `id`, waiting for its first build.
	fn answer(&self, id: DurableTxId, retry: Retry) -> Answer {
		self.answers(&[(id, transfer(retry), 0)])[0]
	}

	fn set_clock(&self, now: u64) {
		self.clock.store(now, Ordering::SeqCst);
	}
}

fn transfer(retry: Retry) -> PolicyParams {
	RecordParams { call: Call::Transfer, retry }.to_params()
}

fn present(exponent: u8, locked_until: Option<u64>) -> Read {
	Read::Coin { now: Some(CoinOnChain { exponent: Exponent::new(exponent).unwrap(), age: 0, locked_until }), emptied_by: None }
}

fn empty() -> Read {
	Read::Coin { now: None, emptied_by: None }
}

fn in_ring(included: u32, locked_until: Option<u64>) -> Read {
	Read::Entry(Some(EntryOnChain {
		ring: 7,
		revision: 1,
		page: 0,
		position: 0,
		members: 10,
		included,
		closed: false,
		expires_at: None,
		locked_until,
	}))
}

fn exponent(raw: u8) -> Exponent {
	Exponent::new(raw).unwrap()
}

#[test]
fn a_split_is_rebuilt_from_its_recorded_coin_and_outputs_in_order() {
	let mut bench = Bench::new();
	let input = bench.coin(3);
	let outputs = [bench.coin(1), bench.coin(2), bench.coin(1)];
	let id = bench.record(&[input], &outputs);
	let view = bench.wallet.view();
	let facts = view.registrations().record(id).unwrap();
	let expected = Transaction {
		call: Call::Split,
		inputs: vec![(input, exponent(3))],
		outputs: vec![(outputs[0], exponent(1)), (outputs[1], exponent(2)), (outputs[2], exponent(1))],
		token: None,
	};
	assert_eq!(transaction(facts, Call::Split, &view), Some(expected));
}

#[test]
fn an_unload_is_rebuilt_from_its_recorded_entries_and_outputs_in_order() {
	let mut bench = Bench::new();
	let entries = [3, 4].map(|raw| bench.wallet.hold(PurseId::MAIN, AssetKind::Entry, raw, in_ring(10, None)));
	let output = bench.coin(2);
	let id = bench.record(&entries, &[output]);
	let view = bench.wallet.view();
	let facts = view.registrations().record(id).unwrap();
	let expected = Transaction {
		call: Call::UnloadToCoins,
		inputs: vec![(entries[0], exponent(3)), (entries[1], exponent(4))],
		outputs: vec![(output, exponent(2))],
		token: None,
	};
	assert_eq!(transaction(facts, Call::UnloadToCoins, &view), Some(expected));
}

/// The peer's coin has no asset record, so its denomination is what F1
/// last saw; the output is the one recorded.
#[test]
fn an_import_is_rebuilt_into_its_recorded_output_from_the_peer_coin_seen() {
	let mut bench = Bench::new();
	let (id, peer) = bench.import(1);
	bench.observe(peer, present(2, None));
	let view = bench.wallet.view();
	let facts = view.registrations().record(id).unwrap();
	let expected = Transaction {
		call: Call::Transfer,
		inputs: vec![(peer, exponent(2))],
		outputs: vec![(facts.rows.outputs[0], exponent(2))],
		token: None,
	};
	assert_eq!(transaction(facts, Call::Transfer, &view), Some(expected));
}

/// A record is built only once every input it consumes is usable now.
#[test]
fn every_input_must_be_usable_before_its_record_is_built() {
	let mut bench = Bench::new();
	let reads = [
		Some(present(2, None)),
		Some(present(2, Some(NOW + 60))),
		Some(empty()),
		None,
		Some(in_ring(10, None)),
		Some(in_ring(0, None)),
		Some(in_ring(10, Some(NOW + 60))),
	];
	let usable = reads.map(|read| {
		let asset = match read {
			Some(Read::Entry(_)) => bench.wallet.hold(PurseId::MAIN, AssetKind::Entry, 3, Read::Failed),
			_ => bench.coin(2),
		};
		if let Some(read) = read {
			bench.observe(asset, read);
		}
		buildable(&asset, &bench.wallet.view(), NOW)
	});
	assert_eq!(usable, [true, false, false, false, true, false, false]);
}

#[test]
fn an_import_is_built_again_while_its_peer_coin_is_on_chain() {
	let mut bench = Bench::new();
	let (id, peer) = bench.import(1);
	bench.observe(peer, present(2, None));
	assert_eq!(bench.answers(&[(id, transfer(Retry::Until(DEADLINE)), 1)]), [Answer::Built]);
}

#[test]
fn an_import_waits_while_its_peer_coin_is_not_on_chain() {
	let mut bench = Bench::new();
	let (id, peer) = bench.import(1);
	bench.observe(peer, empty());
	assert_eq!(bench.answer(id, Retry::Until(DEADLINE)), Answer::Waits);
}

#[test]
fn an_import_is_built_once_its_peer_coin_appears() {
	let mut bench = Bench::new();
	let (id, peer) = bench.import(1);
	bench.observe(peer, empty());
	let before = bench.answer(id, Retry::Until(DEADLINE));
	bench.observe(peer, present(2, None));
	assert_eq!([before, bench.answer(id, Retry::Until(DEADLINE))], [Answer::Waits, Answer::Built]);
}

/// Each record is decided from itself, never from the rest of its batch.
#[test]
fn each_import_is_decided_by_its_own_coin() {
	let mut bench = Bench::new();
	let (arrived, arrived_peer) = bench.import(1);
	let (missing, missing_peer) = bench.import(2);
	bench.observe(arrived_peer, present(2, None));
	bench.observe(missing_peer, empty());
	let batch = [(arrived, transfer(Retry::Until(DEADLINE)), 0), (missing, transfer(Retry::Until(DEADLINE)), 0)];
	assert_eq!(bench.answers(&batch), [Answer::Built, Answer::Waits]);
}

#[test]
fn a_coin_a_fork_took_away_is_not_claimed_on_an_older_observation() {
	let mut bench = Bench::new();
	let (id, peer) = bench.import(1);
	bench.observe(peer, present(2, None));
	bench.observe(peer, empty());
	assert_eq!(bench.answer(id, Retry::Until(DEADLINE)), Answer::Waits);
}

#[test]
fn a_failed_read_does_not_erase_what_the_chain_last_showed() {
	let mut bench = Bench::new();
	let (id, peer) = bench.import(1);
	bench.observe(peer, present(2, None));
	bench.observe(peer, Read::Failed);
	assert_eq!(bench.answer(id, Retry::Until(DEADLINE)), Answer::Built);
}

#[test]
fn a_record_whose_input_is_gone_past_its_deadline_is_given_up() {
	let mut bench = Bench::new();
	let (id, peer) = bench.import(1);
	bench.observe(peer, empty());
	bench.set_clock(DEADLINE + 1);
	assert_eq!(bench.answer(id, Retry::Until(DEADLINE)), Answer::GaveUp);
}

/// The deadline is read from the clock at each call: no new observation is
/// needed to notice it passed.
#[test]
fn a_deadline_that_passes_is_noticed_without_a_new_observation() {
	let mut bench = Bench::new();
	let (id, peer) = bench.import(1);
	bench.observe(peer, empty());
	let before = bench.answer(id, Retry::Until(DEADLINE));
	bench.set_clock(DEADLINE + 1);
	assert_eq!([before, bench.answer(id, Retry::Until(DEADLINE))], [Answer::Waits, Answer::GaveUp]);
}

/// C2 invariant 5: past the deadline, a record is given up only once its
/// input is gone. A coin still there is claimed however late.
#[test]
fn a_record_whose_input_is_still_there_is_built_past_its_deadline() {
	let mut bench = Bench::new();
	let (id, peer) = bench.import(1);
	bench.observe(peer, present(2, None));
	bench.set_clock(DEADLINE + 1);
	let first_and_rebuilt = [0, 3].map(|attempts| bench.answers(&[(id, transfer(Retry::Until(DEADLINE)), attempts)])[0]);
	assert_eq!(first_and_rebuilt, [Answer::Built; 2]);
}

#[test]
fn past_its_deadline_a_present_input_is_built_and_a_gone_one_given_up() {
	let mut bench = Bench::new();
	let (present_import, present_peer) = bench.import(1);
	let (gone_import, gone_peer) = bench.import(2);
	bench.observe(present_peer, present(2, None));
	bench.observe(gone_peer, empty());
	bench.set_clock(DEADLINE + 1);
	let batch = [(present_import, transfer(Retry::Until(DEADLINE)), 1), (gone_import, transfer(Retry::Until(DEADLINE)), 1)];
	assert_eq!(bench.answers(&batch), [Answer::Built, Answer::GaveUp]);
}

/// "Gone" is a fact F1 read, never the lack of one.
#[test]
fn nothing_is_given_up_past_its_deadline_without_an_observation() {
	let mut bench = Bench::new();
	let (id, _) = bench.import(1);
	bench.set_clock(DEADLINE + 1);
	assert_eq!(bench.answer(id, Retry::Until(DEADLINE)), Answer::Waits);
}

#[test]
fn an_input_seen_missing_before_the_deadline_keeps_the_record_waiting() {
	let mut bench = Bench::new();
	let (id, peer) = bench.import(1);
	bench.observe(peer, empty());
	bench.set_clock(DEADLINE - 1);
	assert_eq!(bench.answer(id, Retry::Until(DEADLINE)), Answer::Waits);
}

/// A record that cannot be assembled is asked about again; one whose reads
/// fail makes the whole call fail. Neither gives the record up.
#[test]
fn a_build_failure_is_not_a_give_up() {
	let mut bench = Bench::new();
	let peer = bench.peer_coin(1);
	let id = bench.record(&[peer], &[]);
	bench.observe(peer, present(2, None));
	let unassembled = bench.answer(id, Retry::Until(DEADLINE));
	bench.chain.tree().faults.heads = true;
	let waiting = DeferredDurableTx {
		id,
		domain: coinage_domain(),
		group: None,
		plan: SubmissionPlan { params: transfer(Retry::Until(DEADLINE)) },
		attempts: 0,
	};
	let unread = block_on(bench.policy.prepare(&[waiting])).is_err();
	assert_eq!((unassembled, unread), (Answer::Waits, true));
}

#[test]
fn a_record_whose_params_do_not_decode_is_given_up_without_holding_up_the_others() {
	let mut bench = Bench::new();
	let (undecodable, _) = bench.import(1);
	let (buildable_import, peer) = bench.import(2);
	bench.observe(peer, present(2, None));
	let batch = [(undecodable, PolicyParams(vec![9, 9]), 0), (buildable_import, transfer(Retry::ExpiryOnly), 0)];
	assert_eq!(bench.answers(&batch), [Answer::GaveUp, Answer::Built]);
}

#[test]
fn a_record_built_once_is_never_retried() {
	let kinds = [VerdictFailureKind::Expired, VerdictFailureKind::DispatchFailed, VerdictFailureKind::Rejected];
	let params = RecordParams { call: Call::Transfer, retry: Retry::Never };
	assert_eq!(kinds.map(|kind| retries(params, kind)), [false; 3]);
}

#[test]
fn a_record_whose_params_do_not_decode_is_never_retried() {
	let bench = Bench::new();
	let entry = DurableTxEntry {
		id: DurableTxId(1),
		domain: coinage_domain(),
		group: None,
		tx_hash: H256::zero(),
		window: Window { birth: 1, period: 64 },
		status: DurableTxStatus::Pending,
		scanned_to: None,
		plan: None,
	};
	let kinds = [VerdictFailureKind::Expired, VerdictFailureKind::DispatchFailed, VerdictFailureKind::Rejected];
	assert_eq!(kinds.map(|kind| bench.policy.can_retry(&entry, &PolicyParams(vec![9, 9]), kind)), [false; 3]);
}
