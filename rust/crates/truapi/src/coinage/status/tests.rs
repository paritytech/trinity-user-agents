use futures::executor::block_on;
use parity_scale_codec::Encode;

use super::*;
use crate::coinage::{
	PrivacyPolicy, PurseId,
	ledger::{self, AssetKind},
	operation::{self, ImportArguments},
	registration::{Admission, Rows},
	testing::{ENTROPY, NOW, TestWallet, chain_params, coin_read, register_rows, set_status, unbuilt_engine},
};

fn key(id: u8) -> OpKey {
	OpKey { scope: b"product".to_vec(), id: [id; 32] }
}

fn begin(wallet: &TestWallet, key: &OpKey, kind: OpKind, arguments: Vec<u8>) {
	let key = key.clone();
	block_on(wallet.db.write(move |tx| operation::begin(tx, &key, kind, &arguments))).unwrap().unwrap();
}

fn item(wallet: &TestWallet, key: &OpKey) -> StatusItem {
	status(&wallet.view(), key, NOW, &chain_params()).unwrap()
}

/// A rebalance of two recycles whose records stand at `statuses`.
fn rebalance_at(statuses: [(&'static str, Option<&'static str>); 2]) -> Status {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let engine = unbuilt_engine(wallet.db.clone());
	begin(&wallet, &key(1), OpKind::Rebalance, Vec::new());
	let rows = (0..2)
		.map(|_| Rows { inputs: vec![wallet.coin(3)], outputs: Vec::new(), tokens: Vec::new() })
		.collect();
	let records = register_rows(&engine, &key(1), Admission::Ordinary, rows).unwrap();
	for (record, (status, failure)) in records.into_iter().zip(statuses) {
		set_status(&wallet.db, record, status, failure);
	}
	item(&wallet, &key(1)).status
}

#[test]
fn an_operations_status_is_the_first_row_of_the_table_that_applies() {
	const DONE: (&str, Option<&str>) = ("FINALIZED_SUCCESS", None);
	const SETTLING: (&str, Option<&str>) = ("PENDING_SUCCESS", None);
	const SUBMITTED: (&str, Option<&str>) = ("PENDING", None);
	const WAITING: (&str, Option<&str>) = ("PENDING_SUBMISSION", None);
	const REJECTED: (&str, Option<&str>) = ("FAILURE", Some("REJECTED"));
	const EXPIRED: (&str, Option<&str>) = ("FAILURE", Some("EXPIRED"));
	const GIVEN_UP: (&str, Option<&str>) = ("FAILURE", Some("ABANDONED"));
	let table = [
		([DONE, DONE], Status::Done),
		([DONE, REJECTED], Status::PartiallyDone),
		([REJECTED, EXPIRED], Status::Failed(Failure::Refused)),
		([EXPIRED, EXPIRED], Status::Failed(Failure::Expired)),
		([GIVEN_UP, EXPIRED], Status::Failed(Failure::GivenUp)),
		([DONE, SUBMITTED], Status::Settling),
		([SETTLING, WAITING], Status::Settling),
		([SUBMITTED, WAITING], Status::Submitted),
		([WAITING, REJECTED], Status::Preparing),
	];
	for (statuses, expected) in table {
		assert_eq!(rebalance_at(statuses), expected, "records at {statuses:?}");
	}
}

#[test]
fn a_record_waiting_on_an_unminted_input_leaves_the_operation_waiting() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let engine = unbuilt_engine(wallet.db.clone());
	begin(&wallet, &key(1), OpKind::Rebalance, Vec::new());
	let minted = block_on(wallet.db.write(|tx| {
		ledger::allocate(tx, PurseId::MAIN, AssetKind::Coin, &[crate::coinage::Exponent::new(2).unwrap()])
	}))
	.unwrap()[0];
	let mint = Rows { inputs: vec![wallet.coin(3)], outputs: vec![minted], tokens: Vec::new() };
	let spend = Rows { inputs: vec![minted], outputs: Vec::new(), tokens: Vec::new() };
	register_rows(&engine, &key(9), Admission::Ordinary, vec![mint]).unwrap();
	register_rows(&engine, &key(1), Admission::Ordinary, vec![spend]).unwrap();
	assert_eq!(item(&wallet, &key(1)).status, Status::Waiting);
}

#[test]
fn progress_counts_settled_and_in_flight_value_from_the_records() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let engine = unbuilt_engine(wallet.db.clone());
	begin(&wallet, &key(1), OpKind::Rebalance, Vec::new());
	let rows = [3, 1].map(|exponent| Rows { inputs: vec![wallet.coin(exponent)], outputs: Vec::new(), tokens: Vec::new() });
	let records = register_rows(&engine, &key(1), Admission::Ordinary, rows.to_vec()).unwrap();
	set_status(&wallet.db, records[0], "FINALIZED_SUCCESS", None);
	assert_eq!(item(&wallet, &key(1)), StatusItem { status: Status::Settling, progress: Progress { settled: 8, in_flight: 2, provisional: 0 } });
}

