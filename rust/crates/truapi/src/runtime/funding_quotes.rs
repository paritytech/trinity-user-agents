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
//!
//! A session choosing a provider has one watch: the ask, the list its host
//! was last shown, and the task that asks every provider again each minute.
//! The watch owns the task, the task owns the quote calls, and each call holds
//! its provider's worker, so removing the watch is the only way quotes end and
//! lets every worker they held go.

use std::{
	collections::HashMap,
	sync::{Arc, Mutex, Weak},
};

use core::time::Duration;

use tracing::warn;

use futures::{
	future::{self, AbortHandle, AbortRegistration, Abortable},
	stream::{self, StreamExt},
};
use truapi::latest::{
	FundingDirection, FundingQuote, FundingQuoteAnswer, FundingQuoteAsk, FundingQuoteRefusal,
};

use super::{
	funding::{FundingSessionBook, lock, within},
	services::RuntimeServices,
};
use crate::{
	host_logic::{
		funding::FundingChoice,
		funding_providers::{FundingSelection, LearnedLimit, LearnedSupport},
		funding_view,
	},
	platform::{
		FundingQuoteList, FundingQuoteRow, FundingQuoteState, FundingQuoteUnavailable, ProductId,
	},
	unix_time::current_unix_millis,
};

/// How long a provider has to answer an ask.
const QUOTE_DEADLINE: Duration = Duration::from_secs(10);
/// How many providers are asked at once, so a long list of published
/// providers does not start every one of their workers together.
const QUOTE_CONCURRENCY: usize = 8;
/// Longest quote id the core keeps.
const QUOTE_ID_LIMIT: usize = 128;
/// How long a quote list stands before the core asks every provider again.
const QUOTE_REFRESH: Duration = Duration::from_secs(60);
/// How long the country the host detected is reused before asking again.
const DETECTED_COUNTRY_REUSE_MS: u64 = 60 * 60 * 1_000;
/// How long a detection that found no country stands before asking again,
/// as location or a SIM can become available soon after.
const UNDETECTED_COUNTRY_RETRY_MS: u64 = 60 * 1_000;

/// Each session's quote request, and the country the host detected.
#[derive(Default)]
pub struct FundingQuotes {
	/// Every session's quote request, behind one lock.
	watches: Mutex<Watches>,
	/// The country the host last detected.
	detected: DetectedCountry,
}

#[derive(Default)]
struct Watches {
	/// The id the next request takes, whatever the session. It only grows,
	/// and is taken under the same lock as the insert, so a session's entry is
	/// always its newest request.
	next_request: u64,
	/// The current request of each session that is choosing a provider. A
	/// session with no entry is not being quoted.
	by_session: HashMap<String, Watch>,
}

/// One session's quote request. Dropping it stops its rounds.
struct Watch {
	/// The id every list of this request carries.
	request_id: u64,
	/// What every round prices. A quote the session is handed takes its rail,
	/// asset and network from here.
	ask: FundingQuoteAsk,
	/// The rows the host was last shown; their quotes are the ones the
	/// session can be handed to.
	shown: Vec<FundingQuoteRow>,
	/// Ends the rounds, with every quote still asked and the worker it holds.
	rounds: AbortHandle,
	/// Whether to hand the session to the best quote once every row
	/// answered.
	start_when_quoted: bool,
}

impl Drop for Watch {
	fn drop(&mut self) {
		self.rounds.abort();
	}
}

impl FundingQuotes {
	/// Make `ask` session `intent`'s quote request, replacing, and so
	/// stopping, the one before it. Returns the request's id and what its
	/// rounds run under.
	fn start(&self, intent: &str, ask: FundingQuoteAsk) -> (u64, AbortRegistration) {
		let (rounds, registration) = AbortHandle::new_pair();
		let mut watches = lock(&self.watches);
		watches.next_request += 1;
		let request_id = watches.next_request;
		watches.by_session.insert(
			intent.to_string(),
			Watch { request_id, ask, shown: Vec::new(), rounds, start_when_quoted: false },
		);
		(request_id, registration)
	}

	/// Stop session `intent`'s quotes, with every quote still asked, which
	/// lets their workers go.
	pub fn end(&self, intent: &str) {
		lock(&self.watches).by_session.remove(intent);
	}

	/// Arm session `intent`'s request `request_id` to start on its best
	/// quote once every row answered. Returns whether it is the current one.
	fn arm(&self, intent: &str, request_id: u64) -> bool {
		lock(&self.watches)
			.by_session
			.get_mut(intent)
			.filter(|watch| watch.request_id == request_id)
			.map(|watch| watch.start_when_quoted = true)
			.is_some()
	}

