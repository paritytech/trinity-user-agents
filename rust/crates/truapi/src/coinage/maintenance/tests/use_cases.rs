//! The recycling sweep as a wallet's coins meet it.

use super::*;
use crate::coinage::{
	registration::{Admission, Rows},
	testing::register_rows,
};

fn entries(wallet: &TestWallet) -> usize {
	wallet.view().assets(PurseId::MAIN).filter(|record| matches!(record.asset, Asset::Entry { .. })).count()
}

/// The sweep reads what is due from the records, so a coin its earlier
/// call recycles is held, and no second entry is allocated for it.
#[test]
fn a_coin_already_being_recycled_is_not_recycled_again() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	wallet.hold(PurseId::MAIN, AssetKind::Coin, 3, old_coin(3, 14));
	assert_eq!(run(&wallet).started.len(), 1);
	let allocated = entries(&wallet);
	assert!(run(&wallet).started.is_empty());
	assert_eq!(entries(&wallet), allocated, "a refused start allocates nothing");
}

#[test]
fn nothing_is_started_when_every_due_coin_is_in_flight() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let first = wallet.hold(PurseId::MAIN, AssetKind::Coin, 3, old_coin(3, 14));
	let second = wallet.hold(PurseId::MAIN, AssetKind::Coin, 2, old_coin(2, 15));
	let engine = unbuilt_engine(wallet.db.clone());
	let spend = Rows { inputs: vec![first, second], outputs: Vec::new(), tokens: Vec::new() };
	register_rows(&engine, &OpKey { scope: b"product".to_vec(), id: [1; 32] }, Admission::Ordinary, vec![spend]).unwrap();
	assert!(run(&wallet).started.is_empty());
}

/// A coin whose secret went to a payee is the payee's to claim; recycling
/// it would take the payment back.
#[test]
fn a_coin_handed_to_a_payee_is_not_recycled() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let coin = wallet.hold(PurseId::MAIN, AssetKind::Coin, 3, old_coin(3, 14));
	block_on(wallet.db.write(move |tx| ledger::mark_disclosed(tx, &[coin]))).unwrap().unwrap();
	assert!(run(&wallet).started.is_empty());
}
