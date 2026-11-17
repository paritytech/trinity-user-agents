//! Balances as a wallet shows them.

use super::*;
use crate::coinage::{RecycleAge, testing::TestWallet};

#[test]
fn an_empty_purse_has_nothing_in_any_bucket() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	assert_eq!(balance(&wallet.view(), PurseId::MAIN, NOW, &chain_params()), Balance::default());
}

/// A coin the recycling sweep takes is unavailable on purpose, to gain
/// privacy: held back, not spendable and not arriving.
#[test]
fn a_coin_the_recycling_sweep_takes_counts_as_held_back() {
	let policy = PrivacyPolicy { recycle_at: RecycleAge::Age(2), max_unavailable_share: 50, ..PrivacyPolicy::fastest() };
	let wallet = TestWallet::new(policy);
	wallet.hold(PurseId::MAIN, AssetKind::Coin, 3, coin(3, 0, None));
	wallet.hold(PurseId::MAIN, AssetKind::Coin, 3, coin(3, 5, None));
	let held = balance(&wallet.view(), PurseId::MAIN, NOW, &chain_params());
	assert_eq!(held, Balance { spendable: 8, held_back: 8, arriving: 0 });
}
