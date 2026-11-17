//! Funding facts for telemetry, built from what the core already knows.
//!
//! Each fact is typed, with closed enums or fixed buckets for everything it
//! tags, and becomes a [`FundingEvent`] the registry in
//! [`crate::funding_events`] accepts and logs as its marker. What the registry calls the session's
//! `session_ref` is drawn here, at random, the first time a session reports
//! anything, and forgotten once it ended, so nothing joins it to the intent.
//!
//! [`Telemetry`] holds the little the facts need beyond the sessions
//! themselves: when the overlay opened, the quote round under way, when a
//! cancel was asked, each provider frame, and what was reported already, so
//! a fact a retry repeats is reported once.

use std::collections::{HashMap, VecDeque};

use truapi::latest::{
	FundingDirection, FundingFailure, FundingFrameOutcome, FundingQuoteAsk, FundingQuoteRefusal,
	FundingRail, GenericError,
};

use crate::{
	funding_events::{self, FundingEventSpec},
	host_logic::funding::{
		CancelConfirmation, CancelOutcome, FundingCancel, FundingSession, FundingStage,
		FundingStatus, FundingStep,
	},
	platform::{
		FundingAmountIssue, FundingQuoteList, FundingQuoteState, FundingQuoteUnavailable, ProductId,
	},
};

pub use crate::funding_events::FundingEvent;

/// Balance units in one CASH.
const CASH_UNIT: u128 = 1_000_000;
/// How many sessions that report nothing more keep what telemetry holds
/// for them, so a fact reported just after a session ended still carries its
/// `session_ref`.
const RETIRED_KEPT: usize = 16;
/// Longest provider id tagged as it is, so every detail stays within what a
/// host keeps.
const PROVIDER_ID_MAX_CHARS: usize = 40;
/// Longest asset or network identifier tagged as it is.
const PLAIN_ID_MAX_CHARS: usize = 16;
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
	/// The host failed to show it, or the session could not be stored.
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
pub enum CancelAnswer {
	/// It offered terms.
	Offered,
	/// It refused.
	Refused,
	/// It stopped the session before money moved.
	Stopped,
	/// Its worker did not answer.
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
pub enum EndOutcome {
	/// The balance was credited.
	ToppedUp,
	/// The provider paid the user out.
	Sent,
	/// The provider gave the user's payment back.
	Refunded,
	/// The payout failed after the balance was debited.
	PayoutFailed,
	/// Ended without moving value.
	Failed,
	/// Ran out of time.
	Expired,
	/// Cancelled.
	Cancelled,
	/// A cancel the user accepted lost to the payout.
	PaidOutAfterCancel,
}

/// What a quote round's rows came to.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QuoteCounts {
	/// Rows asked.
	pub asked: u64,
	/// Rows quoted.
	pub quoted: u64,
	/// Refused for the country.
	pub refused_country: u64,
	/// Refused below a minimum.
	pub refused_min: u64,
	/// Refused above a maximum.
	pub refused_max: u64,
	/// Refused as unavailable, or quoted something the core could not use.
	pub refused_unavailable: u64,
	/// Refused for a reason of the provider's own.
	pub refused_other: u64,
	/// Did not answer in time.
	pub timed_out: u64,
	/// Not answered yet.
	pub pending: u64,
}

impl QuoteCounts {
	/// The rows of `list`, counted by how each stands.
	pub fn of(list: &FundingQuoteList) -> Self {
		let mut counts = Self { asked: list.rows.len() as u64, ..Self::default() };
		for row in &list.rows {
			let count = match &row.state {
				FundingQuoteState::Pending => &mut counts.pending,
				FundingQuoteState::Quoted { .. } => &mut counts.quoted,
				FundingQuoteState::Unavailable { reason: FundingQuoteUnavailable::Timeout } => {
					&mut counts.timed_out
				},
				FundingQuoteState::Unavailable {
					reason: FundingQuoteUnavailable::Refused { reason },
				} => match reason {
					FundingQuoteRefusal::CountryUnsupported => &mut counts.refused_country,
					FundingQuoteRefusal::BelowMinimum { .. } => &mut counts.refused_min,
					FundingQuoteRefusal::AboveMaximum { .. } => &mut counts.refused_max,
					FundingQuoteRefusal::Unavailable => &mut counts.refused_unavailable,
					FundingQuoteRefusal::Other { .. } => &mut counts.refused_other,
				},
			};
			*count += 1;
		}
		counts
	}
}

