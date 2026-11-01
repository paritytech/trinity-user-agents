//! Product-facing payment capability adapters.
//!
//! Incoming top-ups use the authenticated wallet's Coinage custody. Account
//! payments, balance/status subscriptions and CoinPayment are not implemented by
//! that custody contract and explicitly return Unsupported.

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

use super::from_latest_error;
use truapi::versioned::{FromLatest, IntoLatest, Versioned};

use crate::{
	host_internal::sso_messages::PaymentTopUpRequest,
	runtime::{
		ProductRuntimeHost,
		authority::{AuthorityError, PaymentTopUpAuthorityError},
		remote_authority_call, remote_authority_context,
		versioning::version,
	},
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
			.exec(|_| async move { Subscription::interrupted(CallError::Unsupported) })
			.await
	}

	#[instrument(skip_all, fields(runtime.method = "payment.request"))]
	async fn request(
		&self,
		_cx: &CallContext,
		request: HostPaymentRequest,
	) -> Result<HostPaymentResponse, CallError<HostPaymentError>> {
		version(request).exec(|_| async move { Err(CallError::Unsupported) }).await
	}

	#[instrument(skip_all, fields(runtime.method = "payment.status_subscribe"))]
	async fn status_subscribe(
		&self,
		_cx: &CallContext,
		request: HostPaymentStatusSubscribeRequest,
	) -> Subscription<HostPaymentStatusSubscribeItem, CallError<HostPaymentStatusSubscribeError>> {
		version(request)
			.exec(|_| async move { Subscription::interrupted(CallError::Unsupported) })
			.await
	}

	#[instrument(skip_all, fields(runtime.method = "payment.top_up"))]
	async fn top_up(
		&self,
		cx: &CallContext,
		request: HostPaymentTopUpRequest,
	) -> Result<HostPaymentTopUpResponse, CallError<HostPaymentTopUpError>> {
		let version = request.version();
		// Install the secret guard before any rejection or suspension point.
		// Keep the canonical payload inside its zeroizing SSO envelope.
		let request = PaymentTopUpRequest {
			calling_product_id: self.connection.product_id().clone(),
			payload: request.into_latest(),
		};
		let result = async {
			let session = self
				.accounts
				.current_session()
				.ok_or_else(|| payment_top_up_error(AuthorityError::Disconnected.into()))?;
			let cx = remote_authority_context(cx);
			remote_authority_call(&cx, self.accounts.payment_top_up(&cx, &session, request))
				.await
				.map_err(payment_top_up_error)
		}
		.await;
		result
			.map(|response| HostPaymentTopUpResponse::from_latest(response, version))
			.map_err(|error| from_latest_error::<HostPaymentTopUpError>(error, version))
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
			.exec(|_| async move { Subscription::interrupted(CallError::Unsupported) })
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
}

fn payment_top_up_error(
	error: PaymentTopUpAuthorityError,
) -> CallError<latest::HostPaymentTopUpError> {
	CallError::Domain(error.into())
}
