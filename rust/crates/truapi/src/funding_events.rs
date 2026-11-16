//! Every funding event the core reports to a host for telemetry, and what
//! each may carry.
//!
//! The counterpart of [`crate::markers`] for structured events: one table
//! names every `truapi.funding.*` event and the tag and measure keys it may
//! carry, and [`check`] refuses an event the table does not list, or one
//! carrying a key its entry does not name. Every key is listed with the
//! closed set of values it takes, so a reviewer reads here what may leave
//! the device.
//!
//! Every event carries [`COMMON_TAGS`] and [`COMMON_MEASURES`] besides its
//! own keys, and a `session_ref`: a random id the core draws per session for
//! telemetry alone. Never carried: a session's intent, quote, transaction or
//! reference ids, an address, an exact amount or fee, a country code, a
//! provider's own text, or a URL. Amounts and fees arrive as buckets and a
//! country as a region.
//!
//! The core emits these on every build. A host forwards them only from
//! builds that are not released, the same gate its event recorder already
//! applies.

use crate::platform::FundingEvent;

/// Tags every funding event carries.
///
/// - `funding.direction`: `in`, `out`.
/// - `funding.opener`: `host`, `product`.
/// - `funding.product`: the opening product's dotNS label when it is a first-party product,
///   `third_party`, or `none` when the host opened it.
/// - `funding.rail`: `card`, `bank`, `crypto`, `none`.
/// - `funding.provider`: the provider's product id when the host lists it as a funding provider,
///   `other`, or `none` before one is chosen.
/// - `funding.asset`, `funding.network`: the symbol or network id a provider route declares when it
///   is a short plain identifier, `other`, or `none`.
/// - `funding.amount_bucket`: the session's CASH amount as `<10`, `10-50`, `50-100`, `100-250`,
///   `250-500`, `500-1k`, `1k-5k`, `5k+`, or `unset`.
pub const COMMON_TAGS: &[&str] = &[
	"funding.direction",
	"funding.opener",
	"funding.product",
	"funding.rail",
	"funding.provider",
	"funding.asset",
	"funding.network",
	"funding.amount_bucket",
];

/// Measures every funding event carries: `funding.age_ms`, the time since
/// the session opened.
pub const COMMON_MEASURES: &[&str] = &["funding.age_ms"];

/// One registered funding event: its name and the keys it may carry
/// besides the common ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FundingEventSpec {
	/// The registered name, `truapi.funding.<thing>`.
	pub name: &'static str,
	/// The tag keys it may carry.
	pub tags: &'static [&'static str],
	/// The measure keys it may carry.
	pub measures: &'static [&'static str],
}

