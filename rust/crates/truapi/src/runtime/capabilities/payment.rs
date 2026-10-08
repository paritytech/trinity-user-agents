//! Product-facing payment capability adapters.
//!
//! Payment returns typed domain errors; CoinPayment returns Unsupported.

use futures::StreamExt;
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
    HostPaymentTopUpResponse, HostPaymentTopUpStatusSubscribeError,
    HostPaymentTopUpStatusSubscribeItem, HostPaymentTopUpStatusSubscribeRequest,
};
use truapi::{CallContext, CallError, Subscription, v01};

use crate::host_internal::extrinsic::sr25519_secret_from_bytes;
use crate::runtime::ProductRuntimeHost;
use crate::runtime::payment_id::host_payment_id;

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
        request: HostPaymentBalanceSubscribeRequest,
    ) -> Subscription<HostPaymentBalanceSubscribeItem, CallError<HostPaymentBalanceSubscribeError>>
    {
        let HostPaymentBalanceSubscribeRequest::V1(request) = request;
        let Some(platform) = self.services.balance_platform() else {
            return Subscription::interrupted(CallError::Unsupported);
        };
        if self.authority.current_session().is_none() {
            return Subscription::interrupted(CallError::Denied);
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
        Subscription::new(Box::pin(
            platform
                .subscribe_balance(&self.product, request.purse)
                .map(|item| {
                    item.map(HostPaymentBalanceSubscribeItem::V1).map_err(|error| {
                        CallError::Domain(HostPaymentBalanceSubscribeError::V1(error))
                    })
                }),
        ))
    }

    #[instrument(skip_all, fields(runtime.method = "payment.request"))]
    async fn request(
        &self,
        _cx: &CallContext,
        request: HostPaymentRequest,
    ) -> Result<HostPaymentResponse, CallError<HostPaymentError>> {
        let HostPaymentRequest::V1(mut request) = request;
        let platform = self
            .services
            .payment_platform()
            .ok_or(CallError::Unsupported)?;
        if self.authority.current_session().is_none() {
            return Err(CallError::Denied);
        }
        request.id = host_payment_id(&self.product, request.id);
        match platform.request_payment(&self.product, request).await {
            Ok(()) => Ok(HostPaymentResponse::V1),
            // Only a product the user lets see the balance may learn that the
            // balance was short; to any other it reads as a refusal.
            Err(v01::HostPaymentError::InsufficientBalance)
                if !self.holds_balance_access().await =>
            {
                Err(CallError::Domain(HostPaymentError::V1(
                    v01::HostPaymentError::Rejected,
                )))
            }
            Err(error) => Err(CallError::Domain(HostPaymentError::V1(error))),
        }
    }

    #[instrument(skip_all, fields(runtime.method = "payment.status_subscribe"))]
    async fn status_subscribe(
        &self,
        _cx: &CallContext,
        request: HostPaymentStatusSubscribeRequest,
    ) -> Subscription<HostPaymentStatusSubscribeItem, CallError<HostPaymentStatusSubscribeError>>
    {
        let HostPaymentStatusSubscribeRequest::V1(request) = request;
        let Some(platform) = self.services.payment_platform() else {
            return Subscription::interrupted(CallError::Unsupported);
        };
        if self.authority.current_session().is_none() {
            return Subscription::interrupted(CallError::Denied);
        }
        Subscription::new(Box::pin(
            platform
                .subscribe_payment_status(&self.product, host_payment_id(&self.product, request.id))
                .map(|item| {
                    item.map(HostPaymentStatusSubscribeItem::V1).map_err(|error| {
                        CallError::Domain(HostPaymentStatusSubscribeError::V1(error))
                    })
                }),
        ))
    }

    #[instrument(skip_all, fields(runtime.method = "payment.top_up"))]
    async fn top_up(
        &self,
        _cx: &CallContext,
        request: HostPaymentTopUpRequest,
    ) -> Result<HostPaymentTopUpResponse, CallError<HostPaymentTopUpError>> {
        let HostPaymentTopUpRequest::V1(mut request) = request;
        let platform = self
            .services
            .top_up_platform()
            .ok_or(CallError::Unsupported)?;
        if self.authority.current_session().is_none() {
            return Err(CallError::Denied);
        }
        let domain = |error| CallError::Domain(HostPaymentTopUpError::V1(error));
        if request.amount == 0 {
            return Err(domain(v01::HostPaymentTopUpError::Unknown {
                reason: "amount must be positive".to_string(),
            }));
        }
        if !source_keys_are_valid(&request.source) {
            return Err(domain(v01::HostPaymentTopUpError::InvalidSource));
        }
        request.id = host_payment_id(&self.product, request.id);
        platform
            .top_up(&self.product, request)
            .await
            .map(|()| HostPaymentTopUpResponse::V1)
            .map_err(domain)
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
        if self.authority.current_session().is_none() {
            return Subscription::interrupted(CallError::Denied);
        }
        Subscription::new(Box::pin(
            platform
                .subscribe_top_up_status(&self.product, host_payment_id(&self.product, request.id))
                .map(|item| {
                    item.map(HostPaymentTopUpStatusSubscribeItem::V1).map_err(|error| {
                        CallError::Domain(HostPaymentTopUpStatusSubscribeError::V1(error))
                    })
                }),
        ))
    }
}

impl ProductRuntimeHost {
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

/// Whether the secret keys a source carries are usable sr25519 keys. A product
/// account carries none, and a coin source must name at least one coin.
fn source_keys_are_valid(source: &v01::PaymentTopUpSource) -> bool {
    match source {
        v01::PaymentTopUpSource::ProductAccount { .. } => true,
        v01::PaymentTopUpSource::PrivateKey { sr25519_secret_key } => {
            sr25519_secret_from_bytes(sr25519_secret_key).is_ok()
        }
        v01::PaymentTopUpSource::Coins {
            sr25519_secret_keys,
        } => {
            !sr25519_secret_keys.is_empty()
                && sr25519_secret_keys
                    .iter()
                    .all(|key| sr25519_secret_from_bytes(key).is_ok())
        }
    }
}
