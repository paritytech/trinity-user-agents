//! The native route as a funding provider: a [`FundingProviderLink`] the core
//! answers itself, and the flow that runs each session it is handed.
//!
//! A top-up runs in four steps, each derived again from the chain whenever
//! the session is handed over, so a restart resumes where the chain is:
//!
//! 1. Report where to pay: the session key's Asset Hub account and what the
//!    quote asked, then wait for it.
//! 2. Once the deposit is finalized on the key, convert it and send the CASH
//!    to the key on People, signed at nonce 0. A key that has signed already
//!    is past this step.
//! 3. Once CASH is on the key on People, start a top-up from the key's secret,
//!    under the key as its id, so a repeat is the same top-up.
//! 4. Report it as crediting the session, then delivered.

use core::time::Duration;
use std::{
	collections::HashMap,
	sync::{Arc, Mutex, Weak},
};

use futures::future::{AbortHandle, Abortable, BoxFuture, FutureExt};
use truapi::{
	CallError,
	latest::{
		FundingAssignment, FundingCancelAnswer, FundingCancelDecision, FundingDeposit,
		FundingDirection, FundingFailure, FundingInboundUpdate, FundingQuote, FundingQuoteAnswer,
		FundingQuoteAsk, FundingQuoteRefusal, FundingRail, FundingUpdate, HostFundingReportError,
		HostPaymentTopUpError, HostPaymentTopUpRequest, PaymentTopUpSource,
	},
};

use super::{
	chain::{CREDIT_UNIT, NativeChain, Tier, creditable},
	keys::{SessionKey, SessionKeySlot, SessionKeys},
	tokens::{NATIVE_NETWORK, NativeToken},
};
use crate::{
	host_logic::product_account::product_public_key_to_address,
	installation::InstallationIdProvider,
	platform::{ExecutableKind, ProductContext, ProductId},
	runtime::{FundingProviderLink, RuntimeServices, payment_id::host_payment_id},
	spawner::Spawner,
	unix_time::current_unix_millis,
};

/// How long a native quote is honoured.
const QUOTE_TTL_MS: u64 = 60_000;
/// Seconds a native top-up takes once the deposit is finalized: the
/// conversion's block and the XCM's on People.
const ETA_SECS: u64 = 60;

/// The provider the native route serves sessions as: the core itself, under
/// its reserved product id, as a worker would.
pub fn native_provider() -> ProductContext {
	ProductContext::new_with_execution(ProductId::native_funding(), ExecutableKind::Worker)
}

/// What the native route does to the sessions it serves, behind a trait so
/// the flow runs against a fake registry in tests.
#[async_trait::async_trait]
pub trait NativeSessions: Send + Sync {
	/// Report `update` on session `intent`.
	async fn report(
		&self,
		intent: &str,
		update: FundingUpdate,
	) -> Result<(), HostFundingReportError>;

	/// Start `request` for session `intent`; `request.id` is the route's own
	/// id for it, not yet derived for the host.
	async fn start_top_up(
		&self,
		intent: &str,
		request: HostPaymentTopUpRequest,
	) -> Result<(), CallError<HostPaymentTopUpError>>;

	/// Whether the route still serves session `intent`.
	fn is_serving(&self, intent: &str) -> bool;
}

#[async_trait::async_trait]
impl NativeSessions for Weak<RuntimeServices> {
	async fn report(
		&self,
		intent: &str,
		update: FundingUpdate,
	) -> Result<(), HostFundingReportError> {
		let Some(services) = self.upgrade() else {
			return Err(HostFundingReportError::NotFound);
		};
		services
			.funding()
			.report(current_unix_millis(), &native_provider(), intent, update)
			.await
			.map_err(|error| HostFundingReportError::Unknown { reason: error.to_string() })?
	}

