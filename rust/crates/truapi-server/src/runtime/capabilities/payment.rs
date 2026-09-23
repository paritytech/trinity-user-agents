//! Product-facing payment capability adapters.
//!
//! Payment returns typed domain errors; CoinPayment returns Unsupported.

use tracing::instrument;
use truapi::api::{CoinPayment, Payment};
use truapi::versioned::coin_payment::{
    HostCoinPaymentCreateChequeError, HostCoinPaymentCreateChequeRequest,
    HostCoinPaymentCreateChequeResponse, HostCoinPaymentCreatePurseError,
    HostCoinPaymentCreatePurseRequest, HostCoinPaymentCreatePurseResponse,
    HostCoinPaymentCreateReceivableError, HostCoinPaymentCreateReceivableRequest,
    HostCoinPaymentCreateReceivableResponse, HostCoinPaymentDeletePurseError,
    HostCoinPaymentDeletePurseItem, HostCoinPaymentDeletePurseRequest, HostCoinPaymentDepositError,
    HostCoinPaymentDepositItem, HostCoinPaymentDepositRequest, HostCoinPaymentListenForError,
    HostCoinPaymentListenForItem, HostCoinPaymentListenForRequest, HostCoinPaymentQueryPurseError,
    HostCoinPaymentQueryPurseRequest, HostCoinPaymentQueryPurseResponse,
    HostCoinPaymentRebalancePurseError, HostCoinPaymentRebalancePurseItem,
    HostCoinPaymentRebalancePurseRequest, HostCoinPaymentRefundError, HostCoinPaymentRefundItem,
    HostCoinPaymentRefundRequest,
};
use truapi::versioned::payment::{
    HostPaymentBalanceSubscribeError, HostPaymentBalanceSubscribeItem,
    HostPaymentBalanceSubscribeRequest, HostPaymentError, HostPaymentRequest, HostPaymentResponse,
    HostPaymentStatusSubscribeError, HostPaymentStatusSubscribeItem,
    HostPaymentStatusSubscribeRequest, HostPaymentTopUpError, HostPaymentTopUpRequest,
    HostPaymentTopUpResponse,
};
use truapi::{CallContext, CallError, Subscription, v01};

use crate::host_logic::sso::messages::PaymentTopUpRequest;
use crate::runtime::authority::{AuthorityError, PaymentTopUpAuthorityError};
use crate::runtime::{
    PAYMENTS_NOT_IMPLEMENTED, ProductRuntimeHost, remote_authority_call, remote_authority_context,
};
use truapi_platform::normalize_product_identifier;

#[truapi::async_trait]
impl CoinPayment for ProductRuntimeHost {
    #[instrument(skip_all, fields(runtime.method = "coin_payment.create_purse"))]
    async fn create_purse(
        &self,
        _cx: &CallContext,
        _request: HostCoinPaymentCreatePurseRequest,
    ) -> Result<HostCoinPaymentCreatePurseResponse, CallError<HostCoinPaymentCreatePurseError>>
    {
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
    ) -> Subscription<HostCoinPaymentDeletePurseItem, CallError<HostCoinPaymentDeletePurseError>>
    {
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
    ) -> Result<HostCoinPaymentCreateChequeResponse, CallError<HostCoinPaymentCreateChequeError>>
    {
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

#[truapi::async_trait]
impl Payment for ProductRuntimeHost {
    #[instrument(skip_all, fields(runtime.method = "payment.balance_subscribe"))]
    async fn balance_subscribe(
        &self,
        _cx: &CallContext,
        _request: HostPaymentBalanceSubscribeRequest,
    ) -> Subscription<HostPaymentBalanceSubscribeItem, CallError<HostPaymentBalanceSubscribeError>>
    {
        Subscription::interrupted(CallError::Domain(HostPaymentBalanceSubscribeError::V1(
            v01::HostPaymentBalanceSubscribeError::PermissionDenied,
        )))
    }

    #[instrument(skip_all, fields(runtime.method = "payment.request"))]
    async fn request(
        &self,
        _cx: &CallContext,
        _request: HostPaymentRequest,
    ) -> Result<HostPaymentResponse, CallError<HostPaymentError>> {
        Err(CallError::Domain(HostPaymentError::V1(
            v01::HostPaymentError::Unknown {
                reason: PAYMENTS_NOT_IMPLEMENTED.to_string(),
            },
        )))
    }

    #[instrument(skip_all, fields(runtime.method = "payment.status_subscribe"))]
    async fn status_subscribe(
        &self,
        _cx: &CallContext,
        _request: HostPaymentStatusSubscribeRequest,
    ) -> Subscription<HostPaymentStatusSubscribeItem, CallError<HostPaymentStatusSubscribeError>>
    {
        Subscription::interrupted(CallError::Domain(HostPaymentStatusSubscribeError::V1(
            v01::HostPaymentStatusSubscribeError::Unknown {
                reason: PAYMENTS_NOT_IMPLEMENTED.to_string(),
            },
        )))
    }

    #[instrument(skip_all, fields(runtime.method = "payment.top_up"))]
    async fn top_up(
        &self,
        cx: &CallContext,
        request: HostPaymentTopUpRequest,
    ) -> Result<HostPaymentTopUpResponse, CallError<HostPaymentTopUpError>> {
        // Install the secret guard before any rejection or suspension point.
        let mut request = PaymentTopUpRequest {
            calling_product_id: self.product_id(),
            payload: request,
        };
        request.calling_product_id = normalize_product_identifier(&request.calling_product_id)
            .map_err(|_| {
                CallError::Domain(HostPaymentTopUpError::V1(
                    v01::HostPaymentTopUpError::InvalidSource,
                ))
            })?;
        let session = self
            .authority
            .current_session()
            .ok_or_else(|| payment_top_up_error(AuthorityError::Disconnected.into()))?;
        let cx = remote_authority_context(cx);
        remote_authority_call(&cx, self.authority.payment_top_up(&cx, &session, request))
            .await
            .map(|()| HostPaymentTopUpResponse::V1)
            .map_err(payment_top_up_error)
    }
}

fn payment_top_up_error(error: PaymentTopUpAuthorityError) -> CallError<HostPaymentTopUpError> {
    CallError::Domain(HostPaymentTopUpError::V1(error.into()))
}
