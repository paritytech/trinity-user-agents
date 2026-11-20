//! The CLI signing host's funding surface: the overlay, provider screens, and
//! the user's answers, as a host app serves them.
//!
//! The host offers the funding providers published on dotNS that
//! `TRUAPI_FUNDING_PROVIDER_NAMES` names, each by its name without a TLD
//! (`fundtest-provider`), as an app's remote configuration lists them: the
//! core reads each one on the stack's TLD, with its manifest from dotNS, and
//! the host runs the worker each one publishes, fetched from dotNS.
//!
//! `TRUAPI_FUNDING_WORKERS` runs a local script as an offered provider's
//! worker instead: each entry is a product id, `=` and the path of the script
//! (`fundtest-provider.paseo=./worker.ts`). The host launches that script when
//! the core asks for the provider's worker, and stops it when the core lets
//! it go. The provider's manifest still comes from dotNS.
//!
//! A product's funding request shows the overlay: the user takes the core's
//! default rail and, for crypto, its first token and network, or the route
//! `TRUAPI_FUNDING_SELECT` names, the core asks
//! each provider routing that rail for a quote, and the user picks a quote or
//! closes the overlay. Every question goes through the
//! host's approval policy, as its other confirmations do: a prompt on a
//! terminal, the first quote and every offer accepted under `--auto-accept`,
//! and a refusal when a prompt has no terminal to ask on. The user cancels a session with
//! `/funding cancel <intent>`, and the provider's cancel offer is put to them
//! the same way.
//!
//! A provider screen cannot be drawn here: its route or URL is printed, and
//! the user finishing it closes it, or returns to the page's return URL. The
//! user acts on a provider's App screen with `/funding write`, which writes
//! the provider's storage as the screen would.
//!
//! With `TRUAPI_FUNDING_LEDGER` set, the host also serves top-ups and payment
//! requests from a ledger file: every one completes in full once started, and
//! the ledger keeps them across a restart. There is no chain behind it.
//!
//! Every overlay, screen, cancel, answer, session change and worker launch,
//! stop or exit is appended to the transcript named by `TRUAPI_FUNDING_LOG`,
//! one JSON object per line, so a battery can assert what the host saw rather
//! than only what the product was told.

use std::{
	collections::{BTreeMap, HashMap, HashSet},
	fs::OpenOptions,
	io::Write,
	path::PathBuf,
	sync::{Arc, Mutex, Weak},
};

use futures::stream::{self, BoxStream, StreamExt};
use tokio::sync::watch;
use truapi::{
	FundingFeed, HostRuntime, SigningHost,
	host_logic::{
		funding::{CancelTerms, FundingCancel, FundingSession},
		funding_providers::{
			FUNDING_MIN_AMOUNT, FundingOptions, FundingSelection, provider_config,
		},
	},
	latest::{
		FundingDirection, FundingFrameOutcome, FundingFrameTarget, FundingRail, GenericError,
		HostFundingStatusSubscribeItem, HostPaymentError, HostPaymentRequest,
		HostPaymentStatusSubscribeError, HostPaymentStatusSubscribeItem, HostPaymentTopUpError,
		HostPaymentTopUpRequest, HostPaymentTopUpStatusSubscribeError,
		HostPaymentTopUpStatusSubscribeItem,
	},
	platform::{
		FundingPlatform, FundingPresentOutcome, FundingPresentation, FundingQuoteList,
		FundingQuoteRow, FundingQuoteState, PaymentPlatform, ProductContext, ProductId,
		ProductStorage, ProductStorageKey, TopUpPlatform,
	},
};

use crate::{
	network::Stack,
	platform::{ApprovalPolicy, CliPlatform},
	published_worker::PublishedWorkers,
	terminal_ui::UiHandle,
	transcript::Transcript,
	worker_host::{CliWorkerHost, WorkerSource},
};

