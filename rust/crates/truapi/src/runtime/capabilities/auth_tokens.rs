//! Product-facing AuthTokens capability adapter.

use std::sync::Arc;

use tracing::instrument;
use truapi::{
	CallContext, CallError,
	api::AuthTokens,
	latest,
	versioned::{
		IntoLatest,
		auth_tokens::{
			HostAuthTokensGetError, HostAuthTokensGetRequest, HostAuthTokensGetResponse,
			HostAuthTokensRemoveError, HostAuthTokensRemoveRequest, HostAuthTokensRemoveResponse,
			HostAuthTokensStoreError, HostAuthTokensStoreRequest, HostAuthTokensStoreResponse,
		},
	},
};

use crate::{
	host_logic::auth_tokens::{check_store, is_valid_service},
	platform::AuthTokenPlatform,
	runtime::ProductRuntimeHost,
};

impl ProductRuntimeHost {
	/// The host's token keeper, for a product with a session. Tokens belong to
	/// the signed-in user, who loses them on sign-out, so without a session
	/// there is nothing to keep them for.
	fn auth_token_platform<E>(&self) -> Result<Arc<dyn AuthTokenPlatform>, CallError<E>> {
		let platform = self.services.auth_token_platform().ok_or(CallError::Unsupported)?;
		if self.authority.current_session().is_none() {
			return Err(CallError::Denied);
		}
		Ok(platform)
	}
}

/// The host's reason for a failure, as the product's catch-all for a get or
/// remove.
fn lookup_failure(error: latest::GenericError) -> latest::HostAuthTokensLookupError {
	latest::HostAuthTokensLookupError::Unknown { reason: error.reason }
}

#[truapi::async_trait]
impl AuthTokens for ProductRuntimeHost {
	#[instrument(skip_all, fields(runtime.method = "auth_tokens.store"))]
	async fn store(
		&self,
		_cx: &CallContext,
		request: HostAuthTokensStoreRequest,
	) -> Result<HostAuthTokensStoreResponse, CallError<HostAuthTokensStoreError>> {
		let platform = self.auth_token_platform()?;
		let latest::HostAuthTokensStoreRequest { service, token } = request.into_latest();
		let domain = |error| CallError::Domain(HostAuthTokensStoreError::V1(error));
		check_store(&service, &token).map_err(domain)?;
		platform
			.store_auth_token(&self.product, service, token)
			.await
			.map(|()| HostAuthTokensStoreResponse::V1)
			.map_err(|error| {
				domain(latest::HostAuthTokensStoreError::Unknown { reason: error.reason })
			})
	}

	#[instrument(skip_all, fields(runtime.method = "auth_tokens.get"))]
	async fn get(
		&self,
		_cx: &CallContext,
		request: HostAuthTokensGetRequest,
	) -> Result<HostAuthTokensGetResponse, CallError<HostAuthTokensGetError>> {
		let platform = self.auth_token_platform()?;
		let latest::HostAuthTokensGetRequest { service } = request.into_latest();
		let domain = |error| CallError::Domain(HostAuthTokensGetError::V1(error));
		if !is_valid_service(&service) {
			return Err(domain(latest::HostAuthTokensLookupError::InvalidService));
		}
		platform
			.auth_token(&self.product, service)
			.await
			.map(|token| HostAuthTokensGetResponse::V1(latest::HostAuthTokensGetResponse { token }))
			.map_err(|error| domain(lookup_failure(error)))
	}

	#[instrument(skip_all, fields(runtime.method = "auth_tokens.remove"))]
	async fn remove(
		&self,
		_cx: &CallContext,
		request: HostAuthTokensRemoveRequest,
	) -> Result<HostAuthTokensRemoveResponse, CallError<HostAuthTokensRemoveError>> {
		let platform = self.auth_token_platform()?;
		let latest::HostAuthTokensRemoveRequest { service } = request.into_latest();
		let domain = |error| CallError::Domain(HostAuthTokensRemoveError::V1(error));
		if !is_valid_service(&service) {
			return Err(domain(latest::HostAuthTokensLookupError::InvalidService));
		}
		platform
			.remove_auth_token(&self.product, service)
			.await
			.map(|()| HostAuthTokensRemoveResponse::V1)
			.map_err(|error| domain(lookup_failure(error)))
	}
}