/// How a session ended, as `ended` reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ending {
	/// The outcome.
	pub outcome: EndOutcome,
	/// The failure's kind, or `none`.
	pub reason: &'static str,
	/// Whether the ending is the user's or the rules', not a fault.
	pub expected: bool,
	/// Whether money moved.
	pub money_moved: bool,
	/// The region the user's quotes were asked for.
	pub region: Region,
	/// Time from opening to the end.
	pub duration_ms: u64,
	/// Time from opening to the provider receiving the payment, if it did.
	pub to_received_ms: Option<u64>,
}

impl Ending {
	/// How `session`, which its provider is done with, ended at `now_ms`.
	pub fn of(session: &FundingSession, region: Region, now_ms: u64) -> Self {
		let money_moved = session.money_moved();
		let (outcome, reason) = match session.status(now_ms) {
			FundingStatus::ToppedUp => (EndOutcome::ToppedUp, "none"),
			FundingStatus::Sent => (EndOutcome::Sent, "none"),
			FundingStatus::PaidOutAfterCancel => (EndOutcome::PaidOutAfterCancel, "none"),
			FundingStatus::Refunded => {
				(EndOutcome::Refunded, failure_reason(&FundingFailure::Refunded))
			},
			FundingStatus::Cancelled { .. } => (EndOutcome::Cancelled, "none"),
			FundingStatus::PayoutFailed { .. } => (EndOutcome::PayoutFailed, "payout_failed"),
			FundingStatus::Failed { reason: FundingFailure::Expired } => {
				(EndOutcome::Expired, failure_reason(&FundingFailure::Expired))
			},
			FundingStatus::Failed { reason: reason @ FundingFailure::Cancelled } => {
				(EndOutcome::Cancelled, failure_reason(&reason))
			},
			FundingStatus::Failed { reason } => (EndOutcome::Failed, failure_reason(&reason)),
			_ => (EndOutcome::Failed, "none"),
		};
		let expected = match outcome {
			EndOutcome::ToppedUp |
			EndOutcome::Sent |
			EndOutcome::PaidOutAfterCancel |
			EndOutcome::Refunded |
			EndOutcome::Cancelled => true,
			EndOutcome::Expired => !money_moved,
			EndOutcome::PayoutFailed => false,
			EndOutcome::Failed => matches!(
				reason,
				"region_unavailable" |
					"verification_required" |
					"verification_refused" |
					"insufficient_balance" |
					"declined"
			),
		};
		let opened = session.opened_at_ms;
		Self {
			outcome,
			reason,
			expected,
			money_moved,
			region,
			duration_ms: now_ms.saturating_sub(opened),
			to_received_ms: reached(session, FundingStep::Received)
				.map(|at_ms| at_ms.saturating_sub(opened)),
		}
	}
}

/// One fact the core reports for telemetry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FundingFact {
	/// The overlay opened on a new session.
	OverlayOpened {
		/// Whether the opener named the amount.
		amount_preset: bool,
	},
	/// The overlay closed.
	OverlayClosed {
		/// How.
		outcome: OverlayOutcome,
		/// How far the user got.
		furthest: Furthest,
		/// How long it was up.
		overlay_ms: u64,
	},
	/// A quote round settled.
	QuotesSettled {
		/// Its rows by how each ended.
		counts: QuoteCounts,
		/// The region it was asked for.
		region: Region,
		/// What the refusals say about the amount.
		issue: Option<FundingAmountIssue>,
		/// Whether a newer request replaced it before it settled.
		superseded: bool,
		/// Time from the request to the first quote.
		first_quote_ms: Option<u64>,
		/// Time from the request to settling.
		settle_ms: u64,
	},
	/// The session was handed to a provider.
	ProviderChosen {
		/// Whether on the best quote.
		best: bool,
		/// The quote's rank by what the user gets, 1 for the best.
		rank: u64,
		/// How many quoted.
		quote_count: u64,
		/// The quote's fees against what the user sends.
		fee_bucket: &'static str,
		/// The quote's time.
		eta_bucket: &'static str,
	},
	/// The provider's worker took the session, or first failed to.
	ProviderTook {
		/// Whether it took it.
		taken: bool,
		/// Time since the session was handed over.
		take_ms: Option<u64>,
	},
	/// A milestone was first reached.
	StepReached {
		/// Which.
		step: FundingStep,
		/// Time since the session opened.
		since_start_ms: u64,
		/// Time since the milestone before.
		since_prev_ms: u64,
	},
	/// A warning state first showed.
	StatusFlag {
		/// Which.
		flag: Flag,
		/// Time since the session opened.
		since_start_ms: u64,
	},
	/// The provider is done with the session.
	Ended(Ending),
	/// The user asked to cancel.
	CancelRequested {
		/// What it did.
		result: CancelOutcome,
		/// The status variant when asked.
		status_at: &'static str,
		/// Whether money had moved.
		money_moved: bool,
	},
	/// The provider answered a cancel, or its worker first failed to.
	CancelAnswered {
		/// How.
		answer: CancelAnswer,
		/// An offer's fee against its total, or `none`.
		fee_bucket: &'static str,
		/// Time since the cancel was asked, when known.
		answer_ms: Option<u64>,
	},
	/// The user answered a cancel offer.
	CancelConfirmed {
		/// What it did.
		result: CancelConfirmation,
	},
	/// The provider asked to show a screen.
	FrameRequested {
		/// What it shows.
		target: FrameTarget,
		/// Whether it replaced one still waiting.
		replaced: bool,
	},
	/// The host opened a screen the provider asked for.
	FrameOpened {
		/// What it shows.
		target: FrameTarget,
		/// Time since it was asked for.
		wait_ms: u64,
	},
	/// A provider's screen closed.
	FrameClosed {
		/// What it showed.
		target: FrameTarget,
		/// How.
		outcome: FrameClose,
		/// Time on screen, once it was opened.
		open_ms: Option<u64>,
	},
}

