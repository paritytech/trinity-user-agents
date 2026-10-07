//! Live quotes for a funding session, asked of each candidate's worker.
//!
//! The host asks once per amount, rail and asset; the core sends the ask to
//! every candidate serving that rail and asset, on `serveSubscribe`, holding
//! its worker while it waits. The worker prices it from its own API, through
//! the onramp adapter when that needs the provider's key, and answers with
//! `answerQuote`. Each provider's row resolves on its own: a provider that does
//! not answer in time is unavailable, so a slow one never holds the list.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use core::time::Duration;

use futures::future::{self, Either};
use futures::stream::{self, BoxStream, StreamExt};
use parity_scale_codec::Encode;
use truapi::latest::{
    FundingQuote, FundingQuoteAnswer, FundingQuoteAsk, FundingQuoteRefusal, FundingRail,
};

use super::services::RuntimeServices;
use crate::platform::{FundingQuoteRow, FundingQuoteState, FundingQuoteUnavailable};
use crate::host_logic::worker_manifest::FundingMode;
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
    /// Quotes offered by session and provider, so a selection can only name
    /// one the host was shown.
    offered: Mutex<HashMap<(String, String), Vec<FundingQuote>>>,
}

impl FundingQuotes {
    /// The quote `quote_id` that `provider_id` offered for session `intent`,
    /// unless it expired by `now_ms`.
    pub fn offered(
        &self,
        intent: &str,
        provider_id: &str,
        quote_id: &str,
        now_ms: u64,
    ) -> Option<FundingQuote> {
        lock(&self.offered)
            .get(&(intent.to_string(), provider_id.to_string()))?
            .iter()
            .find(|quote| quote.quote_id == quote_id && !expired(quote, now_ms))
            .cloned()
    }

    fn offer(&self, intent: &str, provider_id: &str, quote: FundingQuote) {
        let mut offered = lock(&self.offered);
        let quotes = offered
            .entry((intent.to_string(), provider_id.to_string()))
            .or_default();
        quotes.retain(|kept| kept.quote_id != quote.quote_id);
        quotes.push(quote);
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
    /// Ask every candidate for session `intent` that serves `ask`'s rail and
    /// asset for a price. Each provider's row is first `Pending`, then
    /// `Quoted` or `Unavailable`; a route whose declared countries leave out
    /// the user's is unavailable without asking. The ask's direction is the
    /// session's. Empty for a session the core does not know or that ended.
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
        let mode = match ask.rail {
            FundingRail::Card => FundingMode::Card,
            FundingRail::Bank => FundingMode::Bank,
            FundingRail::Crypto => FundingMode::Crypto,
        };
        let mut rows = Vec::new();
        for candidate in self.funding_candidates(session.direction) {
            let routes: Vec<_> = candidate
                .routes
                .iter()
                .filter(|route| route.mode == mode && route.assets.contains(&ask.asset))
                .collect();
            if routes.is_empty() {
                continue;
            }
            let provider_id = candidate.provider_id;
            let outside_countries = ask.country.as_ref().is_some_and(|country| {
                routes.iter().all(|route| {
                    route
                        .countries
                        .as_ref()
                        .is_some_and(|countries| !countries.contains(country))
                })
            });
            if outside_countries {
                let row = FundingQuoteRow {
                    provider_id,
                    state: FundingQuoteState::Unavailable {
                        reason: FundingQuoteUnavailable::Refused {
                            reason: FundingQuoteRefusal::CountryUnsupported,
                        },
                    },
                };
                rows.push(stream::once(future::ready(row)).boxed());
                continue;
            }
            let pending = FundingQuoteRow {
                provider_id: provider_id.clone(),
                state: FundingQuoteState::Pending,
            };
            let services = self.clone();
            let intent = intent.to_string();
            let ask = ask.clone();
            rows.push(
                stream::once(future::ready(pending))
                    .chain(stream::once(async move {
                        services.quote_one(&intent, &provider_id, ask, deadline).await
                    }))
                    .boxed(),
            );
        }
        stream::select_all(rows).boxed()
    }

    /// Ask one provider, holding its worker while it answers.
    async fn quote_one(
        &self,
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
                    quotes.remember(provider_id, &ask, current_unix_millis(), answer.clone());
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
                quotes.offer(intent, provider_id, quote.clone());
                FundingQuoteState::Quoted { quote }
            }
        };
        FundingQuoteRow {
            provider_id: provider_id.to_string(),
            state,
        }
    }
}

fn expired(quote: &FundingQuote, now_ms: u64) -> bool {
    quote.expires_at.is_some_and(|expires_at| now_ms >= expires_at)
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}
