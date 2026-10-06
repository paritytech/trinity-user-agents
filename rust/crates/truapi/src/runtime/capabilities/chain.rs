//! Product-facing chain capability adapters.
//!
//! `ChainRuntime` keeps one `chainHead_v1` connection per genesis hash over
//! the platform provider, mapping JSON-RPC replies and follow notifications
//! into typed TrUAPI results.

use futures::{FutureExt, StreamExt, pin_mut};
use tracing::instrument;
use truapi::api::Chain;
use truapi::versioned::chain::{
    RemoteChainHeadBodyError, RemoteChainHeadBodyRequest, RemoteChainHeadBodyResponse,
    RemoteChainHeadCallError, RemoteChainHeadCallRequest, RemoteChainHeadCallResponse,
    RemoteChainHeadContinueError, RemoteChainHeadContinueRequest, RemoteChainHeadContinueResponse,
    RemoteChainHeadFollowError, RemoteChainHeadFollowItem, RemoteChainHeadFollowRequest,
    RemoteChainHeadHeaderError, RemoteChainHeadHeaderRequest, RemoteChainHeadHeaderResponse,
    RemoteChainHeadStopOperationError, RemoteChainHeadStopOperationRequest,
    RemoteChainHeadStopOperationResponse, RemoteChainHeadStorageError,
    RemoteChainHeadStorageRequest, RemoteChainHeadStorageResponse, RemoteChainHeadUnpinError,
    RemoteChainHeadUnpinRequest, RemoteChainHeadUnpinResponse, RemoteChainInfoError,
    RemoteChainInfoRequest, RemoteChainInfoResponse, RemoteChainSpecChainNameError,
    RemoteChainSpecChainNameRequest, RemoteChainSpecChainNameResponse,
    RemoteChainSpecGenesisHashError, RemoteChainSpecGenesisHashRequest,
    RemoteChainSpecGenesisHashResponse, RemoteChainSpecPropertiesError,
    RemoteChainSpecPropertiesRequest, RemoteChainSpecPropertiesResponse,
    RemoteChainTransactionBroadcastError, RemoteChainTransactionBroadcastRequest,
    RemoteChainTransactionBroadcastResponse, RemoteChainTransactionStopError,
    RemoteChainTransactionStopRequest, RemoteChainTransactionStopResponse,
};
use truapi::{CallContext, CallError, CancellationReason, Subscription, v01};

use crate::host_logic::features::{chain_info, supported_chains};
use crate::runtime::{
    AUTHORITY_CANCEL_UNWIND_GRACE, PERMISSION_DENIED_REASON, ProductConnection, ProductRuntimeHost,
    runtime_failure_to_call_error,
};

impl ProductConnection {
    /// Stop a broadcast whose call was withdrawn, bounded so a stalled stop
    /// cannot hold back the withdrawn call's answer.
    async fn stop_withdrawn_broadcast(&self, genesis_hash: Vec<u8>, operation_id: String) {
        let stop = self
            .services
            .chain
            .remote_chain_transaction_stop(truapi::latest::RemoteChainTransactionStopRequest {
                genesis_hash,
                operation_id: operation_id.clone(),
            })
            .fuse();
        let deadline = futures_timer::Delay::new(AUTHORITY_CANCEL_UNWIND_GRACE).fuse();
        pin_mut!(stop, deadline);
        let reason = futures::select! {
            result = stop => match result {
                Ok(()) => return,
                Err(err) => err.reason(),
            },
            () = deadline => "timed out".to_string(),
        };
        tracing::warn!(%operation_id, %reason, "could not stop a withdrawn broadcast");
    }
}

#[truapi::async_trait]
impl Chain for ProductRuntimeHost {
    #[instrument(skip_all, fields(runtime.method = "chain.follow_head_subscribe"))]
    async fn follow_head_subscribe(
        &self,
        cx: &CallContext,
        request: RemoteChainHeadFollowRequest,
    ) -> Subscription<RemoteChainHeadFollowItem, CallError<RemoteChainHeadFollowError>> {
        let RemoteChainHeadFollowRequest::V1(inner) = request;
        let follow_subscription_id = self.connection.follow_id(cx.request_id());
        let stream = self.connection
            .services
            .chain
            .remote_chain_head_follow(follow_subscription_id, inner)
            .map(|item| {
                item.map(RemoteChainHeadFollowItem::V1)
                    .map_err(runtime_failure_to_call_error)
            });
        Subscription::new(stream)
    }

    #[instrument(skip_all, fields(runtime.method = "chain.get_head_header"))]
    async fn get_head_header(
        &self,
        _cx: &CallContext,
        request: RemoteChainHeadHeaderRequest,
    ) -> Result<RemoteChainHeadHeaderResponse, CallError<RemoteChainHeadHeaderError>> {
        let RemoteChainHeadHeaderRequest::V1(mut inner) = request;
        inner.follow_subscription_id = self.connection.follow_id(&inner.follow_subscription_id);
        self.connection.services
            .chain
            .remote_chain_head_header(inner)
            .await
            .map(RemoteChainHeadHeaderResponse::V1)
            .map_err(runtime_failure_to_call_error)
    }