/// How loud a fact is: an ending that is a fault is an error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Level {
	Info,
	Error,
}

/// The tags and measure every event of one session carries.
#[derive(Debug, Clone)]
pub struct Common {
	tags: Vec<(&'static str, String)>,
	age_ms: Option<u64>,
}

impl Common {
	/// The common tags of `session`, its route taken from `route` when
	/// given. `listed` says whether the host lists a provider. A session the
	/// core no longer holds tags `unknown`.
	pub fn of(
		session: Option<&FundingSession>,
		route: Option<&FundingQuoteAsk>,
		listed: impl Fn(&ProductId) -> bool,
		now_ms: u64,
	) -> Self {
		let Some(session) = session else {
			let tags = funding_events::COMMON_TAGS
				.iter()
				.map(|key| (*key, "unknown".to_string()))
				.collect();
			return Self { tags, age_ms: None };
		};
		let choice = session.choice.as_ref();
		let rail = route.map(|ask| ask.rail).or(choice.map(|choice| choice.rail));
		let asset = route
			.map(|ask| ask.asset.as_str())
			.or(choice.map(|choice| choice.asset.as_str()));
		let network = match route {
			Some(ask) => ask.network.as_deref(),
			None => choice.and_then(|choice| choice.network.as_deref()),
		};
		let tags = vec![
			("direction", direction(session.direction).to_string()),
			(
				"opener",
				if session.owner_product_id.is_some() { "product" } else { "host" }.to_string(),
			),
			("product", product_tag(session.owner_product_id.as_ref())),
			("rail", rail.map_or("none", rail_tag).to_string()),
			(
				"provider",
				match &session.provider_id {
					None => "none".to_string(),
					Some(provider)
						if listed(provider) && provider.as_str().len() <= PROVIDER_ID_MAX_CHARS =>
					{
						provider.as_str().to_string()
					},
					Some(_) => "other".to_string(),
				},
			),
			("asset", plain_tag(asset)),
			("network", plain_tag(network)),
			("amount_bucket", amount_bucket(session.amount).to_string()),
		];
		Self { tags, age_ms: Some(now_ms.saturating_sub(session.opened_at_ms)) }
	}
}

impl FundingFact {
	/// The registry entry this fact is reported under.
	pub fn spec(&self) -> FundingEventSpec {
		match self {
			Self::OverlayOpened { .. } => funding_events::OVERLAY_OPENED,
			Self::OverlayClosed { .. } => funding_events::OVERLAY_CLOSED,
			Self::QuotesSettled { .. } => funding_events::QUOTES_SETTLED,
			Self::ProviderChosen { .. } => funding_events::PROVIDER_CHOSEN,
			Self::ProviderTook { .. } => funding_events::PROVIDER_TOOK,
			Self::StepReached { .. } => funding_events::STEP_REACHED,
			Self::StatusFlag { .. } => funding_events::STATUS_FLAG,
			Self::Ended(_) => funding_events::ENDED,
			Self::CancelRequested { .. } => funding_events::CANCEL_REQUESTED,
			Self::CancelAnswered { .. } => funding_events::CANCEL_ANSWERED,
			Self::CancelConfirmed { .. } => funding_events::CANCEL_CONFIRMED,
			Self::FrameRequested { .. } => funding_events::FRAME_REQUESTED,
			Self::FrameOpened { .. } => funding_events::FRAME_OPENED,
			Self::FrameClosed { .. } => funding_events::FRAME_CLOSED,
		}
	}

