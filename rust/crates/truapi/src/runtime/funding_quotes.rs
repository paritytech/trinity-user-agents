//! Live quotes for a funding session, asked of each candidate's worker.
//!
//! The host asks once per amount, rail and asset; the core sends the ask to
//! every funding provider, on `serveSubscribe`, whatever its manifest says it
//! serves, since a provider can serve more or less than it last published.
//! The worker prices it from its own API and answers with `answerQuote`. Each
//! provider's row resolves on its own: a provider that does not answer in time
//! is unavailable, so a slow one never holds the list. What an answer shows
//! about what the provider serves is kept for twelve hours and folded into the
//! candidates.
//!
//! The host's list marks the best quote, and the core asks every provider
//! again each minute while the session waits for the user to choose.

use std::{
	collections::HashMap,
	sync::{Arc, Mutex},
};

use core::time::Duration;

use futures::{
	future::{self, Either},
	stream::{self, BoxStream, StreamExt},
};
use parity_scale_codec::Encode;
use truapi::latest::{
	FundingDirection, FundingQuote, FundingQuoteAnswer, FundingQuoteAsk, FundingQuoteRefusal,
};

use super::{funding::lock, services::RuntimeServices};
use crate::{
	host_logic::{
		funding::FundingChoice,
		funding_providers::{LearnedLimit, LearnedSupport},
	},
	platform::{FundingQuoteList, FundingQuoteRow, FundingQuoteState, FundingQuoteUnavailable},
	unix_time::current_unix_millis,
};

/// How long a provider has to answer an ask.
const QUOTE_DEADLINE: Duration = Duration::from_secs(10);
/// How long a provider's answer is reused for the same ask, so editing the
/// amount back and forth does not ask every provider again.
const ANSWER_REUSE_MS: u64 = 30_000;
/// How long a quote list stands before the core asks every provider again.
const QUOTE_REFRESH: Duration = Duration::from_secs(60);
/// How long the country the host detected is reused before asking again.
const DETECTED_COUNTRY_REUSE_MS: u64 = 60 * 60 * 1_000;
/// How long a detection that found no country stands before asking again,
/// as location or a SIM can become available soon after.
const UNDETECTED_COUNTRY_RETRY_MS: u64 = 60 * 1_000;

/// A provider's answer and when it came.
#[derive(Clone)]
struct RecentAnswer {
	at_ms: u64,
	answer: FundingQuoteAnswer,
}

/// Recent answers, and the quotes each session was offered.
#[derive(Default)]
pub struct FundingQuotes {
	/// Answers by provider and encoded ask, with when each came.
	answers: Mutex<HashMap<(String, Vec<u8>), RecentAnswer>>,
	/// Quotes offered by session and provider, with the rail and asset each
	/// priced, so a selection can only name one the host was shown.
	offered: Mutex<HashMap<(String, String), Vec<FundingChoice>>>,
	/// The latest list each session's host is shown, by a number that grows
	/// with each new ask, so an older list stops asking again.
	watching: Mutex<HashMap<String, u64>>,
	/// The number the next watched list takes.
	next_watch: Mutex<u64>,
	/// The country the host last detected, and when.
	detected: Mutex<Option<(u64, Option<String>)>>,
}

impl FundingQuotes {
	/// The quote `quote_id` that `provider_id` offered for session `intent`,
	/// with what it priced, unless it expired by `now_ms`.
	pub fn offered(
		&self,
		intent: &str,
		provider_id: &str,
		quote_id: &str,
		now_ms: u64,
	) -> Option<FundingChoice> {
		lock(&self.offered)
			.get(&(intent.to_string(), provider_id.to_string()))?
			.iter()
			.find(|choice| choice.quote.quote_id == quote_id && !expired(&choice.quote, now_ms))
			.cloned()
	}

	/// Keep `quote` as offered for session `intent`, dropping the session's
	/// quotes from `provider_id` that expired by `now_ms`.
	fn offer(
		&self,
		intent: &str,
		provider_id: &str,
		quote: FundingQuote,
		ask: &FundingQuoteAsk,
		now_ms: u64,
	) {
		let mut offered = lock(&self.offered);
		let choices = offered.entry((intent.to_string(), provider_id.to_string())).or_default();
		choices
			.retain(|kept| kept.quote.quote_id != quote.quote_id && !expired(&kept.quote, now_ms));
		choices.push(FundingChoice {
			quote,
			rail: ask.rail,
			asset: ask.asset.clone(),
			network: ask.network.clone(),
		});
	}

