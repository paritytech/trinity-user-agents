//! Funding facts for telemetry, emitted as [`CoreEvent`]s under
//! `truapi.funding.*` markers. The table keeps every one of them on the
//! device bar the fault, which is exported at error.
//!
//! A fact is a [`CoreEvent`] carrying closed values only: labels from enum
//! names and fixed buckets, conditions, and counts or durations. Before it
//! goes out it gains the `funding.ref` label, the labels every session
//! carries and the session's `age_ms`. What each marker carries is
//! documented on its entry in [`crate::markers`].
//!
//! The `funding.ref` is drawn at random the first time a session emits
//! anything and forgotten once it ended, so nothing joins it to the intent.
//! [`Telemetry`] keeps it, with the little else the facts need beyond the
//! sessions themselves, and what was emitted already, so a fact a retry
//! repeats is emitted once.

use std::collections::{HashMap, VecDeque};

use truapi::latest::{
	FundingFailure, FundingFrameOutcome, FundingQuoteAsk, FundingQuoteRefusal, GenericError,
};

use crate::{
	host_logic::funding::{
		FundingCancel, FundingSession, FundingStage, FundingStatus, FundingStep,
	},
	logging::CoreEvent,
	markers,
	platform::{FundingQuoteList, FundingQuoteState, FundingQuoteUnavailable, ProductId},
	stack::Stack,
};

/// Balance units in one CASH.
const CASH_UNIT: u128 = 1_000_000;
/// How many sessions that emit nothing more keep what telemetry holds for
/// them, so a fact emitted just after a session ended still carries its
/// `funding.ref`.
const RETIRED_KEPT: usize = 16;
/// How many sessions whose entry went are remembered as gone, so a fact
/// arriving later for one of them is dropped rather than drawing a new
/// `funding.ref`.
const GONE_KEPT: usize = 256;
/// Assets tagged by name; any other is `other`. The product-sdk asset
/// catalog replaces this list once the core reads it.
const KNOWN_ASSETS: &[&str] = &["dot", "usdt", "usdc", "eur", "usd", "gbp"];
/// Networks tagged by name; any other is `other`. The product-sdk asset
/// catalog replaces this list once the core reads it.
const KNOWN_NETWORKS: &[&str] =
	&["polkadot", "polkadot-asset-hub", "kusama", "paseo", "ethereum"];
/// ISO 3166-1 alpha-2 codes of the European Union's member states.
const EU_COUNTRIES: &[&str] = &[
	"AT", "BE", "BG", "CY", "CZ", "DE", "DK", "EE", "ES", "FI", "FR", "GR", "HR", "HU", "IE", "IT",
	"LT", "LU", "LV", "MT", "NL", "PL", "PT", "RO", "SE", "SI", "SK",
];

/// A country as telemetry carries it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, strum::IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
pub enum Region {
	/// A member state of the European Union.
	Eu,
	/// The United Kingdom.
	Uk,
	/// The United States.
	Us,
	/// Any other country.
	Other,
	/// No country known.
	#[default]
	Unset,
}

impl Region {
	/// The region ISO 3166-1 alpha-2 `country` lies in.
	pub fn of(country: Option<&str>) -> Self {
		match country.map(str::to_ascii_uppercase).as_deref() {
			None => Self::Unset,
			Some("GB" | "UK") => Self::Uk,
			Some("US") => Self::Us,
			Some(code) if EU_COUNTRIES.contains(&code) => Self::Eu,
			Some(_) => Self::Other,
		}
	}
}

/// How the overlay closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
pub enum OverlayOutcome {
	/// The user started the session.
	Started,
	/// The user closed it first.
	Dismissed,
	/// The host failed to show it.
	Failed,
}

/// How far the user got in the overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
pub enum Furthest {
	/// No further than the amount.
	Amount,
	/// Picked how to pay or be paid.
	Rail,
	/// Saw providers' quotes.
	Quotes,
	/// Handed the session to a provider.
	Provider,
}

/// A warning state a session can show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
pub enum Flag {
	/// Nothing came from the provider for longer than its quote said.
	TakingLonger,
	/// The provider is retrying a top-up.
	Retrying,
	/// The provider is retrying a payout.
	RetryingPayout,
}

