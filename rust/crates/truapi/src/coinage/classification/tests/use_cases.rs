//! Asset states as balances and payment statuses read them.

use super::*;

fn export(wallet: &Wallet, coin: Asset) {
	block_on(wallet.db.write(move |tx| mark_disclosed(tx, &[coin]))).unwrap().unwrap();
}

/// A minting not yet built is not yet a coin anyone could claim, and the
/// export status stream keeps going.
#[test]
fn an_exported_coin_whose_minting_waits_to_be_built_is_not_yet_minted() {
	let mut wallet = Wallet::new(PrivacyPolicy::fastest());
	let coin = wallet.allocate(AssetKind::Coin, 2);
	wallet.register(mint(coin), "PENDING_SUBMISSION", None);
	export(&wallet, coin);
	assert_eq!(wallet.state(&coin).export, Some(ExportStatus::NotYetMinted));
}

/// The payee may have claimed the coin, or it may not have landed yet; an
/// empty account alone does not tell (F1 invariant 1).
#[test]
fn an_exported_coin_nothing_has_seen_is_not_guessed_to_be_claimed() {
	let mut wallet = Wallet::new(PrivacyPolicy::fastest());
	let coin = wallet.allocate(AssetKind::Coin, 2);
	wallet.register(mint(coin), "PENDING_SUCCESS", None);
	export(&wallet, coin);
	wallet.observe(coin, Read::Coin { now: None, emptied_by: None }, NOW);
	assert_eq!(wallet.state(&coin).export, Some(ExportStatus::NotYetMinted));
}

#[test]
fn an_exported_coin_the_chain_still_holds_awaits_its_payee_however_old_the_read() {
	let wallet = Wallet::new(PrivacyPolicy::fastest());
	let coin = wallet.coin(2, 0);
	export(&wallet, coin);
	assert_eq!(wallet.state(&coin).export, Some(ExportStatus::AwaitingPayee));
}

/// The balance never counts a coin before F1 has seen it, even once its
/// minting is final (P9).
#[test]
fn a_coin_whose_minting_finalized_before_any_read_is_still_arriving() {
	let mut wallet = Wallet::new(PrivacyPolicy::fastest());
	let coin = wallet.allocate(AssetKind::Coin, 2);
	wallet.register(mint(coin), "FINALIZED_SUCCESS", None);
	assert_eq!(wallet.state(&coin).selectability, Selectability::Arriving(Arriving::NeverObserved));
}

/// The fastest policy asks no ring size and no dwell of an entry, only that
/// its key is included.
#[test]
fn under_the_fastest_policy_an_included_entry_is_spendable_whatever_its_ring() {
	let wallet = Wallet::new(PrivacyPolicy::fastest());
	let entry = wallet.allocate(AssetKind::Entry, 4);
	wallet.observe(entry, in_ring(1, 1, false, None), NOW);
	assert_eq!(wallet.state(&entry).selectability, Selectability::Now);
}

/// The most private policy wants 90 percent of 767 members, 691; a ring of
/// 690 holds the entry back until its 32-member rule's ten minutes pass.
#[test]
fn under_the_most_private_policy_an_entry_in_a_part_filled_ring_is_held_back() {
	let wallet = Wallet::new(PrivacyPolicy::most_private());
	let entry = wallet.allocate(AssetKind::Entry, 4);
	wallet.observe(entry, in_ring(690, 690, false, None), NOW);
	assert_eq!(wallet.state(&entry).selectability, Selectability::HeldBack(HeldBack::Dwell));
	let later = classify(&entry, &block_on(wallet.db.read(snapshot)).unwrap(), NOW + 600, &chain_params());
	assert_eq!(later.selectability, Selectability::Now);
}
