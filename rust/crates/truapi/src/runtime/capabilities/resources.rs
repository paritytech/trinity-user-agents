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

use crate::runtime::ProductRuntimeHost;
use crate::runtime::authority::AuthorityError;

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
        let Some(operation) = self.authority.current_operation() else {
            return Err(CallError::Domain(HostRequestResourceAllocationError::V1(
                v01::ResourceAllocationError::Unknown {
                    reason: "No active session".to_string(),
                },
            )));
        };

        self.authority
            .allocate_resources(cx, &operation, &self.product, inner)
            .await
            .map(HostRequestResourceAllocationResponse::V1)
            .map_err(|error| match error {
                AuthorityError::ConfirmationFailed(error) => CallError::HostFailure {
                    reason: format!("resource allocation confirmation failed: {error:?}"),
                },
                error => CallError::Domain(HostRequestResourceAllocationError::V1(
                    v01::ResourceAllocationError::Unknown {
                        reason: error.to_string(),
                    },
                )),
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
        let Some(operation) = self.authority.current_operation() else {
            return Err(CallError::Domain(HostDeriveEntropyError::V1(
                v01::HostDeriveEntropyError::Unknown {
                    reason: "Not connected".to_string(),
                },
            )));
        };
        let session = &operation.session;
        let entropy = self
            .authority
            .require_current_operation(&operation)
            .and_then(|()| {
                self.authority
                    .derive_entropy(session, &self.product_id(), &context)
            })
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