/// Offer the providers `TRUAPI_FUNDING_PROVIDER_NAMES` names, comma-separated
/// and each without a TLD, as an app offers those its remote configuration
/// lists. The core reads each one's manifest from dotNS on the stack's TLD,
/// and skips a name that carries a TLD or is not a product id.
pub fn offer_providers(runtime: &HostRuntime<SigningHost>) {
	let names = provider_names();
	if !names.is_empty() {
		runtime.set_funding_provider_config(names);
	}
}

/// The names `TRUAPI_FUNDING_PROVIDER_NAMES` lists, comma-separated.
fn provider_names() -> Vec<String> {
	std::env::var("TRUAPI_FUNDING_PROVIDER_NAMES")
		.unwrap_or_default()
		.split(',')
		.map(str::trim)
		.filter(|name| !name.is_empty())
		.map(str::to_string)
		.collect()
}

/// The product ids of the providers [`offer_providers`] offers on `tld`.
fn offered_providers(tld: &str) -> Vec<ProductId> {
	let (entries, _invalid) = provider_config(&provider_names(), tld);
	entries.into_iter().map(|entry| entry.product_id).collect()
}

/// Each entry of `TRUAPI_FUNDING_WORKERS`: a product id, and the path of the
/// local script its worker runs. An entry without `=` names no script and is
/// skipped.
fn worker_scripts() -> Vec<(String, PathBuf)> {
	std::env::var("TRUAPI_FUNDING_WORKERS")
		.unwrap_or_default()
		.split(',')
		.map(str::trim)
		.filter(|entry| !entry.is_empty())
		.filter_map(|entry| match entry.split_once('=') {
			Some((provider, script)) => {
				Some((provider.trim().to_string(), PathBuf::from(script.trim())))
			},
			None => {
				tracing::warn!(%entry, "skipping a funding worker without `=` and a script");
				None
			},
		})
		.collect()
}

/// How long an overlay no provider serves yet waits for the core to read the
/// offered providers' manifests from dotNS.
const PROVIDER_READ_WAIT: std::time::Duration = std::time::Duration::from_secs(30);

/// The funding overlay and provider screens of the CLI signing host.
pub struct CliFundingHost {
	/// Where the user's answers come from.
	user: Arc<CliPlatform>,
	transcript: Arc<Transcript>,
	runtime: Weak<HostRuntime<SigningHost>>,
	/// Sessions whose current cancel offer the user was asked about.
	asked: Mutex<HashSet<String>>,
	/// Bumped on every quote list the core pushes, so an overlay waiting on
	/// its quotes reads the core's latest list again.
	quotes_pushed: watch::Sender<u64>,
	/// Bumped every time the core says an overlay's options changed, so an
	/// overlay waiting on its providers reads its options again.
	options_changed: watch::Sender<u64>,
	/// Whether the host offers any provider, so an overlay with none yet
	/// waits for the core to read their manifests from dotNS.
	offers_providers: bool,
	/// The route `TRUAPI_FUNDING_SELECT` picks in every overlay.
	route: Option<ScriptedRoute>,
	/// Whether provider screens wait for the provider or the user even under
	/// `--auto-accept` (`TRUAPI_FUNDING_HOLD_SCREENS=1`).
	hold_screens: bool,
}

