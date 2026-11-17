use super::*;
use crate::{host_logic::funding::test_choice, platform::FundingAmountIssue};

const NOW: u64 = 10_000_000;

fn session(direction: truapi::latest::FundingDirection, amount: Option<u128>) -> FundingSession {
	FundingSession::new("fs_secret".to_string(), None, direction, amount, NOW - 5_000)
}

fn top_up() -> FundingSession {
	session(truapi::latest::FundingDirection::In, Some(1))
}

/// Facts of every shape, built the way the core builds them.
fn facts() -> Vec<FundingFact> {
	let mut telemetry = Telemetry::default();
	let mut delivered = top_up();
	delivered.stage = FundingStage::Delivered { credited: 1, settled_at_ms: NOW };
	let mut failed = top_up();
	failed.fail(FundingFailure::Other { code: "x".into(), message: "see https://x".into() }, NOW);
	let list = FundingQuoteList {
		request_id: 1,
		rows: Vec::new(),
		best: None,
		refresh_at_ms: 0,
		amount: 1,
		refusal: None,
		issue: Some(FundingAmountIssue::BelowMinimum { min: 123_456_789 }),
		complete: true,
	};
	let settled = QuotesSettled { list, superseded: false, first_quote_ms: Some(3), settle_ms: 9 };
	vec![
		FundingFact::new(markers::FUNDING_OVERLAY_OPENED).flag("amount_preset", true),
		telemetry.quotes_superseded("fs_secret", settled),
		telemetry.changed(Some(&top_up()), &delivered, NOW).pop().expect("ended"),
		telemetry.changed(Some(&top_up()), &failed, NOW).pop().expect("ended"),
		telemetry.provider_took(&top_up(), false, NOW).expect("first"),
	]
}

// Facts log as `funding.ref`, the session's tags, the fact's own tags, its
// age, then its measures, and no line carries the intent, an exact amount, a
// limit, an id or a provider's text, whatever the session holds.
#[test]
fn a_line_carries_closed_values_only() {
	let mut held = session(truapi::latest::FundingDirection::In, Some(123_456_789));
	held.owner_product_id = Some(ProductId::for_test("someones-shop.dot"));
	held.provider_id = Some(ProductId::for_test("ramp.dot"));
	held.choice = Some(test_choice());
	let common = Common::of(Some(&held), None, |_| false, NOW);
	let events: Vec<FundingEvent> = facts().iter().map(|fact| fact.event(&common, "ref")).collect();
	for event in &events {
		let line = format!("{}: {}", event.marker.target, event.detail);
		for secret in ["fs_secret", "123456789", "someones-shop", "ramp.dot", "q-test", "https"] {
			assert!(!line.contains(secret), "a line carries {secret}: {line}");
		}
	}
	assert_eq!(
		events[0].detail,
		"funding.ref=ref direction=in opener=product product=third_party rail=crypto \
		 provider=other asset=usdt network=polkadot amount_bucket=100-250 \
		 amount_preset=true age_ms=5000"
	);
	assert_eq!(
		events.iter().map(|event| event.marker).collect::<Vec<_>>(),
		[
			markers::FUNDING_OVERLAY_OPENED,
			markers::FUNDING_QUOTES_SETTLED,
			markers::FUNDING_ENDED,
			markers::FUNDING_ENDED_UNEXPECTEDLY,
			markers::FUNDING_PROVIDER_TOOK,
		]
	);
	assert_eq!(
		(events[1].value("issue"), events[3].value("reason")),
		(Some("below_minimum"), Some("provider_other"))
	);
}

// Each fact logs under its own marker, at the severity of the marker's
// class, and reaches the host's sink as that marker with its detail.
#[test]
fn a_fact_reaches_the_host_sink_as_its_marker() {
	let event = FundingEvent {
		marker: markers::FUNDING_ENDED_UNEXPECTEDLY,
		detail: "funding.ref=ref outcome=failed".to_string(),
	};
	let lines = crate::logging::testing::host_sink_lines(|| event.log());
	assert_eq!(lines, ["truapi.funding.ended_unexpectedly funding.ref=ref outcome=failed"]);
}

