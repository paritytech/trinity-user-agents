//! Live quotes for a funding session, asked of each candidate's worker.
//!
//! The host asks once per amount, rail and asset; the core sends the ask to
//! every funding provider's worker as a `quote` request, whatever its manifest
//! says it serves, since a provider can serve more or less than it last
//! published. The worker prices it from its own API and answers. Each
//! provider's row resolves on its own: a provider that does not answer in time
//! is unavailable, so a slow one never holds the list. What an answer shows
//! about what the provider serves is kept for twelve hours and folded into the
//! candidates.

use std::{
	collections::HashMap,
	sync::{Arc, Mutex},
};

use core::time::Duration;

use futures::{
	future::{self, AbortHandle, Either},
	stream::{self, BoxStream, StreamExt},
};
use parity_scale_codec::Encode;
use truapi::latest::{
	FundingDirection, FundingQuote, FundingQuoteAnswer, FundingQuoteAsk, FundingQuoteRefusal,
};

use super::{funding::lock, services::RuntimeServices};
use crate::{
	host_logic::{funding::FundingChoice, funding_providers::LearnedSupport},
	platform::{FundingQuoteRow, FundingQuoteState, FundingQuoteUnavailable, ProductId},
	unix_time::current_unix_millis,
};

/// How long a provider has to answer an ask.
const QUOTE_DEADLINE: Duration = Duration::from_secs(10);
/// How long a provider's answer is reused for the same ask in the same
/// session, so editing the amount back and forth does not ask every provider
/// again. Another session asks afresh, so no two sessions share a quote.
const ANSWER_REUSE_MS: u64 = 30_000;
/// How many providers are asked at once, so a long list of published
/// providers does not start every one of their workers together.
const QUOTE_CONCURRENCY: usize = 8;
/// Longest quote id the core keeps.
const QUOTE_ID_LIMIT: usize = 128;

/// The session, the provider and the encoded ask an answer was given for.
type AnswerKey = (String, ProductId, Vec<u8>);

/// A provider's answer and when it came.
#[derive(Clone)]
struct RecentAnswer {
	at_ms: u64,
	answer: FundingQuoteAnswer,
}

/// Recent answers, the quotes each session was offered, and each session's
/// open ask.
#[derive(Default)]
pub struct FundingQuotes {
	/// Answers by session, provider and encoded ask, with when each came.
	answers: Mutex<HashMap<AnswerKey, RecentAnswer>>,
	/// Each session's latest ask, which a newer one or the session leaving
	/// the overlay withdraws.
	asks: Mutex<HashMap<String, AbortHandle>>,
	/// Quotes offered by session and provider, with the rail and asset each
	/// priced, so a selection can only name one the host was shown.
	offered: Mutex<HashMap<(String, ProductId), Vec<FundingChoice>>>,
}

impl FundingQuotes {
	/// The quote `quote_id` that `provider_id` offered for session `intent`,
	/// with what it priced, unless it expired by `now_ms`.
	pub fn offered(
		&self,
		intent: &str,
		provider_id: &ProductId,
		quote_id: &str,
		now_ms: u64,
	) -> Option<FundingChoice> {
		lock(&self.offered)
			.get(&(intent.to_string(), provider_id.clone()))?
			.iter()
			.find(|choice| choice.quote.quote_id == quote_id && !expired(&choice.quote, now_ms))
			.cloned()
	}

	/// Keep `quote` as offered for session `intent`, dropping the session's
	/// quotes from `provider_id` that expired by `now_ms`.
	pub fn offer(
		&self,
		intent: &str,
		provider_id: &ProductId,
		quote: FundingQuote,
		ask: &FundingQuoteAsk,
		now_ms: u64,
	) {
		let mut offered = lock(&self.offered);
		let choices = offered.entry((intent.to_string(), provider_id.clone())).or_default();
		choices
			.retain(|kept| kept.quote.quote_id != quote.quote_id && !expired(&kept.quote, now_ms));
		choices.push(FundingChoice { quote, rail: ask.rail, asset: ask.asset.clone() });
	}