	/// The fact as the core logs it, with `common` and `session_ref`. An
	/// ending that is a fault is logged under its own marker.
	pub fn event(&self, common: &Common, session_ref: &str) -> FundingEvent {
		let (level, tags, measures) = self.parts();
		let spec = match (self, level) {
			(Self::Ended(_), Level::Error) => funding_events::ENDED_UNEXPECTEDLY,
			_ => self.spec(),
		};
		let common_measure = common.age_ms.map(|age_ms| ("age_ms", Some(age_ms)));
		FundingEvent {
			marker: spec.marker,
			session_ref: session_ref.to_string(),
			tags: common
				.tags
				.iter()
				.map(|(key, value)| (*key, value.clone()))
				.chain(tags.into_iter().map(|(key, value)| (key, value.to_string())))
				.collect(),
			measures: common_measure
				.into_iter()
				.chain(measures)
				.filter_map(|(key, value)| value.map(|value| (key, value)))
				.collect(),
		}
	}

	/// The fact's level, its own tags and its own measures, a measure left
	/// out when unknown.
	#[allow(clippy::type_complexity)]
	fn parts(
		&self,
	) -> (Level, Vec<(&'static str, &'static str)>, Vec<(&'static str, Option<u64>)>) {
		use Level::Info;
		match self {
			Self::OverlayOpened { amount_preset } => {
				(Info, vec![("amount_preset", yes(*amount_preset))], vec![])
			},
			Self::OverlayClosed { outcome, furthest, overlay_ms } => (
				Info,
				vec![
					("outcome", outcome.into()),
					("furthest", furthest.into()),
					("overlay_bucket", duration_bucket(*overlay_ms)),
				],
				vec![("overlay_ms", Some(*overlay_ms))],
			),
			Self::QuotesSettled {
				counts,
				region,
				issue,
				superseded,
				first_quote_ms,
				settle_ms,
			} => (
				Info,
				vec![
					("region", region.into()),
					("issue", issue_tag(issue.as_ref())),
					("superseded", yes(*superseded)),
					("latency_bucket", duration_bucket(*settle_ms)),
				],
				vec![
					("asked", Some(counts.asked)),
					("quoted", Some(counts.quoted)),
					("refused_country", Some(counts.refused_country)),
					("refused_min", Some(counts.refused_min)),
					("refused_max", Some(counts.refused_max)),
					("refused_unavailable", Some(counts.refused_unavailable)),
					("refused_other", Some(counts.refused_other)),
					("timed_out", Some(counts.timed_out)),
					("pending", Some(counts.pending)),
					("first_quote_ms", *first_quote_ms),
					("settle_ms", Some(*settle_ms)),
				],
			),
			Self::ProviderChosen { best, rank, quote_count, fee_bucket, eta_bucket } => (
				Info,
				vec![("best", yes(*best)), ("fee_bucket", fee_bucket), ("eta_bucket", eta_bucket)],
				vec![("rank", Some(*rank)), ("quote_count", Some(*quote_count))],
			),
			Self::ProviderTook { taken, take_ms } => (
				Info,
				vec![
					("outcome", if *taken { "taken" } else { "not_taken" }),
					("take_bucket", take_ms.map_or("unknown", duration_bucket)),
				],
				vec![("take_ms", *take_ms)],
			),
			Self::StepReached { step, since_start_ms, since_prev_ms } => (
				Info,
				vec![("step", step_tag(*step))],
				vec![
					("since_start_ms", Some(*since_start_ms)),
					("since_prev_ms", Some(*since_prev_ms)),
				],
			),
			Self::StatusFlag { flag, since_start_ms } => {
				(Info, vec![("flag", flag.into())], vec![("since_start_ms", Some(*since_start_ms))])
			},
			Self::Ended(ending) => {
				let faulty = !ending.expected &&
					matches!(
						ending.outcome,
						EndOutcome::Failed | EndOutcome::PayoutFailed | EndOutcome::Expired
					);
				(
					if faulty { Level::Error } else { Info },
					vec![
						("outcome", ending.outcome.into()),
						("reason", ending.reason),
						("expected", yes(ending.expected)),
						("money_moved", yes(ending.money_moved)),
						("region", ending.region.into()),
						("duration_bucket", duration_bucket(ending.duration_ms)),
					],
					vec![
						("duration_ms", Some(ending.duration_ms)),
						("to_received_ms", ending.to_received_ms),
					],
				)
			},
			Self::CancelRequested { result, status_at, money_moved } => (
				Info,
				vec![
					("result", cancel_outcome_tag(*result)),
					("status_at", status_at),
					("money_moved", yes(*money_moved)),
				],
				vec![],
			),
			Self::CancelAnswered { answer, fee_bucket, answer_ms } => (
				Info,
				vec![("answer", answer.into()), ("fee_bucket", fee_bucket)],
				vec![("answer_ms", *answer_ms)],
			),
			Self::CancelConfirmed { result } => {
				(Info, vec![("result", confirmation_tag(result))], vec![])
			},
			Self::FrameRequested { target, replaced } => {
				(Info, vec![("target", target.into()), ("replaced", yes(*replaced))], vec![])
			},
			Self::FrameOpened { target, wait_ms } => (
				Info,
				vec![("target", target.into()), ("wait_bucket", duration_bucket(*wait_ms))],
				vec![("wait_ms", Some(*wait_ms))],
			),
			Self::FrameClosed { target, outcome, open_ms } => (
				Info,
				vec![("target", target.into()), ("outcome", outcome.into())],
				vec![("open_ms", *open_ms)],
			),
		}
	}
}

/// What telemetry keeps per session, beyond the session itself.
#[derive(Debug, Default)]
struct Tracked {
	/// The random id joining the session's events.
	session_ref: Option<String>,
	/// When the overlay opened.
	overlay_opened_ms: Option<u64>,
	/// Whether the overlay showed any quote list.
	quoted: bool,
	/// The region the quotes were last asked for.
	region: Region,
	/// The quote round under way.
	round: Option<Round>,
	/// Warning states reported already.
	flags: Vec<Flag>,
	/// Whether the worker taking the session was reported, and how.
	took: Option<bool>,
	/// When the user last asked to cancel.
	cancel_asked_ms: Option<u64>,
	/// Whether the worker not answering that cancel was reported.
	cancel_unanswered: bool,
}

/// One quote request, from its first list until it settles.
#[derive(Debug)]
struct Round {
	request_id: u64,
	started_ms: u64,
	first_quote_ms: Option<u64>,
	settled: bool,
	/// The latest list shown, for the counts should a newer request
	/// replace it.
	last: Option<FundingQuoteList>,
}

/// One provider frame, from the provider asking until it closes.
#[derive(Debug)]
struct TrackedFrame {
	intent: String,
	target: FrameTarget,
	requested_ms: u64,
	opened_ms: Option<u64>,
}

/// What telemetry keeps across facts.
#[derive(Debug, Default)]
pub struct Telemetry {
	sessions: HashMap<String, Tracked>,
	/// Sessions that report nothing more, oldest first, whose entries go once
	/// [`RETIRED_KEPT`] newer ones follow.
	retired: VecDeque<String>,
	frames: HashMap<u64, TrackedFrame>,
}

impl Telemetry {
	/// Session `intent`'s random id, drawn on first use.
	pub fn session_ref(&mut self, intent: &str) -> String {
		self.tracked(intent)
			.session_ref
			.get_or_insert_with(|| nanoid::nanoid!(16))
			.clone()
	}

