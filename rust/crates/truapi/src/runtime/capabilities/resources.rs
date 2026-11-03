//! Product-facing resources capability adapters.

use crate::platform::{ResourceAllocationReview, UserConfirmationReview};
use tracing::instrument;
use truapi::{
	CallContext, CallError,
	api::{Entropy, ResourceAllocation},
	latest,
	versioned::{
		entropy::{HostDeriveEntropyError, HostDeriveEntropyRequest, HostDeriveEntropyResponse},
		resource_allocation::{
			HostRequestResourceAllocationError, HostRequestResourceAllocationRequest,
			HostRequestResourceAllocationResponse,
		},
	},
};

use crate::runtime::{
	ProductRuntimeHost, RESOURCE_ALLOCATION_REMOTE_AUTHORITY_RESPONSE_TIMEOUT,
	authority::AuthorityError, remote_authority_call, remote_authority_context_with_default,
	versioning::version,
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
		version(request)
			.exec(|inner| async move {
				let session = self.accounts.current_session().ok_or_else(|| {
					CallError::Domain(latest::ResourceAllocationError::Unknown {
						reason: "No active session".to_string(),
					})
				})?;

				let review = UserConfirmationReview::ResourceAllocation(ResourceAllocationReview {
					calling_product_id: self.connection.product_id().clone(),
					resources: inner.resources.clone(),
				});
				match self.confirm(cx, review).await {
					Ok(()) => {},
					Err(AuthorityError::Rejected) => {
						return Ok(latest::HostRequestResourceAllocationResponse {
							outcomes: vec![
								latest::AllocationOutcome::Rejected;
								inner.resources.len()
							],
						});
					},
					Err(AuthorityError::ConfirmationFailed(error)) => {
						return Err(CallError::HostFailure {
							reason: format!("resource allocation confirmation failed: {error:?}"),
						});
					},
					Err(error) => {
						return Err(CallError::Domain(latest::ResourceAllocationError::Unknown {
							reason: error.to_string(),
						}));
					},
				}
				let cx = remote_authority_context_with_default(
					cx,
					RESOURCE_ALLOCATION_REMOTE_AUTHORITY_RESPONSE_TIMEOUT,
				);
				remote_authority_call(&cx, self.accounts.allocate_resources(&cx, &session, inner))
					.await
					.map_err(|err| {
						CallError::Domain(latest::ResourceAllocationError::Unknown {
							reason: err.to_string(),
						})
					})
			})
			.await
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
		version(request)
			.exec(|latest::HostDeriveEntropyRequest { context }| async move {
				let session = self.accounts.current_session().ok_or_else(|| {
					CallError::Domain(latest::HostDeriveEntropyError::Unknown {
						reason: "Not connected".to_string(),
					})
				})?;
				let entropy = self.accounts.derive_entropy(&session, &context).map_err(|err| {
					CallError::Domain(latest::HostDeriveEntropyError::Unknown {
						reason: err.to_string(),
					})
				})?;

				Ok(latest::HostDeriveEntropyResponse { entropy })
			})
			.await
	}
}
