//! N2 · RFC-6 surface: the coinage layers as the host's top-up, payment and
//! balance engine.
//!
//! The engine implements the interfaces the core has always asked a host
//! for, [`TopUpEngine`], [`PaymentEngine`] and [`BalanceView`], so
//! RFC-6 calls and the funding sessions that follow them by id work over
//! the coinage layers unchanged. The id each call receives already has the
//! product hashed in, and names the operation in the product's own layer-1
//! scope (N2 invariant 3). Amounts are in the payment asset's units,
//! moved rounded down to whole cents (N2 invariant 5).

use core::time::Duration;
use std::sync::Arc;

use futures::stream::{self, BoxStream, StreamExt};
use truapi::latest::{
	HostPaymentBalanceSubscribeError, HostPaymentBalanceSubscribeItem, HostPaymentError,
	HostPaymentRequest, HostPaymentStatusSubscribeError, HostPaymentStatusSubscribeItem, HostPaymentTopUpError,
	HostPaymentTopUpRequest, HostPaymentTopUpStatusSubscribeError, HostPaymentTopUpStatusSubscribeItem,
	PaymentTopUpSource, ProductAccountId, RemotePermissionRequest,
};

use crate::{
	Bytes32,
	coinage::{
		CoinSecret, Initiator, PurseId,
		assembly::{Funding, KeypairFunding},
		layer::{Error, Op},
		operation::{OpKey, Retry},
		payments::{
			flows::Payments,
			permissions::{Action, Requirement, Surface, requirements},
			records::{self, OperationRecord, Rfc6Kind, Rfc6Record},
			status::{payment_status, top_up_status},
		},
	},
	host_internal::permissions::PermissionsService,
	platform::{
		CoinPaymentSpendReview, PermissionAuthorizationStatus, Platform, ProductContext, UserConfirmationReview,
	},
	runtime::{BalanceView, PaymentEngine, TopUpEngine},
	stream_extensions::StreamExtensions,
};

/// How long a top-up waits for its source to hold the funds.
const TOP_UP_WINDOW: Duration = Duration::from_secs(3600);

/// The reason a status stream ends with when layer 1 cannot read the status.
const STATUS_UNAVAILABLE: &str = "the status is unavailable";

/// The keys of a product's own accounts, which a top-up may be paid from.
pub trait ProductFunding: Send + Sync {
	/// The funding capability of `account`; none when the wallet cannot
	/// derive it.
	fn product_account(&self, account: &ProductAccountId) -> Option<Arc<dyn Funding>>;
}

/// The coinage layers as the host's RFC-6 engine.
pub struct CoinageEngine {
	/// Coinage payments.
	pub payments: Payments,
	/// The host, which asks the user.
	pub platform: Arc<dyn Platform>,
	/// The keys of products' own accounts.
	pub funding: Arc<dyn ProductFunding>,
}

impl CoinageEngine {
	/// Asks for everything J1 lists for `action` on `purse` (RFC-6, N2
	/// invariant 2): stored permissions through the permission service, and
	/// a spend's confirmation of `amount` cents. Whether all were granted.
	async fn authorize(
		&self,
		product: &ProductContext,
		purse: PurseId,
		action: Action,
		amount: u64,
		takes_held_back_value: bool,
	) -> Result<bool, Error> {
		let creator = if purse == PurseId::MAIN { None } else { Some(self.payments.layer().query_purse(purse).await?.tag) };
		let id = product.product_id.as_str().as_bytes();
		for requirement in requirements(id, purse, creator.as_deref(), action, Surface::Rfc6) {
			let granted = match requirement {
				Requirement::Permission(permission) => {
					let service = PermissionsService::new(self.platform.as_ref(), self.platform.as_ref(), product);
					let request = RemotePermissionRequest { permission };
					matches!(service.authorize_remote(request).await, Ok(PermissionAuthorizationStatus::Authorized))
				},
				Requirement::ConfirmSpend => self.confirm_spend(product, purse, amount, takes_held_back_value).await,
			};
			if !granted {
				return Ok(false);
			}
		}
		Ok(true)
	}

	async fn confirm_spend(
		&self,
		product: &ProductContext,
		purse: PurseId,
		amount: u64,
		takes_held_back_value: bool,
	) -> bool {
		let review = UserConfirmationReview::CoinPaymentSpend(CoinPaymentSpendReview {
			calling_product_id: product.product_id.clone(),
			from_purse: purse.0,
			amount: u128::from(amount).saturating_mul(self.payments.layer().asset_unit()),
			takes_held_back_value,
		});
		matches!(self.platform.confirm_user_action(review).await, Ok(true))
	}

