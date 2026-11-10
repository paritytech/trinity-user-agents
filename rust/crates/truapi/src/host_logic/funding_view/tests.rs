use truapi::latest::{FundingDirection, FundingFailure, FundingPayout, FundingQuote, FundingRail};

use super::*;
use crate::{
	host_logic::{
		funding::{FundingChoice, FundingSessionSummary, FundingStage},
		worker_manifest::FundingRoute,
	},
	platform::ProductId,
};

fn route(rail: FundingRail, assets: &[&str], networks: Option<&[&str]>) -> FundingRoute {
	FundingRoute {
		rail,
		directions: vec![FundingDirection::In],
		assets: assets.iter().map(|asset| asset.to_string()).collect(),
		networks: networks.map(|networks| networks.iter().map(|n| n.to_string()).collect()),
		countries: None,
	}
}

fn candidate(name: &str, routes: Vec<FundingRoute>) -> FundingCandidate {
	FundingCandidate {
		provider_id: ProductId::for_test(name),
		routes,
		unsupported: Vec::new(),
		limits: Vec::new(),
		brand: None,
	}
}

fn row(name: &str, state: FundingQuoteState) -> FundingQuoteRow {
	FundingQuoteRow { provider_id: ProductId::for_test(name), state }
}

fn refused(reason: FundingQuoteRefusal) -> FundingQuoteState {
	FundingQuoteState::Unavailable { reason: FundingQuoteUnavailable::Refused { reason } }
}

fn quote(send: u128, receive: u128, provider_fee: u128, network_fee: u128) -> FundingQuote {
	FundingQuote {
		quote_id: "q".to_string(),
		send_amount: send,
		receive_amount: receive,
		provider_fee,
		network_fee,
		eta_secs: None,
		expires_at: None,
		asset_decimals: 2,
	}
}

// Divergence "Rails shown": only routed rails, while a provider that
// declares no routes may serve any.
#[test]
fn only_routed_rails_are_offered() {
	let cases: [(Vec<FundingCandidate>, Vec<FundingRail>); 3] = [
		(
			vec![candidate("ramp.dot", vec![route(FundingRail::Bank, &["EUR"], None)])],
			vec![FundingRail::Bank],
		),
		(
			vec![candidate("any.dot", Vec::new())],
			vec![FundingRail::Crypto, FundingRail::Card, FundingRail::Bank],
		),
		(Vec::new(), Vec::new()),
	];
	for (candidates, expected) in cases {
		assert_eq!(routed_rails(&candidates), expected);
	}
}

// Divergence "Who is asked for a quote": a provider is asked only for a
// rail it routes, or when it declares no routes.
#[test]
fn only_providers_routing_the_rail_are_asked() {
	let card = candidate("ramp.dot", vec![route(FundingRail::Card, &["EUR"], None)]);
	let open = candidate("any.dot", Vec::new());
	assert_eq!(
		(
			routes_rail(&card, FundingRail::Card),
			routes_rail(&card, FundingRail::Bank),
			routes_rail(&open, FundingRail::Bank),
		),
		(true, false, true)
	);
}

#[test]
fn the_fiat_asset_follows_the_users_currency_when_taken() {
	let eur = [candidate("ramp.dot", vec![route(FundingRail::Card, &["EUR", "USD"], None)])];
	let cases: [(&[FundingCandidate], Option<&str>, &str); 5] = [
		(&eur, Some("usd"), "usd"),
		(&eur, Some("GBP"), "EUR"),
		(&eur, None, "EUR"),
		(&[], Some("GBP"), "GBP"),
		(&[], None, "USD"),
	];
	for (candidates, currency, expected) in cases {
		assert_eq!(fiat_asset(candidates, FundingRail::Card, currency), expected);
	}
}

#[test]
fn tokens_are_listed_once_per_network() {
	let candidates = [
		candidate(
			"a.dot",
			vec![route(FundingRail::Crypto, &["USDT", "USDC"], Some(&["polkadot"]))],
		),
		candidate("b.dot", vec![route(FundingRail::Crypto, &["USDT", "ETH"], Some(&["ethereum"]))]),
		candidate("c.dot", vec![route(FundingRail::Crypto, &["DOT"], None)]),
	];
	assert_eq!(crypto_tokens(&candidates, Some("polkadot")), ["USDT", "USDC", "DOT"]);
	assert_eq!(crypto_tokens(&candidates, None), ["USDT", "USDC", "ETH", "DOT"]);
}