    #[instrument(skip_all, fields(runtime.method = "chain.get_head_body"))]
    async fn get_head_body(
        &self,
        _cx: &CallContext,
        request: RemoteChainHeadBodyRequest,
    ) -> Result<RemoteChainHeadBodyResponse, CallError<RemoteChainHeadBodyError>> {
        let RemoteChainHeadBodyRequest::V1(mut inner) = request;
        inner.follow_subscription_id = self.connection.follow_id(&inner.follow_subscription_id);
        self.connection.services
            .chain
            .remote_chain_head_body(inner)
            .await
            .map(RemoteChainHeadBodyResponse::V1)
            .map_err(runtime_failure_to_call_error)
    }

    #[instrument(skip_all, fields(runtime.method = "chain.get_head_storage"))]
    async fn get_head_storage(
        &self,
        _cx: &CallContext,
        request: RemoteChainHeadStorageRequest,
    ) -> Result<RemoteChainHeadStorageResponse, CallError<RemoteChainHeadStorageError>> {
        let RemoteChainHeadStorageRequest::V1(mut inner) = request;
        inner.follow_subscription_id = self.connection.follow_id(&inner.follow_subscription_id);
        self.connection.services
            .chain
            .remote_chain_head_storage(inner)
            .await
            .map(RemoteChainHeadStorageResponse::V1)
            .map_err(runtime_failure_to_call_error)
    }

    #[instrument(skip_all, fields(runtime.method = "chain.call_head"))]
    async fn call_head(
        &self,
        _cx: &CallContext,
        request: RemoteChainHeadCallRequest,
    ) -> Result<RemoteChainHeadCallResponse, CallError<RemoteChainHeadCallError>> {
        let RemoteChainHeadCallRequest::V1(mut inner) = request;
        inner.follow_subscription_id = self.connection.follow_id(&inner.follow_subscription_id);
        self.connection.services
            .chain
            .remote_chain_head_call(inner)
            .await
            .map(RemoteChainHeadCallResponse::V1)
            .map_err(runtime_failure_to_call_error)
    }

    #[instrument(skip_all, fields(runtime.method = "chain.unpin_head"))]
    async fn unpin_head(
        &self,
        _cx: &CallContext,
        request: RemoteChainHeadUnpinRequest,
    ) -> Result<RemoteChainHeadUnpinResponse, CallError<RemoteChainHeadUnpinError>> {
        let RemoteChainHeadUnpinRequest::V1(mut inner) = request;
        inner.follow_subscription_id = self.connection.follow_id(&inner.follow_subscription_id);
        self.connection.services
            .chain
            .remote_chain_head_unpin(inner)
            .await
            .map(|()| RemoteChainHeadUnpinResponse::V1)
            .map_err(runtime_failure_to_call_error)
    }

    #[instrument(skip_all, fields(runtime.method = "chain.continue_head"))]
    async fn continue_head(
        &self,
        _cx: &CallContext,
        request: RemoteChainHeadContinueRequest,
    ) -> Result<RemoteChainHeadContinueResponse, CallError<RemoteChainHeadContinueError>> {
        let RemoteChainHeadContinueRequest::V1(mut inner) = request;
        inner.follow_subscription_id = self.connection.follow_id(&inner.follow_subscription_id);
        self.connection.services
            .chain
            .remote_chain_head_continue(inner)
            .await
            .map(|()| RemoteChainHeadContinueResponse::V1)
            .map_err(runtime_failure_to_call_error)
    }

    #[instrument(skip_all, fields(runtime.method = "chain.stop_head_operation"))]
    async fn stop_head_operation(
        &self,
        _cx: &CallContext,
        request: RemoteChainHeadStopOperationRequest,
    ) -> Result<RemoteChainHeadStopOperationResponse, CallError<RemoteChainHeadStopOperationError>>
    {
        let RemoteChainHeadStopOperationRequest::V1(mut inner) = request;
        inner.follow_subscription_id = self.connection.follow_id(&inner.follow_subscription_id);
        self.connection.services
            .chain
            .remote_chain_head_stop_operation(inner)
            .await
            .map(|()| RemoteChainHeadStopOperationResponse::V1)
            .map_err(runtime_failure_to_call_error)
    }

    #[instrument(skip_all, fields(runtime.method = "chain.get_spec_genesis_hash"))]
    async fn get_spec_genesis_hash(
        &self,
        _cx: &CallContext,
        request: RemoteChainSpecGenesisHashRequest,
    ) -> Result<RemoteChainSpecGenesisHashResponse, CallError<RemoteChainSpecGenesisHashError>>
    {
        let RemoteChainSpecGenesisHashRequest::V1(inner) = request;
        self.connection.services
            .chain
            .remote_chain_spec_genesis_hash(inner.genesis_hash)
            .await
            .map(RemoteChainSpecGenesisHashResponse::V1)
            .map_err(runtime_failure_to_call_error)
    }