	/// Disarm session `intent`'s start when quoted.
	fn disarm(&self, intent: &str) {
		if let Some(watch) = lock(&self.watches).by_session.get_mut(intent) {
			watch.start_when_quoted = false;
		}
	}

	/// Disarm the start of `list`'s request once every row of it answered,
	/// and say whether it was armed.
	fn take_armed(&self, intent: &str, list: &FundingQuoteList) -> bool {
		if list.rows.iter().any(|row| row.state == FundingQuoteState::Pending) {
			return false;
		}
		lock(&self.watches)
			.by_session
			.get_mut(intent)
			.filter(|watch| watch.request_id == list.request_id)
			.is_some_and(|watch| core::mem::take(&mut watch.start_when_quoted))
	}

	/// Disarm session `intent`'s start when quoted, and stop its quotes when
	/// `selection` names another rail, asset, network or country than their
	/// ask, clearing its list from `book`. Returns whether it stopped them.
	fn reselect(
		&self,
		intent: &str,
		selection: &FundingSelection,
		book: &FundingSessionBook,
	) -> bool {
		let mut watches = lock(&self.watches);
		let Some(watch) = watches.by_session.get_mut(intent) else {
			return false;
		};
		watch.start_when_quoted = false;
		let ask = &watch.ask;
		// A field the user has not picked yet does not move the ask.
		let kept = |picked: &Option<String>, asked: Option<&str>| {
			picked.as_deref().is_none_or(|picked| Some(picked) == asked)
		};
		if ask.rail == selection.rail &&
			kept(&selection.asset, Some(&ask.asset)) &&
			kept(&selection.network, ask.network.as_deref()) &&
			kept(&selection.country, ask.country.as_deref())
		{
			return false;
		}
		watches.by_session.remove(intent);
		book.quotes_withdrawn(intent);
		true
	}

	/// Keep `list` as what session `intent`'s host is shown, recorded in
	/// `book`, and say whether to show it: only a list of the session's
	/// current request, while the session can still be quoted. A session that
	/// cannot loses its request.
	fn publish(
		&self,
		intent: &str,
		list: &FundingQuoteList,
		quotable: bool,
		book: &FundingSessionBook,
	) -> bool {
		let mut watches = lock(&self.watches);
		let Some(watch) = watches
			.by_session
			.get_mut(intent)
			.filter(|watch| watch.request_id == list.request_id)
		else {
			return false;
		};
		if quotable {
			watch.shown = list.rows.clone();
			book.quotes_changed(intent, list.clone());
			return true;
		}
		watches.by_session.remove(intent);
		false
	}

	/// The quote `quote_id` that `provider_id` offered in the list session
	/// `intent`'s host was last shown, with what it priced, unless it expired
	/// by `now_ms`.
	pub fn offered(
		&self,
		intent: &str,
		provider_id: &ProductId,
		quote_id: &str,
		now_ms: u64,
	) -> Option<FundingChoice> {
		let watches = lock(&self.watches);
		let watch = watches.by_session.get(intent)?;
		watch.shown.iter().find_map(|row| match &row.state {
			FundingQuoteState::Quoted { quote }
				if row.provider_id == *provider_id &&
					quote.quote_id == quote_id &&
					!expired(quote, now_ms) =>
			{
				Some(FundingChoice {
					quote: quote.clone(),
					rail: watch.ask.rail,
					asset: watch.ask.asset.clone(),
					network: watch.ask.network.clone(),
				})
			},
			_ => None,
		})
	}

	/// The rows session `intent`'s host was last shown.
	#[cfg(test)]
	pub(crate) fn shown(&self, intent: &str) -> Option<Vec<FundingQuoteRow>> {
		lock(&self.watches).by_session.get(intent).map(|watch| watch.shown.clone())
	}

	/// Show `rows` for `ask` in session `intent`, as a round would, without
	/// asking any provider.
	#[cfg(test)]
	pub(crate) fn show(&self, intent: &str, ask: FundingQuoteAsk, rows: Vec<FundingQuoteRow>) {
		let amount = ask.amount;
		let (request_id, _) = self.start(intent, ask);
		let list = FundingQuoteList {
			request_id,
			rows,
			best: None,
			refresh_at_ms: 0,
			amount,
			refusal: None,
			issue: None,
		};
		self.publish(intent, &list, true, &FundingSessionBook::default());
	}
}