#[test]
fn amounts_fees_and_times_are_bucketed() {
	let cash = |whole: u128| Some(whole * CASH_UNIT);
	assert_eq!(
		[None, cash(9), cash(10), cash(99), cash(250), cash(999), cash(4_999), cash(5_000)]
			.map(amount_bucket),
		["unset", "<10", "10-50", "50-100", "250-500", "500-1k", "1k-5k", "5k+"]
	);
	assert_eq!(
		[(0, 0), (49, 10_000), (50, 10_000), (199, 10_000), (499, 10_000), (1, 1)]
			.map(|(fee, of)| fee_bucket(fee, of)),
		["none", "<50", "50-100", "100-200", "200-500", "500+"]
	);
	assert_eq!(
		[None, Some(59), Some(60), Some(3_599), Some(3_600), Some(86_400)].map(eta_bucket),
		["unknown", "<1m", "1-10m", "10-60m", "1-24h", "1d+"]
	);
}

#[test]
fn a_country_is_only_its_region() {
	assert_eq!(
		[None, Some("de"), Some("GB"), Some("US"), Some("CH"), Some("BR")].map(Region::of),
		[Region::Unset, Region::Eu, Region::Uk, Region::Us, Region::Other, Region::Other]
	);
}

// Only the user's or the rules' endings count as expected; a provider's
// own failure, or one with money moved that ran out of time, is a fault.
#[test]
fn endings_say_whether_they_were_expected() {
	let ended = |reason: FundingFailure| {
		let mut ended = top_up();
		ended.fail(reason, NOW);
		let fact = Telemetry::default().changed(Some(&top_up()), &ended, NOW).pop().expect("ended");
		let event = fact.event(&Common::of(None, None, |_| false, NOW), "ref");
		let [outcome, reason] = ["outcome", "reason"].map(|key| event.value(key).map(String::from));
		(outcome, reason, event.marker == markers::FUNDING_ENDED)
	};
	let some = |value: &str| Some(value.to_string());
	assert_eq!(
		[
			ended(FundingFailure::Declined),
			ended(FundingFailure::Expired),
			ended(FundingFailure::Cancelled),
			ended(FundingFailure::ProviderTimeout),
			ended(FundingFailure::Other { code: "not_claimed".into(), message: String::new() }),
		],
		[
			(some("failed"), some("declined"), true),
			(some("expired"), some("expired"), true),
			(some("cancelled"), some("cancelled"), true),
			(some("failed"), some("provider_timeout"), false),
			(some("failed"), some("not_claimed"), false),
		]
	);
}

// A change logs the end once, however often the ended session is stored
// again.
#[test]
fn a_session_ends_once() {
	let mut telemetry = Telemetry::default();
	let open = top_up();
	let mut ended = open.clone();
	ended.fail(FundingFailure::Declined, NOW);
	let ends = |facts: Vec<FundingFact>| {
		facts.iter().filter(|fact| fact.marker() == markers::FUNDING_ENDED).count()
	};
	assert_eq!(ends(telemetry.changed(Some(&open), &ended, NOW)), 1);
	let mut acknowledged = ended.clone();
	acknowledged.acknowledged = true;
	assert_eq!(ends(telemetry.changed(Some(&ended), &acknowledged, NOW)), 0);
}

// A worker that keeps failing to take a session, then takes it, logs one
// failure and one take; taking it again logs nothing.
#[test]
fn a_worker_taking_its_session_logs_once_each_way() {
	let mut telemetry = Telemetry::default();
	let held = top_up();
	let mut took = |taken| telemetry.provider_took(&held, taken, NOW).is_some();
	assert_eq!(
		[took(false), took(false), took(true), took(true), took(false)],
		[true, false, true, false, false]
	);
}

#[test]
fn route_identifiers_are_tagged_only_when_plain() {
	assert_eq!(
		[None, Some("USDT"), Some("polkadot-asset-hub"), Some("a b"), Some("https://x")]
			.map(plain_tag),
		["none", "usdt", "other", "other", "other"]
	);
}
