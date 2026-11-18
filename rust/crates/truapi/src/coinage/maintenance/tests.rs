
use futures::executor::block_on;

use super::*;
use crate::coinage::{
	PrivacyPolicy,
	ledger::{AssetKind, CoinOnChain, EntryOnChain},
	testing::{ENTROPY, NOW, TestWallet, chain_params, unbuilt_engine},
};

const DAY: u64 = 86_400;

fn run(wallet: &TestWallet) -> Due {
	let engine = unbuilt_engine(wallet.db.clone());
	let params = chain_params();
	let launcher = Launcher { engine: &engine, params: &params, entropy: &ENTROPY };
	block_on(run_due(&launcher, &wallet.db, NOW)).unwrap()
}

fn expiring_entry(expires_at: u64) -> Read {
	Read::Entry(Some(EntryOnChain {
		ring: 2,
		revision: 1,
		page: 0,
		position: 0,
		members: 10,
		included: 10,
		closed: true,
		expires_at: Some(expires_at),
		locked_until: None,
		unloaded: None,
	}))
}

fn old_coin(exponent: u8, age: u32) -> Read {
	Read::Coin { now: Some(CoinOnChain { exponent: Exponent::new(exponent).unwrap(), age, locked_until: None }), emptied_by: None }
}

#[test]
fn an_entry_within_the_rescue_margin_is_rescued_once() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	wallet.hold(PurseId::MAIN, AssetKind::Entry, 4, expiring_entry(NOW + DAY));
	let first = run(&wallet);
	assert_eq!(first.started.len(), 1);
	assert!(first.started[0].is_own());
	assert!(first.cannot_tell.is_empty());
	assert!(run(&wallet).started.is_empty(), "the rescue now holds the entry");
}

#[test]
fn a_coin_at_the_age_cutoff_is_recycled() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	wallet.hold(PurseId::MAIN, AssetKind::Coin, 3, old_coin(3, 14));
	wallet.hold(PurseId::MAIN, AssetKind::Coin, 3, old_coin(3, 2));
	let due = run(&wallet);
	assert_eq!(due.started.len(), 1, "only the coin at the cutoff");
}

#[test]
fn an_unobserved_asset_makes_the_sweep_say_it_cannot_tell() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let outputs = [Exponent::new(2).unwrap()];
	block_on(wallet.db.write(move |tx| ledger::allocate(tx, PurseId::MAIN, AssetKind::Entry, &outputs))).unwrap();
	let due = run(&wallet);
	assert_eq!(due.cannot_tell, [PurseId::MAIN]);
	assert_eq!(due.call_again_within, Some(Duration::ZERO));
}

#[test]
fn the_host_may_wait_until_the_next_ring_reaches_its_rescue_margin() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	wallet.hold(PurseId::MAIN, AssetKind::Entry, 4, expiring_entry(NOW + 30 * DAY));
	let due = run(&wallet);
	assert!(due.started.is_empty());
	assert_eq!(due.call_again_within, Some(Duration::from_secs(23 * DAY)));
}