/// Declares every funding event once, emitting its constant, its entry in
/// [`FUNDING_EVENTS`] and its arm in [`spec_of`].
macro_rules! funding_events {
	($(
		$(#[$entry:meta])*
		$name:ident = $target:literal {
			tags: [$($tag:literal),* $(,)?],
			measures: [$($measure:literal),* $(,)?] $(,)?
		};
	)*) => {
		$(
			$(#[$entry])*
			pub const $name: FundingEventSpec = FundingEventSpec {
				name: $target,
				tags: &[$($tag),*],
				measures: &[$($measure),*],
			};
		)*

		/// Every funding event the core may report, in flow order.
		pub const FUNDING_EVENTS: &[FundingEventSpec] = &[$($name,)*];

		/// The entry for `name`, or `None` when the table does not list it.
		pub fn spec_of(name: &str) -> Option<&'static FundingEventSpec> {
			match name {
				$($target => Some(&$name),)*
				_ => None,
			}
		}
	};
}

funding_events! {
	/// The overlay opened on a new session. `funding.amount_preset`: `true`
	/// when the opener named the amount, `false` when the user picks it.
	OVERLAY_OPENED = "truapi.funding.overlay_opened" {
		tags: ["funding.amount_preset"],
		measures: [],
	};

	/// The overlay closed. `funding.outcome`: `started`, `dismissed`,
	/// `failed`. `funding.furthest`, how far the user got: `amount`, `rail`,
	/// `quotes`, `provider`. `funding.overlay_bucket` and `funding.overlay_ms`:
	/// how long it was up.
	OVERLAY_CLOSED = "truapi.funding.overlay_closed" {
		tags: ["funding.outcome", "funding.furthest", "funding.overlay_bucket"],
		measures: ["funding.overlay_ms"],
	};

	/// Every provider asked for one quote request answered, or a newer
	/// request replaced it first (`funding.superseded`: `true`, `false`).
	/// `funding.region`: `eu`, `uk`, `us`, `other`, `unset`.
	/// `funding.issue`, what the refusals say about the amount: `none`,
	/// `not_enough_balance`, `below_minimum`, `above_maximum`, never the
	/// limit itself. `funding.latency_bucket`: how long the answers took.
	/// Measures count the rows by how each ended, and time the first quote
	/// and the last answer from the request.
	QUOTES_SETTLED = "truapi.funding.quotes_settled" {
		tags: ["funding.region", "funding.issue", "funding.superseded", "funding.latency_bucket"],
		measures: [
			"funding.asked",
			"funding.quoted",
			"funding.refused_country",
			"funding.refused_min",
			"funding.refused_max",
			"funding.refused_unavailable",
			"funding.refused_other",
			"funding.timed_out",
			"funding.pending",
			"funding.first_quote_ms",
			"funding.settle_ms",
		],
	};

	/// The session was handed to a provider on one of its quotes.
	/// `funding.best`: whether it was the best quote. `funding.fee_bucket`:
	/// the quote's fees against what the user sends, in basis points, `<50`,
	/// `50-100`, `100-200`, `200-500`, `500+`. `funding.eta_bucket`: the
	/// quote's time, `<1m`, `1-10m`, `10-60m`, `1-24h`, `1d+`, `unknown`.
	/// Measures: the quote's rank by what the user gets, and how many quoted.
	PROVIDER_CHOSEN = "truapi.funding.provider_chosen" {
		tags: ["funding.best", "funding.fee_bucket", "funding.eta_bucket"],
		measures: ["funding.rank", "funding.quote_count"],
	};

	/// The provider's worker took the session, or first failed to.
	/// `funding.outcome`: `taken`, `not_taken`. `funding.take_bucket` and
	/// `funding.take_ms`: time since the session was handed over.
	PROVIDER_TOOK = "truapi.funding.provider_took" {
		tags: ["funding.outcome", "funding.take_bucket"],
		measures: ["funding.take_ms"],
	};

	/// The session first reached a milestone. `funding.step`: `received`,
	/// `added`, `sent`. Measures: time since it opened and since the
	/// milestone before.
	STEP_REACHED = "truapi.funding.step_reached" {
		tags: ["funding.step"],
		measures: ["funding.since_start_ms", "funding.since_prev_ms"],
	};

	/// The session first showed a warning state. `funding.flag`:
	/// `taking_longer`, `retrying`, `retrying_payout`.
	STATUS_FLAG = "truapi.funding.status_flag" {
		tags: ["funding.flag"],
		measures: ["funding.since_start_ms"],
	};

	/// The provider is done with the session. `funding.outcome`:
	/// `topped_up`, `sent`, `refunded`, `payout_failed`, `failed`, `expired`,
	/// `cancelled`, `paid_out_after_cancel`. `funding.reason`: `none`, a
	/// failure's kind such as `region_unavailable` or `declined`, the core's
	/// own `not_claimed` or `payment_failed`, or `provider_other` for a
	/// provider's own code. `funding.expected`: whether the ending is the
	/// user's or the rules' rather than a fault. `funding.money_moved`,
	/// `funding.region`, `funding.duration_bucket`. Measures: time to the end
	/// and to the payment's arrival.
	ENDED = "truapi.funding.ended" {
		tags: [
			"funding.outcome",
			"funding.reason",
			"funding.expected",
			"funding.money_moved",
			"funding.region",
			"funding.duration_bucket",
		],
		measures: ["funding.duration_ms", "funding.to_received_ms"],
	};

	/// The user asked to cancel. `funding.result`: `ended`, `requested`,
	/// `pending`, `too_late`, `refused`. `funding.status_at`: the session's
	/// status variant when asked, such as `waiting_for_payment`.
	/// `funding.money_moved`.
	CANCEL_REQUESTED = "truapi.funding.cancel_requested" {
		tags: ["funding.result", "funding.status_at", "funding.money_moved"],
		measures: [],
	};

	/// The provider answered a cancel, or its worker first failed to.
	/// `funding.answer`: `offered`, `refused`, `stopped`, `no_answer`.
	/// `funding.fee_bucket`: an offer's fee against its total, as on
	/// `provider_chosen`, or `none`. `funding.answer_ms`: time since asked.
	CANCEL_ANSWERED = "truapi.funding.cancel_answered" {
		tags: ["funding.answer", "funding.fee_bucket"],
		measures: ["funding.answer_ms"],
	};

	/// The user answered a cancel offer. `funding.result`: `accepted`,
	/// `declined`, `lapsed`, `no_offer`, `too_late`.
	CANCEL_CONFIRMED = "truapi.funding.cancel_confirmed" {
		tags: ["funding.result"],
		measures: [],
	};

	/// The provider asked to show a screen. `funding.target`: `app`, `page`.
	/// `funding.replaced`: whether it replaced one still waiting.
	FRAME_REQUESTED = "truapi.funding.frame_requested" {
		tags: ["funding.target", "funding.replaced"],
		measures: [],
	};

	/// The host opened a screen the provider asked for. `funding.wait_bucket`
	/// and `funding.wait_ms`: time since it was asked for.
	FRAME_OPENED = "truapi.funding.frame_opened" {
		tags: ["funding.target", "funding.wait_bucket"],
		measures: ["funding.wait_ms"],
	};

	/// A provider's screen closed. `funding.outcome`: `closed`, `dismissed`,
	/// `returned`, `failed` once shown; `closed_by_core` when opened but
	/// withdrawn before it showed; `dropped_unopened` when the user never saw
	/// it. `funding.open_ms`: time on screen.
	FRAME_CLOSED = "truapi.funding.frame_closed" {
		tags: ["funding.target", "funding.outcome"],
		measures: ["funding.open_ms"],
	};
}

/// Why [`check`] refused an event.
#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display)]
pub enum Refused {
	/// The table does not list the name.
	#[display("{_0} is not a registered funding event")]
	UnknownName(String),
	/// The entry does not name the key.
	#[display("{name} does not carry {key}")]
	UnknownKey {
		/// The event.
		name: String,
		/// The key it carried.
		key: String,
	},
}

/// Accept `event` only when the table lists its name and names every key it
/// carries.
pub fn check(event: &FundingEvent) -> Result<(), Refused> {
	let spec = spec_of(&event.name).ok_or_else(|| Refused::UnknownName(event.name.clone()))?;
	let unknown =
		|key: &str| Refused::UnknownKey { name: event.name.clone(), key: key.to_string() };
	if let Some(tag) = event
		.tags
		.iter()
		.find(|tag| !COMMON_TAGS.contains(&&*tag.key) && !spec.tags.contains(&&*tag.key))
	{
		return Err(unknown(&tag.key));
	}
	if let Some(measure) = event.measures.iter().find(|measure| {
		!COMMON_MEASURES.contains(&&*measure.key) && !spec.measures.contains(&&*measure.key)
	}) {
		return Err(unknown(&measure.key));
	}
	Ok(())
}

#[cfg(test)]
mod tests;
