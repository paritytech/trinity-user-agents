//! Product-facing payment capability adapters.
//!
//! Payment returns typed domain errors; CoinPayment returns Unsupported.

use futures::{StreamExt, stream::BoxStream};
use tracing::instrument;
use truapi::{
	CallContext, CallError, Subscription,
	api::{CoinPayment, Payment},
	latest,
	versioned::{
		FromLatest, IntoLatest, Versioned,
		coin_payment::{
			HostCoinPaymentCreateChequeError, HostCoinPaymentCreateChequeRequest,
			HostCoinPaymentCreateChequeResponse, HostCoinPaymentCreatePurseError,
			HostCoinPaymentCreatePurseRequest, HostCoinPaymentCreatePurseResponse,
			HostCoinPaymentCreateReceivableError, HostCoinPaymentCreateReceivableRequest,
			HostCoinPaymentCreateReceivableResponse, HostCoinPaymentDeletePurseError,
			HostCoinPaymentDeletePurseItem, HostCoinPaymentDeletePurseRequest,
			HostCoinPaymentDepositError, HostCoinPaymentDepositItem, HostCoinPaymentDepositRequest,
			HostCoinPaymentListenForError, HostCoinPaymentListenForItem,
			HostCoinPaymentListenForRequest, HostCoinPaymentQueryPurseError,
			HostCoinPaymentQueryPurseRequest, HostCoinPaymentQueryPurseResponse,
			HostCoinPaymentRebalancePurseError, HostCoinPaymentRebalancePurseItem,
			HostCoinPaymentRebalancePurseRequest, HostCoinPaymentRefundError,
			HostCoinPaymentRefundItem, HostCoinPaymentRefundRequest,
		},
		payment::{
			HostPaymentBalanceSubscribeError, HostPaymentBalanceSubscribeItem,
			HostPaymentBalanceSubscribeRequest, HostPaymentError, HostPaymentRequest,
			HostPaymentResponse, HostPaymentStatusSubscribeError, HostPaymentStatusSubscribeItem,
			HostPaymentStatusSubscribeRequest, HostPaymentTopUpError, HostPaymentTopUpRequest,
			HostPaymentTopUpResponse, HostPaymentTopUpStatusSubscribeError,
			HostPaymentTopUpStatusSubscribeItem, HostPaymentTopUpStatusSubscribeRequest,
		},
	},
};

use super::from_latest_error;
use crate::{
	host_internal::extrinsic::sr25519_secret_from_bytes,
	runtime::{ProductRuntimeHost, payment_id::host_payment_id},
};

#[truapi::async_trait]
impl CoinPayment for ProductRuntimeHost {
	#[instrument(skip_all, fields(runtime.method = "coin_payment.create_purse"))]
	async fn create_purse(
		&self,
		_cx: &CallContext,
		request: HostCoinPaymentCreatePurseRequest,
	) -> Result<HostCoinPaymentCreatePurseResponse, CallError<HostCoinPaymentCreatePurseError>> {
		let version = request.version();
		let _request = request.into_latest();
		Err(from_latest_error(CallError::Unsupported, version))
	}

	#[instrument(skip_all, fields(runtime.method = "coin_payment.query_purse"))]
	async fn query_purse(
		&self,
		_cx: &CallContext,
		request: HostCoinPaymentQueryPurseRequest,
	) -> Result<HostCoinPaymentQueryPurseResponse, CallError<HostCoinPaymentQueryPurseError>> {
		let version = request.version();
		let _request = request.into_latest();
		Err(from_latest_error(CallError::Unsupported, version))
	}

	#[instrument(skip_all, fields(runtime.method = "coin_payment.rebalance_purse"))]
	async fn rebalance_purse(
		&self,
		_cx: &CallContext,
		request: HostCoinPaymentRebalancePurseRequest,
	) -> Subscription<
		HostCoinPaymentRebalancePurseItem,
		CallError<HostCoinPaymentRebalancePurseError>,
	> {
		let version = request.version();
		let _request = request.into_latest();
		Subscription::interrupted(from_latest_error(CallError::Unsupported, version))
	}

	#[instrument(skip_all, fields(runtime.method = "coin_payment.delete_purse"))]
	async fn delete_purse(
		&self,
		_cx: &CallContext,
		request: HostCoinPaymentDeletePurseRequest,
	) -> Subscription<HostCoinPaymentDeletePurseItem, CallError<HostCoinPaymentDeletePurseError>> {
		let version = request.version();
		let _request = request.into_latest();
		Subscription::interrupted(from_latest_error(CallError::Unsupported, version))
	}

	#[instrument(skip_all, fields(runtime.method = "coin_payment.create_receivable"))]
	async fn create_receivable(
		&self,
		_cx: &CallContext,
		request: HostCoinPaymentCreateReceivableRequest,
	) -> Result<
		HostCoinPaymentCreateReceivableResponse,
		CallError<HostCoinPaymentCreateReceivableError>,
	> {
		let version = request.version();
		let _request = request.into_latest();
		Err(from_latest_error(CallError::Unsupported, version))
	}