	/// `amount` in the asset's units, rounded down to whole cents.
	fn cents(&self, amount: u128) -> u64 {
		u64::try_from(amount / self.payments.layer().asset_unit().max(1)).unwrap_or(u64::MAX)
	}

	/// Records an RFC-6 operation under `id` before it starts, or refuses an
	/// id the product already used.
	async fn record(&self, record: Rfc6Record) -> Result<Result<(), records::AlreadyExists>, String> {
		self.payments.store().write(move |tx| records::record_rfc6(tx, &record)).await.map_err(|error| error.to_string())
	}

	/// Whether `product` already used `id` for a payment or a top-up.
	async fn used(&self, product: &ProductContext, id: Bytes32) -> Result<bool, String> {
		let product = product.product_id.as_str().as_bytes().to_vec();
		let record = self.payments.store().read(move |conn| records::rfc6(conn, &product, id)).await;
		record.map(|record| record.is_some()).map_err(|error| error.to_string())
	}

	/// Records that layer 1 refused to start the operation `key`.
	async fn refuse(&self, key: &OpKey, error: &Error) {
		let (key, refusal) = (key.clone(), crate::coinage::payments::status::coin_payment_error(error));
		let _ = self.payments.store().write(move |tx| records::record_refusal(tx, &key, &refusal)).await;
	}

	/// The RFC-6 operation `product` started under `id`, when it is of `kind`.
	async fn find(&self, product: &ProductContext, id: Bytes32, kind: Rfc6Kind) -> Option<Rfc6Record> {
		let product = product.product_id.as_str().as_bytes().to_vec();
		let record = self.payments.store().read(move |conn| records::rfc6(conn, &product, id)).await.ok()??;
		(record.kind == kind).then_some(record)
	}

	/// An operation's status items from layer 1, mapped by `map`, each only
	/// when it differs from the last; a refused operation's single terminal
	/// item instead. A status layer 1 cannot read ends the stream with
	/// `unavailable`, never with an outcome (layer 1, P11).
	async fn statuses<T, E>(
		&self,
		record: &Rfc6Record,
		refused: T,
		unavailable: E,
		map: impl Fn(crate::coinage::status::StatusItem) -> T + Send + 'static,
		is_last: impl Fn(&T) -> bool + Send + 'static,
	) -> BoxStream<'static, Result<T, E>>
	where
		T: PartialEq + Clone + Send + 'static,
		E: Send + 'static,
	{
		if record.operation.refused.is_some() {
			return stream::once(core::future::ready(Ok(refused))).boxed();
		}
		match self.payments.layer().status(record.operation.key.clone()).await {
			Ok(items) => items.map(map).take_until_inclusive(is_last).map(Ok).distinct_until_changed(),
			Err(_) => stream::once(core::future::ready(Err(unavailable))).boxed(),
		}
	}
}

fn key_of(product: &ProductContext, id: Bytes32) -> OpKey {
	OpKey { scope: product.product_id.as_str().as_bytes().to_vec(), id }
}

fn now() -> u64 {
	crate::unix_time::current_unix_secs()
}

