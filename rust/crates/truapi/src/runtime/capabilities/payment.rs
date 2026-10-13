//! Product-facing payment capability adapters.
//!
//! Payment returns typed domain errors; CoinPayment returns Unsupported.

use futures::{StreamExt, stream::BoxStream};
use tracing::instrument;
use truapi::{
	CallContext, CallError, Subscription,
	api::{CoinPayment, Payment},
	v01,
	versioned::{
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

use crate::{
	current_unix_millis,
	host_internal::extrinsic::sr25519_secret_from_bytes,
	platform::ProductExecutionKind,
	runtime::{ProductRuntimeHost, payment_id::host_payment_id},
};

#[truapi::async_trait]
impl CoinPayment for ProductRuntimeHost {
	#[instrument(skip_all, fields(runtime.method = "coin_payment.create_purse"))]
	async fn create_purse(
		&self,
		_cx: &CallContext,
		_request: HostCoinPaymentCreatePurseRequest,
	) -> Result<HostCoinPaymentCreatePurseResponse, CallError<HostCoinPaymentCreatePurseError>> {
		Err(CallError::Unsupported)
	}

	#[instrument(skip_all, fields(runtime.method = "coin_payment.query_purse"))]
	async fn query_purse(
		&self,
		_cx: &CallContext,
		_request: HostCoinPaymentQueryPurseRequest,
	) -> Result<HostCoinPaymentQueryPurseResponse, CallError<HostCoinPaymentQueryPurseError>> {
		Err(CallError::Unsupported)
	}

	#[instrument(skip_all, fields(runtime.method = "coin_payment.rebalance_purse"))]
	async fn rebalance_purse(
		&self,
		_cx: &CallContext,
		_request: HostCoinPaymentRebalancePurseRequest,
	) -> Subscription<
		HostCoinPaymentRebalancePurseItem,
		CallError<HostCoinPaymentRebalancePurseError>,
	> {
		Subscription::interrupted(CallError::Unsupported)
	}

	#[instrument(skip_all, fields(runtime.method = "coin_payment.delete_purse"))]
	async fn delete_purse(
		&self,
		_cx: &CallContext,
		_request: HostCoinPaymentDeletePurseRequest,
	) -> Subscription<HostCoinPaymentDeletePurseItem, CallError<HostCoinPaymentDeletePurseError>> {
		Subscription::interrupted(CallError::Unsupported)
	}

	#[instrument(skip_all, fields(runtime.method = "coin_payment.create_receivable"))]
	async fn create_receivable(
		&self,
		_cx: &CallContext,
		_request: HostCoinPaymentCreateReceivableRequest,
	) -> Result<
		HostCoinPaymentCreateReceivableResponse,
		CallError<HostCoinPaymentCreateReceivableError>,
	> {
		Err(CallError::Unsupported)
	}

	#[instrument(skip_all, fields(runtime.method = "coin_payment.create_cheque"))]
	async fn create_cheque(
		&self,
		_cx: &CallContext,
		_request: HostCoinPaymentCreateChequeRequest,
	) -> Result<HostCoinPaymentCreateChequeResponse, CallError<HostCoinPaymentCreateChequeError>> {
		Err(CallError::Unsupported)
	}

	#[instrument(skip_all, fields(runtime.method = "coin_payment.deposit"))]
	async fn deposit(
		&self,
		_cx: &CallContext,
		_request: HostCoinPaymentDepositRequest,
	) -> Subscription<HostCoinPaymentDepositItem, CallError<HostCoinPaymentDepositError>> {
		Subscription::interrupted(CallError::Unsupported)
	}

	#[instrument(skip_all, fields(runtime.method = "coin_payment.refund"))]
	async fn refund(
		&self,
		_cx: &CallContext,
		_request: HostCoinPaymentRefundRequest,
	) -> Subscription<HostCoinPaymentRefundItem, CallError<HostCoinPaymentRefundError>> {
		Subscription::interrupted(CallError::Unsupported)
	}

	#[instrument(skip_all, fields(runtime.method = "coin_payment.listen_for_payment"))]
	async fn listen_for_payment(
		&self,
		_cx: &CallContext,
		_request: HostCoinPaymentListenForRequest,
	) -> Subscription<HostCoinPaymentListenForItem, CallError<HostCoinPaymentListenForError>> {
		Subscription::interrupted(CallError::Unsupported)
	}
}

impl ProductRuntimeHost {
	/// Record funds this product's worker starts on the funding sessions it
	/// serves, so its reports can name them, before the host sees them: funds
	/// whose record cannot be stored are not started. Returns whether any
	/// session recorded them.
	async fn record_funding_start<E>(
		&self,
		direction: v01::FundingDirection,
		host_id: [u8; 32],
		amount: u128,
	) -> Result<bool, CallError<E>> {
		let registry = self.services.funding();
		if self.product.execution_kind != ProductExecutionKind::Worker ||
			registry.platform().is_none()
		{
			return Ok(false);
		}
		Ok(registry
			.record_start(
				self.platform.as_ref(),
				current_unix_millis(),
				&self.product.product_id,
				direction,
				host_id,
				amount,
			)
			.await?)
	}

	/// Drop the record of funds the host refused to start, if `recorded`.
	async fn forget_funding_start(&self, recorded: bool, host_id: [u8; 32]) {
		if !recorded {
			return;
		}
		let forgotten = self
			.services
			.funding()
			.forget_start(
				self.platform.as_ref(),
				current_unix_millis(),
				&self.product.product_id,
				host_id,
			)
			.await;
		if let Err(error) = forgotten {
			tracing::warn!(%error, "dropping the record of refused funds failed");
		}
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
		let HostPaymentBalanceSubscribeRequest::V1(request) = request;
		let Some(platform) = self.services.balance_platform() else {
			return Subscription::interrupted(CallError::Unsupported);
		};
		if let Err(error) = self.require_session() {
			return Subscription::interrupted(error);
		}
		if let Err(error) = self
			.require_remote_permission(
				v01::RemotePermission::BalanceAccess,
				HostPaymentBalanceSubscribeError::V1(
					v01::HostPaymentBalanceSubscribeError::PermissionDenied,
				),
			)
			.await
		{
			return Subscription::interrupted(error);
		}
		domain_subscription(
			platform.subscribe_balance(&self.product, request.purse),
			HostPaymentBalanceSubscribeItem::V1,
			HostPaymentBalanceSubscribeError::V1,
		)
	}

	#[instrument(skip_all, fields(runtime.method = "payment.request"))]
	async fn request(
		&self,
		_cx: &CallContext,
		request: HostPaymentRequest,
	) -> Result<HostPaymentResponse, CallError<HostPaymentError>> {
		let HostPaymentRequest::V1(mut request) = request;
		let platform = self.services.payment_platform().ok_or(CallError::Unsupported)?;
		self.require_session()?;
		if request.amount == 0 {
			return Err(CallError::Domain(HostPaymentError::V1(
				v01::HostPaymentError::InvalidAmount,
			)));
		}
		request.id = host_payment_id(&self.product, request.id);
		let (host_id, amount) = (request.id, request.amount);
		let recorded =
			self.record_funding_start(v01::FundingDirection::Out, host_id, amount).await?;
		let requested = platform.request_payment(&self.product, request).await;
		if requested.is_err() {
			self.forget_funding_start(recorded, host_id).await;
		}
		match requested {
			Ok(()) => Ok(HostPaymentResponse::V1),
			// Only a product the user lets see the balance may learn that the
			// balance was short; to any other it reads as a refusal.
			Err(v01::HostPaymentError::InsufficientBalance)
				if !self.holds_balance_access().await =>
			{
				Err(CallError::Domain(HostPaymentError::V1(v01::HostPaymentError::Rejected)))
			},
			Err(error) => Err(CallError::Domain(HostPaymentError::V1(error))),
		}
	}

	#[instrument(skip_all, fields(runtime.method = "payment.status_subscribe"))]
	async fn status_subscribe(
		&self,
		_cx: &CallContext,
		request: HostPaymentStatusSubscribeRequest,
	) -> Subscription<HostPaymentStatusSubscribeItem, CallError<HostPaymentStatusSubscribeError>> {
		let HostPaymentStatusSubscribeRequest::V1(request) = request;
		let Some(platform) = self.services.payment_platform() else {
			return Subscription::interrupted(CallError::Unsupported);
		};
		if let Err(error) = self.require_session() {
			return Subscription::interrupted(error);
		}
		domain_subscription(
			platform.subscribe_payment_status(
				&self.product,
				host_payment_id(&self.product, request.id),
			),
			HostPaymentStatusSubscribeItem::V1,
			HostPaymentStatusSubscribeError::V1,
		)
	}

	#[instrument(skip_all, fields(runtime.method = "payment.top_up"))]
	async fn top_up(
		&self,
		_cx: &CallContext,
		request: HostPaymentTopUpRequest,
	) -> Result<HostPaymentTopUpResponse, CallError<HostPaymentTopUpError>> {
		let HostPaymentTopUpRequest::V1(mut request) = request;
		let platform = self.services.top_up_platform().ok_or(CallError::Unsupported)?;
		self.require_session()?;
		let domain = |error| CallError::Domain(HostPaymentTopUpError::V1(error));
		if request.amount == 0 {
			return Err(domain(v01::HostPaymentTopUpError::InvalidAmount));
		}
		if !source_keys_are_valid(&request.source) {
			return Err(domain(v01::HostPaymentTopUpError::InvalidSource));
		}
		request.id = host_payment_id(&self.product, request.id);
		let (host_id, amount) = (request.id, request.amount);
		let recorded =
			self.record_funding_start(v01::FundingDirection::In, host_id, amount).await?;
		let started = platform.top_up(&self.product, request).await;
		if started.is_err() {
			self.forget_funding_start(recorded, host_id).await;
		}
		started.map(|()| HostPaymentTopUpResponse::V1).map_err(domain)
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
		let HostPaymentTopUpStatusSubscribeRequest::V1(request) = request;
		let Some(platform) = self.services.top_up_platform() else {
			return Subscription::interrupted(CallError::Unsupported);
		};
		if let Err(error) = self.require_session() {
			return Subscription::interrupted(error);
		}
		domain_subscription(
			platform
				.subscribe_top_up_status(&self.product, host_payment_id(&self.product, request.id)),
			HostPaymentTopUpStatusSubscribeItem::V1,
			HostPaymentTopUpStatusSubscribeError::V1,
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
				.peek_remote(&v01::RemotePermissionRequest {
					permission: v01::RemotePermission::BalanceAccess,
				})
				.await,
			Ok(crate::platform::PermissionAuthorizationStatus::Authorized)
		)
	}
}

/// A subscription over a host stream, with its items and errors wrapped in
/// their versioned envelopes.
fn domain_subscription<T, E, Item, Error>(
	stream: BoxStream<'static, Result<T, E>>,
	item: fn(T) -> Item,
	error: fn(E) -> Error,
) -> Subscription<Item, CallError<Error>>
where
	T: 'static,
	E: 'static,
	Item: 'static,
	Error: 'static,
{
	Subscription::new(
		stream.map(move |next| next.map(item).map_err(|failure| CallError::Domain(error(failure)))),
	)
}

/// Whether the secret keys a source carries are usable sr25519 keys. A product
/// account carries none, and a coin source must name at least one coin.
fn source_keys_are_valid(source: &v01::PaymentTopUpSource) -> bool {
	match source {
		v01::PaymentTopUpSource::ProductAccount { .. } => true,
		v01::PaymentTopUpSource::PrivateKey { sr25519_secret_key } => {
			sr25519_secret_from_bytes(sr25519_secret_key).is_ok()
		},
		v01::PaymentTopUpSource::Coins { sr25519_secret_keys } => {
			!sr25519_secret_keys.is_empty() &&
				sr25519_secret_keys.iter().all(|key| sr25519_secret_from_bytes(key).is_ok())
		},
	}
}
