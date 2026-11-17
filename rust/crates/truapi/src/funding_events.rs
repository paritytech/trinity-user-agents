//! What each funding fact the core logs may carry.
//!
//! Funding facts leave the core as exported `truapi.funding.*` markers in
//! [`crate::markers`], on the same log path as every other exported marker:
//! a host forwards them with its core log lines, and only from builds that
//! are not released. This table names, for each of those markers, the tag
//! and measure keys its detail may carry, and [`check`] refuses an event the
//! table does not list or one carrying a key its entry does not name. Every
//! key is listed with the closed set of values it takes, so a reviewer reads
//! here what may leave the device.
//!
//! A detail reads `funding.ref=<ref> key=value ...`: the session's random
//! reference, then the tags, then the measures, each in the order the fact
//! builds them. Every event carries [`COMMON_TAGS`] and [`COMMON_MEASURES`]
//! besides its own keys. Never carried: a session's intent, quote,
//! transaction or reference ids, an address, an exact amount or fee, a
//! country code, a provider's own text, or a URL. Amounts and fees arrive as
//! buckets and a country as a region.

use crate::{
	logging::HOST_SINK_DETAIL_MAX_CHARS,
	markers::{self, Marker},
};

/// Tags every funding event carries.
///
/// - `direction`: `in`, `out`.
/// - `opener`: `host`, `product`.
/// - `product`: the opening product's dotNS label when it is a first-party
///   product, `third_party`, or `none` when the host opened it.
/// - `rail`: `card`, `bank`, `crypto`, `none`.
/// - `provider`: the provider's product id when the host lists it as a
///   funding provider, `other`, or `none` before one is chosen.
/// - `asset`, `network`: the symbol or network id a provider route declares
///   when it is a short plain identifier, `other`, or `none`.
/// - `amount_bucket`: the session's CASH amount as `<10`, `10-50`, `50-100`,
///   `100-250`, `250-500`, `500-1k`, `1k-5k`, `5k+`, or `unset`.
pub const COMMON_TAGS: &[&str] =
	&["direction", "opener", "product", "rail", "provider", "asset", "network", "amount_bucket"];

/// Measures every funding event carries: `age_ms`, the time since the
/// session opened.
pub const COMMON_MEASURES: &[&str] = &["age_ms"];

/// One registered funding event: the marker it is logged under and the keys
/// its detail may carry besides the common ones.
#[derive(Clone)]
pub struct FundingEventSpec {
	/// The marker it is logged under.
	pub marker: Marker,
	/// The tag keys it may carry.
	pub tags: &'static [&'static str],
	/// The measure keys it may carry.
	pub measures: &'static [&'static str],
}

