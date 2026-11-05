use truapi::latest::{FundingDirection, FundingRail};

use super::*;

const NOW: u64 = 1_700_000_000_000;

fn quote(quote_id: &str, expires_at: Option<u64>) -> FundingQuote {
	FundingQuote {
		quote_id: quote_id.to_string(),
		send_amount: 1_020,
		receive_amount: 1_000,
		provider_fee: 20,
		network_fee: 0,
		eta_secs: None,
		expires_at,
	}
}

// Refusing the country of an ask that named none says nothing about
// where the provider serves, so it must not be learned as a refusal
// everywhere, which would hide the provider from every user.
#[test]
fn only_an_ask_naming_a_country_learns_a_country_refusal() {
	let refused = FundingQuoteAnswer::Refused { reason: FundingQuoteRefusal::CountryUnsupported };

	assert_eq!(
		(shows_support(&refused, true), shows_support(&refused, false)),
		(Some(false), None)
	);
}

// A host asks again each time the user edits the amount, so a session's
// expired quotes are dropped rather than piling up while its overlay is
// open.
#[test]
fn offering_a_quote_drops_the_expired_ones() {
	let quotes = FundingQuotes::default();
	let ask = FundingQuoteAsk {
		direction: FundingDirection::In,
		rail: FundingRail::Card,
		asset: "EUR".to_string(),
		amount: 1_000,
		country: None,
	};
	quotes.offer(
		"fs_1",
		&ProductId::for_test("ramp.dot"),
		quote("expired", Some(NOW)),
		&ask,
		NOW - 1,
	);
	quotes.offer("fs_1", &ProductId::for_test("ramp.dot"), quote("live", None), &ask, NOW);

	let kept: Vec<String> = lock(&quotes.offered)
		.values()
		.flatten()
		.map(|choice| choice.quote.quote_id.clone())
		.collect();
	assert_eq!(kept, vec!["live".to_string()]);
}