impl CliFundingHost {
	/// Serve funding on `runtime` of `network`, asking `user`, with the
	/// providers, worker scripts and files the environment names. Worker
	/// output goes to `ui` when the host runs one.
	pub fn install(
		runtime: &Arc<HostRuntime<SigningHost>>,
		network: &Stack,
		user: Arc<CliPlatform>,
		ui: Option<UiHandle>,
	) -> anyhow::Result<()> {
		let route = ScriptedRoute::from_env()?;
		let hold_screens =
			std::env::var("TRUAPI_FUNDING_HOLD_SCREENS").is_ok_and(|value| value == "1");
		let mut workers = HashMap::new();
		let offered = offered_providers(network.tld());
		if !offered.is_empty() {
			match PublishedWorkers::new(network, user.chains()).zip(user.state_dir()) {
				Some((source, state_dir)) => {
					let source = WorkerSource::Published {
						workers: Arc::new(source),
						cache: state_dir.join("published-workers"),
					};
					for product_id in &offered {
						workers.insert(product_id.clone(), source.clone());
					}
				},
				None => tracing::warn!(
					"published funding providers get no worker: the stack names no dotNS \
					 registry or IPFS gateway, or the host keeps no state"
				),
			}
		}
		for (provider, script) in worker_scripts() {
			match provider.parse::<ProductId>() {
				Ok(provider) if offered.contains(&provider) => {
					workers.insert(provider, WorkerSource::Script(script));
				},
				Ok(provider) => {
					tracing::warn!(%provider, "skipping a funding worker for a provider not offered");
				},
				Err(error) => {
					tracing::warn!(%provider, %error, "skipping a funding worker that is not a product id");
				},
			}
		}
		// One transcript per process: each runtime the host builds, such as one
		// provisioned after startup, appends to it rather than truncating what
		// an earlier one recorded.
		static TRANSCRIPT: std::sync::OnceLock<Arc<Transcript>> = std::sync::OnceLock::new();
		let transcript = TRANSCRIPT
			.get_or_init(|| Arc::new(Transcript::from_env("TRUAPI_FUNDING_LOG", "funding")))
			.clone();
		CliWorkerHost::install(runtime, workers, ui, transcript.clone())?;
		let host = Arc::new(Self {
			user,
			transcript,
			runtime: Arc::downgrade(runtime),
			asked: Mutex::new(HashSet::new()),
			quotes_pushed: watch::Sender::new(0),
			options_changed: watch::Sender::new(0),
			offers_providers: !offered.is_empty(),
			route,
			hold_screens,
		});
		runtime.set_funding_feed(host.clone());
		runtime.set_funding_platform(host);
		if let Some(ledger) = std::env::var_os("TRUAPI_FUNDING_LEDGER").map(PathBuf::from) {
			let payments = Arc::new(CliPaymentHost { ledger });
			runtime.set_top_up_platform(payments.clone());
			runtime.set_payment_platform(payments);
		}
		Ok(())
	}

	/// Put the provider's cancel offer on `session` to the user, once per
	/// offer, and pass on their answer.
	fn ask_about_offer(&self, intent: String, session: Option<&FundingSession>) {
		let Some(runtime) = self.runtime.upgrade() else {
			return;
		};
		let offer = match session.and_then(|session| session.cancel.clone()) {
			Some(FundingCancel::Offered { fee, refund, .. }) => CancelTerms { fee, refund },
			_ => {
				self.lock_asked().remove(&intent);
				return;
			},
		};
		if !self.lock_asked().insert(intent.clone()) {
			return;
		}
		let user = self.user.clone();
		let transcript = self.transcript.clone();
		tokio::spawn(async move {
			let accepted = user
				.decide(
					"accept the provider's cancel offer",
					format!(
						"{intent}: the provider keeps {} and returns {}",
						offer.fee, offer.refund
					),
				)
				.await;
			match runtime.confirm_funding_cancel(&intent, accepted.then_some(offer)).await {
				Ok(answer) => transcript.record(serde_json::json!({
					"kind": "cancel-confirm",
					"intent": intent,
					"answer": format!("{answer:?}"),
				})),
				Err(error) => {
					tracing::warn!(%intent, reason = %error.reason, "answering a cancel offer failed")
				},
			}
		});
	}

