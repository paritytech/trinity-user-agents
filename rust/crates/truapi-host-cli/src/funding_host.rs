//! Scripted funding host for the CLI test host.
//!
//! It stands in for the host's funding overlay and the provider behind it:
//! each request a product makes is answered with the next outcome from
//! `TRUAPI_FUNDING_OUTCOMES` (`deliver:<credited>`, `release:<debited>`,
//! `fail` or `dismiss`, comma-separated), and a session the user started is
//! then settled through the core's test hook, as though its funds had moved.
//! There is no chain behind it: this exists so a battery can drive the
//! product's side of funding headlessly.
//!
//! `provide` and `provide-cancel` instead hand the session to the product that
//! asked, which `TRUAPI_FUNDING_PROVIDERS` must name as a provider, so a
//! provider worker can serve its own request; `provide-cancel`
//! then cancels it, as a user would. Top-ups and payment requests that worker
//! starts are accepted and complete in full, recorded in the ledger named by
//! `TRUAPI_FUNDING_LEDGER` so their statuses outlive a restart.
//!
//! Every overlay request and every session change the core reports is
//! appended to the transcript named by `TRUAPI_FUNDING_LOG`, one JSON object
//! per line, so a battery can assert what the host saw rather than only what
//! the product was told.

use std::{
	collections::VecDeque,
	fs::OpenOptions,
	io::Write,
	path::PathBuf,
	sync::{Arc, Mutex, Weak},
	time::Duration,
};

use futures::stream::{self, BoxStream, StreamExt};
use truapi::{
	SigningHostRuntime, current_unix_millis,
	host_logic::{funding::FundingStage, funding_providers::FundingProviderEntry},
	latest::{
		FundingDirection, FundingFailure, FundingFrameOutcome, FundingFrameTarget, FundingQuoteAsk,
		FundingRail, GenericError, HostFundingStatusSubscribeItem, HostPaymentError,
		HostPaymentRequest, HostPaymentStatusSubscribeError, HostPaymentStatusSubscribeItem,
		HostPaymentTopUpError, HostPaymentTopUpRequest, HostPaymentTopUpStatusSubscribeError,
		HostPaymentTopUpStatusSubscribeItem,
	},
	platform::{
		CoreStorage, FundingPlatform, FundingPresentOutcome, FundingPresentation, FundingQuoteRow,
		FundingQuoteState, PaymentPlatform, ProductContext, TopUpPlatform, async_trait,
	},
};

use crate::transcript::Transcript;

/// Worker manifest every scripted provider publishes: card in, and crypto in
/// and out, quoted by its worker.
const SCRIPTED_PROVIDER_MANIFEST: &str = r#"{"$v":1,"appVersion":[1,0,0],"kind":"worker","entrypoint":"index.js","includes":{"funding":{"routes":[{"mode":"CARD","directions":["In"],"assets":["EUR"]},{"mode":"CRYPTO","directions":["In","Out"],"assets":["USDT"]}]}}}"#;

/// Offer each product named in `TRUAPI_FUNDING_PROVIDERS` (comma-separated)
/// as a funding provider publishing [`SCRIPTED_PROVIDER_MANIFEST`], seeded
/// into the core's manifest cache as though read from dotNS, so the core
/// neither reads the chain for it nor drops it for publishing nothing there.
pub async fn offer_scripted_providers(
	runtime: &SigningHostRuntime,
	platform: &dyn CoreStorage,
) -> anyhow::Result<()> {
	let Ok(spec) = std::env::var("TRUAPI_FUNDING_PROVIDERS") else {
		return Ok(());
	};
	let now_secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs();
	let mut providers = Vec::new();
	for product_id in spec.split(',').map(str::trim).filter(|id| !id.is_empty()) {
		platform
			.write_core_storage(
				truapi::worker_manifest_cache_key(product_id),
				truapi::encode_cached_root_manifest(Some(SCRIPTED_PROVIDER_MANIFEST), now_secs),
			)
			.await
			.map_err(|error| {
				anyhow::anyhow!("seeding the funding provider {product_id}: {error:?}")
			})?;
		providers.push(FundingProviderEntry {
			product_id: product_id.to_string(),
			worker_manifest: Some(SCRIPTED_PROVIDER_MANIFEST.to_string()),
		});
	}
	runtime
		.set_funding_providers(providers)
		.map_err(|error| anyhow::anyhow!("offering funding providers: {}", error.reason))
}

