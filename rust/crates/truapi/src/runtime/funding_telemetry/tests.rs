use super::*;
use crate::{
	host_logic::funding::{FundingChoice, test_choice},
	platform::FundingQuoteRow,
};

const NOW: u64 = 10_000_000;

fn session(direction: FundingDirection, amount: Option<u128>) -> FundingSession {
	FundingSession::new("fs_secret".to_string(), None, direction, amount, NOW - 5_000)
}

/// A top-up that delivered.
fn delivered() -> FundingSession {
	let mut delivered = session(FundingDirection::In, Some(1));
	delivered.stage = FundingStage::Delivered { credited: 1, settled_at_ms: NOW };
	delivered
}

/// One fact of each kind, and an ending of each level, so every marker is
/// checked against the registry.
fn every_fact() -> Vec<FundingFact> {
	vec![
		FundingFact::OverlayOpened { amount_preset: true },
		FundingFact::OverlayClosed {
			outcome: OverlayOutcome::Dismissed,
			furthest: Furthest::Quotes,
			overlay_ms: 4_000,
		},
		FundingFact::QuotesSettled {
			counts: QuoteCounts { asked: 2, quoted: 1, timed_out: 1, ..QuoteCounts::default() },
			region: Region::Uk,
			issue: Some(FundingAmountIssue::BelowMinimum { min: 123_456_789 }),
			superseded: false,
			first_quote_ms: Some(300),
			settle_ms: 9_000,
		},
		FundingFact::ProviderChosen {
			best: true,
			rank: 1,
			quote_count: 3,
			fee_bucket: fee_bucket(150, 10_000),
			eta_bucket: eta_bucket(Some(90)),
		},
		FundingFact::ProviderTook { taken: false, take_ms: None },
		FundingFact::StepReached {
			step: FundingStep::Received,
			since_start_ms: 1,
			since_prev_ms: 1,
		},
		FundingFact::StatusFlag { flag: Flag::TakingLonger, since_start_ms: 1 },
		FundingFact::Ended(Ending::of(&delivered(), Region::Eu, NOW)),
		FundingFact::Ended(Ending::of(&session(FundingDirection::In, Some(1)), Region::Eu, NOW)),
		FundingFact::CancelRequested {
			result: CancelOutcome::TooLate,
			status_at: "adding_to_balance",
			money_moved: true,
		},
		FundingFact::CancelAnswered {
			answer: CancelAnswer::Offered,
			fee_bucket: "<50",
			answer_ms: Some(10),
		},
		FundingFact::CancelConfirmed { result: CancelConfirmation::Lapsed },
		FundingFact::FrameRequested { target: FrameTarget::Page, replaced: false },
		FundingFact::FrameOpened { target: FrameTarget::App, wait_ms: 20 },
		FundingFact::FrameClosed {
			target: FrameTarget::App,
			outcome: FrameClose::DroppedUnopened,
			open_ms: None,
		},
	]
}

// Every fact becomes an event the registry accepts, and none carries the
// intent, an exact amount or a limit, whatever the session holds.
#[test]
fn every_fact_is_registered_and_carries_nothing_identifying() {
	let mut held = session(FundingDirection::In, Some(123_456_789));
	held.owner_product_id = Some(ProductId::for_test("someones-shop.dot"));
	held.provider_id = Some(ProductId::for_test("ramp.dot"));
	held.choice = Some(test_choice());
	let common = Common::of(Some(&held), None, |_| false, NOW);
	let events: Vec<FundingEvent> =
		every_fact().iter().map(|fact| fact.event(&common, "ref")).collect();
	for event in &events {
		assert_eq!(funding_events::check(event), Ok(()), "{:?} is refused", event.marker);
		let line = format!("{}: {}", event.marker.target, event.detail());
		for secret in ["fs_secret", "123456789", "someones-shop", "ramp.dot", "q-test"] {
			assert!(!line.contains(secret), "a line carries {secret}: {line}");
		}
	}
	assert_eq!(
		(
			events[0].tag("product"),
			events[0].tag("provider"),
			events[0].tag("amount_bucket"),
			events[0].detail(),
		),
		(
			Some("third_party"),
			Some("other"),
			Some("100-250"),
			"funding.ref=ref direction=in opener=product product=third_party rail=crypto \
			 provider=other asset=usdt network=polkadot amount_bucket=100-250 \
			 amount_preset=true age_ms=5000"
				.to_string(),
		)
	);
	let markers: Vec<&str> = events.iter().map(|event| event.marker.target).collect();
	let registered: Vec<&str> =
		funding_events::FUNDING_EVENTS.iter().map(|spec| spec.marker.target).collect();
	assert_eq!(markers, registered, "a registered event has no fact");
}

