//! Product-facing payment capability adapters.
//!
//! Payment returns typed domain errors; CoinPayment returns Unsupported.

use std::sync::Arc;

use futures::{StreamExt, stream::BoxStream};
use tracing::instrument;
use truapi::{
	CallContext, CallError, Subscription,
	api::{CoinPayment, Payment},
	latest,
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
	host_internal::{
		extrinsic::sr25519_secret_from_bytes,
		permissions::{PermissionsService, TemporaryPermissions},
	},
	platform::{
		PermissionAuthorizationStatus, PermissionStatusHost, Platform, ProductContext,
		ProductExecutionKind,
	},
	runtime::{ProductRuntimeHost, payment_id::host_payment_id, versioning::version},
};

#[truapi::async_trait]
impl CoinPayment for ProductRuntimeHost {
	#[instrument(skip_all, fields(runtime.method = "coin_payment.create_purse"))]
	async fn create_purse(
		&self,
		_cx: &CallContext,
		request: HostCoinPaymentCreatePurseRequest,
	) -> Result<HostCoinPaymentCreatePurseResponse, CallError<HostCoinPaymentCreatePurseError>> {
		version(request).exec(|_| async move { Err(CallError::Unsupported) }).await
	}

	#[instrument(skip_all, fields(runtime.method = "coin_payment.query_purse"))]
	async fn query_purse(
		&self,
		_cx: &CallContext,
		request: HostCoinPaymentQueryPurseRequest,
	) -> Result<HostCoinPaymentQueryPurseResponse, CallError<HostCoinPaymentQueryPurseError>> {
		version(request).exec(|_| async move { Err(CallError::Unsupported) }).await
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
		version(request)
			.exec(|_| async move { Subscription::interrupted(CallError::Unsupported) })
			.await
	}

	#[instrument(skip_all, fields(runtime.method = "coin_payment.delete_purse"))]
	async fn delete_purse(
		&self,
		_cx: &CallContext,
		request: HostCoinPaymentDeletePurseRequest,
	) -> Subscription<HostCoinPaymentDeletePurseItem, CallError<HostCoinPaymentDeletePurseError>> {
		version(request)
			.exec(|_| async move { Subscription::interrupted(CallError::Unsupported) })
			.await
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
		version(request).exec(|_| async move { Err(CallError::Unsupported) }).await
	}

	#[instrument(skip_all, fields(runtime.method = "coin_payment.create_cheque"))]
	async fn create_cheque(
		&self,
		_cx: &CallContext,
		request: HostCoinPaymentCreateChequeRequest,
	) -> Result<HostCoinPaymentCreateChequeResponse, CallError<HostCoinPaymentCreateChequeError>> {
		version(request).exec(|_| async move { Err(CallError::Unsupported) }).await
	}

	#[instrument(skip_all, fields(runtime.method = "coin_payment.deposit"))]
	async fn deposit(
		&self,
		_cx: &CallContext,
		request: HostCoinPaymentDepositRequest,
	) -> Subscription<HostCoinPaymentDepositItem, CallError<HostCoinPaymentDepositError>> {
		version(request)
			.exec(|_| async move { Subscription::interrupted(CallError::Unsupported) })
			.await
	}

	#[instrument(skip_all, fields(runtime.method = "coin_payment.refund"))]
	async fn refund(
		&self,
		_cx: &CallContext,
		request: HostCoinPaymentRefundRequest,
	) -> Subscription<HostCoinPaymentRefundItem, CallError<HostCoinPaymentRefundError>> {
		version(request)
			.exec(|_| async move { Subscription::interrupted(CallError::Unsupported) })
			.await
	}

	#[instrument(skip_all, fields(runtime.method = "coin_payment.listen_for_payment"))]
	async fn listen_for_payment(
		&self,
		_cx: &CallContext,
		request: HostCoinPaymentListenForRequest,
	) -> Subscription<HostCoinPaymentListenForItem, CallError<HostCoinPaymentListenForError>> {
		version(request)
			.exec(|_| async move { Subscription::interrupted(CallError::Unsupported) })
			.await
	}
}