	fn tracked(&mut self, intent: &str) -> &mut Tracked {
		self.sessions.entry(intent.to_string()).or_default()
	}

	/// Retire session `intent`, which reports nothing more: what is kept for
	/// it goes once [`RETIRED_KEPT`] other sessions retired after it.
	pub fn forget(&mut self, intent: &str) {
		if self.retired.iter().any(|retired| retired == intent) {
			return;
		}
		self.retired.push_back(intent.to_string());
		while self.retired.len() > RETIRED_KEPT {
			if let Some(oldest) = self.retired.pop_front() {
				self.sessions.remove(&oldest);
			}
		}
	}

	/// The overlay opened on session `intent` at `now_ms`.
	pub fn overlay_opened(&mut self, intent: &str, now_ms: u64) {
		self.tracked(intent).overlay_opened_ms = Some(now_ms);
	}

	/// How long session `intent`'s overlay was up by `now_ms`, and whether it
	/// showed quotes.
	pub fn overlay_closed(&mut self, intent: &str, now_ms: u64) -> (u64, bool) {
		let tracked = self.tracked(intent);
		let opened = tracked.overlay_opened_ms.take().unwrap_or(now_ms);
		(now_ms.saturating_sub(opened), tracked.quoted)
	}

	/// The region session `intent`'s quotes are asked for.
	pub fn asked_in(&mut self, intent: &str, region: Region) {
		self.tracked(intent).region = region;
	}