/// How a provider answered a cancel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
enum CancelAnswer {
	Offered,
	Refused,
	Stopped,
	NoAnswer,
}

/// What a provider frame shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
pub enum FrameTarget {
	/// A route of the provider's own App.
	App,
	/// An external page.
	Page,
}

impl FrameTarget {
	/// What `target` shows, without its route or URL.
	pub fn of(target: &truapi::latest::FundingFrameTarget) -> Self {
		match target {
			truapi::latest::FundingFrameTarget::App { .. } => Self::App,
			truapi::latest::FundingFrameTarget::Page { .. } => Self::Page,
		}
	}
}

/// How a provider frame closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
pub enum FrameClose {
	/// The provider's screen closed itself.
	Closed,
	/// The user closed it.
	Dismissed,
	/// The page reached its return URL.
	Returned,
	/// The host failed to show it.
	Failed,
	/// The host opened it, but it was withdrawn before it showed.
	ClosedByCore,
	/// The user never opened it.
	DroppedUnopened,
}

impl FrameClose {
	/// How a frame shown on screen closed, from the host's answer.
	pub fn of(shown: &Result<FundingFrameOutcome, GenericError>) -> Self {
		match shown {
			Ok(FundingFrameOutcome::Closed) => Self::Closed,
			Ok(FundingFrameOutcome::Dismissed) => Self::Dismissed,
			Ok(FundingFrameOutcome::Returned { .. }) => Self::Returned,
			Err(_) => Self::Failed,
		}
	}
}

/// How a session ended for its provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
enum EndOutcome {
	ToppedUp,
	Sent,
	Refunded,
	PayoutFailed,
	Failed,
	Expired,
	Cancelled,
	PaidOutAfterCancel,
}

/// What a funding fact adds to [`CoreEvent`]'s own builder.
pub trait FundingAttributes: Sized {
	/// With label `key` set to `value`, taken from a closed set, so a fact
	/// cannot carry a string it was handed.
	fn tag(self, key: &'static str, value: impl Into<&'static str>) -> Self;

	/// With count `key` set to `value`, when known.
	fn count_if_known(self, key: &'static str, value: Option<u64>) -> Self;
}

impl FundingAttributes for CoreEvent {
	fn tag(self, key: &'static str, value: impl Into<&'static str>) -> Self {
		self.text(key, value.into())
	}

	fn count_if_known(self, key: &'static str, value: Option<u64>) -> Self {
		match value {
			Some(value) => self.count(key, value),
			None => self,
		}
	}
}

/// The labels and the age every fact of one session carries.
#[derive(Debug, Clone)]
pub struct Common {
	tags: Vec<(&'static str, String)>,
	age_ms: Option<u64>,
}

impl Common {
	/// The common labels of `session`, its route taken from `route` when
	/// given. `listed` says whether the host lists a provider, and `stack` is
	/// the stack the session runs on, `None` once its runtime is gone. A
	/// session the core no longer holds carries nothing but its absence.
	pub fn of(
		session: Option<&FundingSession>,
		route: Option<&FundingQuoteAsk>,
		listed: impl Fn(&ProductId) -> bool,
		stack: Option<&Stack>,
		now_ms: u64,
	) -> Self {
		let Some(session) = session else {
			return Self { tags: vec![("session", "unknown".to_string())], age_ms: None };
		};
		let choice = session.choice.as_ref();
		let rail = route.map(|ask| ask.rail).or(choice.map(|choice| choice.rail));
		let asset = route.map(|ask| ask.asset.as_str()).or(choice.map(|c| c.asset.as_str()));
		let network = match route {
			Some(ask) => ask.network.as_deref(),
			None => choice.and_then(|choice| choice.network.as_deref()),
		};
		let lower = |value: &'static str| value.to_ascii_lowercase();
		let provider = match &session.provider_id {
			None => "none".to_string(),
			Some(provider) if listed(provider) => provider.as_str().to_string(),
			Some(_) => "other".to_string(),
		};
		let tags = vec![
			("direction", lower(session.direction.into())),
			("opener", if session.owner_product_id.is_some() { "product" } else { "host" }.into()),
			("product", product_tag(session.owner_product_id.as_ref(), stack)),
			("rail", rail.map_or("none".to_string(), |rail| lower(rail.into()))),
			("provider", provider),
			("asset", known_tag(asset, KNOWN_ASSETS).to_string()),
			("network", known_tag(network, KNOWN_NETWORKS).to_string()),
			("amount_bucket", amount_bucket(session.amount).to_string()),
		];
		Self { tags, age_ms: Some(now_ms.saturating_sub(session.opened_at_ms)) }
	}