#[async_trait::async_trait]
impl TopUpEngine for CoinageEngine {
	async fn top_up(&self, product: &ProductContext, request: HostPaymentTopUpRequest) -> Result<(), HostPaymentTopUpError> {
		let unknown = |reason: String| HostPaymentTopUpError::Unknown { reason };
		let cents = self.cents(request.amount);
		if cents == 0 {
			return Err(HostPaymentTopUpError::InvalidAmount);
		}
		if self.used(product, request.id).await.map_err(unknown)? {
			return Err(HostPaymentTopUpError::AlreadyExists);
		}
		let into = request.into.map_or(PurseId::MAIN, PurseId);
		if !self.authorize(product, into, Action::TopUp, cents, false).await.map_err(|error| unknown(error.to_string()))? {
			return Err(unknown("the user did not allow the top-up".into()));
		}
		let key = key_of(product, request.id);
		let op = Op { key: key.clone(), retry: Retry::Until(now() + TOP_UP_WINDOW.as_secs()) };
		let funding: Option<Arc<dyn Funding>> = match &request.source {
			PaymentTopUpSource::Coins { .. } => None,
			PaymentTopUpSource::PrivateKey { sr25519_secret_key } => {
				let secret = schnorrkel::SecretKey::from_bytes(sr25519_secret_key)
					.map_err(|_| HostPaymentTopUpError::InvalidSource)?;
				Some(Arc::new(KeypairFunding(secret.to_keypair())))
			},
			PaymentTopUpSource::ProductAccount { derivation_index } => {
				let account =
					ProductAccountId { dot_ns_identifier: product.product_id.clone(), derivation_index: derivation_index.clone() };
				Some(self.funding.product_account(&account).ok_or(HostPaymentTopUpError::InvalidSource)?)
			},
		};
		let source = funding.as_ref().map(|funding| funding.account());
		if let Some(account) = source {
			let busy = self.payments.store().read(move |conn| records::top_ups_from(conn, account)).await;
			for earlier in busy.map_err(|error| unknown(error.to_string()))? {
				let ended = earlier.operation.refused.is_some() ||
					self.payments.layer().completion(earlier.operation.key.clone()).await.is_ok_and(|done| done.is_some());
				if !ended {
					return Err(HostPaymentTopUpError::SourceBusy);
				}
			}
		}
		let coins: Vec<CoinSecret> = match &request.source {
			PaymentTopUpSource::Coins { sr25519_secret_keys } => sr25519_secret_keys.iter().map(|secret| CoinSecret(*secret)).collect(),
			_ => Vec::new(),
		};
		if self.payments.layer().importing(&coins).await.map_err(|error| unknown(error.to_string()))? {
			return Err(HostPaymentTopUpError::SourceBusy);
		}
		let record = Rfc6Record {
			product: product.product_id.as_str().as_bytes().to_vec(),
			id: request.id,
			kind: Rfc6Kind::TopUp,
			amount: request.amount,
			source,
			funding: request.funding.clone(),
			operation: OperationRecord { key: key.clone(), refused: None },
		};
		self.record(record).await.map_err(unknown)?.map_err(|_| HostPaymentTopUpError::AlreadyExists)?;
		let started = match funding {
			None => self.payments.layer().import(op, into, coins).await,
			Some(funding) => self.payments.layer().top_up(op, into, cents, funding).await,
		};
		match started {
			Ok(_) => Ok(()),
			Err(Error::InvalidSecret(_)) => {
				self.refuse(&key, &Error::InvalidSecret(0)).await;
				Err(HostPaymentTopUpError::InvalidSource)
			},
			Err(error) => {
				self.refuse(&key, &error).await;
				Err(unknown(error.to_string()))
			},
		}
	}

	fn subscribe_top_up_status(
		&self,
		product: &ProductContext,
		id: Bytes32,
	) -> BoxStream<'static, Result<HostPaymentTopUpStatusSubscribeItem, HostPaymentTopUpStatusSubscribeError>> {
		let (engine, product) = (self.clone_handle(), product.clone());
		stream::once(async move {
			let Some(record) = engine.find(&product, id, Rfc6Kind::TopUp).await else {
				return stream::once(core::future::ready(Err(HostPaymentTopUpStatusSubscribeError::TopUpNotFound))).boxed();
			};
			let (amount, unit) = (record.amount, engine.payments.layer().asset_unit());
			let is_last = |item: &HostPaymentTopUpStatusSubscribeItem| {
				matches!(
					item,
					HostPaymentTopUpStatusSubscribeItem::Claimed { finalized: true } |
						HostPaymentTopUpStatusSubscribeItem::ClaimedPartially { .. } |
						HostPaymentTopUpStatusSubscribeItem::NotClaimed
				)
			};
			let refused = HostPaymentTopUpStatusSubscribeItem::NotClaimed;
			let unavailable = HostPaymentTopUpStatusSubscribeError::Unknown { reason: STATUS_UNAVAILABLE.into() };
			engine.statuses(&record, refused, unavailable, move |item| top_up_status(item, amount, unit), is_last).await
		})
		.flatten()
		.boxed()
	}
}