/// The country the host last detected, and when.
#[derive(Default)]
struct DetectedCountry(Mutex<Option<(u64, Option<String>)>>);

impl DetectedCountry {
	/// The country detected at most an hour before `now_ms`, or a minute
	/// before when none was found; `None` once it is due to be asked again.
	fn recent(&self, now_ms: u64) -> Option<Option<String>> {
		let (at_ms, country) = lock(&self.0).clone()?;
		let reuse_ms = match country {
			Some(_) => DETECTED_COUNTRY_REUSE_MS,
			None => UNDETECTED_COUNTRY_RETRY_MS,
		};
		(now_ms.saturating_sub(at_ms) < reuse_ms).then_some(country)
	}

	fn store(&self, now_ms: u64, country: Option<String>) {
		*lock(&self.0) = Some((now_ms, country));
	}
}

impl RuntimeServices {
	/// Quotes for funding session `intent`, sent to the host through
	/// [`crate::platform::FundingPlatform::funding_quotes_changed`]. Every
	/// funding provider whose routes carry the ask's rail, or that declares
	/// none, is asked to price `ask` in the session's direction; an ask
	/// naming no country takes the one the host detected. Each list is
	/// complete and replaces the one before: a provider's row is `Pending`
	/// until it first answers, then `Quoted` or `Unavailable`, and keeps its
	/// answer until it answers again. A quote that does not price the ask
	/// counts as the provider being unable to serve it. At most
	/// [`QUOTE_CONCURRENCY`] providers are asked at once, each with its own
	/// deadline, and every provider is asked again at the list's
	/// `refresh_at_ms`, or at once when the providers change. Quotes stop once
	/// the session is handed to a provider, ends or leaves the overlay, a
	/// newer request replaces this one, or a selection names another route,
	/// and no list of this request reaches the host after that. Returns the
	/// request id every list carries.
	pub fn request_funding_quotes(self: &Arc<Self>, intent: &str, ask: FundingQuoteAsk) -> u64 {
		self.request_funding_quotes_within(intent, ask, QUOTE_DEADLINE, QUOTE_REFRESH)
	}

	/// [`Self::request_funding_quotes`] with `deadline` for each provider and
	/// a new round every `refresh`.
	pub(crate) fn request_funding_quotes_within(
		self: &Arc<Self>,
		intent: &str,
		ask: FundingQuoteAsk,
		deadline: Duration,
		refresh: Duration,
	) -> u64 {
		let direction =
			self.funding().get(intent).map_or(ask.direction, |session| session.direction);
		let ask = FundingQuoteAsk { direction, ..ask };
		let (request_id, registration) = self.funding_quotes().start(intent, ask.clone());
		let rounds =
			self.clone()
				.quote_rounds(intent.to_string(), request_id, ask, deadline, refresh);
		(self.spawner)(Box::pin(async move {
			let _ = Abortable::new(rounds, registration).await;
		}));
		request_id
	}

	/// Request `request_id`'s rounds for session `intent`: every provider's
	/// row, then each answer as it comes, and the same again every `refresh`,
	/// until a list is refused.
	async fn quote_rounds(
		self: Arc<Self>,
		intent: String,
		request_id: u64,
		ask: FundingQuoteAsk,
		deadline: Duration,
		refresh: Duration,
	) {
		let country = match ask.country {
			Some(country) => Some(country),
			None => self.detected_funding_country().await,
		};
		let ask = FundingQuoteAsk { country, ..ask };
		let mut rows: Vec<FundingQuoteRow> = Vec::new();
		loop {
			// The providers changing starts the next round at once, so one
			// dotNS answered for meanwhile is asked too.
			let mut providers_changed = self.funding_providers().on_change();
			self.refresh_funding_providers();
			let targets = self.funding_providers().quote_targets_for(ask.direction, ask.rail);
			// A row keeps its last answer until the provider answers again.
			rows = targets
				.iter()
				.map(|provider_id| {
					rows.iter().find(|row| row.provider_id == *provider_id).cloned().unwrap_or_else(
						|| FundingQuoteRow {
							provider_id: provider_id.clone(),
							state: FundingQuoteState::Pending,
						},
					)
				})
				.collect();
			let refresh_at_ms = current_unix_millis().saturating_add(refresh.as_millis() as u64);
			let mut list = FundingQuoteList {
				request_id,
				rows,
				best: None,
				refresh_at_ms,
				amount: ask.amount,
				refusal: None,
				issue: None,
			};
			if !self.publish_quotes(&intent, &mut list) {
				return;
			}
			let mut answers = stream::iter(targets)
				.map(|provider_id| self.quote_one(provider_id, &ask, deadline))
				.buffer_unordered(QUOTE_CONCURRENCY);
			while let Some(answer) = answers.next().await {
				if let Some(row) =
					list.rows.iter_mut().find(|row| row.provider_id == answer.provider_id)
				{
					*row = answer;
				}
				if !self.publish_quotes(&intent, &mut list) {
					return;
				}
			}
			rows = list.rows;
			let wait = refresh_at_ms.saturating_sub(current_unix_millis());
			let wait = futures_timer::Delay::new(Duration::from_millis(wait));
			let _ = future::select(wait, providers_changed.next()).await;
		}
	}