	/// Drop the quotes offered for every session `quotable` rejects: one that
	/// was assigned or ended can no longer be handed a quote.
	pub fn retain_sessions(&self, quotable: impl Fn(&str) -> bool) {
		lock(&self.offered).retain(|(intent, _), _| quotable(intent));
		lock(&self.watching).retain(|intent, _| quotable(intent));
	}

	/// Make a new list the one session `intent` watches, ending any older
	/// one's asking again. Returns the list's number.
	fn watch(&self, intent: &str) -> u64 {
		let mut next = lock(&self.next_watch);
		*next += 1;
		lock(&self.watching).insert(intent.to_string(), *next);
		*next
	}

	/// Whether list `watch` is still the one session `intent` watches.
	fn is_watched(&self, intent: &str, watch: u64) -> bool {
		lock(&self.watching).get(intent) == Some(&watch)
	}

	/// Stop asking again for session `intent`'s quotes.
	pub fn stop_watching(&self, intent: &str) {
		lock(&self.watching).remove(intent);
	}

	/// `provider_id`'s answer to the same ask, when it came recently and, if
	/// it is a quote, has not expired.
	fn reusable(
		&self,
		provider_id: &str,
		ask: &FundingQuoteAsk,
		now_ms: u64,
	) -> Option<FundingQuoteAnswer> {
		let RecentAnswer { at_ms, answer } =
			lock(&self.answers).get(&(provider_id.to_string(), ask.encode()))?.clone();
		let fresh = now_ms.saturating_sub(at_ms) < ANSWER_REUSE_MS;
		let live = match &answer {
			FundingQuoteAnswer::Quoted { quote } => !expired(quote, now_ms),
			FundingQuoteAnswer::Refused { .. } => true,
		};
		(fresh && live).then_some(answer)
	}

	fn remember(
		&self,
		provider_id: &str,
		ask: &FundingQuoteAsk,
		now_ms: u64,
		answer: FundingQuoteAnswer,
	) {
		let mut answers = lock(&self.answers);
		answers.retain(|_, recent| now_ms.saturating_sub(recent.at_ms) < ANSWER_REUSE_MS);
		answers.insert(
			(provider_id.to_string(), ask.encode()),
			RecentAnswer { at_ms: now_ms, answer },
		);
	}
}