/// The ask a scripted session is quoted on: a card payment in EUR inbound,
/// USDT outbound, as [`SCRIPTED_PROVIDER_MANIFEST`] serves them.
fn scripted_ask(direction: FundingDirection, amount: Option<u128>) -> FundingQuoteAsk {
	let (rail, asset) = match direction {
		FundingDirection::In => (FundingRail::Card, "EUR"),
		FundingDirection::Out => (FundingRail::Crypto, "USDT"),
	};
	FundingQuoteAsk {
		direction,
		rail,
		asset: asset.to_string(),
		amount: amount.unwrap_or(1_000),
		country: None,
	}
}

/// How long a started session stays in flight before it is settled, so a
/// product sees its first status before the terminal one.
const SETTLE_AFTER: Duration = Duration::from_millis(500);

/// What the scripted provider does with one request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
	/// The user starts and the funds arrive.
	Deliver(u128),
	/// The user starts and the funds leave.
	Release(u128),
	/// The user starts and the provider fails.
	Fail,
	/// The user closes the overlay.
	Dismiss,
	/// The user picks the product that asked as the provider.
	Provide,
	/// As `Provide`, then the user cancels.
	ProvideThenCancel,
}

/// Parse `deliver:900,release:500,fail,dismiss`.
fn parse_outcomes(spec: &str) -> VecDeque<Outcome> {
	spec.split(',')
		.map(str::trim)
		.filter_map(|entry| match entry.split_once(':') {
			Some(("deliver", amount)) => amount.parse().ok().map(Outcome::Deliver),
			Some(("release", amount)) => amount.parse().ok().map(Outcome::Release),
			None if entry == "fail" => Some(Outcome::Fail),
			None if entry == "dismiss" => Some(Outcome::Dismiss),
			None if entry == "provide" => Some(Outcome::Provide),
			None if entry == "provide-cancel" => Some(Outcome::ProvideThenCancel),
			_ => None,
		})
		.collect()
}

/// A funding overlay and provider that follow a script.
pub struct CliFundingHost {
	outcomes: Mutex<VecDeque<Outcome>>,
	transcript: Transcript,
	runtime: Weak<SigningHostRuntime>,
}

impl CliFundingHost {
	/// Install a scripted funding host on `runtime` when
	/// `TRUAPI_FUNDING_OUTCOMES` is set, recording to `TRUAPI_FUNDING_LOG`.
	pub fn install_from_env(runtime: &Arc<SigningHostRuntime>) {
		let Ok(spec) = std::env::var("TRUAPI_FUNDING_OUTCOMES") else {
			return;
		};
		runtime.set_funding_platform(Arc::new(Self {
			outcomes: Mutex::new(parse_outcomes(&spec)),
			transcript: Transcript::from_env("TRUAPI_FUNDING_LOG", "funding"),
			runtime: Arc::downgrade(runtime),
		}));
		if let Some(ledger) = std::env::var_os("TRUAPI_FUNDING_LEDGER").map(PathBuf::from) {
			let payments = Arc::new(CliPaymentHost { ledger });
			runtime.set_top_up_platform(payments.clone());
			runtime.set_payment_platform(payments);
		}
	}

