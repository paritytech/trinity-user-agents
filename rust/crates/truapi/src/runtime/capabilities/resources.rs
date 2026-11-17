//! Product-facing resources capability adapters.

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
	AccountHolder, ProductRuntimeHost, authority::AuthorityError, versioning::version,
};

#[async_trait::async_trait]
impl<H: AccountHolder> ResourceAllocation for ProductRuntimeHost<H> {
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
				self.accounts.allocate_resources(cx, &session, inner).await.map_err(|error| {
					match error {
						AuthorityError::ConfirmationFailed(error) => CallError::HostFailure {
							reason: format!("resource allocation confirmation failed: {error:?}"),
						},
						error => CallError::Domain(latest::ResourceAllocationError::Unknown {
							reason: error.to_string(),
						}),
					}
				})
			})
			.await
	}
}

#[async_trait::async_trait]
impl<H: AccountHolder> Entropy for ProductRuntimeHost<H> {
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
