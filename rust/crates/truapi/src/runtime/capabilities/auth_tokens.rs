//! Product-facing AuthTokens capability adapter.

use std::sync::Arc;

use tracing::instrument;
use truapi::{
	CallContext, CallError,
	api::AuthTokens,
	latest,
	versioned::{
		FromLatest, IntoLatest, Versioned,
		auth_tokens::{
			HostAuthTokensGetError, HostAuthTokensGetRequest, HostAuthTokensGetResponse,
			HostAuthTokensRemoveError, HostAuthTokensRemoveRequest, HostAuthTokensRemoveResponse,
			HostAuthTokensStoreError, HostAuthTokensStoreRequest, HostAuthTokensStoreResponse,
		},
	},
};

use super::from_latest_error;
use crate::{
	host_logic::auth_tokens::{
		MAX_SERVICES_PER_PRODUCT, check_store, is_valid_service, storage_key,
	},
	platform::AuthTokenPlatform,
	runtime::ProductRuntimeHost,
};

impl ProductRuntimeHost {
	/// The host's token keeper and the key `service` is kept under, for a
	/// product with a session. Tokens belong to the signed-in account, so
	/// without a session there is nobody to keep them for.
	fn auth_token_slot<E>(
		&self,
		service: &str,
	) -> Result<(Arc<dyn AuthTokenPlatform>, String), CallError<E>> {
		let platform = self.services.auth_token_platform().ok_or(CallError::Unsupported)?;
		let session = self.authority.current_session().ok_or(CallError::Denied)?;
		Ok((platform, storage_key(&session.public_key, &self.product.product_id, service)))
	}
}

/// The host's reason for a failure, as the product's catch-all for a get or
/// remove.
fn lookup_failure(error: latest::GenericError) -> CallError<latest::HostAuthTokensLookupError> {
	CallError::Domain(latest::HostAuthTokensLookupError::Unknown { reason: error.reason })
}

fn store_failure(error: latest::GenericError) -> CallError<latest::HostAuthTokensStoreError> {
	CallError::Domain(latest::HostAuthTokensStoreError::Unknown { reason: error.reason })
}

#[truapi::async_trait]
impl AuthTokens for ProductRuntimeHost {
	/// A service the product has no token for yet only gets one while the
	/// product keeps fewer than [`MAX_SERVICES_PER_PRODUCT`].
	#[instrument(skip_all, fields(runtime.method = "auth_tokens.store"))]
	async fn store(
		&self,
		_cx: &CallContext,
		request: HostAuthTokensStoreRequest,
	) -> Result<HostAuthTokensStoreResponse, CallError<HostAuthTokensStoreError>> {
		let version = request.version();
		let latest::HostAuthTokensStoreRequest { service, token } = request.into_latest();
		let result = async {
			let (platform, key) = self.auth_token_slot(&service)?;
			check_store(&service, &token).map_err(CallError::Domain)?;
			let product_id = self.product.product_id.as_str();
			let replacing = platform.auth_token(product_id, &key).await.map_err(store_failure)?;
			if replacing.is_none() &&
				platform.auth_token_count(product_id).await.map_err(store_failure)? >=
					MAX_SERVICES_PER_PRODUCT
			{
				return Err(CallError::Domain(latest::HostAuthTokensStoreError::TooManyServices {
					max: MAX_SERVICES_PER_PRODUCT,
				}));
			}
			platform.store_auth_token(product_id, &key, token).await.map_err(store_failure)
		}
		.await;
		result
			.map(|()| HostAuthTokensStoreResponse::from_latest((), version))
			.map_err(|error| from_latest_error(error, version))
	}

	#[instrument(skip_all, fields(runtime.method = "auth_tokens.get"))]
	async fn get(
		&self,
		_cx: &CallContext,
		request: HostAuthTokensGetRequest,
	) -> Result<HostAuthTokensGetResponse, CallError<HostAuthTokensGetError>> {
		let version = request.version();
		let latest::HostAuthTokensGetRequest { service } = request.into_latest();
		let result = async {
			let (platform, key) = self.auth_token_slot(&service)?;
			if !is_valid_service(&service) {
				return Err(CallError::Domain(latest::HostAuthTokensLookupError::InvalidService));
			}
			platform
				.auth_token(&self.product.product_id, &key)
				.await
				.map(|token| latest::HostAuthTokensGetResponse { token })
				.map_err(lookup_failure)
		}
		.await;
		result
			.map(|response| HostAuthTokensGetResponse::from_latest(response, version))
			.map_err(|error| from_latest_error(error, version))
	}

	#[instrument(skip_all, fields(runtime.method = "auth_tokens.remove"))]
	async fn remove(
		&self,
		_cx: &CallContext,
		request: HostAuthTokensRemoveRequest,
	) -> Result<HostAuthTokensRemoveResponse, CallError<HostAuthTokensRemoveError>> {
		let version = request.version();
		let latest::HostAuthTokensRemoveRequest { service } = request.into_latest();
		let result = async {
			let (platform, key) = self.auth_token_slot(&service)?;
			if !is_valid_service(&service) {
				return Err(CallError::Domain(latest::HostAuthTokensLookupError::InvalidService));
			}
			platform
				.remove_auth_token(&self.product.product_id, &key)
				.await
				.map_err(lookup_failure)
		}
		.await;
		result
			.map(|()| HostAuthTokensRemoveResponse::from_latest((), version))
			.map_err(|error| from_latest_error(error, version))
	}
}
