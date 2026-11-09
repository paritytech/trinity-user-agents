//! The CLI signing host's funding surface: the overlay, provider screens, and
//! the user's answers, as a host app serves them.
//!
//! The host offers the providers named in `TRUAPI_FUNDING_PROVIDERS`, each
//! bundled with the manifest in [`SHIPPED_PROVIDER_MANIFEST`], as an app
//! bundles its providers. Each entry is a product id, optionally
//! followed by `=` and the path of the script its worker runs
//! (`ramp.dot=./worker.ts,other.dot`). The host launches that script as the
//! provider's worker when the core asks for it, and stops it when the core
//! lets it go; a provider without a script has no worker here.
//!
//! A product's funding request shows the overlay: the user takes the core's
//! default rail and, for crypto, its first token and network, the core asks
//! each provider routing that rail for a quote, and the user picks a quote or
//! closes the overlay. Every question goes through the
//! host's approval policy, as its other confirmations do: a prompt on a
//! terminal, the first quote and every offer accepted under `--auto-accept`,
//! and a refusal when a prompt has no terminal to ask on. The user cancels a session with
//! `/funding cancel <intent>`, and the provider's cancel offer is put to them
//! the same way.
//!
//! A provider screen cannot be drawn here: its route or URL is printed, and
//! the user finishing it closes it, or returns to the page's return URL.
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
	FundingFeed, SigningHostRuntime,
	host_logic::{
		funding::{CancelTerms, FundingCancel, FundingProgress, FundingSession},
		funding_providers::{
			FUNDING_MIN_AMOUNT, FundingOptions, FundingProviderEntry, FundingProviderOrigin,
			FundingSelection,
		},
	},
	latest::{
		FundingDirection, FundingFrameOutcome, FundingFrameTarget, FundingQuoteAsk, FundingRail,
		GenericError, HostFundingStatusSubscribeItem, HostPaymentError, HostPaymentRequest,
		HostPaymentStatusSubscribeError, HostPaymentStatusSubscribeItem, HostPaymentTopUpError,
		HostPaymentTopUpRequest, HostPaymentTopUpStatusSubscribeError,
		HostPaymentTopUpStatusSubscribeItem,
	},
	platform::{
		CoreStorage, FundingFrameContext, FundingPlatform, FundingPresentOutcome,
		FundingPresentation, FundingQuoteList, FundingQuoteRow, FundingQuoteState, PaymentPlatform,
		ProductContext, ProductId, TopUpPlatform,
	},
};

use crate::{
	platform::{ApprovalPolicy, CliPlatform},
	terminal_ui::UiHandle,
	transcript::Transcript,
	worker_host::CliWorkerHost,
};

/// The manifest the CLI ships for every provider it offers: card in, and
/// crypto in and out, quoted by its worker.
const SHIPPED_PROVIDER_MANIFEST: &str = r#"{"$v":1,"appVersion":[1,0,0],"kind":"worker","entrypoint":"index.js","includes":{"funding":{"routes":[{"mode":"CARD","directions":["In"],"assets":["EUR"]},{"mode":"CRYPTO","directions":["In","Out"],"assets":["USDT"],"networks":["polkadot"]}]}}}"#;

/// Offer each product named in `TRUAPI_FUNDING_PROVIDERS` as a funding
/// provider bundled with [`SHIPPED_PROVIDER_MANIFEST`], so the core never
/// reads dotNS for what it serves. The manifest is also kept as its cached
/// dotNS manifest, which its worker execution is read from.
pub async fn offer_providers(
	runtime: &SigningHostRuntime,
	platform: &dyn CoreStorage,
) -> anyhow::Result<()> {
	let entries = provider_entries();
	if entries.is_empty() {
		return Ok(());
	}
	let now_secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs();
	let mut providers = Vec::new();
	for (entry, _script) in entries {
		let product_id: truapi::platform::ProductId = entry
			.parse()
			.map_err(|error| anyhow::anyhow!("funding provider {entry}: {error}"))?;
		platform
			.write_core_storage(
				truapi::worker_manifest_cache_key(&product_id),
				truapi::encode_cached_root_manifest(Some(SHIPPED_PROVIDER_MANIFEST), now_secs),
			)
			.await
			.map_err(|error| anyhow::anyhow!("caching the manifest of {entry}: {error:?}"))?;
		providers.push(FundingProviderEntry {
			product_id,
			worker_manifest: Some(SHIPPED_PROVIDER_MANIFEST.to_string()),
			origin: FundingProviderOrigin::Bundled,
			has_app: None,
		});
	}
	runtime
		.set_funding_providers(providers)
		.map_err(|error| anyhow::anyhow!("offering funding providers: {}", error.reason))
}