/// Declares every funding event once, against the marker it is logged
/// under, emitting its constant and its entry in [`FUNDING_EVENTS`].
macro_rules! funding_events {
	($(
		$(#[$entry:meta])*
		$name:ident = $marker:ident {
			tags: [$($tag:literal),* $(,)?],
			measures: [$($measure:literal),* $(,)?] $(,)?
		};
	)*) => {
		$(
			$(#[$entry])*
			pub const $name: FundingEventSpec = FundingEventSpec {
				marker: markers::$marker,
				tags: &[$($tag),*],
				measures: &[$($measure),*],
			};
		)*

		/// Every funding event the core may log, in flow order.
		pub const FUNDING_EVENTS: &[FundingEventSpec] = &[$($name,)*];

		/// Log `event` under its marker, at the level of its class. Refused
		/// for a marker the table does not list.
		fn log(event: &FundingEvent, detail: &str) -> Result<(), Refused> {
			$(
				if event.marker.target == markers::$marker.target {
					match markers::$marker.class {
						markers::MarkerClass::Export { severity: markers::Severity::Error } => {
							tracing::error!(target: markers::$marker.target, "{detail}")
						},
						_ => tracing::info!(target: markers::$marker.target, "{detail}"),
					}
					return Ok(());
				}
			)*
			Err(Refused::UnknownMarker(event.marker.target))
		}
	};
}

funding_events! {
	/// The overlay opened on a new session. `amount_preset`: `true`
	/// when the opener named the amount, `false` when the user picks it.
	OVERLAY_OPENED = FUNDING_OVERLAY_OPENED {
		tags: ["amount_preset"],
		measures: [],
	};

	/// The overlay closed. `outcome`: `started`, `dismissed`,
	/// `failed`. `furthest`, how far the user got: `amount`, `rail`,
	/// `quotes`, `provider`. `overlay_bucket` and `overlay_ms`:
	/// how long it was up.
	OVERLAY_CLOSED = FUNDING_OVERLAY_CLOSED {
		tags: ["outcome", "furthest", "overlay_bucket"],
		measures: ["overlay_ms"],
	};

	/// Every provider asked for one quote request answered, or a newer
	/// request replaced it first (`superseded`: `true`, `false`).
	/// `region`: `eu`, `uk`, `us`, `other`, `unset`.
	/// `issue`, what the refusals say about the amount: `none`,
	/// `not_enough_balance`, `below_minimum`, `above_maximum`, never the
	/// limit itself. `latency_bucket`: how long the answers took.
	/// Measures count the rows by how each ended, and time the first quote
	/// and the last answer from the request.
	QUOTES_SETTLED = FUNDING_QUOTES_SETTLED {
		tags: ["region", "issue", "superseded", "latency_bucket"],
		measures: [
			"asked",
			"quoted",
			"refused_country",
			"refused_min",
			"refused_max",
			"refused_unavailable",
			"refused_other",
			"timed_out",
			"pending",
			"first_quote_ms",
			"settle_ms",
		],
	};

	/// The session was handed to a provider on one of its quotes.
	/// `best`: whether it was the best quote. `fee_bucket`:
	/// the quote's fees against what the user sends, in basis points, `<50`,
	/// `50-100`, `100-200`, `200-500`, `500+`. `eta_bucket`: the
	/// quote's time, `<1m`, `1-10m`, `10-60m`, `1-24h`, `1d+`, `unknown`.
	/// Measures: the quote's rank by what the user gets, and how many quoted.
	PROVIDER_CHOSEN = FUNDING_PROVIDER_CHOSEN {
		tags: ["best", "fee_bucket", "eta_bucket"],
		measures: ["rank", "quote_count"],
	};

	/// The provider's worker took the session, or first failed to.
	/// `outcome`: `taken`, `not_taken`. `take_bucket` and
	/// `take_ms`: time since the session was handed over.
	PROVIDER_TOOK = FUNDING_PROVIDER_TOOK {
		tags: ["outcome", "take_bucket"],
		measures: ["take_ms"],
	};

	/// The session first reached a milestone. `step`: `received`,
	/// `added`, `sent`. Measures: time since it opened and since the
	/// milestone before.
	STEP_REACHED = FUNDING_STEP_REACHED {
		tags: ["step"],
		measures: ["since_start_ms", "since_prev_ms"],
	};

	/// The session first showed a warning state. `flag`:
	/// `taking_longer`, `retrying`, `retrying_payout`.
	STATUS_FLAG = FUNDING_STATUS_FLAG {
		tags: ["flag"],
		measures: ["since_start_ms"],
	};

	/// The provider is done with the session, on an ending that is no fault. `outcome`:
	/// `topped_up`, `sent`, `refunded`, `payout_failed`, `failed`, `expired`,
	/// `cancelled`, `paid_out_after_cancel`. `reason`: `none`, a
	/// failure's kind such as `region_unavailable` or `declined`, the core's
	/// own `not_claimed` or `payment_failed`, or `provider_other` for a
	/// provider's own code. `expected`: whether the ending is the
	/// user's or the rules' rather than a fault. `money_moved`,
	/// `region`, `duration_bucket`. Measures: time to the end
	/// and to the payment's arrival.
	ENDED = FUNDING_ENDED {
		tags: [
			"outcome",
			"reason",
			"expected",
			"money_moved",
			"region",
			"duration_bucket",
		],
		measures: ["duration_ms", "to_received_ms"],
	};

	/// An ending that is a fault: failed, a failed payout, or expired with
	/// money moved, and not the user's or the rules' doing. Carries what
	/// `ENDED` does.
	ENDED_UNEXPECTEDLY = FUNDING_ENDED_UNEXPECTEDLY {
		tags: [
			"outcome",
			"reason",
			"expected",
			"money_moved",
			"region",
			"duration_bucket",
		],
		measures: ["duration_ms", "to_received_ms"],
	};

	/// The user asked to cancel. `result`: `ended`, `requested`,
	/// `pending`, `too_late`, `refused`. `status_at`: the session's
	/// status variant when asked, such as `waiting_for_payment`.
	/// `money_moved`.
	CANCEL_REQUESTED = FUNDING_CANCEL_REQUESTED {
		tags: ["result", "status_at", "money_moved"],
		measures: [],
	};

	/// The provider answered a cancel, or its worker first failed to.
	/// `answer`: `offered`, `refused`, `stopped`, `no_answer`.
	/// `fee_bucket`: an offer's fee against its total, as on
	/// `provider_chosen`, or `none`. `answer_ms`: time since asked.
	CANCEL_ANSWERED = FUNDING_CANCEL_ANSWERED {
		tags: ["answer", "fee_bucket"],
		measures: ["answer_ms"],
	};

	/// The user answered a cancel offer. `result`: `accepted`,
	/// `declined`, `lapsed`, `no_offer`, `too_late`.
	CANCEL_CONFIRMED = FUNDING_CANCEL_CONFIRMED {
		tags: ["result"],
		measures: [],
	};

	/// The provider asked to show a screen. `target`: `app`, `page`.
	/// `replaced`: whether it replaced one still waiting.
	FRAME_REQUESTED = FUNDING_FRAME_REQUESTED {
		tags: ["target", "replaced"],
		measures: [],
	};

	/// The host opened a screen the provider asked for. `wait_bucket`
	/// and `wait_ms`: time since it was asked for.
	FRAME_OPENED = FUNDING_FRAME_OPENED {
		tags: ["target", "wait_bucket"],
		measures: ["wait_ms"],
	};

	/// A provider's screen closed. `outcome`: `closed`, `dismissed`,
	/// `returned`, `failed` once shown; `closed_by_core` when opened but
	/// withdrawn before it showed; `dropped_unopened` when the user never saw
	/// it. `open_ms`: time on screen.
	FRAME_CLOSED = FUNDING_FRAME_CLOSED {
		tags: ["target", "outcome"],
		measures: ["open_ms"],
	};}

/// The entry for the marker `target`, or `None` when the table does not
/// list it.
pub fn spec_of(target: &str) -> Option<&'static FundingEventSpec> {
	FUNDING_EVENTS.iter().find(|spec| spec.marker.target == target)
}

/// One funding fact as the core logs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FundingEvent {
	/// The marker it is logged under.
	pub marker: Marker,
	/// The random id joining one session's events. It is drawn per session
	/// and kept in memory only, so nothing leads from it to the session.
	pub session_ref: String,
	/// Closed-value tags, in the order the fact builds them.
	pub tags: Vec<(&'static str, String)>,
	/// Counts and durations in milliseconds, in the order the fact builds
	/// them.
	pub measures: Vec<(&'static str, u64)>,
}

impl FundingEvent {
	/// The event's detail: `funding.ref=<ref>`, then each tag, then each
	/// measure, as `key=value`.
	pub fn detail(&self) -> String {
		let tags = self.tags.iter().map(|(key, value)| format!("{key}={value}"));
		let measures = self.measures.iter().map(|(key, value)| format!("{key}={value}"));
		core::iter::once(format!("funding.ref={}", self.session_ref))
			.chain(tags)
			.chain(measures)
			.collect::<Vec<_>>()
			.join(" ")
	}

	/// The tag `key`, when the event carries it.
	pub fn tag(&self, key: &str) -> Option<&str> {
		self.tags.iter().find(|(tagged, _)| *tagged == key).map(|(_, value)| value.as_str())
	}

	/// The measure `key`, when the event carries it.
	pub fn measure(&self, key: &str) -> Option<u64> {
		self.measures.iter().find(|(measured, _)| *measured == key).map(|(_, value)| *value)
	}
}

/// Why an event was refused.
#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display)]
pub enum Refused {
	/// The table does not list the marker.
	#[display("{_0} is not a registered funding event")]
	UnknownMarker(&'static str),
	/// The entry does not name the key.
	#[display("{marker} does not carry {key}")]
	UnknownKey {
		/// The event's marker.
		marker: &'static str,
		/// The key it carried.
		key: &'static str,
	},
	/// The detail would not reach a host whole.
	#[display("{_0}'s detail is longer than a host keeps")]
	TooLong(&'static str),
}

/// Accept `event` only when the table lists its marker, its entry names every
/// key it carries, and its detail fits what a host keeps whole.
pub fn check(event: &FundingEvent) -> Result<(), Refused> {
	let marker = event.marker.target;
	let spec = spec_of(marker).ok_or(Refused::UnknownMarker(marker))?;
	let known = |key: &&str, own: &[&str]| COMMON_TAGS.contains(key) || own.contains(key);
	if let Some((key, _)) = event.tags.iter().find(|(key, _)| !known(key, spec.tags)) {
		return Err(Refused::UnknownKey { marker, key });
	}
	let known = |key: &&str| COMMON_MEASURES.contains(key) || spec.measures.contains(key);
	if let Some((key, _)) = event.measures.iter().find(|(key, _)| !known(key)) {
		return Err(Refused::UnknownKey { marker, key });
	}
	if event.detail().chars().count() > HOST_SINK_DETAIL_MAX_CHARS {
		return Err(Refused::TooLong(marker));
	}
	Ok(())
}

/// Log `event` under its marker once [`check`] accepts it. Returns the
/// detail logged.
pub fn emit(event: &FundingEvent) -> Result<String, Refused> {
	check(event)?;
	let detail = event.detail();
	log(event, &detail)?;
	Ok(detail)
}

#[cfg(test)]
mod tests;