	/// Session `intent` started quote request `request_id` at `now_ms`. The
	/// round it replaced, if that had not settled, settles as superseded.
	pub fn quotes_requested(
		&mut self,
		intent: &str,
		request_id: u64,
		now_ms: u64,
	) -> Option<FundingFact> {
		let tracked = self.tracked(intent);
		let region = tracked.region;
		let replaced = tracked.round.replace(Round {
			request_id,
			started_ms: now_ms,
			first_quote_ms: None,
			settled: false,
			last: None,
		})?;
		if replaced.settled {
			return None;
		}
		let last = replaced.last?;
		Some(FundingFact::QuotesSettled {
			counts: QuoteCounts::of(&last),
			region,
			issue: None,
			superseded: true,
			first_quote_ms: replaced.first_quote_ms,
			settle_ms: now_ms.saturating_sub(replaced.started_ms),
		})
	}

	/// Session `intent`'s host was shown `list` at `now_ms`. Returns the
	/// round settling, the first time every row answered.
	pub fn quotes_shown(
		&mut self,
		intent: &str,
		list: &FundingQuoteList,
		now_ms: u64,
	) -> Option<FundingFact> {
		let tracked = self.tracked(intent);
		tracked.quoted = true;
		let region = tracked.region;
		let round = tracked.round.as_mut().filter(|round| round.request_id == list.request_id)?;
		let quoted = list
			.rows
			.iter()
			.any(|row| matches!(row.state, FundingQuoteState::Quoted { .. }));
		if quoted && round.first_quote_ms.is_none() {
			round.first_quote_ms = Some(now_ms.saturating_sub(round.started_ms));
		}
		round.last = Some(list.clone());
		if !list.complete || round.settled {
			return None;
		}
		round.settled = true;
		Some(FundingFact::QuotesSettled {
			counts: QuoteCounts::of(list),
			region,
			issue: list.issue.clone(),
			superseded: false,
			first_quote_ms: round.first_quote_ms,
			settle_ms: now_ms.saturating_sub(round.started_ms),
		})
	}

	/// The user asked to cancel session `intent` at `now_ms`.
	pub fn cancel_asked(&mut self, intent: &str, now_ms: u64) {
		let tracked = self.tracked(intent);
		tracked.cancel_asked_ms = Some(now_ms);
		tracked.cancel_unanswered = false;
	}

	/// The worker did not answer session `intent`'s cancel: reported the
	/// first time for each cancel.
	pub fn cancel_unanswered(&mut self, intent: &str, now_ms: u64) -> Option<FundingFact> {
		let tracked = self.tracked(intent);
		if core::mem::replace(&mut tracked.cancel_unanswered, true) {
			return None;
		}
		Some(FundingFact::CancelAnswered {
			answer: CancelAnswer::NoAnswer,
			fee_bucket: "none",
			answer_ms: tracked.cancel_asked_ms.map(|asked| now_ms.saturating_sub(asked)),
		})
	}

	/// The worker took session `session`, or did not: reported once each
	/// way, and not at all once taken.
	pub fn provider_took(
		&mut self,
		session: &FundingSession,
		taken: bool,
		now_ms: u64,
	) -> Option<FundingFact> {
		let tracked = self.tracked(&session.intent);
		match tracked.took {
			Some(true) => return None,
			Some(false) if !taken => return None,
			_ => {},
		}
		tracked.took = Some(taken);
		Some(FundingFact::ProviderTook {
			taken,
			take_ms: session.assigned_at_ms.map(|at_ms| now_ms.saturating_sub(at_ms)),
		})
	}

	/// Session `session` showed `flag`: reported the first time.
	pub fn flagged(
		&mut self,
		session: &FundingSession,
		flag: Flag,
		now_ms: u64,
	) -> Option<FundingFact> {
		let tracked = self.tracked(&session.intent);
		if tracked.flags.contains(&flag) {
			return None;
		}
		tracked.flags.push(flag);
		Some(FundingFact::StatusFlag {
			flag,
			since_start_ms: now_ms.saturating_sub(session.opened_at_ms),
		})
	}

