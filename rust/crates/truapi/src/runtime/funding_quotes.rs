//! Live quotes for a funding session, asked of each candidate's worker.
//!
//! The host asks once per amount, rail and asset; the core sends the ask to
//! every funding provider, on `serveSubscribe`, whatever its manifest says it
//! serves, since a provider can serve more or less than it last published.
//! The worker prices it from its own API, through the onramp adapter when that
//! needs the provider's key, and answers with `answerQuote`. Each provider's
//! row resolves on its own: a provider that does not answer in time is
//! unavailable, so a slow one never holds the list. What an answer shows about
//! what the provider serves is kept for twelve hours and folded into the
//! candidates.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use core::time::Duration;

use futures::future::{self, Either};
use futures::stream::{self, BoxStream, StreamExt};
use parity_scale_codec::Encode;
use truapi::latest::{FundingQuote, FundingQuoteAnswer, FundingQuoteAsk, FundingQuoteRefusal};

use super::services::RuntimeServices;
use crate::platform::{FundingQuoteRow, FundingQuoteState, FundingQuoteUnavailable};
use crate::host_logic::funding::FundingChoice;
use crate::host_logic::funding_providers::LearnedSupport;
use crate::unix_time::current_unix_millis;

/// How long a provider has to answer an ask.
const QUOTE_DEADLINE: Duration = Duration::from_secs(10);
/// How long a provider's answer is reused for the same ask, so editing the
/// amount back and forth does not ask every provider again.
const ANSWER_REUSE_MS: u64 = 30_000;

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

    fn offer(&self, intent: &str, provider_id: &str, quote: FundingQuote, ask: &FundingQuoteAsk) {
        let mut offered = lock(&self.offered);
        let choices = offered
            .entry((intent.to_string(), provider_id.to_string()))
            .or_default();
        choices.retain(|kept| kept.quote.quote_id != quote.quote_id);
        choices.push(FundingChoice {
            quote,
            rail: ask.rail,
            asset: ask.asset.clone(),
            amount: ask.amount,
        });
    }

    /// `provider_id`'s answer to the same ask, when it came recently and, if
    /// it is a quote, has not expired.
    fn reusable(&self, provider_id: &str, ask: &FundingQuoteAsk, now_ms: u64) -> Option<FundingQuoteAnswer> {
        let RecentAnswer { at_ms, answer } = lock(&self.answers)
            .get(&(provider_id.to_string(), ask.encode()))?
            .clone();
        let fresh = now_ms.saturating_sub(at_ms) < ANSWER_REUSE_MS;
        let live = match &answer {
            FundingQuoteAnswer::Quoted { quote } => !expired(quote, now_ms),
            FundingQuoteAnswer::Refused { .. } => true,
        };
        (fresh && live).then_some(answer)
    }

    fn remember(&self, provider_id: &str, ask: &FundingQuoteAsk, now_ms: u64, answer: FundingQuoteAnswer) {
        let mut answers = lock(&self.answers);
        answers.retain(|_, recent| now_ms.saturating_sub(recent.at_ms) < ANSWER_REUSE_MS);
        answers.insert(
            (provider_id.to_string(), ask.encode()),
            RecentAnswer {
                at_ms: now_ms,
                answer,
            },
        );
    }
}

impl RuntimeServices {
    /// Ask every funding provider to price `ask` for session `intent`. Each
    /// provider's row is first `Pending`, then `Quoted` or `Unavailable`. The
    /// ask's direction is the session's. Empty for a session the core does
    /// not know or that ended.
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
        let ask = FundingQuoteAsk {
            direction: session.direction,
            ..ask
        };
        self.refresh_funding_providers();
        let rows: Vec<_> = self
            .funding_providers
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

    /// Ask one provider, holding its worker while it answers.
    async fn quote_one(
        self: &Arc<Self>,
        intent: &str,
        provider_id: &str,
        ask: FundingQuoteAsk,
        deadline: Duration,
    ) -> FundingQuoteRow {
        let quotes = &self.funding_quotes;
        let answer = match quotes.reusable(provider_id, &ask, current_unix_millis()) {
            Some(answer) => Some(answer),
            None => {
                let registry = self.funding();
                self.worker_ledger.acquire(provider_id);
                let (ask_id, answered) = registry.ask_quote(provider_id, ask.clone());
                let timeout = futures_timer::Delay::new(deadline);
                let answer = match future::select(answered, timeout).await {
                    Either::Left((Ok(answer), _)) => Some(answer),
                    _ => {
                        registry.withdraw_ask(&ask_id);
                        None
                    }
                };
                self.worker_ledger.release(provider_id);
                if let Some(answer) = &answer {
                    let now_ms = current_unix_millis();
                    quotes.remember(provider_id, &ask, now_ms, answer.clone());
                    if let Some(supported) = shows_support(answer) {
                        let (min, max) = limits_shown(answer);
                        self.learn_funding_support(LearnedSupport {
                            provider_id: provider_id.to_string(),
                            direction: ask.direction,
                            rail: ask.rail,
                            asset: ask.asset.clone(),
                            network: ask.network.clone(),
                            country: ask.country.clone(),
                            supported,
                            min,
                            max,
                            learned_at_ms: now_ms,
                        });
                    }
                }
                answer
            }
        };
        let state = match answer {
            None => FundingQuoteState::Unavailable {
                reason: FundingQuoteUnavailable::Timeout,
            },
            Some(FundingQuoteAnswer::Refused { reason }) => FundingQuoteState::Unavailable {
                reason: FundingQuoteUnavailable::Refused { reason },
            },
            Some(FundingQuoteAnswer::Quoted { quote }) => {
                quotes.offer(intent, provider_id, quote.clone(), &ask);
                FundingQuoteState::Quoted { quote }
            }
        };
        FundingQuoteRow {
            provider_id: provider_id.to_string(),
            state,
        }
    }
}

/// What an answer shows about whether the provider serves the ask: a quote,
/// or a refusal of only the amount, means it does; refusing the country means
/// it does not there; anything else may pass and shows nothing.
fn shows_support(answer: &FundingQuoteAnswer) -> Option<bool> {
    match answer {
        FundingQuoteAnswer::Quoted { .. } => Some(true),
        FundingQuoteAnswer::Refused { reason } => match reason {
            FundingQuoteRefusal::BelowMinimum { .. } | FundingQuoteRefusal::AboveMaximum { .. } => {
                Some(true)
            }
            FundingQuoteRefusal::CountryUnsupported => Some(false),
            FundingQuoteRefusal::Unavailable | FundingQuoteRefusal::Other { .. } => None,
        },
    }
}

/// The minimum or maximum a refusal of only the amount showed.
fn limits_shown(answer: &FundingQuoteAnswer) -> (Option<u128>, Option<u128>) {
    match answer {
        FundingQuoteAnswer::Refused {
            reason: FundingQuoteRefusal::BelowMinimum { min },
        } => (Some(*min), None),
        FundingQuoteAnswer::Refused {
            reason: FundingQuoteRefusal::AboveMaximum { max },
        } => (None, Some(*max)),
        _ => (None, None),
    }
}

fn expired(quote: &FundingQuote, now_ms: u64) -> bool {
    quote.expires_at.is_some_and(|expires_at| now_ms >= expires_at)
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}
