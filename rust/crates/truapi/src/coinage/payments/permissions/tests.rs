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
		requirements(SHOP, PurseId::MAIN, None, Action::Spend, Surface::Rfc6),
		vec![purse_permission(PurseId::MAIN, CoinPaymentPurseAction::Spend), Requirement::ConfirmSpend],
		"RFC-6 still needs access to the main purse"
	);
}

/// RFC-6 asks for its balance access before it reads the main purse, so a
/// second permission for the same fact would ask the user twice.
#[test]
fn reading_the_main_purse_through_rfc6_needs_only_its_balance_access() {
	assert_eq!(
		requirements(SHOP, PurseId::MAIN, None, Action::Query, Surface::Rfc6),
		vec![Requirement::Permission(RemotePermission::BalanceAccess)]
	);
	assert_eq!(
		requirements(SHOP, PurseId::MAIN, None, Action::Query, Surface::Rfc17),
		vec![COIN_PAYMENT, purse_permission(PurseId::MAIN, CoinPaymentPurseAction::Query)],
		"RFC-17 reads the main purse with its own permissions"
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
