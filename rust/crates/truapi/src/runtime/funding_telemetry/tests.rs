use super::{read::*, *};
use crate::{
	host_logic::funding::test_choice,
	logging::{CORE_EVENT_MAX_ATTRIBUTES, CoreAttribute, CoreAttributeValue},
	platform::FundingAmountIssue,
};

const NOW: u64 = 10_000_000;

fn session(direction: truapi::latest::FundingDirection, amount: Option<u128>) -> FundingSession {
	FundingSession::new("fs_secret".to_string(), None, direction, amount, NOW - 5_000)
}

fn top_up() -> FundingSession {
	session(truapi::latest::FundingDirection::In, Some(1))
}

/// Facts of every shape, built the way the core builds them.
fn facts() -> Vec<CoreEvent> {
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
		CoreEvent::new(markers::FUNDING_OVERLAY_OPENED).flag("amount_preset", true),
		telemetry.quotes_superseded("fs_secret", settled).expect("tracked"),
		telemetry.changed(Some(&top_up()), &delivered, NOW).pop().expect("ended"),
		telemetry.changed(Some(&top_up()), &failed, NOW).pop().expect("ended"),
		telemetry.provider_took(&top_up(), false, NOW).expect("first"),
	]
}

// A fact goes out with its own attributes, then `funding.ref`, the session's
// labels and its age, and none carries the intent, an exact amount, a limit,
// an id or a provider's text, whatever the session holds.
#[test]
fn a_fact_carries_closed_values_only() {
	let mut held = session(truapi::latest::FundingDirection::In, Some(123_456_789));
	held.owner_product_id = Some(ProductId::for_test("someones-shop.dot"));
	held.provider_id = Some(ProductId::for_test("ramp.dot"));
	held.choice = Some(test_choice());
	let common = Common::of(Some(&held), None, |_| false, None, NOW);
	let events: Vec<CoreEvent> = facts().into_iter().map(|fact| common.attach(fact, "ref")).collect();
	for event in &events {
		let line = line(event);
		for secret in ["fs_secret", "123456789", "someones-shop", "ramp.dot", "q-test", "https"] {
			assert!(!line.contains(secret), "a fact carries {secret}: {line}");
		}
		assert!(event.attributes().len() < CORE_EVENT_MAX_ATTRIBUTES, "{line}");
	}
	let attribute = |key: &str, value: Option<CoreAttributeValue>| CoreAttribute {
		key: key.to_string(),
		value: value.expect("a value"),
	};
	assert_eq!(
		events[0].attributes(),
		[
			attribute("amount_preset", flag(true)),
			attribute("funding.ref", text("ref")),
			attribute("direction", text("in")),
			attribute("opener", text("product")),
			attribute("product", text("third_party")),
			attribute("rail", text("crypto")),
			attribute("provider", text("other")),
			attribute("asset", text("usdt")),
			attribute("network", text("polkadot")),
			attribute("amount_bucket", text("100-250")),
			attribute("age_ms", count(5_000)),
		]
	);
	assert_eq!(
		events.iter().map(CoreEvent::marker).collect::<Vec<_>>(),
		[
			markers::FUNDING_OVERLAY_OPENED,
			markers::FUNDING_QUOTES_SETTLED,
			markers::FUNDING_ENDED,
			markers::FUNDING_ENDED_UNEXPECTEDLY,
			markers::FUNDING_PROVIDER_TOOK,
		]
	);
	assert_eq!(
		(
			value(&events[1], "issue"),
			value(&events[1], "superseded"),
			value(&events[1], "first_quote_ms"),
			value(&events[3], "reason"),
			value(&events[3], "money_moved"),
		),
		(text("below_minimum"), flag(false), count(3), text("provider_other"), flag(false))
	);
}

// A measure nobody knows is left off rather than sent as zero.
#[test]
fn an_unknown_measure_is_left_off() {
	let took = Telemetry::default().provider_took(&top_up(), false, NOW).expect("first");
	assert_eq!((value(&took, "outcome"), value(&took, "take_ms")), (text("not_taken"), None));
}

// The table gates both ways out of the core on registration, so a funding
// marker it does not list would reach no host at all. Every marker a fact goes
// out under is local, reaching a host's own log and nothing past it, bar an
// unexpected ending at error so it can be alerted on.
#[test]
fn only_a_funding_fault_leaves_the_device() {
	use crate::markers::{MarkerClass, Severity};
	let funding: Vec<_> =
		markers::MARKERS.iter().filter(|marker| marker.target.starts_with("truapi.funding.")).collect();
	assert_eq!(funding.len(), 15);
	for marker in funding {
		let expected = if *marker == markers::FUNDING_ENDED_UNEXPECTEDLY {
			MarkerClass::Export { severity: Severity::Error }
		} else {
			MarkerClass::Local
		};
		assert_eq!(marker.class, expected, "{}", marker.target);
	}
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
		let event = Common::of(None, None, |_| false, None, NOW).attach(fact, "ref");
		let [outcome, reason] = ["outcome", "reason"].map(|key| value(&event, key));
		(outcome, reason, event.marker() == markers::FUNDING_ENDED)
	};
	let some = text;
	assert_eq!(
		[
			ended(FundingFailure::Declined),
			ended(FundingFailure::BelowMinimum),
			ended(FundingFailure::AboveMaximum),
			ended(FundingFailure::WrongAssetOrChain),
			ended(FundingFailure::Expired),
			ended(FundingFailure::Cancelled),
			ended(FundingFailure::Refunded),
			ended(FundingFailure::ProviderTimeout),
			ended(FundingFailure::Other { code: "not_claimed".into(), message: String::new() }),
		],
		[
			(some("failed"), some("declined"), true),
			(some("failed"), some("below_minimum"), true),
			(some("failed"), some("above_maximum"), true),
			(some("failed"), some("wrong_asset_or_chain"), true),
			(some("expired"), some("expired"), true),
			(some("cancelled"), some("cancelled"), true),
			(some("refunded"), some("refunded"), true),
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
	let ends = |facts: Vec<CoreEvent>| {
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

// A route's asset or network is a string its manifest wrote, so only a
// known id is tagged as it is.
#[test]
fn route_identifiers_are_tagged_only_when_known() {
	assert_eq!(
		[None, Some("USDT"), Some("dot"), Some("ramp-token"), Some("a b"), Some("https://x")]
			.map(|asset| known_tag(asset, KNOWN_ASSETS)),
		["none", "usdt", "dot", "other", "other", "other"]
	);
	assert_eq!(
		[Some("polkadot-asset-hub"), Some("Paseo"), Some("someones-chain")]
			.map(|network| known_tag(network, KNOWN_NETWORKS)),
		["polkadot-asset-hub", "paseo", "other"]
	);
}

// A fact arriving for a session whose entry went, once enough others
// retired after it, emits nothing and leaves no entry behind.
#[test]
fn a_session_gone_from_telemetry_stays_gone() {
	let mut telemetry = Telemetry::default();
	let first = telemetry.session_ref("fs_old").expect("tracked");
	telemetry.forget("fs_old");
	let retired = telemetry.session_ref("fs_old");
	for index in 0..RETIRED_KEPT {
		telemetry.forget(&format!("fs_{index}"));
	}
	let mut late = top_up();
	late.intent = "fs_old".to_string();

	assert_eq!(
		(
			retired,
			telemetry.session_ref("fs_old"),
			telemetry.provider_took(&late, false, NOW).is_some(),
			telemetry.changed(None, &late, NOW).len(),
			telemetry.sessions.contains_key("fs_old"),
		),
		(Some(first), None, false, 0, false)
	);
}