#[async_trait::async_trait]
impl PaymentEngine for CoinageEngine {
	async fn request_payment(&self, product: &ProductContext, request: HostPaymentRequest) -> Result<(), HostPaymentError> {
		let unknown = |reason: String| HostPaymentError::Unknown { reason };
		let cents = self.cents(request.amount);
		if cents == 0 {
			return Err(HostPaymentError::InvalidAmount);
		}
		if self.used(product, request.id).await.map_err(unknown)? {
			return Err(HostPaymentError::AlreadyExists);
		}
		let from = request.from.map_or(PurseId::MAIN, PurseId);
		if !self.authorize(product, from, Action::Spend, cents, false).await.map_err(|error| unknown(error.to_string()))? {
			return Err(HostPaymentError::Rejected);
		}
		let key = key_of(product, request.id);
		let record = Rfc6Record {
			product: product.product_id.as_str().as_bytes().to_vec(),
			id: request.id,
			kind: Rfc6Kind::Payment,
			amount: request.amount,
			source: None,
			funding: request.funding.clone(),
			operation: OperationRecord { key: key.clone(), refused: None },
		};
		self.record(record).await.map_err(unknown)?.map_err(|_| HostPaymentError::AlreadyExists)?;
		let layer = self.payments.layer();
		let offload = |confirmed| {
			let op = Op { key: key.clone(), retry: Retry::ExpiryOnly };
			layer.offload(op, from, cents, request.destination, Initiator::Product, confirmed)
		};
		let started = match offload(false).await {
			Err(Error::NeedsConfirmation) if self.confirm_spend(product, from, cents, true).await => offload(true).await,
			started => started,
		};
		match started {
			Ok(_) => Ok(()),
			Err(error) => {
				self.refuse(&key, &error).await;
				Err(match error {
					Error::InsufficientFunds { .. } => HostPaymentError::InsufficientBalance,
					Error::NeedsConfirmation => HostPaymentError::Rejected,
					error => unknown(error.to_string()),
				})
			},
		}
	}

	fn subscribe_payment_status(
		&self,
		product: &ProductContext,
		id: Bytes32,
	) -> BoxStream<'static, Result<HostPaymentStatusSubscribeItem, HostPaymentStatusSubscribeError>> {
		let (engine, product) = (self.clone_handle(), product.clone());
		stream::once(async move {
			let Some(record) = engine.find(&product, id, Rfc6Kind::Payment).await else {
				return stream::once(core::future::ready(Err(HostPaymentStatusSubscribeError::PaymentNotFound))).boxed();
			};
			let (amount, unit) = (record.amount, engine.payments.layer().asset_unit());
			let is_last = |item: &HostPaymentStatusSubscribeItem| !matches!(item, HostPaymentStatusSubscribeItem::Processing);
			let refused = HostPaymentStatusSubscribeItem::Failed { reason: "the payment did not start".into() };
			let unavailable = HostPaymentStatusSubscribeError::Unknown { reason: STATUS_UNAVAILABLE.into() };
			engine.statuses(&record, refused, unavailable, move |item| payment_status(item, amount, unit), is_last).await
		})
		.flatten()
		.boxed()
	}
}

impl BalanceView for CoinageEngine {
	fn subscribe_balance(
		&self,
		product: &ProductContext,
		purse: Option<u32>,
	) -> BoxStream<'static, Result<HostPaymentBalanceSubscribeItem, HostPaymentBalanceSubscribeError>> {
		let (engine, product) = (self.clone_handle(), product.clone());
		let purse = purse.map_or(PurseId::MAIN, PurseId);
		stream::once(async move {
			let denied = || stream::once(core::future::ready(Err(HostPaymentBalanceSubscribeError::PermissionDenied))).boxed();
			if !engine.authorize(&product, purse, Action::Query, 0, false).await.unwrap_or(false) {
				return denied();
			}
			let unit = engine.payments.layer().asset_unit();
			match engine.payments.layer().balance(purse).await {
				Ok(balances) => balances
					.map(move |balance| {
						Ok(HostPaymentBalanceSubscribeItem { available: u128::from(balance.spendable).saturating_mul(unit) })
					})
					.boxed(),
				Err(error) => stream::once(core::future::ready(Err(HostPaymentBalanceSubscribeError::Unknown {
					reason: error.to_string(),
				})))
				.boxed(),
			}
		})
		.flatten()
		.boxed()
	}
}

impl CoinageEngine {
	/// A second handle on the same engine, for a stream that outlives the
	/// call.
	fn clone_handle(&self) -> Arc<Self> {
		Arc::new(Self { payments: self.payments.clone(), platform: self.platform.clone(), funding: self.funding.clone() })
	}
}