impl RuntimeServices {
	/// Ask every funding provider to price `ask` for session `intent`. Each
	/// provider's row is first `Pending`, then `Quoted` or `Unavailable`. The
	/// ask's direction is the session's, and an ask naming no country takes
	/// the one the host detected. Empty for a session the core does not know
	/// or that ended.
	pub fn get_funding_quote(
		self: &Arc<Self>,
		intent: &str,
		ask: FundingQuoteAsk,
	) -> BoxStream<'static, FundingQuoteRow> {
		self.get_funding_quote_within(intent, ask, QUOTE_DEADLINE)
	}

	/// [`Self::get_funding_quote`] with `deadline` for each provider.
	pub fn get_funding_quote_within(
		self: &Arc<Self>,
		intent: &str,
		ask: FundingQuoteAsk,
		deadline: Duration,
	) -> BoxStream<'static, FundingQuoteRow> {
		let Some(session) = self.funding().get(intent).filter(|session| !session.is_terminal())
		else {
			return stream::empty().boxed();
		};
		let services = self.clone();
		let intent = intent.to_string();
		stream::once(async move {
			let country = match ask.country {
				Some(country) => Some(country),
				None => services.detected_funding_country().await,
			};
			let ask = FundingQuoteAsk { direction: session.direction, country, ..ask };
			services.quote_every_provider(&intent, ask, deadline)
		})
		.flatten()
		.boxed()
	}

	/// The country the host detected for the user as an ISO 3166-1 alpha-2
	/// code, reused for an hour. A detection that found none, or answered
	/// something that is not a two-letter code, is asked again after a
	/// minute.
	pub async fn detected_funding_country(&self) -> Option<String> {
		let now_ms = current_unix_millis();
		let cached = lock(&self.funding_quotes().detected).clone();
		if let Some((at_ms, country)) = cached {
			let reuse_ms = match country {
				Some(_) => DETECTED_COUNTRY_REUSE_MS,
				None => UNDETECTED_COUNTRY_RETRY_MS,
			};
			if now_ms.saturating_sub(at_ms) < reuse_ms {
				return country;
			}
		}
		let platform = self.funding().platform()?;
		let country = platform.detected_country().await.and_then(alpha2);
		*lock(&self.funding_quotes().detected) = Some((now_ms, country.clone()));
		country
	}

	/// One row stream per funding provider for `ask`, merged.
	fn quote_every_provider(
		self: &Arc<Self>,
		intent: &str,
		ask: FundingQuoteAsk,
		deadline: Duration,
	) -> BoxStream<'static, FundingQuoteRow> {
		self.refresh_funding_providers();
		let rows: Vec<_> = self
			.funding_providers()
			.quote_targets()
			.into_iter()
			.map(|provider_id| {
				let pending = FundingQuoteRow {
					provider_id: provider_id.clone(),
					state: FundingQuoteState::Pending,
				};
				let services = self.clone();
				let intent = intent.to_string();
				let ask = ask.clone();
				stream::once(future::ready(pending))
					.chain(stream::once(async move {
						services.quote_one(&intent, &provider_id, ask, deadline).await
					}))
					.boxed()
			})
			.collect();
		stream::select_all(rows).boxed()
	}

	/// [`Self::get_funding_quote`] as the host's list, under the returned
	/// request id: the rows so far with the best quote marked, again after
	/// each answer, and a new round of asks every minute. Lists stop, mid
	/// round too, once the session is handed to a provider or ends, or a
	/// newer request or [`FundingQuotes::stop_watching`] replaces this one.
	pub fn watch_funding_quotes(
		self: &Arc<Self>,
		intent: &str,
		ask: FundingQuoteAsk,
	) -> (u64, BoxStream<'static, FundingQuoteList>) {
		self.watch_funding_quotes_within(intent, ask, QUOTE_DEADLINE, QUOTE_REFRESH)
	}

	/// [`Self::watch_funding_quotes`] with `deadline` for each provider and
	/// a new round every `refresh`.
	pub fn watch_funding_quotes_within(
		self: &Arc<Self>,
		intent: &str,
		ask: FundingQuoteAsk,
		deadline: Duration,
		refresh: Duration,
	) -> (u64, BoxStream<'static, FundingQuoteList>) {
		let watch = self.funding_quotes().watch(intent);
		let current = {
			let services = self.clone();
			let intent = intent.to_string();
			move || services.quotes_still_wanted(&intent, watch)
		};
		let services = self.clone();
		let intent = intent.to_string();
		let refresh_ms = refresh.as_millis() as u64;
		let still_current = current.clone();
		let lists = stream::unfold(None, move |round_at: Option<u64>| {
			let current = current.clone();
			let services = services.clone();
			let intent = intent.clone();
			let ask = ask.clone();
			async move {
				if let Some(round_at) = round_at {
					let wait = round_at.saturating_sub(current_unix_millis());
					futures_timer::Delay::new(Duration::from_millis(wait)).await;
				}
				if !current() {
					return None;
				}
				let refresh_at_ms = current_unix_millis().saturating_add(refresh_ms);
				let direction = services
					.funding()
					.get(&intent)
					.map_or(ask.direction, |session| session.direction);
				let pending = services
					.funding_providers()
					.quote_targets()
					.into_iter()
					.map(|provider_id| FundingQuoteRow {
						provider_id,
						state: FundingQuoteState::Pending,
					})
					.collect();
				let rows = services.get_funding_quote_within(&intent, ask, deadline);
				let start = FundingQuoteList {
					request_id: watch,
					rows: pending,
					best: None,
					refresh_at_ms,
				};
				let providers = services.clone();
				let lists = rows
					.scan(start, move |list, row| {
						match list.rows.iter_mut().find(|kept| kept.provider_id == row.provider_id)
						{
							Some(kept) => *kept = row,
							None => list.rows.push(row),
						}
						list.best = best_quote(direction, &list.rows, |provider_id| {
							providers.funding_providers().is_quote_target(provider_id)
						});
						future::ready(Some(list.clone()))
					})
					.boxed();
				Some((lists, Some(refresh_at_ms)))
			}
		})
		.flatten()
		.take_while(move |_| future::ready(still_current()))
		.boxed();
		(watch, lists)
	}

	/// Whether list `watch` of session `intent` is still wanted: the latest
	/// one asked for, for a session still open and not yet handed to a
	/// provider.
	fn quotes_still_wanted(&self, intent: &str, watch: u64) -> bool {
		self.funding_quotes().is_watched(intent, watch) &&
			self.funding()
				.get(intent)
				.is_some_and(|session| !session.is_terminal() && session.provider_id.is_none())
	}

	/// Ask one provider, holding its worker while it answers.
	async fn quote_one(
		self: &Arc<Self>,
		intent: &str,
		provider_id: &str,
		ask: FundingQuoteAsk,
		deadline: Duration,
	) -> FundingQuoteRow {
		let quotes = &self.funding_quotes();
		let answer = match quotes.reusable(provider_id, &ask, current_unix_millis()) {
			Some(answer) => Some(answer),
			None => {
				let pending = PendingQuote::ask(self, provider_id, ask.clone());
				let timeout = futures_timer::Delay::new(deadline);
				let answer = match future::select(pending.answered, timeout).await {
					Either::Left((Ok(answer), _)) => Some(answer),
					_ => None,
				};
				drop(pending.release);
				if let Some(answer) = &answer {
					let now_ms = current_unix_millis();
					quotes.remember(provider_id, &ask, now_ms, answer.clone());
					if let Some(supported) = shows_support(answer, ask.country.is_some()) {
						let (min, max) = limits_shown(answer);
						let limit = |amount| LearnedLimit { amount, learned_at_ms: now_ms };
						self.learn_funding_support(LearnedSupport {
							provider_id: provider_id.to_string(),
							direction: ask.direction,
							rail: ask.rail,
							asset: ask.asset.clone(),
							network: ask.network.clone(),
							country: ask.country.clone(),
							asked: ask.amount,
							supported,
							min: min.map(limit),
							max: max.map(limit),
							learned_at_ms: now_ms,
						});
					}
				}
				answer
			},
		};
		let state = match answer {
			None => FundingQuoteState::Unavailable { reason: FundingQuoteUnavailable::Timeout },
			Some(FundingQuoteAnswer::Refused { reason }) => FundingQuoteState::Unavailable {
				reason: FundingQuoteUnavailable::Refused { reason },
			},
			Some(FundingQuoteAnswer::Quoted { quote }) => {
				quotes.offer(intent, provider_id, quote.clone(), &ask, current_unix_millis());
				FundingQuoteState::Quoted { quote }
			},
		};
		FundingQuoteRow { provider_id: provider_id.to_string(), state }
	}
}