	/// Hand session `intent` to the best quote of its request `request_id`
	/// once every row of it answered, as the host would on the user's pick.
	/// A new request, a selection, [`Self::cancel_funding_auto_start`] or a
	/// list without a usable quote disarms it.
	pub fn start_funding_when_quoted(self: &Arc<Self>, intent: &str, request_id: u64) {
		if self.funding_quotes().arm(intent, request_id) &&
			let Some(list) = self.funding().book().quotes(intent)
		{
			self.start_if_quoted(intent, &list);
		}
	}

	/// Disarm [`Self::start_funding_when_quoted`] for session `intent`, as the
	/// user leaves its quotes.
	pub fn cancel_funding_auto_start(&self, intent: &str) {
		self.funding_quotes().disarm(intent);
	}

	/// Start session `intent` on `list`'s best quote when its request was
	/// armed and every row answered.
	fn start_if_quoted(self: &Arc<Self>, intent: &str, list: &FundingQuoteList) {
		if !self.funding_quotes().take_armed(intent, list) {
			return;
		}
		let Some((provider_id, quote_id)) = list.rows.iter().find_map(|row| match &row.state {
			FundingQuoteState::Quoted { quote } if list.best.as_ref() == Some(&row.provider_id) => {
				Some((row.provider_id.clone(), quote.quote_id.clone()))
			},
			_ => None,
		}) else {
			return;
		};
		let (services, intent) = (self.clone(), intent.to_string());
		(self.spawner)(Box::pin(async move {
			if let Err(error) =
				services.select_funding_provider(&intent, &provider_id, &quote_id).await
			{
				warn!(%intent, %error, "funding session not started when quoted");
			}
		}));
	}

	/// Stop session `intent`'s quotes when `selection` no longer matches their
	/// ask, telling the host its list is gone; see [`FundingQuotes::reselect`].
	pub fn withdraw_unselected_quotes(&self, intent: &str, selection: &FundingSelection) {
		if self.funding_quotes().reselect(intent, selection, self.funding().book()) &&
			let Some(feed) = self.funding().feed()
		{
			feed.funding_quotes_withdrawn(intent.to_string());
		}
	}

	/// Mark `list`'s best quote and show it to session `intent`'s host, unless
	/// it no longer answers the session's request or the session can no
	/// longer be quoted. Returns whether it was shown.
	fn publish_quotes(self: &Arc<Self>, intent: &str, list: &mut FundingQuoteList) -> bool {
		let book = self.funding().book();
		// A session the core no longer knows loses its request.
		let Some(session) = self.funding().get(intent) else {
			return self.funding_quotes().publish(intent, list, false, book);
		};
		list.best = best_quote(session.direction, &list.rows, |provider_id| {
			self.funding_providers().is_quote_target(provider_id)
		});
		list.refusal = funding_view::quotes_refusal(&list.rows);
		list.issue = funding_view::quotes_issue(&list.rows);
		let quotable = !session.is_terminal() && session.provider_id.is_none();
		if !self.funding_quotes().publish(intent, list, quotable, book) {
			return false;
		}
		if let Some(platform) = self.funding().platform() {
			platform.funding_quotes_changed(intent.to_string(), list.clone());
		}
		self.send_funding_options(intent);
		self.start_if_quoted(intent, list);
		true
	}

	/// The country the host detected within the reuse window, without
	/// asking the host: `None` when it is due to be asked again, `Some(None)`
	/// when it found none.
	pub fn recent_funding_country(&self) -> Option<Option<String>> {
		self.funding_quotes().detected.recent(current_unix_millis())
	}

