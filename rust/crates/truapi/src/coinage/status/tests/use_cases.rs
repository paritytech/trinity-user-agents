//! Operation statuses as top-ups, claims and payments report them.

use super::*;
use crate::{
	coinage::{
		Exponent,
		ledger::{BlockRef, Facts},
		testing::funding::register_records,
	},
	durable::DurableTxId,
};

/// A rebalance whose records spend coins of `exponents`, one record each.
fn rebalance(wallet: &TestWallet, exponents: &[u8]) -> Vec<DurableTxId> {
	let engine = unbuilt_engine(wallet.db.clone());
	begin(wallet, &key(1), OpKind::Rebalance, Vec::new());
	let rows = exponents
		.iter()
		.map(|exponent| Rows { inputs: vec![wallet.coin(*exponent)], outputs: Vec::new(), tokens: Vec::new() })
		.collect();
	register_rows(&engine, &key(1), Admission::Ordinary, rows).unwrap()
}

/// An import of one peer coin per account, each claimed into a fresh coin
/// of 4 cents that F1 has not observed yet; its claims and their outputs.
fn import(wallet: &TestWallet, accounts: &[[u8; 32]]) -> (Vec<DurableTxId>, Vec<Asset>) {
	let engine = unbuilt_engine(wallet.db.clone());
	let arguments = ImportArguments { into: PurseId::MAIN, accounts: accounts.to_vec() };
	begin(wallet, &key(1), OpKind::Import, arguments.encode());
	let outputs: Vec<Asset> = accounts
		.iter()
		.map(|_| {
			let exponent = [Exponent::new(2).unwrap()];
			block_on(wallet.db.write(move |tx| ledger::allocate(tx, PurseId::MAIN, AssetKind::Coin, &exponent))).unwrap()[0]
		})
		.collect();
	let rows = accounts
		.iter()
		.zip(&outputs)
		.map(|(account, output)| Rows { inputs: vec![Asset::PeerCoin(*account)], outputs: vec![*output], tokens: Vec::new() })
		.collect();
	(register_rows(&engine, &key(1), Admission::Import, rows).unwrap(), outputs)
}

fn observe(wallet: &TestWallet, asset: Asset, read: Read) {
	let facts = Facts { at: BlockRef { number: 2, hash: [2; 32] }, reads: vec![(Subject::Asset(asset), read)] };
	block_on(wallet.db.write(move |tx| ledger::observe(tx, &facts, NOW, 255))).unwrap();
}

fn progress(settled: u64, in_flight: u64, provisional: u64) -> Progress {
	Progress { settled, in_flight, provisional }
}

/// One record that succeeded on the best chain says nothing about its
/// sibling still submitted, so RFC-6 must not read it as the whole amount.
#[test]
fn value_that_succeeded_on_the_best_chain_is_provisional_and_counted_apart() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let records = rebalance(&wallet, &[3, 1]);
	set_status(&wallet.db, records[0], "PENDING_SUCCESS", None);
	set_status(&wallet.db, records[1], "PENDING", None);
	assert_eq!(item(&wallet, &key(1)), StatusItem { status: Status::Settling, progress: progress(0, 10, 8) });
}

#[test]
fn the_reported_amount_does_not_count_a_record_that_failed() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let records = rebalance(&wallet, &[3, 1, 2]);
	set_status(&wallet.db, records[0], "FINALIZED_SUCCESS", None);
	set_status(&wallet.db, records[1], "FAILURE", Some("REJECTED"));
	assert_eq!(item(&wallet, &key(1)), StatusItem { status: Status::Settling, progress: progress(8, 4, 0) });
}

/// Layer 1 recomputes the status from the records at every read, so a
/// success a reorganisation retracts stops being reported.
#[test]
fn a_success_a_fork_retracts_is_no_longer_reported_as_settling() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let records = rebalance(&wallet, &[3]);
	set_status(&wallet.db, records[0], "PENDING_SUCCESS", None);
	assert_eq!(item(&wallet, &key(1)), StatusItem { status: Status::Settling, progress: progress(0, 8, 8) });
	set_status(&wallet.db, records[0], "PENDING", None);
	assert_eq!(item(&wallet, &key(1)), StatusItem { status: Status::Submitted, progress: progress(0, 8, 0) });
}