/// An ask sent to a provider: the receiver of its answer, and the hold on
/// the provider's worker.
struct PendingQuote<'a> {
	answered: futures::channel::oneshot::Receiver<FundingQuoteAnswer>,
	release: AskRelease<'a>,
}

/// Withdraws an ask and releases the provider's worker when dropped, so a
/// quote stream dropped while it waits leaves neither behind.
struct AskRelease<'a> {
	services: &'a RuntimeServices,
	provider_id: &'a str,
	ask_id: String,
}

impl<'a> PendingQuote<'a> {
	/// Hold `provider_id`'s worker and send it `ask`.
	fn ask(services: &'a RuntimeServices, provider_id: &'a str, ask: FundingQuoteAsk) -> Self {
		services.worker_ledger.acquire(provider_id);
		let (ask_id, answered) = services.funding().ask_quote(provider_id, ask);
		Self { answered, release: AskRelease { services, provider_id, ask_id } }
	}
}

impl Drop for AskRelease<'_> {
	fn drop(&mut self) {
		self.services.funding().withdraw_ask(&self.ask_id);
		self.services.worker_ledger.release(self.provider_id);
	}
}

/// What an answer shows about whether the provider serves the ask: a quote,
/// or a refusal of only the amount, means it does; refusing the country of
/// an ask that named one means it does not there; anything else may pass and
/// shows nothing. A country refusal for an ask without a country only says
/// the provider needs one, not that it serves nowhere.
fn shows_support(answer: &FundingQuoteAnswer, names_country: bool) -> Option<bool> {
	match answer {
		FundingQuoteAnswer::Quoted { .. } => Some(true),
		FundingQuoteAnswer::Refused { reason } => match reason {
			FundingQuoteRefusal::BelowMinimum { .. } | FundingQuoteRefusal::AboveMaximum { .. } => {
				Some(true)
			},
			FundingQuoteRefusal::CountryUnsupported => names_country.then_some(false),
			FundingQuoteRefusal::Unavailable | FundingQuoteRefusal::Other { .. } => None,
		},
	}
}