	#[instrument(skip_all, fields(runtime.method = "coin_payment.create_cheque"))]
	async fn create_cheque(
		&self,
		_cx: &CallContext,
		request: HostCoinPaymentCreateChequeRequest,
	) -> Result<HostCoinPaymentCreateChequeResponse, CallError<HostCoinPaymentCreateChequeError>> {
		let version = request.version();
		let _request = request.into_latest();
		Err(from_latest_error(CallError::Unsupported, version))
	}

	#[instrument(skip_all, fields(runtime.method = "coin_payment.deposit"))]
	async fn deposit(
		&self,
		_cx: &CallContext,
		request: HostCoinPaymentDepositRequest,
	) -> Subscription<HostCoinPaymentDepositItem, CallError<HostCoinPaymentDepositError>> {
		let version = request.version();
		let _request = request.into_latest();
		Subscription::interrupted(from_latest_error(CallError::Unsupported, version))
	}

	#[instrument(skip_all, fields(runtime.method = "coin_payment.refund"))]
	async fn refund(
		&self,
		_cx: &CallContext,
		request: HostCoinPaymentRefundRequest,
	) -> Subscription<HostCoinPaymentRefundItem, CallError<HostCoinPaymentRefundError>> {
		let version = request.version();
		let _request = request.into_latest();
		Subscription::interrupted(from_latest_error(CallError::Unsupported, version))
	}

	#[instrument(skip_all, fields(runtime.method = "coin_payment.listen_for_payment"))]
	async fn listen_for_payment(
		&self,
		_cx: &CallContext,
		request: HostCoinPaymentListenForRequest,
	) -> Subscription<HostCoinPaymentListenForItem, CallError<HostCoinPaymentListenForError>> {
		let version = request.version();
		let _request = request.into_latest();
		Subscription::interrupted(from_latest_error(CallError::Unsupported, version))
	}
}

#[truapi::async_trait]
impl Payment for ProductRuntimeHost {
	#[instrument(skip_all, fields(runtime.method = "payment.balance_subscribe"))]
	async fn balance_subscribe(
		&self,
		_cx: &CallContext,
		request: HostPaymentBalanceSubscribeRequest,
	) -> Subscription<HostPaymentBalanceSubscribeItem, CallError<HostPaymentBalanceSubscribeError>>
	{
		let version = request.version();
		let latest::HostPaymentBalanceSubscribeRequest { purse } = request.into_latest();
		let Some(platform) = self.services.balance_platform() else {
			return Subscription::interrupted(CallError::Unsupported);
		};
		if let Err(error) = self.require_session() {
			return Subscription::interrupted(error);
		}
		if let Err(error) = self
			.require_remote_permission(
				latest::RemotePermission::BalanceAccess,
				latest::HostPaymentBalanceSubscribeError::PermissionDenied,
			)
			.await
		{
			return Subscription::interrupted(from_latest_error(error, version));
		}
		versioned_subscription(platform.subscribe_balance(&self.product, purse), version)
	}

	#[instrument(skip_all, fields(runtime.method = "payment.request"))]
	async fn request(
		&self,
		_cx: &CallContext,
		request: HostPaymentRequest,
	) -> Result<HostPaymentResponse, CallError<HostPaymentError>> {
		let version = request.version();
		self.request_payment(request.into_latest())
			.await
			.map(|()| HostPaymentResponse::from_latest((), version))
			.map_err(|error| from_latest_error(error, version))
	}

	#[instrument(skip_all, fields(runtime.method = "payment.status_subscribe"))]
	async fn status_subscribe(
		&self,
		_cx: &CallContext,
		request: HostPaymentStatusSubscribeRequest,
	) -> Subscription<HostPaymentStatusSubscribeItem, CallError<HostPaymentStatusSubscribeError>> {
		let version = request.version();
		let latest::HostPaymentStatusSubscribeRequest { id } = request.into_latest();
		let Some(platform) = self.services.payment_platform() else {
			return Subscription::interrupted(CallError::Unsupported);
		};
		if let Err(error) = self.require_session() {
			return Subscription::interrupted(error);
		}
		versioned_subscription(
			platform.subscribe_payment_status(&self.product, host_payment_id(&self.product, id)),
			version,
		)
	}

	#[instrument(skip_all, fields(runtime.method = "payment.top_up"))]
	async fn top_up(
		&self,
		_cx: &CallContext,
		request: HostPaymentTopUpRequest,
	) -> Result<HostPaymentTopUpResponse, CallError<HostPaymentTopUpError>> {
		let version = request.version();
		self.request_top_up(request.into_latest())
			.await
			.map(|()| HostPaymentTopUpResponse::from_latest((), version))
			.map_err(|error| from_latest_error(error, version))
	}

