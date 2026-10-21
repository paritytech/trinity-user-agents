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

use super::from_latest_error;
use truapi::versioned::{FromLatest, IntoLatest, Versioned};

use crate::runtime::{AccountHolder, ProductRuntimeHost, authority::AuthorityError};

#[truapi::async_trait]
impl<H: AccountHolder> ResourceAllocation for ProductRuntimeHost<H> {
	#[instrument(skip_all, fields(runtime.method = "resource_allocation.request"))]
	async fn request(
		&self,
		cx: &CallContext,
		request: HostRequestResourceAllocationRequest,
	) -> Result<HostRequestResourceAllocationResponse, CallError<HostRequestResourceAllocationError>>
	{
		let version = request.version();
		let inner = request.into_latest();
		let result = async {
			let Some(authority_session) = self.accounts.current_session() else {
				return Err(CallError::Domain(latest::ResourceAllocationError::Unknown {
					reason: "No active session".to_string(),
				}));
			};

			self.accounts
				.allocate_resources(cx, &authority_session, inner)
				.await
				.map_err(|error| match error {
					AuthorityError::ConfirmationFailed(error) => CallError::HostFailure {
						reason: format!("resource allocation confirmation failed: {error:?}"),
					},
					error => CallError::Domain(latest::ResourceAllocationError::Unknown {
						reason: error.to_string(),
					}),
				})
		}
		.await;
		result
			.map(|response| HostRequestResourceAllocationResponse::from_latest(response, version))
			.map_err(|error| {
				from_latest_error::<HostRequestResourceAllocationError>(error, version)
			})
	}
}

#[truapi::async_trait]
impl<H: AccountHolder> Entropy for ProductRuntimeHost<H> {
	#[instrument(skip_all, fields(runtime.method = "entropy.derive"))]
	async fn derive(
		&self,
		_cx: &CallContext,
		request: HostDeriveEntropyRequest,
	) -> Result<HostDeriveEntropyResponse, CallError<HostDeriveEntropyError>> {
		let version = request.version();
		let latest::HostDeriveEntropyRequest { context } = request.into_latest();
		let result = async {
			let Some(authority_session) = self.accounts.current_session() else {
				return Err(CallError::Domain(latest::HostDeriveEntropyError::Unknown {
					reason: "Not connected".to_string(),
				}));
			};
			let entropy =
				self.accounts.derive_entropy(&authority_session, &context).map_err(|err| {
					CallError::Domain(latest::HostDeriveEntropyError::Unknown {
						reason: err.to_string(),
					})
				})?;

			Ok(latest::HostDeriveEntropyResponse { entropy })
		}
		.await;
		result
			.map(|response| HostDeriveEntropyResponse::from_latest(response, version))
			.map_err(|error| from_latest_error::<HostDeriveEntropyError>(error, version))
	}
}