/// The minimum or maximum a refusal of only the amount showed.
fn limits_shown(answer: &FundingQuoteAnswer) -> (Option<u128>, Option<u128>) {
	match answer {
		FundingQuoteAnswer::Refused { reason: FundingQuoteRefusal::BelowMinimum { min } } => {
			(Some(*min), None)
		},
		FundingQuoteAnswer::Refused { reason: FundingQuoteRefusal::AboveMaximum { max } } => {
			(None, Some(*max))
		},
		_ => (None, None),
	}
}

/// The provider whose quote is best for the user in `direction`, among
/// those a session can still be handed to: the least to pay inbound, the
/// most to receive outbound. The earliest row wins a tie.
fn best_quote(
	direction: FundingDirection,
	rows: &[FundingQuoteRow],
	selectable: impl Fn(&str) -> bool,
) -> Option<String> {
	let quoted = rows.iter().filter_map(|row| match &row.state {
		FundingQuoteState::Quoted { quote } if selectable(&row.provider_id) => Some((row, quote)),
		_ => None,
	});
	// Amounts in the asset are compared at each quote's own decimals.
	let best = match direction {
		FundingDirection::In => {
			quoted.min_by_key(|(_, quote)| scaled(quote.send_amount, quote.asset_decimals))
		},
		FundingDirection::Out => quoted
			.rev()
			.max_by_key(|(_, quote)| scaled(quote.receive_amount, quote.asset_decimals)),
	};
	best.map(|(row, _)| row.provider_id.clone())
}

/// `amount` in an asset with `decimals`, at 18 decimals, so amounts quoted at
/// different decimals compare by value.
fn scaled(amount: u128, decimals: u8) -> u128 {
	const SCALE: u32 = 18;
	let decimals = u32::from(decimals);
	if decimals <= SCALE {
		amount.saturating_mul(10u128.pow(SCALE - decimals))
	} else {
		amount / 10u128.saturating_pow(decimals - SCALE)
	}
}

/// `raw` as an ISO 3166-1 alpha-2 code, uppercased, when it is one.
fn alpha2(raw: String) -> Option<String> {
	let code = raw.trim().to_ascii_uppercase();
	(code.len() == 2 && code.bytes().all(|byte| byte.is_ascii_uppercase())).then_some(code)
}

fn expired(quote: &FundingQuote, now_ms: u64) -> bool {
	quote.expires_at.is_some_and(|expires_at| now_ms >= expires_at)
}

#[cfg(test)]
mod tests {
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
		let refused =
			FundingQuoteAnswer::Refused { reason: FundingQuoteRefusal::CountryUnsupported };

		assert_eq!(
			(shows_support(&refused, true), shows_support(&refused, false)),
			(Some(false), None)
		);
	}

	fn row(provider_id: &str, send_amount: u128, receive_amount: u128) -> FundingQuoteRow {
		FundingQuoteRow {
			provider_id: provider_id.to_string(),
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
				provider_id: "silent.dot".to_string(),
				state: FundingQuoteState::Pending,
			},
		];

		let any = |_: &str| true;

		assert_eq!(
			(
				best_quote(FundingDirection::In, &rows, any),
				best_quote(FundingDirection::Out, &rows, any),
				best_quote(FundingDirection::In, &rows[3..], any),
				best_quote(FundingDirection::In, &rows, |provider_id| provider_id != "second.dot"),
			),
			(
				Some("second.dot".to_string()),
				Some("first.dot".to_string()),
				None,
				Some("third.dot".to_string()),
			)
		);
	}

	// A host asks again each time the user edits the amount, so a session's
	// expired quotes are dropped rather than piling up while its overlay is
	// open.
	#[test]
	fn offering_a_quote_drops_the_expired_ones() {
		let quotes = FundingQuotes::default();
		let ask = FundingQuoteAsk {
			direction: FundingDirection::In,
			rail: FundingRail::Card,
			asset: "EUR".to_string(),
			network: None,
			amount: 1_000,
			country: None,
		};
		quotes.offer("fs_1", "ramp.dot", quote("expired", Some(NOW)), &ask, NOW - 1);
		quotes.offer("fs_1", "ramp.dot", quote("live", None), &ask, NOW);

		let kept: Vec<String> = lock(&quotes.offered)
			.values()
			.flatten()
			.map(|choice| choice.quote.quote_id.clone())
			.collect();
		assert_eq!(kept, vec!["live".to_string()]);
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
}
