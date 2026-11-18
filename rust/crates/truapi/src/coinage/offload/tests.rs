
use futures::executor::block_on;

use super::*;
use crate::coinage::{
	Exponent, PrivacyPolicy,
	ledger::AssetKind,
	testing::{NOW, TestWallet, chain_params},
};

fn offload(amount: u64) -> Offload {
	Offload { from: PurseId::MAIN, amount, to: [7; 32], initiator: Initiator::User, confirmed: false }
}

fn key() -> OpKey {
	OpKey { scope: b"p".to_vec(), id: [1; 32] }
}

fn phase_of(wallet: &TestWallet, amount: u64) -> Phase {
	plan(&wallet.view(), &key(), &offload(amount), NOW, &chain_params())
}

#[test]
fn usable_entries_that_cover_the_amount_are_offboarded() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let entry = wallet.entry(4, 0);
	let Phase::Offboard(selection) = phase_of(&wallet, 10) else { panic!("an offboard") };
	assert_eq!(selection.inputs, [entry]);
}

#[test]
fn usable_coins_are_recycled_to_cover_the_deficit() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let large = wallet.coin(4);
	wallet.coin(1);
	assert_eq!(phase_of(&wallet, 9), Phase::Recycle(vec![large]), "the largest coin covers it");
}

#[test]
fn value_still_arriving_is_waited_for() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let outputs = [Exponent::new(4).unwrap()];
	block_on(wallet.db.write(move |tx| ledger::allocate(tx, PurseId::MAIN, AssetKind::Entry, &outputs))).unwrap();
	assert_eq!(phase_of(&wallet, 10), Phase::Wait(None));
}

#[test]
fn an_offload_nothing_covers_fails() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	wallet.coin(1);
	assert_eq!(phase_of(&wallet, 10), Phase::Failed);
}

#[test]
fn an_offloaded_amount_is_done() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	assert_eq!(phase_of(&wallet, 0), Phase::Done);
}