#[test]
fn an_import_waits_while_its_coin_has_not_appeared() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	import(&wallet, &[[1; 32]]);
	assert_eq!(item(&wallet, &key(1)), StatusItem { status: Status::Waiting, progress: progress(0, 4, 0) });
}

#[test]
fn an_import_whose_claims_are_in_flight_is_submitted() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let (claims, _) = import(&wallet, &[[1; 32], [2; 32]]);
	set_status(&wallet.db, claims[0], "PENDING_SUCCESS", None);
	set_status(&wallet.db, claims[1], "PENDING", None);
	assert_eq!(item(&wallet, &key(1)), StatusItem { status: Status::Submitted, progress: progress(0, 8, 4) });
}

#[test]
fn a_claim_only_included_in_a_block_does_not_end_the_import() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let (claims, _) = import(&wallet, &[[1; 32]]);
	set_status(&wallet.db, claims[0], "PENDING_SUCCESS", None);
	assert!(!item(&wallet, &key(1)).status.is_terminal());
}

/// A claim a reorganisation retracts takes its provisional value with it.
#[test]
fn a_claim_a_fork_retracts_is_no_longer_counted_as_provisional() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let (claims, _) = import(&wallet, &[[1; 32]]);
	set_status(&wallet.db, claims[0], "PENDING_SUCCESS", None);
	assert_eq!(item(&wallet, &key(1)).progress, progress(0, 4, 4));
	set_status(&wallet.db, claims[0], "PENDING", None);
	assert_eq!(item(&wallet, &key(1)).progress, progress(0, 4, 0));
}

/// A coin moved by an import settles at the exponent observed in its
/// output account, so the import ends only once that output was observed.
#[test]
fn an_import_ends_once_every_claim_succeeded_finally_and_its_output_was_observed() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let (claims, outputs) = import(&wallet, &[[1; 32], [2; 32]]);
	for claim in &claims {
		set_status(&wallet.db, *claim, "FINALIZED_SUCCESS", None);
	}
	assert!(!item(&wallet, &key(1)).status.is_terminal(), "no output observed yet");
	for output in outputs {
		observe(&wallet, output, coin_read(2));
	}
	assert_eq!(item(&wallet, &key(1)), StatusItem { status: Status::Done, progress: progress(8, 0, 0) });
}

#[test]
fn an_import_whose_coins_never_arrived_before_its_window_closed_moved_nothing() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let (claims, _) = import(&wallet, &[[1; 32]]);
	set_status(&wallet.db, claims[0], "FAILURE", Some("ABANDONED"));
	assert_eq!(item(&wallet, &key(1)).status, Status::Failed(Failure::NothingMoved));
	let completion = completion(&wallet.view(), &key(1), NOW, &chain_params(), &ENTROPY).unwrap();
	assert_eq!(completion.coins, vec![CoinOutcome { account: [1; 32], result: CoinResult::Absent }]);
}

/// A load's input is its account's funds, which no status reads, so a
/// top-up whose loads wait to be built waits for its funds: RFC-6 reports
/// it `Detecting` (N2 invariant 4).
#[test]
fn a_top_up_whose_loads_wait_to_be_built_is_waiting_for_its_funds() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let engine = unbuilt_engine(wallet.db.clone());
	begin(&wallet, &key(1), OpKind::TopUp, Vec::new());
	let exponent = [Exponent::new(3).unwrap()];
	let entry = block_on(wallet.db.write(move |tx| ledger::allocate(tx, PurseId::MAIN, AssetKind::Entry, &exponent))).unwrap()[0];
	let load = RecordParams { call: Call::Load([1; 32]), retry: crate::coinage::operation::Retry::Until(NOW + 3600) };
	let rows = Rows { inputs: Vec::new(), outputs: vec![entry], tokens: Vec::new() };
	register_records(&engine, &key(1), Admission::Ordinary, vec![(rows, load)]).unwrap();
	assert_eq!(item(&wallet, &key(1)), StatusItem { status: Status::Waiting, progress: progress(0, 8, 0) });
}