// Every fact's line fits what a host keeps of a marker's detail, with the
// longest values its keys take.
#[test]
fn every_line_fits_a_host_detail_at_its_longest() {
	let mut held = session(FundingDirection::Out, Some(u128::MAX));
	held.owner_product_id = Some(ProductId::for_test("someones-shop.dot"));
	held.provider_id = Some(ProductId::for_test(&format!("{}.dot", "p".repeat(36))));
	held.choice = Some(FundingChoice {
		asset: "a".repeat(PLAIN_ID_MAX_CHARS),
		network: Some("n".repeat(PLAIN_ID_MAX_CHARS)),
		..test_choice()
	});
	let common = Common::of(Some(&held), None, |_| true, NOW);
	let long_ms = 9_999_999_999;
	let busiest = FundingFact::QuotesSettled {
		counts: QuoteCounts {
			asked: 999,
			quoted: 999,
			refused_country: 999,
			refused_min: 999,
			refused_max: 999,
			refused_unavailable: 999,
			refused_other: 999,
			timed_out: 999,
			pending: 999,
		},
		region: Region::Unset,
		issue: Some(FundingAmountIssue::NotEnoughBalance),
		superseded: false,
		first_quote_ms: Some(long_ms),
		settle_ms: long_ms,
	};
	for fact in every_fact().into_iter().chain([busiest]) {
		let event = fact.event(&common, &"r".repeat(16));
		assert_eq!(funding_events::check(&event), Ok(()), "{}", event.detail());
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
	assert_eq!(
		[999, 1_000, 59_999, 86_400_000].map(duration_bucket),
		["<1s", "1-5s", "15-60s", "1d+"]
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
		let mut ended = session(FundingDirection::In, Some(1));
		ended.fail(reason, NOW);
		let ending = Ending::of(&ended, Region::Unset, NOW);
		(ending.outcome, ending.reason, ending.expected)
	};
	assert_eq!(
		[
			ended(FundingFailure::Declined),
			ended(FundingFailure::Expired),
			ended(FundingFailure::Cancelled),
			ended(FundingFailure::ProviderTimeout),
			ended(FundingFailure::Other {
				code: "kyc_vendor_down".into(),
				message: "Our vendor at https://example.invalid is down".into(),
			}),
			ended(FundingFailure::Other { code: "not_claimed".into(), message: String::new() }),
		],
		[
			(EndOutcome::Failed, "declined", true),
			(EndOutcome::Expired, "expired", true),
			(EndOutcome::Cancelled, "cancelled", true),
			(EndOutcome::Failed, "provider_timeout", false),
			(EndOutcome::Failed, "provider_other", false),
			(EndOutcome::Failed, "not_claimed", false),
		]
	);
	let mut delivered = session(FundingDirection::In, Some(1));
	delivered.stage = FundingStage::Delivered { credited: 1, settled_at_ms: NOW };
	let ending = Ending::of(&delivered, Region::Unset, NOW);
	assert_eq!(
		(ending.outcome, ending.expected, ending.duration_ms),
		(EndOutcome::ToppedUp, true, 5_000)
	);
}

// A change reports the end once, however often the ended session is stored
// again.
#[test]
fn a_session_ends_once() {
	let mut telemetry = Telemetry::default();
	let open = session(FundingDirection::Out, None);
	let mut ended = open.clone();
	ended.fail(FundingFailure::Declined, NOW);
	let ends = |facts: Vec<FundingFact>| {
		facts.iter().filter(|fact| matches!(fact, FundingFact::Ended(_))).count()
	};
	assert_eq!(ends(telemetry.changed(Some(&open), &ended, NOW)), 1);
	let mut acknowledged = ended.clone();
	acknowledged.acknowledged = true;
	assert_eq!(ends(telemetry.changed(Some(&ended), &acknowledged, NOW)), 0);
}

// A worker that keeps failing to take a session, then takes it, reports
// one failure and one take; taking it again reports nothing.
#[test]
fn a_worker_taking_its_session_reports_once_each_way() {
	let mut telemetry = Telemetry::default();
	let held = session(FundingDirection::In, Some(1));
	let took =
		|telemetry: &mut Telemetry, taken| telemetry.provider_took(&held, taken, NOW).is_some();
	assert_eq!(
		[
			took(&mut telemetry, false),
			took(&mut telemetry, false),
			took(&mut telemetry, true),
			took(&mut telemetry, true),
			took(&mut telemetry, false),
		],
		[true, false, true, false, false]
	);
}

// A request replaced before every provider answered settles as superseded,
// with the rows still pending counted.
#[test]
fn a_replaced_quote_request_settles_as_superseded() {
	let mut telemetry = Telemetry::default();
	assert_eq!(telemetry.quotes_requested("fs_1", 1, NOW), None);
	let pending = FundingQuoteList {
		request_id: 1,
		rows: vec![FundingQuoteRow {
			provider_id: ProductId::for_test("ramp.dot"),
			state: FundingQuoteState::Pending,
		}],
		best: None,
		refresh_at_ms: 0,
		amount: 1,
		refusal: None,
		issue: None,
		complete: false,
	};
	assert_eq!(telemetry.quotes_shown("fs_1", &pending, NOW + 10), None);
	let Some(FundingFact::QuotesSettled { counts, superseded, settle_ms, .. }) =
		telemetry.quotes_requested("fs_1", 2, NOW + 50)
	else {
		panic!("the replaced request did not settle");
	};
	assert_eq!((counts.pending, superseded, settle_ms), (1, true, 50));
}

#[test]
fn route_identifiers_are_tagged_only_when_plain() {
	assert_eq!(
		[None, Some("USDT"), Some("polkadot-asset-hub"), Some("a b"), Some("https://x")]
			.map(plain_tag),
		["none", "usdt", "other", "other", "other"]
	);
}