	fn lock_asked(&self) -> std::sync::MutexGuard<'_, HashSet<String>> {
		self.asked.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
	}

	/// The rail and what session `intent`'s overlay offers on it once the
	/// user picks `route`, or else takes its defaults: the first rail the
	/// core routes and, for crypto, the first token and then the first
	/// network it lists. The core answers a selection with the options it
	/// leads to before `select_funding_options` returns.
	fn select_route(
		runtime: &HostRuntime<SigningHost>,
		intent: &str,
		route: Option<&ScriptedRoute>,
	) -> Option<(FundingRail, FundingOptions)> {
		let offered = runtime.funding_options(intent)?;
		let rail = route.map_or_else(|| default_rail(&offered), |route| route.rail);
		let picked = |field: fn(&ScriptedRoute) -> Option<&String>| route.and_then(field).cloned();
		let token = match rail {
			FundingRail::Crypto => {
				picked(|route| route.token.as_ref()).or_else(|| offered.tokens.first().cloned())
			},
			FundingRail::Card | FundingRail::Bank => None,
		};
		let select = |network: Option<String>| {
			runtime.select_funding_options(
				intent,
				FundingSelection {
					rail,
					asset: token.clone(),
					network,
					country: picked(|route| route.country.as_ref()),
					currency: picked(|route| route.currency.as_ref()),
				},
			);
			runtime.funding_options(intent)
		};
		let options = select(None)?;
		match rail {
			FundingRail::Crypto => {
				let network = picked(|route| route.network.as_ref())
					.or_else(|| options.networks.first().map(|network| network.network.clone()));
				match network {
					Some(network) => select(Some(network)).map(|options| (rail, options)),
					None => Some((rail, options)),
				}
			},
			FundingRail::Card | FundingRail::Bank => Some((rail, options)),
		}
	}

	/// [`Self::select_route`] for session `intent`, once a provider serves
	/// it. The core offers a published provider only once it has read its
	/// manifest from dotNS, so while none serves the session the overlay
	/// waits for its options to change, as a user watches it load, for up to
	/// [`PROVIDER_READ_WAIT`].
	async fn served_route(
		&self,
		runtime: &HostRuntime<SigningHost>,
		intent: &str,
	) -> Option<(FundingRail, FundingOptions)> {
		let mut changed = self.options_changed.subscribe();
		let deadline = tokio::time::Instant::now() + PROVIDER_READ_WAIT;
		loop {
			let options = Self::select_route(runtime, intent, self.route.as_ref());
			// Selecting sends the options again; only a later change is news.
			changed.borrow_and_update();
			let served =
				options.as_ref().is_some_and(|(_, options)| !options.candidates.is_empty());
			if served || !self.offers_providers {
				return options;
			}
			// The sender lives as long as `self`, so only the deadline ends this.
			if tokio::time::timeout_at(deadline, changed.changed()).await.is_err() {
				return options;
			}
		}
	}

	/// Ask for session `intent`'s quotes of `amount` on the selection, and
	/// wait for the first list in which every provider has answered, as a
	/// user waits on the list. Empty when the selection cannot be quoted.
	async fn answered_quotes(
		&self,
		runtime: &HostRuntime<SigningHost>,
		intent: &str,
		amount: u128,
	) -> Vec<FundingQuoteRow> {
		let mut pushed = self.quotes_pushed.subscribe();
		let Some(request_id) = runtime.request_funding_quotes(intent, amount) else {
			return Vec::new();
		};
		loop {
			let rows = runtime
				.funding_quotes(intent)
				.filter(|list| list.request_id == request_id)
				.map(|list| list.rows);
			if let Some(rows) = rows &&
				!rows.iter().any(|row| matches!(row.state, FundingQuoteState::Pending))
			{
				return rows;
			}
			// The sender lives as long as `self`.
			if pushed.changed().await.is_err() {
				return Vec::new();
			}
		}
	}
}

/// The route a run picks in the overlay instead of the defaults, from
/// `TRUAPI_FUNDING_SELECT`: `crypto:<network>:<token>`, or
/// `card|bank:<currency>[:<country>]`, as a user picks them.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ScriptedRoute {
	rail: FundingRail,
	network: Option<String>,
	token: Option<String>,
	currency: Option<String>,
	country: Option<String>,
}

impl ScriptedRoute {
	/// The route `TRUAPI_FUNDING_SELECT` names, if any. One it cannot read is
	/// an error, so a run never quietly falls back to the defaults.
	fn from_env() -> anyhow::Result<Option<Self>> {
		match std::env::var("TRUAPI_FUNDING_SELECT") {
			Ok(value) if !value.trim().is_empty() => Self::parse(value.trim()).map(Some),
			_ => Ok(None),
		}
	}