	async fn start_top_up(
		&self,
		intent: &str,
		mut request: HostPaymentTopUpRequest,
	) -> Result<(), CallError<HostPaymentTopUpError>> {
		let services = self.upgrade().ok_or(CallError::Unsupported)?;
		let provider = native_provider();
		request.id = host_payment_id(&provider, request.id);
		services.start_funding_top_up(&provider, intent.to_string(), request).await
	}

	fn is_serving(&self, intent: &str) -> bool {
		self.upgrade().is_some_and(|services| {
			services.funding().is_serving(&ProductId::native_funding(), intent)
		})
	}
}

/// What the route needs to run.
pub struct NativeFundingDeps {
	/// The sessions it serves.
	pub sessions: Arc<dyn NativeSessions>,
	/// The chain work.
	pub chain: Arc<dyn NativeChain>,
	/// Derives session keys.
	pub keys: Arc<dyn SessionKeys>,
	/// The installation session keys are derived under.
	pub installation: Arc<dyn InstallationIdProvider>,
	/// Runs each session's flow.
	pub spawner: Spawner,
	/// How long to wait between chain reads while a session waits.
	pub poll: Duration,
}

/// The native funding route.
pub struct NativeFunding {
	deps: NativeFundingDeps,
	/// The sessions whose flow runs, by intent.
	running: Mutex<HashMap<String, Running>>,
}

/// A session whose flow runs.
struct Running {
	abort: AbortHandle,
	/// Whether the deposit reached the key, after which the session can no
	/// longer be cancelled.
	paid: bool,
}

/// The tier and token a native quote id names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct QuotedRoute {
	token: NativeToken,
	tier: Tier,
}

impl QuotedRoute {
	const PREFIX: &'static str = "native";

	fn quote_id(self) -> String {
		format!("{}:{}:{}", Self::PREFIX, self.token.symbol(), self.tier.label())
	}

	fn from_quote_id(quote_id: &str) -> Option<Self> {
		let mut parts = quote_id.split(':');
		(parts.next()? == Self::PREFIX).then_some(())?;
		let token = NativeToken::from_symbol(parts.next()?)?;
		let tier = Tier::from_label(parts.next()?)?;
		parts.next().is_none().then_some(Self { token, tier })
	}
}

impl NativeFunding {
	/// The route over `deps`.
	pub fn new(deps: NativeFundingDeps) -> Arc<Self> {
		Arc::new(Self { deps, running: Mutex::default() })
	}

	/// The tokens the route can take in now, read live.
	pub async fn tokens_in(&self) -> Vec<NativeToken> {
		self.deps.chain.tokens_in().await.unwrap_or_else(|error| {
			tracing::warn!(%error, "reading what the native funding route serves failed");
			Vec::new()
		})
	}

	/// Price `ask` from the chain as it stands.
	pub async fn quote_ask(&self, ask: FundingQuoteAsk) -> FundingQuoteAnswer {
		let refused = |reason| FundingQuoteAnswer::Refused { reason };
		let token = match (ask.direction, ask.rail, ask.network.as_deref()) {
			(FundingDirection::In, FundingRail::Crypto, None | Some(NATIVE_NETWORK)) => {
				NativeToken::from_symbol(&ask.asset)
			},
			_ => None,
		};
		let Some(token) = token else {
			return refused(FundingQuoteRefusal::Unavailable);
		};
		if !self.tokens_in().await.contains(&token) {
			return refused(FundingQuoteRefusal::Unavailable);
		}
		match self.deps.chain.price_in(token, ask.amount).await {
			Ok(price) => FundingQuoteAnswer::Quoted {
				quote: FundingQuote {
					quote_id: QuotedRoute { token, tier: price.tier }.quote_id(),
					send_amount: price.send,
					receive_amount: ask.amount,
					provider_fee: 0,
					network_fee: price.network_fee,
					eta_secs: Some(ETA_SECS),
					expires_at: Some(current_unix_millis().saturating_add(QUOTE_TTL_MS)),
					asset_decimals: token.decimals(),
				},
			},
			Err(error) => {
				tracing::warn!(%error, token = token.symbol(), "pricing a native top-up failed");
				refused(FundingQuoteRefusal::Unavailable)
			},
		}
	}

