use truapi::latest::{
	FundingDirection, FundingFailure, FundingFrameTarget, FundingPayout, FundingQuote, FundingRail,
};

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

// Divergence "Default rail": the first routed rail in core order, so crypto
// leads when a provider routes it, and card only leads without crypto.
#[test]
fn the_default_rail_is_the_first_routed_in_core_order() {
	let card_bank = candidate(
		"ramp.dot",
		vec![route(FundingRail::Card, &["EUR"], None), route(FundingRail::Bank, &["EUR"], None)],
	);
	let crypto =
		candidate("swap.dot", vec![route(FundingRail::Crypto, &["USDT"], Some(&["polkadot"]))]);
	let bank_only = candidate("bank.dot", vec![route(FundingRail::Bank, &["EUR"], None)]);
	let cases: [(&[FundingCandidate], FundingRail); 4] = [
		(&[card_bank.clone(), crypto.clone()], FundingRail::Crypto),
		(&[card_bank], FundingRail::Card),
		(&[bank_only], FundingRail::Bank),
		(&[], FundingRail::Card),
	];
	for (candidates, expected) in cases {
		assert_eq!(default_rail(&routed_rails(candidates)), expected, "{candidates:?}");
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
fn networks_read_by_name() {
	let cases = [
		("polkadot", "Polkadot"),
		("ethereum", "Ethereum"),
		("arbitrum one", "Arbitrum One"),
		("BNB", "Bnb"),
		("base-sepolia", "Base-Sepolia"),
	];
	for (id, name) in cases {
		assert_eq!(network_name(id), name);
	}
}

#[test]
fn a_shared_reason_for_no_quote_is_given_once() {
	let timeout = FundingQuoteState::Unavailable { reason: FundingQuoteUnavailable::Timeout };
	let other =
		|message: &str| refused(FundingQuoteRefusal::Other { message: message.to_string() });
	let cases: [(Vec<FundingQuoteRow>, Option<FundingQuotesRefusal>); 6] = [
		(
			vec![row("a.dot", timeout.clone()), row("b.dot", timeout.clone())],
			Some(FundingQuotesRefusal::Timeout),
		),
		(
			vec![row("a.dot", refused(FundingQuoteRefusal::CountryUnsupported))],
			Some(FundingQuotesRefusal::CountryUnsupported),
		),
		(
			vec![row("a.dot", other("KYC needed"))],
			Some(FundingQuotesRefusal::Other { message: "KYC needed".to_string() }),
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

#[test]
fn held_up_statuses_are_delayed() {
	let cases = [
		(FundingStatus::TakingLonger, true),
		(FundingStatus::Retrying, true),
		(FundingStatus::RetryingPayout, true),
		(FundingStatus::CancelOffered { fee: 0, refund: 0 }, true),
		(FundingStatus::CancelRefused { reason: String::new() }, true),
		(FundingStatus::WaitingForTransfer, false),
		(FundingStatus::Sent, false),
	];
	for (status, delayed) in cases {
		assert_eq!(status_delayed(&status), delayed, "{status:?}");
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
		ended: FundingEndedView::default(),
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
		assert_eq!(ended_view(&summary).stopped_at, stopped_at, "{:?}", summary.status);
	}
}

#[test]
fn an_ended_session_reads_its_outcome_and_amounts() {
	let released = FundingStage::Released { debited: 95, settled_at_ms: 2 };
	let pending = ended_view(&summary(
		FundingDirection::Out,
		released.clone(),
		FundingStatus::PreparingPayout,
	));
	assert_eq!(
		pending,
		FundingEndedView {
			outcome: FundingEndedOutcome::Succeeded,
			payout_state: Some(FundingPayoutState::Pending),
			cash_amount: Some(95),
			differing_requested: Some(100),
			stopped_at: None,
		}
	);
	let mut paid = summary(FundingDirection::Out, released, FundingStatus::Sent);
	paid.payout = Some(FundingPayout::PaidOut);
	let paid = ended_view(&paid);
	assert_eq!((paid.payout_state, paid.stopped_at), (Some(FundingPayoutState::PaidOut), None));
	let mut expired = summary(
		FundingDirection::In,
		FundingStage::Failed { reason: FundingFailure::Expired, settled_at_ms: 2 },
		FundingStatus::Failed { reason: FundingFailure::Expired },
	);
	expired.amount = None;
	let expired = ended_view(&expired);
	assert_eq!(
		(expired.outcome, expired.cash_amount, expired.differing_requested),
		(FundingEndedOutcome::Failed { reason: FundingFailure::Expired }, Some(90), None)
	);
}

// Divergence "Withdrawal fee rounding": up to the cent, never under the
// cost; a top-up's fee is in the provider's asset and shown as it is.
#[test]
fn a_withdrawal_fee_rounds_up_to_the_cent() {
	let cases = [
		(quote(10_000_000, 9_000, 15_001, 0), FundingDirection::Out, 20_000, 9_984_999),
		(quote(10_000_000, 9_000, 10_000, 0), FundingDirection::Out, 10_000, 9_990_000),
		(quote(10_500, 10_000_000, 400, 100), FundingDirection::In, 500, 10_000),
	];
	for (quote, direction, shown, net) in cases {
		let breakdown = fee_breakdown(&quote, direction);
		assert_eq!((breakdown.total_fee_rounded_up, breakdown.net), (shown, net), "{quote:?}");
	}
	let inbound = fee_breakdown(&quote(10_500, 10_000_000, 400, 100), FundingDirection::In);
	assert_eq!((inbound.rate_num, inbound.rate_den), (10_000, 10_000_000));
	let outbound = fee_breakdown(&quote(10_000_000, 9_000, 10_000, 0), FundingDirection::Out);
	assert_eq!((outbound.rate_num, outbound.rate_den), (9_000, 9_990_000));
}

// Divergences "Bad amount text" (None, not zero) and "Fiat decimals" (the
// quote's decimals the caller passes, here 2 and 6).
#[test]
fn typed_amounts_parse_to_units_or_none() {
	let cases: [(&str, u8, Option<u128>); 13] = [
		("12.34", 2, Some(1_234)),
		("12.345", 2, Some(1_234)),
		("12.34", 6, Some(12_340_000)),
		("0.", 2, Some(0)),
		(".5", 2, Some(50)),
		("7", 0, Some(7)),
		("", 2, None),
		(".", 2, None),
		("1e3", 2, None),
		("12abc", 2, None),
		("-1", 2, None),
		("1.2.3", 2, None),
		("340282366920938463463374607431768211456", 0, None),
	];
	for (text, decimals, expected) in cases {
		assert_eq!(units(text, decimals), expected, "{text} @ {decimals}");
	}
}

#[test]
fn fiat_is_told_from_crypto_by_iso_code() {
	let cases = [
		("EUR", true),
		("usd", true),
		("CHF", true),
		("USDT", false),
		("DOT", false),
		("BTC", false),
	];
	for (code, fiat) in cases {
		assert_eq!(asset_is_fiat(code), fiat, "{code}");
	}
	assert!(ISO_4217.windows(2).all(|pair| pair[0] < pair[1]), "sorted for binary search");
}

// Divergence "Sent funds button": the fact behind it holds on a bank top-up
// only; an App route gets the path inside the App.
#[test]
fn a_frame_knows_its_app_path_and_whether_the_user_transfers() {
	let app = |route: &str| FundingFrameTarget::App { route: route.to_string() };
	let page =
		FundingFrameTarget::Page { url: "https://pay.example".to_string(), return_url: None };
	let cases = [
		(app("kyc"), FundingDirection::In, Some(FundingRail::Bank), Some("/kyc"), true),
		(app("/card?x=1"), FundingDirection::In, Some(FundingRail::Card), Some("/card?x=1"), false),
		(app("#pay"), FundingDirection::Out, Some(FundingRail::Bank), Some("#pay"), false),
		(page, FundingDirection::In, None, None, false),
	];
	for (target, direction, rail, path, transfer) in cases {
		let context = frame_context(&target, direction, rail);
		assert_eq!(
			(context.app_path.as_deref(), context.awaits_user_transfer),
			(path, transfer),
			"{target:?}"
		);
	}
}