#[test]
fn a_shared_reason_for_no_quote_is_given_once() {
	let timeout = FundingQuoteState::Unavailable { reason: FundingQuoteUnavailable::Timeout };
	let other =
		|message: &str| refused(FundingQuoteRefusal::Other { message: message.to_string() });
	let refusal = |reason| Some(FundingQuoteUnavailable::Refused { reason });
	let cases: [(Vec<FundingQuoteRow>, Option<FundingQuoteUnavailable>); 6] = [
		(
			vec![row("a.dot", timeout.clone()), row("b.dot", timeout.clone())],
			Some(FundingQuoteUnavailable::Timeout),
		),
		(
			vec![row("a.dot", refused(FundingQuoteRefusal::CountryUnsupported))],
			refusal(FundingQuoteRefusal::CountryUnsupported),
		),
		(
			vec![row("a.dot", other("KYC needed"))],
			refusal(FundingQuoteRefusal::Other { message: "KYC needed".to_string() }),
		),
		(vec![row("a.dot", other("  "))], None),
		(vec![row("a.dot", timeout.clone()), row("b.dot", FundingQuoteState::Pending)], None),
		(
			vec![
				row("a.dot", timeout),
				row("b.dot", refused(FundingQuoteRefusal::CountryUnsupported)),
			],
			None,
		),
	];
	for (rows, expected) in cases {
		assert_eq!(quotes_refusal(&rows), expected, "{rows:?}");
	}
}

// Divergence "Amount issue": the issue includes every provider refusing
// this exact amount, after the balance and before the limits.
#[test]
fn the_amount_issue_includes_every_provider_refusing_the_amount() {
	let rows = [
		row("a.dot", refused(FundingQuoteRefusal::BelowMinimum { min: 30 })),
		row("b.dot", refused(FundingQuoteRefusal::BelowMinimum { min: 20 })),
		row("c.dot", refused(FundingQuoteRefusal::AboveMaximum { max: 5 })),
	];
	let refused_all = quotes_issue(&rows);
	assert_eq!(refused_all, Some(FundingAmountIssue::BelowMinimum { min: 20 }));
	let limits = FundingAmountLimits { min: Some(10), max: Some(1_000) };
	let out = FundingDirection::Out;
	let cases = [
		(
			out,
			Some((15, refused_all.as_ref())),
			15,
			Some(10),
			Some(FundingAmountIssue::NotEnoughBalance),
		),
		(
			out,
			Some((15, refused_all.as_ref())),
			15,
			None,
			Some(FundingAmountIssue::BelowMinimum { min: 20 }),
		),
		(out, Some((16, refused_all.as_ref())), 15, None, None),
		(out, None, 5, None, Some(FundingAmountIssue::BelowMinimum { min: 10 })),
		(out, None, 0, None, None),
		(out, None, 2_000, None, Some(FundingAmountIssue::AboveMaximum { max: 1_000 })),
		(FundingDirection::In, None, 15, Some(10), None),
	];
	for (direction, quoted, amount, spendable, expected) in cases {
		assert_eq!(amount_issue(direction, &limits, quoted, amount, spendable), expected);
	}
	let maxes = [
		row("a.dot", refused(FundingQuoteRefusal::AboveMaximum { max: 5 })),
		row("b.dot", refused(FundingQuoteRefusal::AboveMaximum { max: 9 })),
	];
	assert_eq!(quotes_issue(&maxes), Some(FundingAmountIssue::AboveMaximum { max: 9 }));
	assert_eq!(quotes_issue(&[row("a.dot", FundingQuoteState::Pending)]), None);
}

// Divergence "Cancel anyway": a repeat cancel only while no money moved.
#[test]
fn a_repeat_cancel_only_before_money_moved() {
	let cases = [
		(FundingStatus::CancelRequested, false, FundingCancelAvailability::AvailableAgain),
		(FundingStatus::CancelRequested, true, FundingCancelAvailability::Unavailable),
		(
			FundingStatus::CancelOffered { fee: 1, refund: 2 },
			false,
			FundingCancelAvailability::Unavailable,
		),
		(FundingStatus::Cancelling, false, FundingCancelAvailability::Unavailable),
		(
			FundingStatus::CancelRefused { reason: String::new() },
			true,
			FundingCancelAvailability::Available,
		),
		(FundingStatus::WaitingForPayment, false, FundingCancelAvailability::Available),
		(FundingStatus::TakingLonger, true, FundingCancelAvailability::Available),
		(FundingStatus::ToppedUp, true, FundingCancelAvailability::Unavailable),
		(
			FundingStatus::Failed { reason: FundingFailure::Expired },
			false,
			FundingCancelAvailability::Unavailable,
		),
	];
	for (status, money_moved, expected) in cases {
		assert_eq!(cancel_availability(&status, money_moved), expected, "{status:?} {money_moved}");
	}
}

