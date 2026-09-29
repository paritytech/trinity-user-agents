//! Product-facing resources capability adapters.

use tracing::instrument;
use truapi::api::{Entropy, ResourceAllocation};
use truapi::versioned::entropy::{
    HostDeriveEntropyError, HostDeriveEntropyRequest, HostDeriveEntropyResponse,
};
use truapi::versioned::resource_allocation::{
    HostRequestResourceAllocationError, HostRequestResourceAllocationRequest,
    HostRequestResourceAllocationResponse,
};
use truapi::{CallContext, CallError, v01};
use truapi_platform::{ResourceAllocationReview, UserConfirmationReview};

use crate::runtime::{
    ProductRuntimeHost, RESOURCE_ALLOCATION_REMOTE_AUTHORITY_RESPONSE_TIMEOUT,
    remote_authority_call, remote_authority_context_with_default,
};

#[truapi::async_trait]
impl ResourceAllocation for ProductRuntimeHost {
    #[instrument(skip_all, fields(runtime.method = "resource_allocation.request"))]
    async fn request(
        &self,
        cx: &CallContext,
        request: HostRequestResourceAllocationRequest,
    ) -> Result<HostRequestResourceAllocationResponse, CallError<HostRequestResourceAllocationError>>
    {
        let HostRequestResourceAllocationRequest::V1(inner) = request;
        let Some(session) = self.authority.current_session() else {
            return Err(CallError::Domain(HostRequestResourceAllocationError::V1(
                v01::ResourceAllocationError::Unknown {
                    reason: "No active session".to_string(),
                },
            )));
        };

        let confirmed = self
            .platform
            .confirm_user_action(UserConfirmationReview::ResourceAllocation(
                ResourceAllocationReview {
                    calling_product_id: self.product_id(),
                    resources: inner.resources.clone(),
                },
            ))
            .await
            .map_err(|err| CallError::HostFailure {
                reason: format!("resource allocation confirmation failed: {err:?}"),
            })?;
        if !confirmed {
            return Err(CallError::Domain(HostRequestResourceAllocationError::V1(
                v01::ResourceAllocationError::Unknown {
                    reason: "User rejected resource allocation".to_string(),
                },
            )));
        }
        let cx = remote_authority_context_with_default(
            cx,
            RESOURCE_ALLOCATION_REMOTE_AUTHORITY_RESPONSE_TIMEOUT,
        );
        remote_authority_call(
            &cx,
            self.authority
                .allocate_resources(&cx, &session, self.product_id(), inner),
        )
        .await
        .map(HostRequestResourceAllocationResponse::V1)
        .map_err(|err| {
            CallError::Domain(HostRequestResourceAllocationError::V1(
                v01::ResourceAllocationError::Unknown {
                    reason: err.to_string(),
                },
            ))
        })
    }
}

#[truapi::async_trait]
impl Entropy for ProductRuntimeHost {
    #[instrument(skip_all, fields(runtime.method = "entropy.derive"))]
    async fn derive(
        &self,
        _cx: &CallContext,
        request: HostDeriveEntropyRequest,
    ) -> Result<HostDeriveEntropyResponse, CallError<HostDeriveEntropyError>> {
        let HostDeriveEntropyRequest::V1(v01::HostDeriveEntropyRequest { context }) = request;
        let Some(session) = self.authority.current_session() else {
            return Err(CallError::Domain(HostDeriveEntropyError::V1(
                v01::HostDeriveEntropyError::Unknown {
                    reason: "Not connected".to_string(),
                },
            )));
        };
        let entropy = self
            .authority
            .derive_entropy(&session, &self.product_id(), &context)
            .map_err(|err| {
                CallError::Domain(HostDeriveEntropyError::V1(
                    v01::HostDeriveEntropyError::Unknown {
                        reason: err.to_string(),
                    },
                ))
            })?;

        Ok(HostDeriveEntropyResponse::V1(
            v01::HostDeriveEntropyResponse { entropy },
        ))
    }
}