	fn parse(value: &str) -> anyhow::Result<Self> {
		let mut parts = value.split(':').map(str::trim);
		let rail = parts.next().unwrap_or_default();
		let mut next = || parts.next().filter(|part| !part.is_empty()).map(str::to_string);
		let route = match rail.to_ascii_lowercase().as_str() {
			"crypto" => Self {
				rail: FundingRail::Crypto,
				network: next(),
				token: next(),
				currency: None,
				country: None,
			},
			"card" | "bank" => Self {
				rail: if rail.eq_ignore_ascii_case("card") {
					FundingRail::Card
				} else {
					FundingRail::Bank
				},
				network: None,
				token: None,
				currency: next(),
				country: next(),
			},
			_ => anyhow::bail!(
				"TRUAPI_FUNDING_SELECT={value}: expected crypto:<network>:<token> or \
				 card|bank:<currency>[:<country>]"
			),
		};
		if next().is_some() {
			anyhow::bail!("TRUAPI_FUNDING_SELECT={value}: too many parts");
		}
		Ok(route)
	}
}

/// The rail a user who takes the defaults is on: the first the core routes,
/// crypto first, or card when it routes none.
fn default_rail(options: &FundingOptions) -> FundingRail {
	options.rails.first().copied().unwrap_or(FundingRail::Card)
}

/// What `session`'s overlay asks to be quoted on `options`, the selection
/// the user made on `rail`: the amount the product asked, or else the
/// smallest one the selection takes. `None` when a crypto selection has no
/// network to ask on.
fn quote_amount(
	rail: FundingRail,
	options: &FundingOptions,
	session: &FundingPresentation,
) -> Option<u128> {
	if rail == FundingRail::Crypto && options.networks.is_empty() {
		return None;
	}
	Some(session.amount.or(options.limits.min).unwrap_or(FUNDING_MIN_AMOUNT))
}

/// Where a provider's App is opened: the web host that serves dotNS
/// products, as a browser opens them.
const APP_GATEWAY: &str = "https://dot.li";

/// The provider screens waiting on the user, by session intent, each
/// answered once.
static FRAMES: Mutex<BTreeMap<String, futures::channel::oneshot::Sender<FundingFrameOutcome>>> =
	Mutex::new(BTreeMap::new());

fn lock_frames() -> std::sync::MutexGuard<
	'static,
	BTreeMap<String, futures::channel::oneshot::Sender<FundingFrameOutcome>>,
