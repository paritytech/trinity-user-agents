//! The CLI signing host's funding surface: the overlay, provider screens, and
//! the user's answers, as a host app serves them.
//!
//! A product's funding request shows the overlay: the user picks one of the
//! providers named in `TRUAPI_FUNDING_PROVIDERS` (comma-separated product
//! ids) or closes it. Every question goes through the host's approval policy,
//! as its other confirmations do: a prompt on a terminal, the first provider
//! and every offer accepted under `--auto-accept`, and a refusal when a
//! prompt has no terminal to ask on. The user cancels a session with
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
//! Every overlay, screen, cancel, answer and session change is appended to
//! the transcript named by `TRUAPI_FUNDING_LOG`, one JSON object per line, so
//! a battery can assert what the host saw rather than only what the product
//! was told.

use std::{
	collections::HashSet,
	fs::OpenOptions,
	io::Write,
	path::PathBuf,
	sync::{Arc, Mutex, Weak},
};

use futures::stream::{self, BoxStream, StreamExt};
use truapi::{
	SigningHostRuntime,
	host_logic::funding::{CancelTerms, FundingCancel},
	latest::{
		FundingDirection, FundingFrameOutcome, FundingFrameTarget, GenericError,
		HostFundingStatusSubscribeItem, HostPaymentError, HostPaymentRequest,
		HostPaymentStatusSubscribeError, HostPaymentStatusSubscribeItem, HostPaymentTopUpError,
		HostPaymentTopUpRequest, HostPaymentTopUpStatusSubscribeError,
		HostPaymentTopUpStatusSubscribeItem,
	},
	platform::{
		FundingPlatform, FundingPresentOutcome, FundingPresentation, PaymentPlatform,
		ProductContext, ProductId, TopUpPlatform, async_trait,
	},
};

use crate::{platform::CliPlatform, transcript::Transcript};

/// The funding overlay and provider screens of the CLI signing host.
pub struct CliFundingHost {
	/// The providers the overlay offers, in order.
	providers: Vec<ProductId>,
	/// Where the user's answers come from.
	user: Arc<CliPlatform>,
	transcript: Arc<Transcript>,
	runtime: Weak<SigningHostRuntime>,
	/// Sessions whose current cancel offer the user was asked about.
	asked: Mutex<HashSet<String>>,
}

impl CliFundingHost {
	/// Serve funding on `runtime`, asking `user`, with the providers and
	/// files the environment names.
	pub fn install(runtime: &Arc<SigningHostRuntime>, user: Arc<CliPlatform>) {
		let providers = std::env::var("TRUAPI_FUNDING_PROVIDERS")
			.unwrap_or_default()
			.split(',')
			.map(str::trim)
			.filter(|entry| !entry.is_empty())
			.filter_map(|entry| match entry.parse() {
				Ok(provider) => Some(provider),
				Err(error) => {
					tracing::warn!(%entry, %error, "skipping a funding provider that is not a product id");
					None
				},
			})
			.collect();
		runtime.set_funding_platform(Arc::new(Self {
			providers,
			user,
			transcript: Arc::new(Transcript::from_env("TRUAPI_FUNDING_LOG", "funding")),
			runtime: Arc::downgrade(runtime),
			asked: Mutex::new(HashSet::new()),
		}));
		if let Some(ledger) = std::env::var_os("TRUAPI_FUNDING_LEDGER").map(PathBuf::from) {
			let payments = Arc::new(CliPaymentHost { ledger });
			runtime.set_top_up_platform(payments.clone());
			runtime.set_payment_platform(payments);
		}
	}

	/// Put the provider's cancel offer on session `intent` to the user, once
	/// per offer, and pass on their answer.
	fn ask_about_offer(&self, intent: String) {
		let Some(runtime) = self.runtime.upgrade() else {
			return;
		};
		let offer = match runtime.funding_session(&intent).and_then(|session| session.cancel) {
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

#[async_trait]
impl FundingPlatform for CliFundingHost {
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
		let mut chosen = None;
		for provider in &self.providers {
			let detail = format!(
				"{asker} asks to move {} {direction} with {provider}",
				amount.as_deref().unwrap_or("an amount you choose"),
			);
			if self.user.decide("fund with a provider", detail).await {
				chosen = Some(provider.clone());
				break;
			}
		}
		self.transcript.record(serde_json::json!({
			"kind": "presented",
			"intent": session.intent,
			"direction": direction,
			"amount": amount,
			"product": product.map(|product| product.product_id.as_str()),
			"provider": chosen.as_ref().map(ProductId::as_str),
			"outcome": if chosen.is_some() { "Started" } else { "Dismissed" },
		}));
		let (Some(provider), Some(runtime)) = (chosen, self.runtime.upgrade()) else {
			return Ok(FundingPresentOutcome::Dismissed);
		};
		runtime.select_funding_provider(&session.intent, provider.as_str()).await?;
		Ok(FundingPresentOutcome::Started)
	}

	async fn present_provider_frame(
		&self,
		provider: &ProductContext,
		intent: String,
		target: FundingFrameTarget,
	) -> Result<FundingFrameOutcome, GenericError> {
		let (shown, finished) = match target {
			FundingFrameTarget::App { route } => (route, FundingFrameOutcome::Closed),
			FundingFrameTarget::Page { url, return_url } => {
				let finished = match return_url {
					Some(url) => FundingFrameOutcome::Returned { url },
					None => FundingFrameOutcome::Closed,
				};
				(url, finished)
			},
		};
		let done = self
			.user
			.decide(
				"finish the provider's screen",
				format!("{} shows {shown} for {intent}", provider.product_id),
			)
			.await;
		self.transcript.record(serde_json::json!({
			"kind": "frame",
			"intent": intent,
			"provider": provider.product_id.as_str(),
			"route": shown,
			"finished": done,
		}));
		Ok(if done { finished } else { FundingFrameOutcome::Dismissed })
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
		self.ask_about_offer(intent);
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
			"ramp.dot".parse().expect("valid product"),
			truapi::platform::ProductExecutionKind::Worker,
		);

		let started = futures::executor::block_on(host.top_up(
			&product,
			HostPaymentTopUpRequest {
				into: None,
				amount: 1,
				source: truapi::latest::PaymentTopUpSource::ProductAccount {
					derivation_index: truapi::latest::DerivationIndex::Index(0),
				},
				id: [1; 32],
				funding: None,
			},
		));

		assert!(matches!(started, Err(HostPaymentTopUpError::Unknown { .. })), "{started:?}");
	}
}