	/// Drop the quotes offered for, and withdraw the ask of, every session
	/// `quotable` rejects: one that was assigned or ended can no longer be
	/// handed a quote.
	pub fn retain_sessions(&self, quotable: impl Fn(&str) -> bool) {
		lock(&self.offered).retain(|(intent, _), _| quotable(intent));
		lock(&self.asks).retain(|intent, ask| {
			let keep = quotable(intent);
			if !keep {
				ask.abort();
			}
			keep
		});
	}

	/// Start a new ask for session `intent`: the one before it is withdrawn,
	/// so its late answers reach neither the host nor the quotes the session
	/// can be handed, and the quotes it offered go with it.
	fn begin_ask(&self, intent: &str, ask: AbortHandle) {
		if let Some(previous) = lock(&self.asks).insert(intent.to_string(), ask) {
			previous.abort();
		}
		lock(&self.offered).retain(|(offered_for, _), _| offered_for != intent);
	}

	/// `provider_id`'s answer to the same ask for session `intent`, when it
	/// came recently and, if it is a quote, has not expired.
	fn reusable(
		&self,
		intent: &str,
		provider_id: &ProductId,
		ask: &FundingQuoteAsk,
		now_ms: u64,
	) -> Option<FundingQuoteAnswer> {
		let RecentAnswer { at_ms, answer } = lock(&self.answers)
			.get(&(intent.to_string(), provider_id.clone(), ask.encode()))?
			.clone();
		let fresh = now_ms.saturating_sub(at_ms) < ANSWER_REUSE_MS;
		let live = match &answer {
			FundingQuoteAnswer::Quoted { quote } => !expired(quote, now_ms),
			FundingQuoteAnswer::Refused { .. } => true,
		};
		(fresh && live).then_some(answer)
	}

	fn remember(
		&self,
		intent: &str,
		provider_id: &ProductId,
		ask: &FundingQuoteAsk,
		now_ms: u64,
		answer: FundingQuoteAnswer,
	) {
		let mut answers = lock(&self.answers);
		answers.retain(|_, recent| now_ms.saturating_sub(recent.at_ms) < ANSWER_REUSE_MS);
		answers.insert(
			(intent.to_string(), provider_id.clone(), ask.encode()),
			RecentAnswer { at_ms: now_ms, answer },
		);
	}
}

