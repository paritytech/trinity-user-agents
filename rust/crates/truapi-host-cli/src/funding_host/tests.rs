use super::*;

// The worker is told a top-up started only once the ledger holds it, as a
// start this host forgets would read as unknown after a restart and never
// settle.
#[test]
fn a_start_the_ledger_cannot_keep_is_refused() {
	let directory = tempfile::tempdir().expect("temporary directory");
	let host = CliPaymentHost { ledger: directory.path().join("missing").join("ledger.jsonl") };
	let product = ProductContext::new_with_execution(
		"ramp.dot".parse().expect("valid product"),
		truapi::platform::ExecutableKind::Worker,
	);

	let started = futures::executor::block_on(host.top_up(
		&product,
		HostPaymentTopUpRequest {
			into: None,
			amount: 1,
			source: truapi::latest::PaymentTopUpSource::ProductAccount {
				derivation_index: truapi::latest::DerivationIndex::Index(0),
			},
			id: [1; 32],
			funding: None,
		},
	));

	assert!(matches!(started, Err(HostPaymentTopUpError::Unknown { .. })), "{started:?}");
}

// A run picks its route as a user would, and a value it cannot read stops
// the host rather than falling back to the defaults.
#[test]
fn a_scripted_route_names_rail_network_token_or_currency_and_country() {
	let crypto = ScriptedRoute::parse("crypto:ethereum:USDC").expect("crypto route");
	assert_eq!(crypto.rail, FundingRail::Crypto);
	assert_eq!(crypto.network.as_deref(), Some("ethereum"));
	assert_eq!(crypto.token.as_deref(), Some("USDC"));

	let card = ScriptedRoute::parse("card:EUR:DE").expect("card route");
	assert_eq!(card.rail, FundingRail::Card);
	assert_eq!(card.currency.as_deref(), Some("EUR"));
	assert_eq!(card.country.as_deref(), Some("DE"));

	let bank = ScriptedRoute::parse("bank").expect("bank route");
	assert_eq!((bank.rail, bank.currency), (FundingRail::Bank, None));

	assert!(ScriptedRoute::parse("wire:EUR").is_err());
	assert!(ScriptedRoute::parse("crypto:ethereum:USDC:extra").is_err());
}