impl ProductRuntimeHost {
	/// Record funds this product's worker starts on the funding sessions it
	/// serves, so its reports can name them, before the host sees them: funds
	/// whose record cannot be stored are not started. Returns whether any
	/// session recorded them.
	async fn record_funding_start<E>(
		&self,
		direction: latest::FundingDirection,
		host_id: [u8; 32],
		amount: u128,
	) -> Result<bool, CallError<E>> {
		let registry = self.connection.services.funding();
		if self.connection.product.execution_kind != ProductExecutionKind::Worker ||
			registry.platform().is_none()
		{
			return Ok(false);
		}
		Ok(registry
			.record_start(
				self.connection.platform.as_ref(),
				current_unix_millis(),
				self.connection.product.product_id.as_str(),
				direction,
				host_id,
				amount,
			)
			.await?)
	}

	/// Drop the record of funds the host refused to start.
	async fn forget_funding_start(&self, host_id: [u8; 32]) {
		let forgotten = self
			.connection
			.services
			.funding()
			.forget_start(
				self.connection.platform.as_ref(),
				current_unix_millis(),
				self.connection.product.product_id.as_str(),
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
		version(request)
			.exec(|latest::HostPaymentBalanceSubscribeRequest { purse }| async move {
				let Some(platform) = self.connection.services.balance_platform() else {
					return Subscription::interrupted(CallError::Unsupported);
				};
				if let Err(error) = self.require_session() {
					return Subscription::interrupted(error);
				}
				if let Err(error) = self
					.connection
					.require_remote_permission(
						latest::RemotePermission::BalanceAccess,
						latest::HostPaymentBalanceSubscribeError::PermissionDenied,
					)
					.await
				{
					return Subscription::interrupted(error);
				}
				let balances = platform.subscribe_balance(&self.connection.product, purse);
				Subscription::new(
					self.while_balance_access_holds(balances)
						.map(|result| result.map_err(CallError::Domain)),
				)
			})
			.await
	}

	#[instrument(skip_all, fields(runtime.method = "payment.request"))]
	async fn request(
		&self,
		_cx: &CallContext,
		request: HostPaymentRequest,
	) -> Result<HostPaymentResponse, CallError<HostPaymentError>> {
		version(request).exec(|request| self.request_payment(request)).await
	}

	#[instrument(skip_all, fields(runtime.method = "payment.status_subscribe"))]
	async fn status_subscribe(
		&self,
		_cx: &CallContext,
		request: HostPaymentStatusSubscribeRequest,
	) -> Subscription<HostPaymentStatusSubscribeItem, CallError<HostPaymentStatusSubscribeError>> {
		version(request)
			.exec(|latest::HostPaymentStatusSubscribeRequest { id }| async move {
				let Some(platform) = self.connection.services.payment_platform() else {
					return Subscription::interrupted(CallError::Unsupported);
				};
				if let Err(error) = self.require_session() {
					return Subscription::interrupted(error);
				}
				Subscription::new(
					platform
						.subscribe_payment_status(
							&self.connection.product,
							host_payment_id(&self.connection.product, id),
						)
						.map(|result| result.map_err(CallError::Domain)),
				)
			})
			.await
	}

	#[instrument(skip_all, fields(runtime.method = "payment.top_up"))]
	async fn top_up(
		&self,
		_cx: &CallContext,
		request: HostPaymentTopUpRequest,
	) -> Result<HostPaymentTopUpResponse, CallError<HostPaymentTopUpError>> {
		version(request).exec(|request| self.request_top_up(request)).await
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
		version(request)
			.exec(|latest::HostPaymentTopUpStatusSubscribeRequest { id }| async move {
				let Some(platform) = self.connection.services.top_up_platform() else {
					return Subscription::interrupted(CallError::Unsupported);
				};
				if let Err(error) = self.require_session() {
					return Subscription::interrupted(error);
				}
				Subscription::new(
					platform
						.subscribe_top_up_status(
							&self.connection.product,
							host_payment_id(&self.connection.product, id),
						)
						.map(|result| result.map_err(CallError::Domain)),
				)
			})
			.await
	}
}

impl ProductRuntimeHost {
	/// `Denied` unless a pairing session is connected.
	pub(crate) fn require_session<E>(&self) -> Result<(), CallError<E>> {
		match self.accounts.current_session() {
			Some(_) => Ok(()),
			None => Err(CallError::Denied),
		}
	}