	/// What changed from `before` to `after`, a session a commit stored at
	/// `now_ms`: milestones first reached, retrying first shown, a cancel
	/// answered, and the end once its provider is done with it.
	pub fn changed(
		&mut self,
		before: Option<&FundingSession>,
		after: &FundingSession,
		now_ms: u64,
	) -> Vec<FundingFact> {
		let mut facts = Vec::new();
		let reached_before = before.map(FundingSession::steps_reached).unwrap_or_default();
		let mut prev_ms = after.opened_at_ms;
		for step in after.steps_reached() {
			let Some(at_ms) = step.reached_at_ms.filter(|_| step.step != FundingStep::Started)
			else {
				continue;
			};
			let known = reached_before
				.iter()
				.any(|known| known.step == step.step && known.reached_at_ms.is_some());
			if !known {
				facts.push(FundingFact::StepReached {
					step: step.step,
					since_start_ms: at_ms.saturating_sub(after.opened_at_ms),
					since_prev_ms: at_ms.saturating_sub(prev_ms),
				});
			}
			prev_ms = at_ms;
		}
		let flag = match after.status(now_ms) {
			FundingStatus::Retrying => Some(Flag::Retrying),
			FundingStatus::RetryingPayout => Some(Flag::RetryingPayout),
			_ => None,
		};
		facts.extend(flag.and_then(|flag| self.flagged(after, flag, now_ms)));
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
			if let Some((answer, fee_bucket)) = answered {
				let asked = self.tracked(&after.intent).cancel_asked_ms;
				facts.push(FundingFact::CancelAnswered {
					answer,
					fee_bucket,
					answer_ms: asked.map(|asked| now_ms.saturating_sub(asked)),
				});
			}
		}
		if !after.serves_provider() && before.is_none_or(FundingSession::serves_provider) {
			let region = self.tracked(&after.intent).region;
			facts.push(FundingFact::Ended(Ending::of(after, region, now_ms)));
		}
		facts
	}

	/// Frame number `frame` of session `intent` was asked for at `now_ms`.
	pub fn frame_requested(&mut self, intent: &str, frame: u64, target: FrameTarget, now_ms: u64) {
		self.frames.insert(
			frame,
			TrackedFrame {
				intent: intent.to_string(),
				target,
				requested_ms: now_ms,
				opened_ms: None,
			},
		);
	}

	/// The host opened frame number `frame` at `now_ms`.
	pub fn frame_opened(&mut self, frame: u64, now_ms: u64) -> Option<(String, FundingFact)> {
		let tracked = self.frames.get_mut(&frame)?;
		tracked.opened_ms = Some(now_ms);
		Some((
			tracked.intent.clone(),
			FundingFact::FrameOpened {
				target: tracked.target,
				wait_ms: now_ms.saturating_sub(tracked.requested_ms),
			},
		))
	}

	/// Frame number `frame` closed at `now_ms`, `shown` when it was on
	/// screen; one never opened was dropped unopened whatever `shown` says.
	pub fn frame_closed(
		&mut self,
		frame: u64,
		shown: Option<FrameClose>,
		now_ms: u64,
	) -> Option<(String, FundingFact)> {
		let tracked = self.frames.remove(&frame)?;
		let outcome = match (tracked.opened_ms, shown) {
			(None, _) => FrameClose::DroppedUnopened,
			(Some(_), Some(shown)) => shown,
			(Some(_), None) => FrameClose::ClosedByCore,
		};
		Some((
			tracked.intent,
			FundingFact::FrameClosed {
				target: tracked.target,
				outcome,
				open_ms: tracked.opened_ms.map(|opened| now_ms.saturating_sub(opened)),
			},
		))
	}
}

/// When `session` first reached `step`.
fn reached(session: &FundingSession, step: FundingStep) -> Option<u64> {
	session
		.steps_reached()
		.into_iter()
		.find(|reached| reached.step == step)
		.and_then(|reached| reached.reached_at_ms)
}

/// The status variant `status` names, without what it carries.
pub fn status_name(status: &FundingStatus) -> &'static str {
	match status {
		FundingStatus::WaitingForTransfer => "waiting_for_transfer",
		FundingStatus::WaitingForPayment => "waiting_for_payment",
		FundingStatus::WaitingForDeposit => "waiting_for_deposit",
		FundingStatus::ConvertingToCash => "converting_to_cash",
		FundingStatus::AddingToBalance => "adding_to_balance",
		FundingStatus::TransactionInitiated => "transaction_initiated",
		FundingStatus::ConvertingTo { .. } => "converting_to",
		FundingStatus::PreparingPayout => "preparing_payout",
		FundingStatus::RetryingPayout => "retrying_payout",
		FundingStatus::Retrying => "retrying",
		FundingStatus::TakingLonger => "taking_longer",
		FundingStatus::ToppedUp => "topped_up",
		FundingStatus::Sent => "sent",
		FundingStatus::Refunded => "refunded",
		FundingStatus::PayoutFailed { .. } => "payout_failed",
		FundingStatus::Failed { .. } => "failed",
		FundingStatus::CancelRequested => "cancel_requested",
		FundingStatus::CancelOffered { .. } => "cancel_offered",
		FundingStatus::CancelRefused { .. } => "cancel_refused",
		FundingStatus::Cancelling => "cancelling",
		FundingStatus::Cancelled { .. } => "cancelled",
		FundingStatus::PaidOutAfterCancel => "paid_out_after_cancel",
	}
}