	/// `fact` as it goes out: with `funding.ref` set to `session_ref`, these
	/// labels and the session's age after its own attributes.
	pub fn attach(&self, fact: CoreEvent, session_ref: &str) -> CoreEvent {
		let fact = fact.text("funding.ref", session_ref);
		let fact = self.tags.iter().fold(fact, |fact, (key, value)| fact.text(key, value.as_str()));
		fact.count_if_known("age_ms", self.age_ms)
	}
}

/// What telemetry keeps per session, beyond the session itself.
#[derive(Debug, Default)]
struct Tracked {
	/// The random id joining the session's facts.
	session_ref: Option<String>,
	/// Whether the overlay showed any quote list.
	quoted: bool,
	/// The region the quotes were last asked for.
	region: Region,
	/// Warning states emitted already.
	flags: Vec<Flag>,
	/// Whether the worker taking the session was emitted, and how.
	took: Option<bool>,
	/// When the user last asked to cancel.
	cancel_asked_ms: Option<u64>,
	/// Whether the worker not answering that cancel was emitted.
	cancel_unanswered: bool,
}

/// What telemetry keeps across facts.
#[derive(Debug, Default)]
pub struct Telemetry {
	sessions: HashMap<String, Tracked>,
	/// Sessions that emit nothing more, oldest first, whose entries go once
	/// [`RETIRED_KEPT`] newer ones follow.
	retired: VecDeque<String>,
	/// Sessions whose entry went, oldest first, the last [`GONE_KEPT`] of
	/// them: they emit nothing and get no new entry.
	gone: VecDeque<String>,
}

impl Telemetry {
	/// Session `intent`'s random id, drawn on first use; `None` once its
	/// entry went, as it emits nothing more.
	pub fn session_ref(&mut self, intent: &str) -> Option<String> {
		let tracked = self.tracked(intent)?;
		Some(tracked.session_ref.get_or_insert_with(|| nanoid::nanoid!(16)).clone())
	}

	/// What is kept for session `intent`, made on first use; `None` once its
	/// entry went.
	fn tracked(&mut self, intent: &str) -> Option<&mut Tracked> {
		if self.gone.iter().any(|gone| gone == intent) {
			return None;
		}
		Some(self.sessions.entry(intent.to_string()).or_default())
	}

	/// Retire session `intent`, which emits nothing more: what is kept for it
	/// goes once [`RETIRED_KEPT`] other sessions retired after it, and from
	/// then on a fact for it is dropped.
	pub fn forget(&mut self, intent: &str) {
		if self.retired.iter().chain(&self.gone).any(|known| known == intent) {
			return;
		}
		self.retired.push_back(intent.to_string());
		while self.retired.len() > RETIRED_KEPT {
			let Some(oldest) = self.retired.pop_front() else {
				break;
			};
			self.sessions.remove(&oldest);
			self.gone.push_back(oldest);
		}
		while self.gone.len() > GONE_KEPT {
			self.gone.pop_front();
		}
	}

	/// Whether session `intent`'s overlay showed any quote list.
	pub fn quoted(&mut self, intent: &str) -> bool {
		self.tracked(intent).is_some_and(|tracked| tracked.quoted)
	}

	/// The region session `intent`'s quotes are asked for.
	pub fn asked_in(&mut self, intent: &str, region: Region) {
		if let Some(tracked) = self.tracked(intent) {
			tracked.region = region;
		}
	}

	/// Session `intent`'s host was shown a quote list. Returns the round's
	/// settling as a fact when `settled` carries one.
	pub fn quotes_shown(
		&mut self,
		intent: &str,
		settled: Option<QuotesSettled>,
	) -> Option<CoreEvent> {
		let tracked = self.tracked(intent)?;
		tracked.quoted = true;
		settled.map(|settled| settled.fact(tracked.region))
	}

	/// The round a newer request replaced before it settled, as a fact.
	pub fn quotes_superseded(
		&mut self,
		intent: &str,
		settled: QuotesSettled,
	) -> Option<CoreEvent> {
		Some(settled.fact(self.tracked(intent)?.region))
	}