	/// Run session `assignment`'s flow unless it runs already.
	fn take(self: &Arc<Self>, assignment: FundingAssignment) {
		let intent = assignment.intent.clone();
		let mut running = self.running.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
		if running.contains_key(&intent) {
			return;
		}
		let (abort, registration) = AbortHandle::new_pair();
		running.insert(intent.clone(), Running { abort, paid: false });
		let route = self.clone();
		let flow = Abortable::new(route.clone().run(assignment), registration);
		self.deps.spawner.spawn(async move {
			let _ = flow.await;
			route.lock_running().remove(&intent);
		});
	}

	fn lock_running(&self) -> std::sync::MutexGuard<'_, HashMap<String, Running>> {
		self.running.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
	}

	async fn run(self: Arc<Self>, assignment: FundingAssignment) {
		let intent = assignment.intent.clone();
		if assignment.direction == FundingDirection::Out {
			self.fail(&intent, "the native route does not take withdrawals yet").await;
			return;
		}
		let Some((route, quote)) = assignment.quote.as_ref().and_then(|quote| {
			QuotedRoute::from_quote_id(&quote.quote_id).map(|route| (route, quote.clone()))
		}) else {
			self.fail(&intent, "the session was not started on a native quote").await;
			return;
		};
		let key = loop {
			match self.session_key(&intent).await {
				Ok(key) => break key,
				Err(reason) => {
					tracing::warn!(%reason, "deriving a native funding key failed");
					self.pause().await;
				},
			}
		};
		if assignment.last_update.is_none() {
			let deposit = FundingDeposit::Crypto {
				address: product_public_key_to_address(key.public),
				network: NATIVE_NETWORK.to_string(),
				asset: route.token.symbol().to_string(),
				amount: quote.send_amount,
				decimals: route.token.decimals(),
				exact: matches!(route.tier, Tier::Direct | Tier::Psm { .. }),
				uri: None,
				expires_at: None,
			};
			self.report(&intent, FundingUpdate::In(FundingInboundUpdate::AwaitingPayment)).await;
			self.report(&intent, FundingUpdate::In(FundingInboundUpdate::Deposit { deposit }))
				.await;
		}
		while self.deps.sessions.is_serving(&intent) {
			match self.step(&intent, route, &quote, &key).await {
				Ok(true) => return,
				Ok(false) => {},
				Err(reason) => tracing::warn!(%reason, %intent, "a native top-up step failed"),
			}
			self.pause().await;
		}
	}

	/// Move session `intent` one step on from where the chain is. Returns
	/// whether it is done.
	async fn step(
		&self,
		intent: &str,
		route: QuotedRoute,
		quote: &FundingQuote,
		key: &SessionKey,
	) -> Result<bool, String> {
		let chain = &self.deps.chain;
		let landed = chain.cash_on_people(key.public).await.map_err(|error| error.reason)?;
		if landed >= CREDIT_UNIT {
			self.mark_paid(intent);
			self.credit(intent, key, creditable(landed)).await?;
			return Ok(true);
		}
		if chain.nonce(key.public).await.map_err(|error| error.reason)? > 0 {
			// Converted already: the CASH is on its way to People.
			self.mark_paid(intent);
			return Ok(false);
		}
		let deposit = chain.balance(route.token, key.public).await.map_err(|error| error.reason)?;
		if deposit > 0 {
			// Whatever reached the key stays the user's; the session no
			// longer stops on a cancel.
			self.mark_paid(intent);
		}
		if deposit < quote.send_amount {
			return Ok(false);
		}
		self.report(
			intent,
			FundingUpdate::In(FundingInboundUpdate::PaymentReceived {
				finalized: true,
				mismatch: None,
			}),
		)
		.await;
		self.report(intent, FundingUpdate::In(FundingInboundUpdate::Converting)).await;
		chain
			.convert(key, route.token, route.tier, deposit, quote.receive_amount)
			.await
			.map_err(|error| error.reason)?;
		Ok(false)
	}

	/// Start the top-up that credits `amount` from `key`'s CASH on People,
	/// and report it.
	async fn credit(&self, intent: &str, key: &SessionKey, amount: u128) -> Result<(), String> {
		let request = HostPaymentTopUpRequest {
			into: None,
			amount,
			source: PaymentTopUpSource::PrivateKey { sr25519_secret_key: *key.secret },
			id: key.public,
			funding: Some(intent.to_string()),
		};
		match self.deps.sessions.start_top_up(intent, request).await {
			Ok(()) | Err(CallError::Domain(HostPaymentTopUpError::AlreadyExists)) => {},
			Err(error) => return Err(format!("starting the native top-up failed: {error:?}")),
		}
		let crediting = FundingInboundUpdate::Crediting { top_up_id: key.public, amount };
		self.report(intent, FundingUpdate::In(crediting)).await;
		self.report(intent, FundingUpdate::In(FundingInboundUpdate::Delivered)).await;
		Ok(())
	}

	async fn session_key(&self, intent: &str) -> Result<SessionKey, String> {
		let installation = self
			.deps
			.installation
			.current_installation()
			.await
			.map_err(|error| error.reason)?;
		let slot =
			SessionKeySlot { installation, intent: intent.to_string(), direction: FundingDirection::In };
		self.deps.keys.session_key(&slot).map_err(|error| error.to_string())
	}

	/// Report `update`, which may already stand from before a restart.
	async fn report(&self, intent: &str, update: FundingUpdate) {
		match self.deps.sessions.report(intent, update).await {
			Ok(()) | Err(HostFundingReportError::OutOfOrder) => {},
			Err(error) => tracing::warn!(?error, %intent, "a native funding report was refused"),
		}
	}

	async fn fail(&self, intent: &str, message: &str) {
		let reason = FundingFailure::Other {
			code: "native_unsupported".to_string(),
			message: message.to_string(),
		};
		self.report(intent, FundingUpdate::Failed { reason }).await;
	}

	fn mark_paid(&self, intent: &str) {
		if let Some(running) = self.lock_running().get_mut(intent) {
			running.paid = true;
		}
	}

	async fn pause(&self) {
		futures_timer::Delay::new(self.deps.poll).await;
	}
}