/// Each entry of `TRUAPI_FUNDING_PROVIDERS`: a product id, and the path of
/// its worker script when `=` follows it.
fn provider_entries() -> Vec<(String, Option<PathBuf>)> {
	std::env::var("TRUAPI_FUNDING_PROVIDERS")
		.unwrap_or_default()
		.split(',')
		.map(str::trim)
		.filter(|entry| !entry.is_empty())
		.map(|entry| match entry.split_once('=') {
			Some((provider, script)) => {
				(provider.trim().to_string(), Some(PathBuf::from(script.trim())))
			},
			None => (entry.to_string(), None),
		})
		.collect()
}

/// The funding overlay and provider screens of the CLI signing host.
pub struct CliFundingHost {
	/// Where the user's answers come from.
	user: Arc<CliPlatform>,
	transcript: Arc<Transcript>,
	runtime: Weak<SigningHostRuntime>,
	/// Sessions whose current cancel offer the user was asked about.
	asked: Mutex<HashSet<String>>,
	/// Bumped on every quote list the core pushes, so an overlay waiting on
	/// its quotes reads the core's latest list again.
	quotes_pushed: watch::Sender<u64>,
}

impl CliFundingHost {
	/// Serve funding on `runtime`, asking `user`, with the providers, worker
	/// scripts and files the environment names. Worker output goes to `ui`
	/// when the host runs one.
	pub fn install(
		runtime: &Arc<SigningHostRuntime>,
		user: Arc<CliPlatform>,
		ui: Option<UiHandle>,
	) -> anyhow::Result<()> {
		let mut workers = HashMap::new();
		for (provider, script) in provider_entries() {
			let Some(script) = script else {
				continue;
			};
			match provider.parse::<ProductId>() {
				Ok(provider) => {
					workers.insert(provider, script);
				},
				Err(error) => {
					tracing::warn!(%provider, %error, "skipping a funding worker that is not a product id");
				},
			}
		}
		let transcript = Arc::new(Transcript::from_env("TRUAPI_FUNDING_LOG", "funding"));
		CliWorkerHost::install(runtime, workers, ui, transcript.clone())?;
		let host = Arc::new(Self {
			user,
			transcript,
			runtime: Arc::downgrade(runtime),
			asked: Mutex::new(HashSet::new()),
			quotes_pushed: watch::Sender::new(0),
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

	/// What session `intent`'s overlay offers once the user takes its
	/// defaults: the first rail the core routes and, for crypto, the first token it
	/// lists. The core answers a selection with the options it leads to
	/// before `select_funding_options` returns.
	fn select_defaults(runtime: &SigningHostRuntime, intent: &str) -> Option<FundingOptions> {
		let offered = runtime.funding_options(intent)?;
		let rail = default_rail(&offered);
		let token = match rail {
			FundingRail::Crypto => offered.tokens.first().cloned(),
			FundingRail::Card | FundingRail::Bank => None,
		};
		runtime.select_funding_options(
			intent,
			FundingSelection { rail, asset: token, network: None, country: None, currency: None },
		);
		runtime.funding_options(intent)
	}

	/// Ask for session `intent`'s quotes and wait for the first list in
	/// which every provider has answered, as a user waits on the list.
	async fn answered_quotes(
		&self,
		runtime: &SigningHostRuntime,
		intent: &str,
		ask: FundingQuoteAsk,
	) -> Vec<FundingQuoteRow> {
		let mut pushed = self.quotes_pushed.subscribe();
		let request_id = runtime.request_funding_quotes(intent, ask);
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

/// The rail a user who takes the defaults is on: the first the core routes,
/// crypto first, or card when it routes none.
fn default_rail(options: &FundingOptions) -> FundingRail {
	options.rails.first().copied().unwrap_or(FundingRail::Card)
}

/// What `session`'s overlay asks to be quoted on `options`, the defaults the
/// user took: the selected rail and the asset the core names for it, on the
/// first network it lists for crypto, for the amount the product asked or
/// else the smallest one the selection takes. `None` when a crypto selection
/// has no network to ask on.
fn default_ask(options: &FundingOptions, session: &FundingPresentation) -> Option<FundingQuoteAsk> {
	let rail = default_rail(options);
	let network = match rail {
		FundingRail::Crypto => Some(options.networks.first()?.network.clone()),
		FundingRail::Card | FundingRail::Bank => None,
	};
	Some(FundingQuoteAsk {
		direction: session.direction,
		rail,
		asset: options.asset.clone()?,
		network,
		amount: session.amount.or(options.limits.min).unwrap_or(FUNDING_MIN_AMOUNT),
		country: None,
	})
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
pub fn sessions(runtime: &SigningHostRuntime) -> Vec<(String, String)> {
	let now = truapi::current_unix_millis();
	runtime
		.funding_entries()
		.into_iter()
		.map(|entry| (entry.session.intent.clone(), format!("{:?}", entry.session.status(now))))
		.collect()
}

/// Cancel session `intent` at the user's request, and record what that did.
pub async fn cancel(runtime: &SigningHostRuntime, intent: &str) -> Result<String, String> {
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

#[async_trait::async_trait]
impl FundingPlatform for CliFundingHost {
	fn funding_quotes_changed(&self, _intent: String, _quotes: FundingQuoteList) {
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
		let options = Self::select_defaults(&runtime, &session.intent);
		let candidates: Vec<ProductId> = options
			.iter()
			.flat_map(|options| &options.candidates)
			.map(|candidate| candidate.provider_id.clone())
			.collect();
		let rows = match options.and_then(|options| default_ask(&options, &session)) {
			Some(ask) => self.answered_quotes(&runtime, &session.intent, ask).await,
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
		context: FundingFrameContext,
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
			"awaits_user_transfer": context.awaits_user_transfer,
		}));
		let outcome = if self.user.approval_policy() == ApprovalPolicy::AutoAccept {
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
	fn funding_session_changed(
		&self,
		intent: String,
		status: HostFundingStatusSubscribeItem,
		session: Option<FundingSession>,
		_progress: Option<FundingProgress>,
	) {
		let (tag, amount) = match status {
			HostFundingStatusSubscribeItem::InProgress { .. } => ("InProgress", None),
			HostFundingStatusSubscribeItem::Delivered { credited } => ("Delivered", Some(credited)),
			HostFundingStatusSubscribeItem::Released { debited } => ("Released", Some(debited)),
			HostFundingStatusSubscribeItem::Failed { .. } => ("Failed", None),
		};
		// The core's own status, which shows what the product's item does
		// not, such as a withdrawal cancelled after its release.
		let progress = session
			.as_ref()
			.map(|session| format!("{:?}", session.status(truapi::current_unix_millis())));
		self.transcript.record(serde_json::json!({
			"kind": "status",
			"intent": intent,
			"tag": tag,
			"amount": amount.map(|amount| amount.to_string()),
			"progress": progress,
		}));
		self.ask_about_offer(intent, session.as_ref());
	}

	fn funding_in_progress_changed(&self, _in_progress: bool) {}

	fn funding_options_changed(&self, _intent: String, _options: FundingOptions) {}
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