	/// The user asked to cancel session `intent` at `now_ms`.
	pub fn cancel_asked(&mut self, intent: &str, now_ms: u64) {
		if let Some(tracked) = self.tracked(intent) {
			tracked.cancel_asked_ms = Some(now_ms);
			tracked.cancel_unanswered = false;
		}
	}

	/// The worker did not answer session `intent`'s cancel: emitted the first
	/// time for each cancel.
	pub fn cancel_unanswered(&mut self, intent: &str, now_ms: u64) -> Option<CoreEvent> {
		let tracked = self.tracked(intent)?;
		if core::mem::replace(&mut tracked.cancel_unanswered, true) {
			return None;
		}
		Some(cancel_answered(CancelAnswer::NoAnswer, "none", tracked.cancel_asked_ms, now_ms))
	}

	/// The worker took `session`, or did not: emitted once each way, and not
	/// at all once taken.
	pub fn provider_took(
		&mut self,
		session: &FundingSession,
		taken: bool,
		now_ms: u64,
	) -> Option<CoreEvent> {
		let tracked = self.tracked(&session.intent)?;
		if tracked.took == Some(true) || tracked.took == Some(taken) {
			return None;
		}
		tracked.took = Some(taken);
		Some(
			CoreEvent::new(markers::FUNDING_PROVIDER_TOOK)
				.tag("outcome", if taken { "taken" } else { "not_taken" })
				.count_if_known("take_ms", session.assigned_at_ms.map(|at| now_ms.saturating_sub(at))),
		)
	}

	/// `session` showed `flag`: emitted the first time.
	pub fn flagged(&mut self, session: &FundingSession, flag: Flag) -> Option<CoreEvent> {
		let flags = &mut self.tracked(&session.intent)?.flags;
		if flags.contains(&flag) {
			return None;
		}
		flags.push(flag);
		Some(CoreEvent::new(markers::FUNDING_STATUS_FLAG).tag("flag", flag))
	}

	/// Session `intent` showed `flag` before this process loaded it, so it
	/// is not emitted again.
	pub fn flagged_before(&mut self, intent: &str, flag: Flag) {
		if let Some(tracked) = self.tracked(intent) &&
			!tracked.flags.contains(&flag)
		{
			tracked.flags.push(flag);
		}
	}