	#[instrument(skip_all, fields(runtime.method = "payment.top_up_status_subscribe"))]
	async fn top_up_status_subscribe(
		&self,
		_cx: &CallContext,
		request: HostPaymentTopUpStatusSubscribeRequest,
	) -> Subscription<
		HostPaymentTopUpStatusSubscribeItem,
		CallError<HostPaymentTopUpStatusSubscribeError>,
	> {
		let version = request.version();
		let latest::HostPaymentTopUpStatusSubscribeRequest { id } = request.into_latest();
		let Some(platform) = self.services.top_up_platform() else {
			return Subscription::interrupted(CallError::Unsupported);
		};
		if let Err(error) = self.require_session() {
			return Subscription::interrupted(error);
		}
		versioned_subscription(
			platform.subscribe_top_up_status(&self.product, host_payment_id(&self.product, id)),
			version,
		)
	}
}

impl ProductRuntimeHost {
	/// `Denied` unless a pairing session is connected.
	fn require_session<E>(&self) -> Result<(), CallError<E>> {
		match self.authority.current_session() {
			Some(_) => Ok(()),
			None => Err(CallError::Denied),
		}
	}

	/// Whether the product already holds balance access, without asking.
	async fn holds_balance_access(&self) -> bool {
		matches!(
			self.permissions_service()
				.peek_remote(&latest::RemotePermissionRequest {
					permission: latest::RemotePermission::BalanceAccess,
				})
				.await,
			Ok(crate::platform::PermissionAuthorizationStatus::Authorized)
		)
	}

	/// Ask the host to collect `request` from the user, under the id the core
	/// derives for this product.
	async fn request_payment(
		&self,
		mut request: latest::HostPaymentRequest,
	) -> Result<(), CallError<latest::HostPaymentError>> {
		let platform = self.services.payment_platform().ok_or(CallError::Unsupported)?;
		self.require_session()?;
		if request.amount == 0 {
			return Err(CallError::Domain(latest::HostPaymentError::InvalidAmount));
		}
		request.id = host_payment_id(&self.product, request.id);
		match platform.request_payment(&self.product, request).await {
			Ok(()) => Ok(()),
			// Only a product the user lets see the balance may learn that the
			// balance was short; to any other it reads as a refusal.
			Err(latest::HostPaymentError::InsufficientBalance)
				if !self.holds_balance_access().await =>
			{
				Err(CallError::Domain(latest::HostPaymentError::Rejected))
			},
			Err(error) => Err(CallError::Domain(error)),
		}
	}

	/// Ask the host to credit `request` to the user, under the id the core
	/// derives for this product.
	async fn request_top_up(
		&self,
		mut request: latest::HostPaymentTopUpRequest,
	) -> Result<(), CallError<latest::HostPaymentTopUpError>> {
		let platform = self.services.top_up_platform().ok_or(CallError::Unsupported)?;
		self.require_session()?;
		if request.amount == 0 {
			return Err(CallError::Domain(latest::HostPaymentTopUpError::InvalidAmount));
		}
		if !source_keys_are_valid(&request.source) {
			return Err(CallError::Domain(latest::HostPaymentTopUpError::InvalidSource));
		}
		request.id = host_payment_id(&self.product, request.id);
		platform.top_up(&self.product, request).await.map_err(CallError::Domain)
	}
}

/// A subscription over a host stream of latest items and errors, each turned
/// into the caller's protocol version as it is polled.
fn versioned_subscription<Item, Error>(
	stream: BoxStream<'static, Result<Item::Latest, Error::Latest>>,
	version: u8,
) -> Subscription<Item, CallError<Error>>
where
	Item: FromLatest + 'static,
	Error: FromLatest + 'static,
	Item::Latest: 'static,
	Error::Latest: 'static,
{
	Subscription::new(stream.map(move |next| {
		next.map(|item| Item::from_latest(item, version))
			.map_err(|failure| CallError::Domain(Error::from_latest(failure, version)))
	}))
}

/// Whether the secret keys a source carries are usable sr25519 keys. A product
/// account carries none, and a coin source must name at least one coin.
fn source_keys_are_valid(source: &latest::PaymentTopUpSource) -> bool {
	match source {
		latest::PaymentTopUpSource::ProductAccount { .. } => true,
		latest::PaymentTopUpSource::PrivateKey { sr25519_secret_key } => {
			sr25519_secret_from_bytes(sr25519_secret_key).is_ok()
		},
		latest::PaymentTopUpSource::Coins { sr25519_secret_keys } => {
			!sr25519_secret_keys.is_empty() &&
				sr25519_secret_keys.iter().all(|key| sr25519_secret_from_bytes(key).is_ok())
		},
	}
}
