use super::*;

const SHOP: &[u8] = b"shop";
const PURSE: PurseId = PurseId(0);
const COIN_PAYMENT: Requirement = Requirement::Permission(RemotePermission::CoinPayment);

fn purse_permission(purse: PurseId, action: CoinPaymentPurseAction) -> Requirement {
	Requirement::Permission(RemotePermission::CoinPaymentPurse { purse: purse.0, action })
}

#[test]
fn every_call_needs_the_coin_payment_permission_except_rfc6_on_the_main_purse() {
	assert_eq!(requirements(SHOP, PURSE, Some(SHOP), Action::Query, Surface::Rfc17), vec![COIN_PAYMENT]);
	assert_eq!(requirements(SHOP, PurseId::MAIN, None, Action::TopUp, Surface::Rfc6), vec![]);
	assert_eq!(
		requirements(SHOP, PurseId::MAIN, None, Action::Query, Surface::Rfc6),
		vec![purse_permission(PurseId::MAIN, CoinPaymentPurseAction::Query)],
		"RFC-6 still needs access to the main purse"
	);
}

#[test]
fn the_creator_queries_and_receives_freely_but_confirms_every_spend() {
	assert_eq!(requirements(SHOP, PURSE, Some(SHOP), Action::Receive, Surface::Rfc17), vec![COIN_PAYMENT]);
	assert_eq!(
		requirements(SHOP, PURSE, Some(SHOP), Action::Spend, Surface::Rfc17),
		vec![COIN_PAYMENT, Requirement::ConfirmSpend]
	);
}

#[test]
fn another_products_purse_needs_a_grant_for_that_purse_and_action() {
	assert_eq!(
		requirements(SHOP, PURSE, Some(b"other"), Action::Spend, Surface::Rfc17),
		vec![COIN_PAYMENT, purse_permission(PURSE, CoinPaymentPurseAction::Spend), Requirement::ConfirmSpend]
	);
	assert_eq!(requirements(SHOP, PURSE, Some(b"other"), Action::TopUp, Surface::Rfc17), vec![COIN_PAYMENT]);
}