	/// What changed from `before` to `after`, a session a commit stored at
	/// `now_ms`: milestones first reached, retrying first shown, a cancel
	/// answered, and the end once its provider is done with it.
	pub fn changed(
		&mut self,
		before: Option<&FundingSession>,
		after: &FundingSession,
		now_ms: u64,
	) -> Vec<CoreEvent> {
		let mut facts = Vec::new();
		if self.tracked(&after.intent).is_none() {
			return facts;
		}
		let reached_before = before.map(FundingSession::steps_reached).unwrap_or_default();
		let mut prev_ms = after.opened_at_ms;
		for step in after.steps_reached() {
			let Some(at_ms) = step.reached_at_ms.filter(|_| step.step != FundingStep::Started)
			else {
				continue;
			};
			if !reached_before
				.iter()
				.any(|known| known.step == step.step && known.reached_at_ms.is_some())
			{
				facts.push(
					CoreEvent::new(markers::FUNDING_STEP_REACHED)
						.tag("step", step.step)
						.count("since_prev_ms", at_ms.saturating_sub(prev_ms)),
				);
			}
			prev_ms = at_ms;
		}
		let flag = match after.status(now_ms) {
			FundingStatus::Retrying => Some(Flag::Retrying),
			FundingStatus::RetryingPayout => Some(Flag::RetryingPayout),
			_ => None,
		};
		facts.extend(flag.and_then(|flag| self.flagged(after, flag)));
		if before.is_some_and(|before| before.cancel == Some(FundingCancel::Requested)) {
			let answered = match (&after.cancel, &after.stage) {
				(Some(FundingCancel::Offered { fee, refund, .. }), _) => {
					Some((CancelAnswer::Offered, fee_bucket(*fee, fee.saturating_add(*refund))))
				},
				(Some(FundingCancel::Refused { .. }), _) => Some((CancelAnswer::Refused, "none")),
				(_, FundingStage::Failed { reason: FundingFailure::Cancelled, .. }) => {
					Some((CancelAnswer::Stopped, "none"))
				},
				_ => None,
			};
			if let Some((answer, fee)) = answered {
				let asked =
					self.tracked(&after.intent).and_then(|tracked| tracked.cancel_asked_ms);
				facts.push(cancel_answered(answer, fee, asked, now_ms));
			}
		}
		if !after.serves_provider() && before.is_none_or(FundingSession::serves_provider) {
			let region = self.tracked(&after.intent).map_or_else(Region::default, |t| t.region);
			facts.push(ended(after, region, now_ms));
		}
		facts
	}
}

/// How a quote round's rows came out, for the `quotes_settled` it emits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuotesSettled {
	/// The rows when it settled.
	pub list: FundingQuoteList,
	/// Whether a newer request replaced it first.
	pub superseded: bool,
	/// Time from the request to the first quote.
	pub first_quote_ms: Option<u64>,
	/// Time from the request to settling.
	pub settle_ms: u64,
}

impl QuotesSettled {
	fn fact(&self, region: Region) -> CoreEvent {
		let mut counts = [0u64; 8];
		for row in &self.list.rows {
			let index = match &row.state {
				FundingQuoteState::Pending => 0,
				FundingQuoteState::Quoted { .. } => 1,
				FundingQuoteState::Unavailable { reason: FundingQuoteUnavailable::Timeout } => 2,
				FundingQuoteState::Unavailable {
					reason: FundingQuoteUnavailable::Refused { reason },
				} => match reason {
					FundingQuoteRefusal::CountryUnsupported => 3,
					FundingQuoteRefusal::BelowMinimum { .. } => 4,
					FundingQuoteRefusal::AboveMaximum { .. } => 5,
					FundingQuoteRefusal::Unavailable => 6,
					FundingQuoteRefusal::Other { .. } => 7,
				},
			};
			counts[index] += 1;
		}
		let issue = self.list.issue.as_ref().map_or("none", Into::into);
		let [pending, quoted, timed_out, country, min, max, unavailable, other] = counts;
		CoreEvent::new(markers::FUNDING_QUOTES_SETTLED)
			.tag("region", region)
			.tag("issue", issue)
			.flag("superseded", self.superseded)
			.count("asked", self.list.rows.len() as u64)
			.count("quoted", quoted)
			.count("refused_country", country)
			.count("refused_min", min)
			.count("refused_max", max)
			.count("refused_unavailable", unavailable)
			.count("refused_other", other)
			.count("timed_out", timed_out)
			.count("pending", pending)
			.count_if_known("first_quote_ms", self.first_quote_ms)
			.count("settle_ms", self.settle_ms)
	}
}

fn cancel_answered(
	answer: CancelAnswer,
	fee: &'static str,
	asked_ms: Option<u64>,
	now_ms: u64,
) -> CoreEvent {
	CoreEvent::new(markers::FUNDING_CANCEL_ANSWERED)
		.tag("answer", answer)
		.tag("fee_bucket", fee)
		.count_if_known("answer_ms", asked_ms.map(|asked| now_ms.saturating_sub(asked)))
}

/// How `session`, which its provider is done with, ended at `now_ms`: under
/// `ended`, or `ended_unexpectedly` when the ending is a fault.
fn ended(session: &FundingSession, region: Region, now_ms: u64) -> CoreEvent {
	let money_moved = session.money_moved();
	let (outcome, reason, expected) = match session.status(now_ms) {
		FundingStatus::ToppedUp => (EndOutcome::ToppedUp, "none", true),
		FundingStatus::Sent => (EndOutcome::Sent, "none", true),
		FundingStatus::PaidOutAfterCancel => (EndOutcome::PaidOutAfterCancel, "none", true),
		FundingStatus::Refunded => (EndOutcome::Refunded, "refunded", true),
		FundingStatus::Cancelled { .. } => (EndOutcome::Cancelled, "none", true),
		FundingStatus::PayoutFailed { .. } => (EndOutcome::PayoutFailed, "payout_failed", false),
		FundingStatus::Failed { reason } => {
			let outcome = match reason {
				FundingFailure::Expired => EndOutcome::Expired,
				FundingFailure::Cancelled => EndOutcome::Cancelled,
				_ => EndOutcome::Failed,
			};
			(outcome, failure_reason(&reason), failure_expected(&reason, money_moved))
		},
		_ => (EndOutcome::Failed, "none", false),
	};
	let marker =
		if expected { markers::FUNDING_ENDED } else { markers::FUNDING_ENDED_UNEXPECTEDLY };
	let received = session
		.steps_reached()
		.into_iter()
		.find(|reached| reached.step == FundingStep::Received)
		.and_then(|reached| reached.reached_at_ms);
	CoreEvent::new(marker)
		.tag("outcome", outcome)
		.tag("reason", reason)
		.flag("money_moved", money_moved)
		.tag("region", region)
		.count_if_known("to_received_ms", received.map(|at| at.saturating_sub(session.opened_at_ms)))
}

/// Whether a session that failed with `failure` ended the way the user, the
/// route's rules or the market made it, rather than through a fault. A new
/// variant has to be placed here.
fn failure_expected(failure: &FundingFailure, money_moved: bool) -> bool {
	match failure {
		FundingFailure::RegionUnavailable |
		FundingFailure::VerificationRequired |
		FundingFailure::VerificationRefused |
		FundingFailure::BelowMinimum |
		FundingFailure::AboveMaximum |
		FundingFailure::InsufficientBalance |
		FundingFailure::WrongAssetOrChain |
		FundingFailure::Declined |
		FundingFailure::Cancelled |
		FundingFailure::Refunded => true,
		FundingFailure::Expired => !money_moved,
		FundingFailure::ProviderTimeout | FundingFailure::Other { .. } => false,
	}
}

/// A failure's kind; a provider's own code is `provider_other`.
fn failure_reason(failure: &FundingFailure) -> &'static str {
	match failure {
		FundingFailure::Other { code, .. } => match code.as_str() {
			"not_claimed" => "not_claimed",
			"payment_failed" => "payment_failed",
			_ => "provider_other",
		},
		failure => failure.into(),
	}
}