fn summary(
	direction: FundingDirection,
	stage: FundingStage,
	status: FundingStatus,
) -> FundingSessionSummary {
	FundingSessionSummary {
		intent: "fs_1".to_string(),
		owner_product_id: None,
		direction,
		amount: Some(100),
		stage,
		opened_at_ms: 1,
		provider_id: None,
		choice: Some(FundingChoice {
			quote: quote(110, 90, 10, 0),
			rail: FundingRail::Bank,
			asset: "EUR".to_string(),
			network: None,
		}),
		status,
		payout: None,
		refunded: None,
		transaction_id: None,
		reference: None,
		ended: FundingEnding::default(),
	}
}

// Divergence "Failure dot on ended timeline": the first milestone not
// reached, not always the middle one.
#[test]
fn an_ended_session_fails_at_its_first_unreached_milestone() {
	let failed = |reason: FundingFailure| FundingStage::Failed { reason, settled_at_ms: 2 };
	let released = FundingStage::Released { debited: 100, settled_at_ms: 2 };
	let mut payout_failed = summary(
		FundingDirection::Out,
		released.clone(),
		FundingStatus::PayoutFailed { refunded: None },
	);
	payout_failed.payout = Some(FundingPayout::Failed { reason: "bank closed".to_string() });
	let cases = [
		(
			summary(
				FundingDirection::In,
				failed(FundingFailure::Declined),
				FundingStatus::Failed { reason: FundingFailure::Declined },
			),
			Some(FundingStep::Received),
		),
		(
			summary(
				FundingDirection::In,
				failed(FundingFailure::Refunded),
				FundingStatus::Refunded,
			),
			Some(FundingStep::Added),
		),
		(
			summary(
				FundingDirection::Out,
				released.clone(),
				FundingStatus::Cancelled { refunded: Some(100) },
			),
			Some(FundingStep::Sent),
		),
		(payout_failed, Some(FundingStep::Sent)),
		(
			summary(
				FundingDirection::In,
				FundingStage::Delivered { credited: 100, settled_at_ms: 2 },
				FundingStatus::ToppedUp,
			),
			None,
		),
	];
	for (summary, stopped_at) in cases {
		assert_eq!(ending(&summary).stopped_at, stopped_at, "{:?}", summary.status);
	}
}

#[test]
fn an_ended_session_reads_its_outcome_and_amounts() {
	let released = FundingStage::Released { debited: 95, settled_at_ms: 2 };
	let pending =
		ending(&summary(FundingDirection::Out, released.clone(), FundingStatus::PreparingPayout));
	assert_eq!(
		pending,
		FundingEnding {
			at_ms: 2,
			outcome: FundingEndedOutcome::Succeeded,
			payout_state: Some(FundingPayoutState::Pending),
			cash_amount: Some(95),
			differing_requested: Some(100),
			stopped_at: None,
			steps: vec![FundingStep::Started, FundingStep::Received, FundingStep::Sent],
			waiting_at: Some(FundingStep::Sent),
		}
	);
	let mut paid = summary(FundingDirection::Out, released, FundingStatus::Sent);
	paid.payout = Some(FundingPayout::PaidOut);
	let paid = ending(&paid);
	assert_eq!(
		(paid.payout_state, paid.stopped_at, paid.waiting_at),
		(Some(FundingPayoutState::PaidOut), None, None)
	);
	let mut expired = summary(
		FundingDirection::In,
		FundingStage::Failed { reason: FundingFailure::Expired, settled_at_ms: 2 },
		FundingStatus::Failed { reason: FundingFailure::Expired },
	);
	expired.amount = None;
	let expired = ending(&expired);
	assert_eq!(
		(expired.outcome, expired.cash_amount, expired.differing_requested, expired.steps),
		(
			FundingEndedOutcome::Failed { reason: FundingFailure::Expired },
			Some(90),
			None,
			vec![FundingStep::Started, FundingStep::Received, FundingStep::Added],
		)
	);
}

#[test]
fn networks_read_by_name() {
	let cases = [
		("polkadot", "Polkadot"),
		("ethereum", "Ethereum"),
		("arbitrum one", "Arbitrum One"),
		("BNB", "Bnb"),
		("base-sepolia", "Base-Sepolia"),
	];
	for (id, name) in cases {
		assert_eq!(funding_network_name(id.into()), name);
	}
}

// Divergence "Sent funds button": the fact behind it holds on a bank top-up
// only.
#[test]
fn only_a_bank_top_up_awaits_the_users_transfer() {
	let cases = [
		(FundingDirection::In, Some(FundingRail::Bank), true),
		(FundingDirection::In, Some(FundingRail::Card), false),
		(FundingDirection::Out, Some(FundingRail::Bank), false),
		(FundingDirection::In, None, false),
	];
	for (direction, rail, awaits) in cases {
		assert_eq!(
			frame_context(direction, rail).awaits_user_transfer,
			awaits,
			"{direction:?} {rail:?}"
		);
	}
}