    #[instrument(skip_all, fields(runtime.method = "chain.get_spec_chain_name"))]
    async fn get_spec_chain_name(
        &self,
        _cx: &CallContext,
        request: RemoteChainSpecChainNameRequest,
    ) -> Result<RemoteChainSpecChainNameResponse, CallError<RemoteChainSpecChainNameError>> {
        let RemoteChainSpecChainNameRequest::V1(inner) = request;
        self.connection.services
            .chain
            .remote_chain_spec_chain_name(inner.genesis_hash)
            .await
            .map(RemoteChainSpecChainNameResponse::V1)
            .map_err(runtime_failure_to_call_error)
    }

    #[instrument(skip_all, fields(runtime.method = "chain.get_spec_properties"))]
    async fn get_spec_properties(
        &self,
        _cx: &CallContext,
        request: RemoteChainSpecPropertiesRequest,
    ) -> Result<RemoteChainSpecPropertiesResponse, CallError<RemoteChainSpecPropertiesError>> {
        let RemoteChainSpecPropertiesRequest::V1(inner) = request;
        self.connection.services
            .chain
            .remote_chain_spec_properties(inner.genesis_hash)
            .await
            .map(RemoteChainSpecPropertiesResponse::V1)
            .map_err(runtime_failure_to_call_error)
    }

    #[instrument(skip_all, fields(runtime.method = "chain.broadcast_transaction"))]
    async fn broadcast_transaction(
        &self,
        cx: &CallContext,
        request: RemoteChainTransactionBroadcastRequest,
    ) -> Result<
        RemoteChainTransactionBroadcastResponse,
        CallError<RemoteChainTransactionBroadcastError>,
    > {
        let RemoteChainTransactionBroadcastRequest::V1(inner) = request;
        let denied = RemoteChainTransactionBroadcastError::V1(v01::GenericError {
            reason: PERMISSION_DENIED_REASON.to_string(),
        });
        self.connection.require_chain_submit(denied).await?;
        if let Some(reason) = cx.cancel().reason() {
            return Err(CallError::Domain(RemoteChainTransactionBroadcastError::V1(
                v01::GenericError {
                    reason: format!("broadcast {reason}"),
                },
            )));
        }
        let genesis_hash = inner.genesis_hash.clone();
        let response = self.connection
            .services
            .chain
            .remote_chain_transaction_broadcast(inner)
            .await
            .map_err(runtime_failure_to_call_error)?;
        // A withdrawn call answers `Cancelled`, so the product never learns
        // the id it would stop this broadcast with. Any other cancellation
        // still answers with the id, and stopping would strand the product.
        if cx.cancel().reason() == Some(CancellationReason::Cancelled)
            && let Some(operation_id) = response.operation_id.clone()
        {
            self.connection
                .stop_withdrawn_broadcast(genesis_hash, operation_id)
                .await;
        }
        Ok(RemoteChainTransactionBroadcastResponse::V1(response))
    }

    #[instrument(skip_all, fields(runtime.method = "chain.stop_transaction"))]
    async fn stop_transaction(
        &self,
        _cx: &CallContext,
        request: RemoteChainTransactionStopRequest,
    ) -> Result<RemoteChainTransactionStopResponse, CallError<RemoteChainTransactionStopError>>
    {
        let RemoteChainTransactionStopRequest::V1(inner) = request;
        // ChainRuntime resolves this product-visible host handle to the
        // provider's short-lived operation id and makes stopping an operation
        // that completed between broadcast and stop idempotent.
        self.connection.services
            .chain
            .remote_chain_transaction_stop(inner)
            .await
            .map(|()| RemoteChainTransactionStopResponse::V1)
            .map_err(runtime_failure_to_call_error)
    }

    #[instrument(skip_all, fields(runtime.method = "chain.get_chain_info"))]
    async fn get_chain_info(
        &self,
        _cx: &CallContext,
        request: RemoteChainInfoRequest,
    ) -> Result<RemoteChainInfoResponse, CallError<RemoteChainInfoError>> {
        let RemoteChainInfoRequest::V1(inner) = request;
        let set = supported_chains(self.connection.services.platform.as_ref())
            .await
            .map_err(|err| {
                CallError::Domain(RemoteChainInfoError::V1(
                    truapi::latest::RemoteChainInfoError::Unknown(err),
                ))
            })?;
        chain_info(&set, &inner)
            .map(RemoteChainInfoResponse::V1)
            .map_err(|err| CallError::Domain(RemoteChainInfoError::V1(err)))
    }
}