	/// Hand session `intent` to the product that asked for it, and cancel it
	/// shortly after when `cancel` is set.
	async fn provide(
		&self,
		runtime: Weak<SigningHostRuntime>,
		product: Option<&ProductContext>,
		intent: String,
		ask: FundingQuoteAsk,
		cancel: bool,
	) -> Result<FundingPresentOutcome, GenericError> {
		let (Some(runtime), Some(product)) = (runtime.upgrade(), product) else {
			return Ok(FundingPresentOutcome::Dismissed);
		};
		let candidates: Vec<String> = runtime
			.funding_candidates(&intent)
			.into_iter()
			.map(|candidate| candidate.provider_id)
			.collect();
		self.transcript.record(serde_json::json!({
			"kind": "candidates",
			"intent": intent,
			"providers": candidates,
		}));
		let rows: Vec<FundingQuoteRow> = runtime.get_funding_quote(&intent, ask).collect().await;
		let chosen = rows.iter().find_map(|row| match &row.state {
			FundingQuoteState::Quoted { quote } if row.provider_id == product.product_id => {
				Some(quote.quote_id.clone())
			},
			_ => None,
		});
		self.transcript.record(serde_json::json!({
			"kind": "quotes",
			"intent": intent,
			"rows": rows.iter().map(|row| serde_json::json!({
				"provider": row.provider_id,
				"state": match &row.state {
					FundingQuoteState::Pending => "Pending".to_string(),
					FundingQuoteState::Quoted { quote } => format!("Quoted:{}", quote.quote_id),
					FundingQuoteState::Unavailable { reason } => format!("Unavailable:{reason:?}"),
				},
			})).collect::<Vec<_>>(),
		}));
		runtime
			.select_funding_provider(&intent, &product.product_id, chosen.as_deref())
			.await?;
		if cancel {
			let runtime = Arc::downgrade(&runtime);
			tokio::spawn(async move {
				tokio::time::sleep(SETTLE_AFTER).await;
				let Some(runtime) = runtime.upgrade() else {
					return;
				};
				if let Err(error) = runtime.cancel_funding(&intent).await {
					tracing::warn!(%intent, reason = %error.reason, "cancelling a scripted funding session failed");
				}
			});
		}
		Ok(FundingPresentOutcome::Started)
	}

	fn next_outcome(&self) -> Outcome {
		self.outcomes
			.lock()
			.unwrap_or_else(|poisoned| poisoned.into_inner())
			.pop_front()
			.unwrap_or(Outcome::Dismiss)
	}
}

/// The stage `outcome` ends a started session in.
fn settled_stage(outcome: Outcome) -> Option<FundingStage> {
	let settled_at_ms = current_unix_millis();
	match outcome {
		Outcome::Deliver(credited) => Some(FundingStage::Delivered { credited, settled_at_ms }),
		Outcome::Release(debited) => Some(FundingStage::Released { debited, settled_at_ms }),
		Outcome::Fail => Some(FundingStage::Failed {
			reason: FundingFailure::Other {
				code: "provider_failed".into(),
				message: "the scripted provider failed the session".into(),
			},
			settled_at_ms,
		}),
		Outcome::Dismiss | Outcome::Provide | Outcome::ProvideThenCancel => None,
	}
}

#[async_trait]
impl FundingPlatform for CliFundingHost {
	async fn present_funding(
		&self,
		product: Option<&ProductContext>,
		session: FundingPresentation,
	) -> Result<FundingPresentOutcome, GenericError> {
		let outcome = self.next_outcome();
		self.transcript.record(serde_json::json!({
			"kind": "presented",
			"intent": session.intent,
			"direction": match session.direction {
				FundingDirection::In => "In",
				FundingDirection::Out => "Out",
			},
			"amount": session.amount.map(|amount| amount.to_string()),
			"product": product.map(|product| product.product_id.clone()),
			"outcome": format!("{outcome:?}"),
		}));
		let runtime = self.runtime.clone();
		let intent = session.intent;
		if matches!(outcome, Outcome::Provide | Outcome::ProvideThenCancel) {
			let ask = scripted_ask(session.direction, session.amount);
			return self
				.provide(runtime, product, intent, ask, outcome == Outcome::ProvideThenCancel)
				.await;
		}
		let Some(stage) = settled_stage(outcome) else {
			return Ok(FundingPresentOutcome::Dismissed);
		};
		tokio::spawn(async move {
			tokio::time::sleep(SETTLE_AFTER).await;
			let Some(runtime) = runtime.upgrade() else {
				return;
			};
			match runtime.settle_funding_for_test(&intent, stage).await {
				Ok(true) => {},
				Ok(false) => {
					tracing::warn!(%intent, "the scripted funding session had already ended")
				},
				Err(error) => {
					tracing::warn!(%intent, reason = %error.reason, "settling a scripted funding session failed")
				},
			}
		});
		Ok(FundingPresentOutcome::Started)
	}