/// The native route as the registry calls it.
pub struct NativeLink(pub Arc<NativeFunding>);

impl FundingProviderLink for NativeLink {
	fn assign(&self, session: FundingAssignment) -> BoxFuture<'static, bool> {
		self.0.take(session);
		futures::future::ready(true).boxed()
	}

	fn quote(&self, ask: FundingQuoteAsk) -> BoxFuture<'static, Option<FundingQuoteAnswer>> {
		let route = self.0.clone();
		async move { Some(route.quote_ask(ask).await) }.boxed()
	}

	fn cancel(&self, intent: String) -> BoxFuture<'static, Option<FundingCancelAnswer>> {
		let mut running = self.0.lock_running();
		let answer = match running.get(&intent) {
			Some(session) if session.paid => FundingCancelAnswer::Refused {
				reason: "the deposit arrived and is being credited".to_string(),
			},
			_ => {
				if let Some(session) = running.remove(&intent) {
					session.abort.abort();
				}
				FundingCancelAnswer::Stopped
			},
		};
		futures::future::ready(Some(answer)).boxed()
	}

	fn settle_cancel(
		&self,
		_intent: String,
		_decision: FundingCancelDecision,
	) -> BoxFuture<'static, bool> {
		// The route never offers terms, so there is nothing to settle.
		futures::future::ready(true).boxed()
	}
}

#[cfg(test)]
mod tests;