> {
	FRAMES.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Answer session `intent`'s open provider screen with `outcome`. Returns
/// whether one was open.
pub fn answer_frame(intent: &str, outcome: FundingFrameOutcome) -> bool {
	lock_frames().remove(intent).is_some_and(|answer| answer.send(outcome).is_ok())
}

/// Each session the core holds in full, newest opened first, with where it
/// stands now.
pub fn sessions(runtime: &HostRuntime<SigningHost>) -> Vec<(String, String)> {
	let now = truapi::current_unix_millis();
	runtime
		.funding_entries()
		.into_iter()
		.map(|entry| (entry.session.intent.clone(), format!("{:?}", entry.session.status(now))))
		.collect()
}

/// Cancel session `intent` at the user's request, and record what that did.
pub async fn cancel(runtime: &HostRuntime<SigningHost>, intent: &str) -> Result<String, String> {
	let outcome = runtime
		.cancel_funding(intent)
		.await
		.map(|outcome| format!("{outcome:?}"))
		.map_err(|error| error.reason)?;
	Transcript::appending_from_env("TRUAPI_FUNDING_LOG", "funding").record(serde_json::json!({
		"kind": "cancel",
		"intent": intent,
		"outcome": outcome,
	}));
	Ok(outcome)
}

/// Write `value`, as UTF-8, under `key` of the storage of the provider
/// serving session `intent`, as the provider's own screen would, and record
/// it. The provider's worker hears it through `localStorage.subscribe`.
pub async fn write_provider_storage(
	runtime: &HostRuntime<SigningHost>,
	storage: &dyn ProductStorage,
	intent: &str,
	key: String,
	value: String,
) -> Result<String, String> {
	let provider = runtime
		.funding_session(intent)
		.ok_or_else(|| format!("no funding session {intent}"))?
		.provider_id
		.ok_or_else(|| format!("no provider serves {intent} yet"))?;
	storage
		.write(ProductStorageKey::new(provider.clone(), key.clone()).encode(), value.into_bytes())
		.await
		.map_err(|error| format!("{error:?}"))?;
	Transcript::appending_from_env("TRUAPI_FUNDING_LOG", "funding").record(serde_json::json!({
		"kind": "provider_storage",
		"intent": intent,
		"provider": provider.as_str(),
		"key": key,
	}));
	Ok(format!("Wrote {key} for {intent} to {provider}'s storage"))
}

#[async_trait::async_trait]
impl FundingPlatform for CliFundingHost {
	/// Record the status the product is told, with the core's own status,
	/// which shows what the product's item does not, such as a withdrawal
	/// cancelled after its release.
	fn funding_session_changed(&self, intent: String, status: HostFundingStatusSubscribeItem) {
		let (tag, amount) = match status {
			HostFundingStatusSubscribeItem::InProgress { .. } => ("InProgress", None),
			HostFundingStatusSubscribeItem::Delivered { credited } => ("Delivered", Some(credited)),
			HostFundingStatusSubscribeItem::Released { debited } => ("Released", Some(debited)),
			HostFundingStatusSubscribeItem::Failed { .. } => ("Failed", None),
		};
		let progress = self
			.runtime
			.upgrade()
			.and_then(|runtime| runtime.funding_session(&intent))
			.map(|session| format!("{:?}", session.status(truapi::current_unix_millis())));
		self.transcript.record(serde_json::json!({
			"kind": "status",
			"intent": intent,
			"tag": tag,
			"amount": amount.map(|amount| amount.to_string()),
			"progress": progress,
		}));
	}

	fn funding_quotes_changed(&self, _intent: String, _quotes: Option<FundingQuoteList>) {
		self.quotes_pushed.send_modify(|pushed| *pushed = pushed.wrapping_add(1));
	}

	async fn present_funding(
		&self,
		product: Option<&ProductContext>,
		session: FundingPresentation,
	) -> Result<FundingPresentOutcome, GenericError> {
		let direction = match session.direction {
			FundingDirection::In => "In",
			FundingDirection::Out => "Out",
		};
		let amount = session.amount.map(|amount| amount.to_string());
		let asker = product.map_or("the host", |product| product.product_id.as_str());
		let Some(runtime) = self.runtime.upgrade() else {
			return Ok(FundingPresentOutcome::Dismissed);
		};
		let options = self.served_route(&runtime, &session.intent).await;
		let candidates: Vec<ProductId> = options
			.iter()
			.flat_map(|(_, options)| &options.candidates)
			.map(|candidate| candidate.provider_id.clone())
			.collect();
		let rows = match options.and_then(|(rail, options)| quote_amount(rail, &options, &session))
		{
			Some(amount) => self.answered_quotes(&runtime, &session.intent, amount).await,
			None => Vec::new(),
		};
		let mut chosen = None;
		for row in &rows {
			let FundingQuoteState::Quoted { quote } = &row.state else {
				continue;
			};
			let detail = format!(
				"{asker} asks to move {} {direction}: {} quotes {} sent for {} received, fees {}",
				amount.as_deref().unwrap_or("an amount you choose"),
				row.provider_id,
				quote.send_amount,
				quote.receive_amount,
				quote.provider_fee + quote.network_fee,
			);
			if self.user.decide("fund with a provider", detail).await {
				chosen = Some((row.provider_id.clone(), quote.quote_id.clone()));
				break;
			}
		}
		self.transcript.record(serde_json::json!({
			"kind": "presented",
			"intent": session.intent,
			"direction": direction,
			"amount": amount,
			"product": product.map(|product| product.product_id.as_str()),
			"providers": candidates,
			"quotes": rows.iter().map(|row| serde_json::json!({
				"provider": row.provider_id,
				"state": match &row.state {
					FundingQuoteState::Pending => "Pending".to_string(),
					FundingQuoteState::Quoted { quote } => format!("Quoted:{}", quote.quote_id),
					FundingQuoteState::Unavailable { reason } => format!("Unavailable:{reason:?}"),
				},
			})).collect::<Vec<_>>(),
			"provider": chosen.as_ref().map(|(provider, _)| provider),
			"outcome": if chosen.is_some() { "Started" } else { "Dismissed" },
		}));
		let Some((provider, quote_id)) = chosen else {
			return Ok(FundingPresentOutcome::Dismissed);
		};
		runtime.select_funding_provider(&session.intent, &provider, &quote_id).await?;
		Ok(FundingPresentOutcome::Started)
	}

	/// Print a link to the provider's screen, which the user opens in a
	/// browser: its App on dotNS at the route, or the page itself. The frame
	/// stays open until the provider closes it, or the user answers it with
	/// `/funding close` or `/funding returned`. Under `--auto-accept` no one is
	/// there to answer, so it finishes at once, a page at its return URL.
	async fn present_provider_frame(
		&self,
		provider: &ProductContext,
		intent: String,
		target: FundingFrameTarget,
	) -> Result<FundingFrameOutcome, GenericError> {
		let (link, finished) = match target {
			FundingFrameTarget::App { route } => (
				format!("{APP_GATEWAY}/{}{route}", provider.product_id),
				FundingFrameOutcome::Closed,
			),
			FundingFrameTarget::Page { url, return_url } => {
				let finished = match return_url {
					Some(url) => FundingFrameOutcome::Returned { url },
					None => FundingFrameOutcome::Closed,
				};
				(url, finished)
			},
		};
		crate::terminal_ui::output_success(
			format!("{} asks you to open a screen for {intent}", provider.product_id),
			Some(format!(
				"{link}\nWhen you are done, the provider closes it, or type \
				 /funding close {intent} or /funding returned {intent} <url>"
			)),
		);
		self.transcript.record(serde_json::json!({
			"kind": "frame",
			"intent": intent,
			"provider": provider.product_id.as_str(),
			"link": link,
			"awaits_user_transfer": self.runtime.upgrade()
				.and_then(|runtime| runtime.funding_progress(&intent))
				.is_some_and(|progress| progress.awaits_user_transfer),
		}));
		let outcome =
			if self.user.approval_policy() == ApprovalPolicy::AutoAccept && !self.hold_screens {
				finished
			} else {
				let (answer, answered) = futures::channel::oneshot::channel();
				lock_frames().insert(intent.clone(), answer);
				answered.await.unwrap_or(FundingFrameOutcome::Dismissed)
			};
		self.transcript.record(serde_json::json!({
			"kind": "frame_answered",
			"intent": intent,
			"outcome": format!("{outcome:?}"),
		}));
		Ok(outcome)
	}

	// The terminal user always looks at the session, so a provider's screen
	// opens as soon as it is asked for, and its link prints at once.
	fn provider_frame_requested(&self, _provider: &ProductContext, intent: String) {
		if let Some(runtime) = self.runtime.upgrade() {
			runtime.open_provider_frame(&intent);
		}
	}

	fn close_provider_frame(&self, provider: &ProductContext, intent: String) {
		answer_frame(&intent, FundingFrameOutcome::Closed);
		self.transcript.record(serde_json::json!({
			"kind": "frame_closed",
			"intent": intent,
			"provider": provider.product_id.as_str(),
		}));
	}

	/// The country `TRUAPI_FUNDING_COUNTRY` names, standing in for the one the
	/// device detects.
	async fn detected_country(&self) -> Option<String> {
		std::env::var("TRUAPI_FUNDING_COUNTRY")
			.ok()
			.filter(|country| !country.is_empty())
	}
}

impl FundingFeed for CliFundingHost {
	/// Put a cancel offer the session now carries to the user.
	fn funding_session_changed(&self, intent: String) {
		let session = self.runtime.upgrade().and_then(|runtime| runtime.funding_session(&intent));
		self.ask_about_offer(intent, session.as_ref());
	}

	fn funding_in_progress_changed(&self, _in_progress: bool) {}

	fn funding_options_changed(&self, _intent: String) {
		self.options_changed.send_modify(|changed| *changed = changed.wrapping_add(1));
	}
}

/// Top-up and payment engines over a ledger file: every top-up claims in full
/// and every payment completes, once started. What was started is kept one
/// JSON object per line, so a restarted host still answers for it.
struct CliPaymentHost {
	ledger: PathBuf,
}

impl CliPaymentHost {
	/// Whether `kind` with `id` was started by `product`.
	fn started(&self, kind: &str, product: &ProductContext, id: &[u8; 32]) -> bool {
		let id = hex::encode(id);
		std::fs::read_to_string(&self.ledger)
			.unwrap_or_default()
			.lines()
			.filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
			.any(|line| {
				line["kind"] == kind &&
					line["product"] == product.product_id.as_str() &&
					line["id"] == id.as_str()
			})
	}

	/// Record a start: `Ok(false)` when the id was already used, an error
	/// when the ledger could not be written, as a start it does not keep would
	/// read as unknown after a restart.
	fn start(
		&self,
		kind: &str,
		product: &ProductContext,
		id: &[u8; 32],
		amount: u128,
	) -> Result<bool, String> {
		if self.started(kind, product, id) {
			return Ok(false);
		}
		let line = serde_json::json!({
			"kind": kind,
			"product": product.product_id.as_str(),
			"id": hex::encode(id),
			"amount": amount.to_string(),
		});
		let appended = OpenOptions::new()
			.create(true)
			.append(true)
			.open(&self.ledger)
			.and_then(|mut file| file.write_all(format!("{line}\n").as_bytes()));
		appended
			.map(|()| true)
			.map_err(|error| format!("funding ledger could not be appended to: {error}"))
	}
}

#[async_trait::async_trait]
impl TopUpPlatform for CliPaymentHost {
	async fn top_up(
		&self,
		product: &ProductContext,
		request: HostPaymentTopUpRequest,
	) -> Result<(), HostPaymentTopUpError> {
		match self.start("top_up", product, &request.id, request.amount) {
			Ok(true) => Ok(()),
			Ok(false) => Err(HostPaymentTopUpError::AlreadyExists),
			Err(reason) => Err(HostPaymentTopUpError::Unknown { reason }),
		}
	}

	fn subscribe_top_up_status(
		&self,
		product: &ProductContext,
		id: [u8; 32],
	) -> BoxStream<
		'static,
		Result<HostPaymentTopUpStatusSubscribeItem, HostPaymentTopUpStatusSubscribeError>,
	> {
		if !self.started("top_up", product, &id) {
			return stream::iter([Err(HostPaymentTopUpStatusSubscribeError::TopUpNotFound)])
				.boxed();
		}
		stream::iter([
			Ok(HostPaymentTopUpStatusSubscribeItem::Detecting),
			Ok(HostPaymentTopUpStatusSubscribeItem::Claiming),
			Ok(HostPaymentTopUpStatusSubscribeItem::Claimed { finalized: true }),
		])
		.boxed()
	}
}

#[async_trait::async_trait]
impl PaymentPlatform for CliPaymentHost {
	async fn request_payment(
		&self,
		product: &ProductContext,
		request: HostPaymentRequest,
	) -> Result<(), HostPaymentError> {
		match self.start("payment", product, &request.id, request.amount) {
			Ok(true) => Ok(()),
			Ok(false) => Err(HostPaymentError::AlreadyExists),
			Err(reason) => Err(HostPaymentError::Unknown { reason }),
		}
	}

	fn subscribe_payment_status(
		&self,
		product: &ProductContext,
		id: [u8; 32],
	) -> BoxStream<'static, Result<HostPaymentStatusSubscribeItem, HostPaymentStatusSubscribeError>>
	{
		if !self.started("payment", product, &id) {
			return stream::iter([Err(HostPaymentStatusSubscribeError::PaymentNotFound)]).boxed();
		}
		stream::iter([
			Ok(HostPaymentStatusSubscribeItem::Processing),
			Ok(HostPaymentStatusSubscribeItem::Completed),
		])
		.boxed()
	}
}

#[cfg(test)]
mod tests;