#[test]
fn a_cancelled_operation_whose_records_were_given_up_fails_cancelled() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let engine = unbuilt_engine(wallet.db.clone());
	begin(&wallet, &key(1), OpKind::Rebalance, Vec::new());
	let rows = vec![Rows { inputs: vec![wallet.coin(3)], outputs: Vec::new(), tokens: Vec::new() }];
	let records = register_rows(&engine, &key(1), Admission::Ordinary, rows).unwrap();
	block_on(wallet.db.write(|tx| operation::request_cancel(tx, &key(1)))).unwrap().unwrap();
	set_status(&wallet.db, records[0], "FAILURE", Some("ABANDONED"));
	assert_eq!(item(&wallet, &key(1)).status, Status::Failed(Failure::Cancelled));
}

#[test]
fn an_export_prepares_until_its_commit_and_fails_not_committed_once_released() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let coin = wallet.coin(3);
	begin(&wallet, &key(1), OpKind::Export, Vec::new());
	block_on(wallet.db.write(move |tx| {
		operation::record_exported(tx, &key(1), &[(coin, crate::coinage::Exponent::new(3).unwrap())])?;
		ledger::mark_disclosed(tx, &[coin]).map(|_| ())
	}))
	.unwrap();
	assert_eq!(item(&wallet, &key(1)), StatusItem { status: Status::Preparing, progress: Progress { settled: 0, in_flight: 8, provisional: 0 } });

	block_on(wallet.db.write(move |tx| ledger::commit_disclosed(tx, &[coin]))).unwrap();
	assert_eq!(item(&wallet, &key(1)), StatusItem { status: Status::Done, progress: Progress { settled: 8, in_flight: 0, provisional: 0 } });

	begin(&wallet, &key(2), OpKind::Export, Vec::new());
	block_on(wallet.db.write(|tx| operation::record_release(tx, &key(2)))).unwrap();
	assert_eq!(item(&wallet, &key(2)).status, Status::Failed(Failure::NotCommitted));
}

#[test]
fn an_import_is_judged_per_coin_and_its_completion_names_coin_accounts_only() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let engine = unbuilt_engine(wallet.db.clone());
	let (claimed, sniped, absent) = ([1; 32], [2; 32], [3; 32]);
	let arguments = ImportArguments { into: PurseId::MAIN, accounts: vec![claimed, sniped, absent] };
	begin(&wallet, &key(1), OpKind::Import, arguments.encode());
	let output = wallet.hold(PurseId::MAIN, AssetKind::Coin, 2, coin_read(2));
	let claims = vec![
		Rows { inputs: vec![Asset::PeerCoin(claimed)], outputs: vec![output], tokens: Vec::new() },
		Rows { inputs: vec![Asset::PeerCoin(sniped)], outputs: Vec::new(), tokens: Vec::new() },
	];
	let records = register_rows(&engine, &key(1), Admission::Import, claims).unwrap();
	assert_eq!(item(&wallet, &key(1)).status, Status::Waiting, "the absent coin's claim never started, the others wait");

	set_status(&wallet.db, records[0], "FINALIZED_SUCCESS", None);
	set_status(&wallet.db, records[1], "FAILURE", Some("DISPATCH_FAILED"));
	let completion = completion(&wallet.view(), &key(1), NOW, &chain_params(), &ENTROPY).unwrap();
	assert_eq!(completion.status, Status::PartiallyDone);
	assert_eq!(
		completion.coins,
		vec![(claimed, CoinResult::Moved), (sniped, CoinResult::TakenByOther), (absent, CoinResult::Absent)]
	);
	let Asset::Coin { purse, installation, index } = output else { unreachable!("held as a coin") };
	let (_, output_account) = coin_key(&ENTROPY, purse, installation, index).unwrap();
	assert_eq!(completion.transactions.len(), 1);
	assert_eq!(completion.transactions[0].consumed, vec![claimed]);
	assert_eq!(completion.transactions[0].created, vec![output_account]);
}

#[test]
fn no_completion_exists_before_the_status_is_terminal() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let engine = unbuilt_engine(wallet.db.clone());
	begin(&wallet, &key(1), OpKind::Rebalance, Vec::new());
	let rows = vec![Rows { inputs: vec![wallet.coin(3)], outputs: Vec::new(), tokens: Vec::new() }];
	register_rows(&engine, &key(1), Admission::Ordinary, rows).unwrap();
	assert_eq!(completion(&wallet.view(), &key(1), NOW, &chain_params(), &ENTROPY), None);
	assert_eq!(open_operations(&wallet.view(), NOW, &chain_params()), vec![key(1)]);
}