impl RuntimeServices {
	/// Ask every funding provider to price `ask` for session `intent`, the
	/// ask's direction being the session's. Each provider's row is first
	/// `Pending`, then `Quoted` or `Unavailable`; a quote that does not price
	/// the ask counts as the provider being unable to serve it. At most
	/// [`QUOTE_CONCURRENCY`] providers are asked at once, each with its own
	/// deadline. The stream ends once every provider has answered or run out
	/// of time, and never errors. It also ends, withdrawing the asks still
	/// open, when a newer ask for the session starts or the session leaves
	/// the overlay; dropping it does the same and lets the providers' workers
	/// go. Empty for a session the core does not know or that ended.
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
		let ask = FundingQuoteAsk { direction: session.direction, ..ask };
		self.refresh_funding_providers();
		let targets = self.funding_providers().quote_targets();
		let pending: Vec<_> = targets
			.iter()
			.map(|provider_id| FundingQuoteRow {
				provider_id: provider_id.clone(),
				state: FundingQuoteState::Pending,
			})
			.collect();
		let services = self.clone();
		let asked = intent.to_string();
		let answers = stream::iter(targets)
			.map(move |provider_id| {
				let services = services.clone();
				let intent = asked.clone();
				let ask = ask.clone();
				async move { services.quote_one(&intent, &provider_id, ask, deadline).await }
			})
			.buffer_unordered(QUOTE_CONCURRENCY);
		let (rows, withdraw) = stream::abortable(stream::iter(pending).chain(answers));
		self.funding_quotes().begin_ask(intent, withdraw);
		rows.boxed()
	}

	/// Ask one provider, holding its worker while it answers.
	async fn quote_one(
		self: &Arc<Self>,
		intent: &str,
		provider_id: &ProductId,
		ask: FundingQuoteAsk,
		deadline: Duration,
	) -> FundingQuoteRow {
		let quotes = &self.funding_quotes();
		// `None` with `timed_out` unset is an answer that does not price the ask.
		let (answer, timed_out) =
			match quotes.reusable(intent, provider_id, &ask, current_unix_millis()) {
				Some(answer) => (Some(answer), false),
				None => {
					let hold = WorkerHold::acquire(self, provider_id);
					let funding = self.funding().clone();
					let asked = {
						let ask = ask.clone();
						let provider_id = provider_id.clone();
						// A worker the host is stopping keeps its link until its
						// connection closes, and the one started in its place
						// connects soon after: a quote that fails on a link is
						// asked again on the next, within the same deadline.
						async move {
							let mut failed = None;
							loop {
								let link = funding
									.connected_link_besides(&provider_id, failed.as_ref())
									.await;
								if let Some(answer) = link.quote(ask.clone()).await {
									return Some(answer);
								}
								failed = Some(link);
							}
						}
					};
					let timeout = futures_timer::Delay::new(deadline);
					let (answer, timed_out) = match future::select(Box::pin(asked), timeout).await {
						Either::Left((answer, _)) => (answer, false),
						Either::Right(_) => (None, true),
					};
					drop(hold);
					let now_ms = current_unix_millis();
					let answer = answer.filter(|answer| prices(answer, &ask, now_ms));
					if let Some(answer) = &answer {
						quotes.remember(intent, provider_id, &ask, now_ms, answer.clone());
						if let Some(supported) = shows_support(answer, ask.country.is_some()) {
							self.learn_funding_support(LearnedSupport {
								provider_id: provider_id.clone(),
								direction: ask.direction,
								rail: ask.rail,
								asset: ask.asset.clone(),
								country: ask.country.clone(),
								supported,
								learned_at_ms: now_ms,
							});
						}
					}
					(answer, timed_out)
				},
			};
		let state = match answer {
			None if timed_out => {
				FundingQuoteState::Unavailable { reason: FundingQuoteUnavailable::Timeout }
			},
			None => FundingQuoteState::Unavailable {
				reason: FundingQuoteUnavailable::Refused {
					reason: FundingQuoteRefusal::Unavailable,
				},
			},
			Some(FundingQuoteAnswer::Refused { reason }) => FundingQuoteState::Unavailable {
				reason: FundingQuoteUnavailable::Refused { reason },
			},
			Some(FundingQuoteAnswer::Quoted { quote }) => {
				quotes.offer(intent, provider_id, quote.clone(), &ask, current_unix_millis());
				FundingQuoteState::Quoted { quote }
			},
		};
		FundingQuoteRow { provider_id: provider_id.clone(), state }
	}
}

/// A reference on a provider's worker while the core asks it for a quote,
/// released when dropped, so a quote stream dropped while it waits leaves
/// none behind.
struct WorkerHold<'a> {
	services: &'a RuntimeServices,
	provider: ProductId,
}

impl<'a> WorkerHold<'a> {
	/// Hold `provider`'s worker, so the host runs it to answer.
	fn acquire(services: &'a RuntimeServices, provider: &ProductId) -> Self {
		services.worker_ledger.acquire(provider);
		Self { services, provider: provider.clone() }
	}
}

impl Drop for WorkerHold<'_> {
	fn drop(&mut self) {
		self.services.worker_ledger.release(&self.provider);
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

fn expired(quote: &FundingQuote, now_ms: u64) -> bool {
	quote.expires_at.is_some_and(|expires_at| now_ms >= expires_at)
}

/// Whether `answer` is one the core can show for `ask`: any refusal, or a
/// live quote with a usable id that prices the ask's amount, moves something
/// both ways, and charges no more in fees than the user sends.
fn prices(answer: &FundingQuoteAnswer, ask: &FundingQuoteAsk, now_ms: u64) -> bool {
	let FundingQuoteAnswer::Quoted { quote } = answer else {
		return true;
	};
	let priced = match ask.direction {
		FundingDirection::In => quote.receive_amount,
		FundingDirection::Out => quote.send_amount,
	};
	!quote.quote_id.is_empty() &&
		quote.quote_id.len() <= QUOTE_ID_LIMIT &&
		!quote.quote_id.chars().any(char::is_control) &&
		priced == ask.amount &&
		quote.send_amount > 0 &&
		quote.receive_amount > 0 &&
		quote.provider_fee.saturating_add(quote.network_fee) <= quote.send_amount &&
		!expired(quote, now_ms)
}

#[cfg(test)]
mod tests;