	async fn present_provider_frame(
		&self,
		provider: &ProductContext,
		intent: String,
		target: FundingFrameTarget,
	) -> Result<FundingFrameOutcome, GenericError> {
		let (shown, outcome) = match target {
			FundingFrameTarget::App { route } => (route, FundingFrameOutcome::Closed),
			FundingFrameTarget::Page { url, return_url } => {
				let outcome = match return_url {
					Some(url) => FundingFrameOutcome::Returned { url },
					None => FundingFrameOutcome::Closed,
				};
				(url, outcome)
			},
		};
		self.transcript.record(serde_json::json!({
			"kind": "frame",
			"intent": intent,
			"provider": provider.product_id,
			"route": shown,
		}));
		Ok(outcome)
	}

	fn funding_session_changed(&self, intent: String, status: HostFundingStatusSubscribeItem) {
		let (tag, amount) = match status {
			HostFundingStatusSubscribeItem::InProgress { .. } => ("InProgress", None),
			HostFundingStatusSubscribeItem::Delivered { credited } => ("Delivered", Some(credited)),
			HostFundingStatusSubscribeItem::Released { debited } => ("Released", Some(debited)),
			HostFundingStatusSubscribeItem::Failed { .. } => ("Failed", None),
		};
		self.transcript.record(serde_json::json!({
			"kind": "status",
			"intent": intent,
			"tag": tag,
			"amount": amount.map(|amount| amount.to_string()),
		}));
	}
}

/// Scripted top-up and payment engines: every top-up claims in full and every
/// payment completes, once started. What was started is kept in a ledger file,
/// one JSON object per line, so a restarted host still answers for it.
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
			"product": product.product_id,
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

#[async_trait]
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

#[async_trait]
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
mod tests {
	use super::*;

	// The worker is told a top-up started only once the ledger holds it, as a
	// start this host forgets would read as unknown after a restart and never
	// settle.
	#[test]
	fn a_start_the_ledger_cannot_keep_is_refused() {
		let directory = tempfile::tempdir().expect("temporary directory");
		let host = CliPaymentHost { ledger: directory.path().join("missing").join("ledger.jsonl") };
		let product = ProductContext::new_with_execution(
			"ramp.dot".to_string(),
			truapi::platform::ProductExecutionKind::Worker,
		)
		.expect("valid product");

		let started = futures::executor::block_on(host.top_up(
			&product,
			HostPaymentTopUpRequest {
				into: None,
				amount: 1,
				source: truapi::latest::PaymentTopUpSource::ProductAccount {
					derivation_index: truapi::latest::DerivationIndex::Index(0),
				},
				id: [1; 32],
			},
		));

		assert!(matches!(started, Err(HostPaymentTopUpError::Unknown { .. })), "{started:?}");
	}

	// The battery seeds one outcome per request it makes, in order; an
	// entry this host cannot read is dropped rather than shifting the rest.
	#[test]
	fn outcomes_are_read_in_order_and_unknown_entries_dropped() {
		assert_eq!(
			parse_outcomes(
				"deliver:900, release:500,fail,bogus,dismiss,deliver:x,provide,provide-cancel"
			),
			VecDeque::from([
				Outcome::Deliver(900),
				Outcome::Release(500),
				Outcome::Fail,
				Outcome::Dismiss,
				Outcome::Provide,
				Outcome::ProvideThenCancel,
			])
		);
	}
}