	/// The country the host detected for the user as an ISO 3166-1 alpha-2
	/// code, reused for an hour. A detection that found none, or answered
	/// something that is not a two-letter code, is asked again after a
	/// minute.
	pub async fn detected_funding_country(&self) -> Option<String> {
		let now_ms = current_unix_millis();
		let detected = &self.funding_quotes().detected;
		if let Some(country) = detected.recent(now_ms) {
			return country;
		}
		let platform = self.funding().platform()?;
		let country = platform.detected_country().await.and_then(alpha2);
		detected.store(now_ms, country.clone());
		country
	}

	/// Ask `provider_id` to price `ask`, holding its worker while it answers.
	async fn quote_one(
		self: &Arc<Self>,
		provider_id: ProductId,
		ask: &FundingQuoteAsk,
		deadline: Duration,
	) -> FundingQuoteRow {
		let hold = WorkerHold::acquire(self, &provider_id);
		// A worker the host is stopping keeps its link until its connection
		// closes, and the one started in its place connects soon after: a
		// quote that fails on a link is asked again on the next, within the
		// same deadline.
		let asked = async {
			let mut failed = None;
			loop {
				let link =
					self.funding().connected_link_besides(&provider_id, failed.as_ref()).await;
				if let Some(answer) = link.quote(ask.clone()).await {
					return answer;
				}
				failed = Some(link);
			}
		};
		let answer = within(deadline, asked).await;
		drop(hold);
		let now_ms = current_unix_millis();
		let state = match answer {
			None => FundingQuoteState::Unavailable { reason: FundingQuoteUnavailable::Timeout },
			Some(answer) if !prices(&answer, ask, now_ms) => FundingQuoteState::Unavailable {
				reason: FundingQuoteUnavailable::Refused {
					reason: FundingQuoteRefusal::Unavailable,
				},
			},
			Some(answer) => {
				self.learn_from(&provider_id, ask, &answer, now_ms);
				match answer {
					FundingQuoteAnswer::Refused { reason } => FundingQuoteState::Unavailable {
						reason: FundingQuoteUnavailable::Refused { reason },
					},
					FundingQuoteAnswer::Quoted { quote } => FundingQuoteState::Quoted { quote },
				}
			},
		};
		FundingQuoteRow { provider_id, state }
	}

	/// Keep what `provider_id`'s `answer` to `ask` shows about what it serves.
	fn learn_from(
		self: &Arc<Self>,
		provider_id: &ProductId,
		ask: &FundingQuoteAsk,
		answer: &FundingQuoteAnswer,
		now_ms: u64,
	) {
		let Some(supported) = shows_support(answer, ask.country.is_some()) else {
			return;
		};
		let (min, max) = answer.limits_shown();
		let limit = |amount| LearnedLimit { amount, learned_at_ms: now_ms };
		self.learn_funding_support(LearnedSupport {
			provider_id: provider_id.clone(),
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

/// A reference on a provider's worker while the core needs it: for a quote,
/// while a session's overlay is up, or while the provider serves a session.
/// Released when dropped, so a call dropped while it waits leaves none
/// behind. Taking and releasing one calls into the host, so neither happens
/// under a lock.
pub struct WorkerHold {
	services: Weak<RuntimeServices>,
	provider: ProductId,
}

impl WorkerHold {
	/// Hold `provider`'s worker, so the host runs it to answer.
	pub fn acquire(services: &Arc<RuntimeServices>, provider: &ProductId) -> Self {
		services.worker_ledger.acquire(provider);
		Self { services: Arc::downgrade(services), provider: provider.clone() }
	}

	/// The provider whose worker this holds.
	pub fn provider(&self) -> &ProductId {
		&self.provider
	}
}

impl Drop for WorkerHold {
	fn drop(&mut self) {
		// Services that are gone took their ledger with them.
		if let Some(services) = self.services.upgrade() {
			services.worker_ledger.release(&self.provider);
		}
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

impl FundingQuoteAnswer {
	/// The minimum or maximum a refusal of only the amount showed.
	fn limits_shown(&self) -> (Option<u128>, Option<u128>) {
		match self {
			FundingQuoteAnswer::Refused { reason: FundingQuoteRefusal::BelowMinimum { min } } => {
				(Some(*min), None)
			},
			FundingQuoteAnswer::Refused { reason: FundingQuoteRefusal::AboveMaximum { max } } => {
				(None, Some(*max))
			},
			_ => (None, None),
		}
	}
}

/// The provider whose quote is best for the user in `direction`, among
/// those a session can still be handed to: the least to pay inbound, the
/// most to receive outbound. The earliest row wins a tie.
fn best_quote(
	direction: FundingDirection,
	rows: &[FundingQuoteRow],
	selectable: impl Fn(&ProductId) -> bool,
) -> Option<ProductId> {
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