/// `amount`, in balance units, as a CASH bucket.
pub fn amount_bucket(amount: Option<u128>) -> &'static str {
	let Some(amount) = amount else {
		return "unset";
	};
	match amount / CASH_UNIT {
		0..10 => "<10",
		10..50 => "10-50",
		50..100 => "50-100",
		100..250 => "100-250",
		250..500 => "250-500",
		500..1_000 => "500-1k",
		1_000..5_000 => "1k-5k",
		_ => "5k+",
	}
}

/// `fee` against `of`, in basis points, as a bucket; `none` against nothing.
pub fn fee_bucket(fee: u128, of: u128) -> &'static str {
	if of == 0 {
		return "none";
	}
	match fee.saturating_mul(10_000) / of {
		0..50 => "<50",
		50..100 => "50-100",
		100..200 => "100-200",
		200..500 => "200-500",
		_ => "500+",
	}
}

/// A quoted time in seconds as a bucket.
pub fn eta_bucket(eta_secs: Option<u64>) -> &'static str {
	match eta_secs {
		None => "unknown",
		Some(0..60) => "<1m",
		Some(60..600) => "1-10m",
		Some(600..3_600) => "10-60m",
		Some(3_600..86_400) => "1-24h",
		Some(_) => "1d+",
	}
}

/// The opening product as a tag: its dotNS label when `stack` trusts it.
/// Without a stack no product is trusted.
fn product_tag(product: Option<&ProductId>, stack: Option<&Stack>) -> String {
	match product {
		None => "none".to_string(),
		Some(product) if stack.is_some_and(|stack| stack.trusts(product)) => {
			product.dotns_name().unwrap_or("third_party").to_string()
		},
		Some(_) => "third_party".to_string(),
	}
}

/// An asset or network a provider route declared, when `known` lists it,
/// in either case; anything else, which only a manifest wrote, is `other`.
fn known_tag(value: Option<&str>, known: &[&'static str]) -> &'static str {
	let Some(value) = value else {
		return "none";
	};
	known.iter().find(|known| known.eq_ignore_ascii_case(value)).copied().unwrap_or("other")
}

/// Reading a fact's attributes back, for tests.
#[cfg(test)]
pub(crate) mod read;

#[cfg(test)]
mod tests;
