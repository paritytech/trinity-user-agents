//! Spends as payments start them.

use super::*;
use crate::coinage::{
	RecycleAge,
	classification::Selectability,
	ledger::{AssetKind, CoinOnChain, Read},
};

fn coin_aged(age: u32) -> Read {
	Read::Coin { now: Some(CoinOnChain { exponent: Exponent::new(3).unwrap(), age, locked_until: None }), emptied_by: None }
}

/// Held-back value is a fallback: a spend the spendable value covers never
/// takes it, even when the user would allow it.
#[test]
fn a_spend_the_spendable_value_covers_takes_no_held_back_value() {
	let policy = PrivacyPolicy { recycle_at: RecycleAge::Age(2), max_unavailable_share: 50, ..PrivacyPolicy::balanced() };
	let wallet = TestWallet::new(policy);
	let spendable = wallet.hold(PurseId::MAIN, AssetKind::Coin, 3, coin_aged(0));
	let held_back = wallet.hold(PurseId::MAIN, AssetKind::Coin, 3, coin_aged(5));
	let held = classify(&held_back, &wallet.view(), NOW, &chain_params()).selectability;
	assert!(matches!(held, Selectability::HeldBack(_)), "{held:?}");
	let confirmed = Request::Select {
		from: PurseId::MAIN,
		amount: 8,
		to: Destination::Purse(PurseId(4)),
		initiator: Some(Initiator::User),
		confirmed: true,
	};
	let registered = started(&wallet, &key(1), confirmed).unwrap();
	let view = wallet.view();
	let inputs: Vec<Asset> =
		registered.records.iter().flat_map(|record| view.registrations().record(*record).unwrap().rows.inputs.clone()).collect();
	assert_eq!(inputs, [spendable]);
}

/// The fastest policy lets a product's spend take held-back value without
/// asking, so no privacy warning is ever raised for it.
#[test]
fn under_the_fastest_policy_a_product_spend_never_needs_a_confirmation() {
	let policy = PrivacyPolicy { recycle_at: RecycleAge::Age(0), max_unavailable_share: 100, ..PrivacyPolicy::fastest() };
	let wallet = TestWallet::new(policy);
	wallet.coin(3);
	let request = Request::Select {
		from: PurseId::MAIN,
		amount: 8,
		to: Destination::Export(PurseId::MAIN),
		initiator: Some(Initiator::Product),
		confirmed: false,
	};
	assert!(!needs_confirmation(&wallet.view(), &request, NOW, &chain_params()));
	assert!(started(&wallet, &key(1), request).is_ok(), "it spends the coin held back for recycling");
}