/// A failure's kind; a provider's own code is `provider_other`.
fn failure_reason(failure: &FundingFailure) -> &'static str {
	match failure {
		FundingFailure::RegionUnavailable => "region_unavailable",
		FundingFailure::VerificationRequired => "verification_required",
		FundingFailure::VerificationRefused => "verification_refused",
		FundingFailure::BelowMinimum => "below_minimum",
		FundingFailure::AboveMaximum => "above_maximum",
		FundingFailure::InsufficientBalance => "insufficient_balance",
		FundingFailure::Expired => "expired",
		FundingFailure::WrongAssetOrChain => "wrong_asset_or_chain",
		FundingFailure::ProviderTimeout => "provider_timeout",
		FundingFailure::Cancelled => "cancelled",
		FundingFailure::Refunded => "refunded",
		FundingFailure::Declined => "declined",
		FundingFailure::Other { code, .. } => match code.as_str() {
			"not_claimed" => "not_claimed",
			"payment_failed" => "payment_failed",
			_ => "provider_other",
		},
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

/// A duration in milliseconds as a bucket.
pub fn duration_bucket(ms: u64) -> &'static str {
	match ms {
		0..1_000 => "<1s",
		1_000..5_000 => "1-5s",
		5_000..15_000 => "5-15s",
		15_000..60_000 => "15-60s",
		60_000..300_000 => "1-5m",
		300_000..1_800_000 => "5-30m",
		1_800_000..7_200_000 => "30m-2h",
		7_200_000..86_400_000 => "2-24h",
		_ => "1d+",
	}
}

fn yes(value: bool) -> &'static str {
	if value { "true" } else { "false" }
}

fn direction(direction: FundingDirection) -> &'static str {
	match direction {
		FundingDirection::In => "in",
		FundingDirection::Out => "out",
	}
}

fn rail_tag(rail: FundingRail) -> &'static str {
	match rail {
		FundingRail::Card => "card",
		FundingRail::Bank => "bank",
		FundingRail::Crypto => "crypto",
	}
}

fn step_tag(step: FundingStep) -> &'static str {
	match step {
		FundingStep::Started => "started",
		FundingStep::Received => "received",
		FundingStep::Added => "added",
		FundingStep::Sent => "sent",
	}
}

fn issue_tag(issue: Option<&FundingAmountIssue>) -> &'static str {
	match issue {
		None => "none",
		Some(FundingAmountIssue::NotEnoughBalance) => "not_enough_balance",
		Some(FundingAmountIssue::BelowMinimum { .. }) => "below_minimum",
		Some(FundingAmountIssue::AboveMaximum { .. }) => "above_maximum",
	}
}

fn cancel_outcome_tag(outcome: CancelOutcome) -> &'static str {
	match outcome {
		CancelOutcome::Ended => "ended",
		CancelOutcome::Requested => "requested",
		CancelOutcome::Pending => "pending",
		CancelOutcome::TooLate => "too_late",
		CancelOutcome::Refused => "refused",
	}
}

fn confirmation_tag(confirmation: &CancelConfirmation) -> &'static str {
	match confirmation {
		CancelConfirmation::Accepted => "accepted",
		CancelConfirmation::Declined => "declined",
		CancelConfirmation::Lapsed => "lapsed",
		CancelConfirmation::NoOffer => "no_offer",
		CancelConfirmation::TooLate => "too_late",
	}
}

/// The opening product as a tag: its dotNS label when first-party.
fn product_tag(product: Option<&ProductId>) -> String {
	match product {
		None => "none".to_string(),
		Some(product) if product.has_trusted_remote_permissions() => {
			product.dotns_name().unwrap_or("third_party").to_string()
		},
		Some(_) => "third_party".to_string(),
	}
}

/// An asset or network a provider route declared, when it is a short plain
/// identifier; anything else is `other`.
fn plain_tag(value: Option<&str>) -> String {
	match value {
		None => "none".to_string(),
		Some(value)
			if !value.is_empty() &&
				value.chars().count() <= PLAIN_ID_MAX_CHARS &&
				value.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') =>
		{
			value.to_ascii_lowercase()
		},
		Some(_) => "other".to_string(),
	}
}

#[cfg(test)]
mod tests;
