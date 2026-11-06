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
		asset_decimals: 2,
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

fn row(provider_id: &str, send_amount: u128, receive_amount: u128) -> FundingQuoteRow {
	FundingQuoteRow {
		provider_id: ProductId::for_test(provider_id),
		state: FundingQuoteState::Quoted {
			quote: FundingQuote { send_amount, receive_amount, ..quote("q", None) },
		},
	}
}

// The user pays the least for what they add, and receives the most for
// what they withdraw; among equals the host's own order decides. A
// provider a session can no longer be handed to is never the best.
#[test]
fn the_best_quote_is_cheapest_in_and_most_generous_out() {
	let rows = [
		row("first.dot", 1_020, 990),
		row("second.dot", 1_010, 990),
		row("third.dot", 1_010, 980),
		FundingQuoteRow {
			provider_id: ProductId::for_test("silent.dot"),
			state: FundingQuoteState::Pending,
		},
	];

	let any = |_: &ProductId| true;

	assert_eq!(
		(
			best_quote(FundingDirection::In, &rows, any),
			best_quote(FundingDirection::Out, &rows, any),
			best_quote(FundingDirection::In, &rows[3..], any),
			best_quote(FundingDirection::In, &rows, |provider_id| provider_id.as_str() !=
				"second.dot"),
		),
		(
			Some(ProductId::for_test("second.dot")),
			Some(ProductId::for_test("first.dot")),
			None,
			Some(ProductId::for_test("third.dot")),
		)
	);
}

// A session can only be handed a live quote from the list its host was
// last shown, priced on that list's rail and asset.
#[test]
fn only_a_live_quote_the_host_was_shown_is_offered() {
	let quotes = FundingQuotes::default();
	let ask = FundingQuoteAsk {
		direction: FundingDirection::In,
		rail: FundingRail::Card,
		asset: "EUR".to_string(),
		network: None,
		amount: 1_000,
		country: None,
	};
	let ramp = ProductId::for_test("ramp.dot");
	let quoted = |quote| FundingQuoteRow {
		provider_id: ramp.clone(),
		state: FundingQuoteState::Quoted { quote },
	};
	quotes.show(
		"fs_1",
		ask,
		vec![quoted(quote("expired", Some(NOW))), quoted(quote("live", None))],
	);
	let offered =
		|intent, quote_id| quotes.offered(intent, &ramp, quote_id, NOW).map(|c| c.quote.quote_id);

	assert_eq!(
		(offered("fs_1", "expired"), offered("fs_1", "live"), offered("fs_2", "live")),
		(None, Some("live".to_string()), None)
	);
}

// A host answers whatever its platform reports; only a two-letter code
// reaches a quote ask.
#[test]
fn only_an_alpha2_code_counts_as_a_detected_country() {
	assert_eq!(
		["de", " PT ", "deu", "D1", ""].map(|raw| alpha2(raw.to_string())),
		[Some("DE".to_string()), Some("PT".to_string()), None, None, None]
	);
}