	/// Pass `balances` through while the product still holds balance access,
	/// checked again before each one. A host pushes a purse's balance to every
	/// product following it, so a grant revoked mid-stream ends the stream
	/// with `PermissionDenied` instead of letting later balances through.
	fn while_balance_access_holds(
		&self,
		balances: BoxStream<
			'static,
			Result<
				latest::HostPaymentBalanceSubscribeItem,
				latest::HostPaymentBalanceSubscribeError,
			>,
		>,
	) -> BoxStream<
		'static,
		Result<latest::HostPaymentBalanceSubscribeItem, latest::HostPaymentBalanceSubscribeError>,
	> {
		let check = Arc::new(self.balance_access_check());
		balances
			.then(move |balance| {
				let check = check.clone();
				async move {
					if check.holds().await {
						balance
					} else {
						Err(latest::HostPaymentBalanceSubscribeError::PermissionDenied)
					}
				}
			})
			// The runtime stops polling at the first `Err`; ending here keeps
			// the subscription's "at most one `Err`" for anything else polling
			// it, since every later push would be denied again.
			.scan(false, |denied, balance| {
				let next = (!*denied).then_some(balance);
				*denied = matches!(
					next,
					Some(Err(latest::HostPaymentBalanceSubscribeError::PermissionDenied))
				);
				futures::future::ready(next)
			})
			.boxed()
	}

	/// Whether the product already holds balance access, without asking.
	async fn holds_balance_access(&self) -> bool {
		self.balance_access_check().holds().await
	}

	fn balance_access_check(&self) -> BalanceAccessCheck {
		BalanceAccessCheck {
			platform: self.connection.platform.clone(),
			status: self.connection.permission_status.clone(),
			temporary: self.connection.temporary_permissions.clone(),
			product: self.connection.product.clone(),
		}
	}

	/// Ask the host to collect `request` from the user, under the id the core
	/// derives for this product.
	async fn request_payment(
		&self,
		mut request: latest::HostPaymentRequest,
	) -> Result<(), CallError<latest::HostPaymentError>> {
		let platform = self.connection.services.payment_platform().ok_or(CallError::Unsupported)?;
		self.require_session()?;
		if request.amount == 0 {
			return Err(CallError::Domain(latest::HostPaymentError::InvalidAmount));
		}
		request.id = host_payment_id(&self.connection.product, request.id);
		let (host_id, amount) = (request.id, request.amount);
		let recorded = self
			.record_funding_start(latest::FundingDirection::Out, host_id, amount)
			.await?;
		let requested = platform.request_payment(&self.connection.product, request).await;
		// A repeat of a payment the host already holds keeps the record an
		// earlier call made, so a worker that restarted can still name it. A
		// record this call made goes: the host's payment under that id may ask
		// another amount or belong to an ended session. Any other failure
		// means no money moves under this id.
		if requested.as_ref().is_err_and(|error| {
			recorded || !matches!(error, latest::HostPaymentError::AlreadyExists)
		}) {
			self.forget_funding_start(host_id).await;
		}
		match requested {
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
		let platform = self.connection.services.top_up_platform().ok_or(CallError::Unsupported)?;
		self.require_session()?;
		if request.amount == 0 {
			return Err(CallError::Domain(latest::HostPaymentTopUpError::InvalidAmount));
		}
		if !source_keys_are_valid(&request.source) {
			return Err(CallError::Domain(latest::HostPaymentTopUpError::InvalidSource));
		}
		request.id = host_payment_id(&self.connection.product, request.id);
		let (host_id, amount) = (request.id, request.amount);
		let recorded =
			self.record_funding_start(latest::FundingDirection::In, host_id, amount).await?;
		let started = platform.top_up(&self.connection.product, request).await;
		// As for payments: only a record an earlier call made survives a
		// top-up the host already holds.
		if started.as_ref().is_err_and(|error| {
			recorded || !matches!(error, latest::HostPaymentTopUpError::AlreadyExists)
		}) {
			self.forget_funding_start(host_id).await;
		}
		started.map_err(CallError::Domain)
	}
}

/// What a balance access lookup reads, owned so a stream can repeat it for
/// each balance a host pushes.
struct BalanceAccessCheck {
	platform: Arc<dyn Platform>,
	status: Option<Arc<dyn PermissionStatusHost>>,
	temporary: Arc<TemporaryPermissions>,
	product: ProductContext,
}

impl BalanceAccessCheck {
	/// Whether the product holds balance access, without asking.
	async fn holds(&self) -> bool {
		matches!(
			PermissionsService::new(self.platform.as_ref(), self.platform.as_ref(), &self.product)
				.with_status_host(self.status.as_deref())
				.with_temporary_permissions(self.temporary.clone())
				.peek_remote(&latest::RemotePermissionRequest {
					permission: latest::RemotePermission::BalanceAccess,
				})
				.await,
			Ok(PermissionAuthorizationStatus::Authorized)
		)
	}
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
