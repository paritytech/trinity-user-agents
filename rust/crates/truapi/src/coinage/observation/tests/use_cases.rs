//! What F1 tells a payer about the coins it handed to a payee.

use super::*;
use crate::coinage::{
	classification::{ExportStatus, classify},
	ledger::{AssetRecord, adopt, mark_disclosed},
	operation::OpKey,
	registration::{Admission, Rows},
	chain::Emptying,
	testing::{register_rows, set_status, unbuilt_engine},
};

/// The transaction the payee's claim was, in the fake chain.
const CLAIM: [u8; 32] = [7; 32];

/// The payee's claim of the coin at `account`, included at block `number`.
fn claimed_at(chain: &FakeChain, account: [u8; 32], number: u64) {
	chain.emptied.lock().insert(account, Emptying { transaction: CLAIM, block: number });
}

fn export(wallet: &TestWallet, coin: Asset) {
	block_on(wallet.db.write(move |tx| {
		mark_disclosed(tx, &[coin])?.unwrap();
		ledger::commit_disclosed(tx, &[coin])
	}))
	.unwrap();
}

fn export_status(wallet: &TestWallet, coin: &Asset) -> Option<ExportStatus> {
	classify(coin, &wallet.view(), NOW, &chain_params()).export
}

fn observe(wallet: &TestWallet, chain: &Arc<FakeChain>, number: u64) {
	block_on(on_block(&deps(wallet, chain.clone()), block(number), NOW, &chain_params())).unwrap();
}

/// A coin of 4 cents handed to a payee, which F1 saw on chain at block 1.
fn handed_out(wallet: &TestWallet) -> (Asset, [u8; 32]) {
	let coin = wallet.coin(2);
	export(wallet, coin);
	(coin, account(0))
}

#[test]
fn a_coin_gone_from_the_finalized_chain_is_proven_claimed() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let (coin, account) = handed_out(&wallet);
	let chain = Arc::new(FakeChain::default());
	claimed_at(&chain, account, 5);
	observe(&wallet, &chain, 8);
	assert_eq!(export_status(&wallet, &coin), Some(ExportStatus::Claimed { stable: true, by: CLAIM }));
}

#[test]
fn a_coin_the_chain_still_holds_awaits_its_payee() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let (coin, account) = handed_out(&wallet);
	let chain = Arc::new(FakeChain::default());
	chain.coins.lock().insert(account, crate::coinage::chain::ChainCoin { instance: 0, value: 2, age: 1, locked_until: None });
	observe(&wallet, &chain, 8);
	assert_eq!(export_status(&wallet, &coin), Some(ExportStatus::AwaitingPayee));
}

/// A claim read above the finalized height may still be retracted, and
/// the cheque's progress counts it only once it is stable (H5 invariant 3).
#[test]
fn a_coin_the_payee_has_taken_reads_as_claimed_before_the_claim_finalizes() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let (coin, account) = handed_out(&wallet);
	let chain = Arc::new(FakeChain::default());
	claimed_at(&chain, account, 12);
	observe(&wallet, &chain, 12);
	assert_eq!(export_status(&wallet, &coin), Some(ExportStatus::Claimed { stable: false, by: CLAIM }));
}

#[test]
fn a_claim_finalizing_with_nothing_local_changing_is_proven_at_the_next_finalized_head() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let (coin, account) = handed_out(&wallet);
	let chain = Arc::new(FakeChain::default());
	claimed_at(&chain, account, 12);
	observe(&wallet, &chain, 12);
	*chain.finalized.lock() = Some(13);
	observe(&wallet, &chain, 13);
	assert_eq!(export_status(&wallet, &coin), Some(ExportStatus::Claimed { stable: true, by: CLAIM }));
}

/// The search starts where the coin was minted, so a claim made before
/// F1 ever read the account is still found.
#[test]
fn a_coin_claimed_while_the_app_was_closed_is_still_proven_claimed() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let exponent = [crate::coinage::Exponent::new(2).unwrap()];
	let coin = block_on(wallet.db.write(move |tx| ledger::allocate(tx, PurseId::MAIN, AssetKind::Coin, &exponent))).unwrap()[0];
	let engine = unbuilt_engine(wallet.db.clone());
	let mint = Rows { inputs: vec![wallet.coin(2)], outputs: vec![coin], tokens: Vec::new() };
	let minted = register_rows(&engine, &OpKey::own([1; 32]), Admission::Ordinary, vec![mint]).unwrap()[0];
	set_status(&wallet.db, minted, "FINALIZED_SUCCESS", None);
	export(&wallet, coin);
	let chain = Arc::new(FakeChain::default());
	claimed_at(&chain, account(0), 6);
	observe(&wallet, &chain, 9);
	assert_eq!(export_status(&wallet, &coin), Some(ExportStatus::Claimed { stable: true, by: CLAIM }));
}

/// A coin of an earlier installation has no minting record here: it was
/// held already, and is claimed once its account is emptied.
#[test]
fn a_coin_recovered_from_an_earlier_installation_is_proven_claimed_once_gone() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let earlier = InstallationId([8; 32]);
	let coin = Asset::Coin { purse: PurseId::MAIN, installation: earlier, index: 0 };
	let record = AssetRecord { asset: coin, exponent: crate::coinage::Exponent::new(2).unwrap() };
	block_on(wallet.db.write(move |tx| adopt(tx, &[record]))).unwrap();
	export(&wallet, coin);
	let chain = Arc::new(FakeChain::default());
	let recovered = coin_key(&ENTROPY, PurseId::MAIN, earlier, 0).unwrap().1;
	claimed_at(&chain, recovered, 4);
	observe(&wallet, &chain, 9);
	assert_eq!(export_status(&wallet, &coin), Some(ExportStatus::Claimed { stable: true, by: CLAIM }));
}

/// An empty account alone is no evidence of a claim: the coin may not have
/// landed yet (F1 invariant 1).
#[test]
fn a_coin_whose_minting_is_not_final_is_not_guessed_to_be_claimed() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let exponent = [crate::coinage::Exponent::new(2).unwrap()];
	let coin = block_on(wallet.db.write(move |tx| ledger::allocate(tx, PurseId::MAIN, AssetKind::Coin, &exponent))).unwrap()[0];
	let engine = unbuilt_engine(wallet.db.clone());
	let mint = Rows { inputs: vec![wallet.coin(2)], outputs: vec![coin], tokens: Vec::new() };
	let minted = register_rows(&engine, &OpKey::own([1; 32]), Admission::Ordinary, vec![mint]).unwrap()[0];
	set_status(&wallet.db, minted, "PENDING_SUCCESS", None);
	export(&wallet, coin);
	let chain = Arc::new(FakeChain::default());
	claimed_at(&chain, account(0), 6);
	observe(&wallet, &chain, 9);
	assert_eq!(export_status(&wallet, &coin), Some(ExportStatus::NotYetMinted));
}
