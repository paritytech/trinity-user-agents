//! Product-facing AuthTokens capability adapter.

use std::sync::Arc;

use tracing::instrument;
use truapi::{
	CallContext, CallError,
	api::AuthTokens,
	latest,
	versioned::auth_tokens::{
		HostAuthTokensGetError, HostAuthTokensGetRequest, HostAuthTokensGetResponse,
		HostAuthTokensRemoveError, HostAuthTokensRemoveRequest, HostAuthTokensRemoveResponse,
		HostAuthTokensStoreError, HostAuthTokensStoreRequest, HostAuthTokensStoreResponse,
	},
};

use crate::{
	host_logic::auth_tokens::{
		MAX_SERVICES_PER_PRODUCT, account_tag, check_store, is_valid_service, storage_key,
	},
	platform::AuthTokenPlatform,
	runtime::{ProductRuntimeHost, versioning::version},
};

impl ProductRuntimeHost {
	/// The host's token keeper, where `service`'s token is kept, for a
	/// product with a session. Tokens belong to the signed-in account, so
	/// without a session there is nobody to keep them for.
	fn auth_token_slot<E>(&self, service: &str) -> Result<AuthTokenSlot, CallError<E>> {
		let platform =
			self.connection.services.auth_token_platform().ok_or(CallError::Unsupported)?;
		let session = self.accounts.current_session().ok_or(CallError::Denied)?;
		Ok(AuthTokenSlot {
			platform,
			owner: session.public_key,
			account: account_tag(&session.public_key),
			key: storage_key(
				&session.public_key,
				self.connection.product.product_id.as_str(),
				service,
			),
		})
	}

	/// Whether `owner` is still the signed-in account.
	fn is_signed_in(&self, owner: &[u8; 32]) -> bool {
		self.accounts
			.current_session()
			.is_some_and(|session| &session.public_key == owner)
	}
}

/// The host's token keeper with the signed-in account, its tag, and the key
/// one service's token is kept under.
struct AuthTokenSlot {
	platform: Arc<dyn AuthTokenPlatform>,
	owner: [u8; 32],
	account: String,
	key: String,
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
	/// product keeps fewer than [`MAX_SERVICES_PER_PRODUCT`] for the signed-in
	/// account. The count check and the write hold one lock, so concurrent
	/// stores cannot pass the cap together. Dropping tokens at sign-out takes
	/// the same lock, and the session is checked again under it, so a store
	/// racing a sign-out or an account switch cannot outlive it.
	#[instrument(skip_all, fields(runtime.method = "auth_tokens.store"))]
	async fn store(
		&self,
		_cx: &CallContext,
		request: HostAuthTokensStoreRequest,
	) -> Result<HostAuthTokensStoreResponse, CallError<HostAuthTokensStoreError>> {
		version(request)
			.exec(|latest::HostAuthTokensStoreRequest { service, token }| async move {
				let AuthTokenSlot { platform, owner, account, key } =
					self.auth_token_slot(&service)?;
				check_store(&service, &token).map_err(CallError::Domain)?;
				let product_id = self.connection.product.product_id.as_str();
				let _writes = self.connection.services.auth_token_writes().await;
				if !self.is_signed_in(&owner) {
					return Err(CallError::Denied);
				}
				let replacing =
					platform.auth_token(product_id, &key).await.map_err(store_failure)?;
				if replacing.is_none() &&
					platform
						.auth_token_count(product_id, &account)
						.await
						.map_err(store_failure)? >=
						MAX_SERVICES_PER_PRODUCT
				{
					return Err(CallError::Domain(
						latest::HostAuthTokensStoreError::TooManyServices {
							max: MAX_SERVICES_PER_PRODUCT,
						},
					));
				}
				platform
					.store_auth_token(product_id, &account, &key, token)
					.await
					.map_err(store_failure)
			})
			.await
	}

	#[instrument(skip_all, fields(runtime.method = "auth_tokens.get"))]
	async fn get(
		&self,
		_cx: &CallContext,
		request: HostAuthTokensGetRequest,
	) -> Result<HostAuthTokensGetResponse, CallError<HostAuthTokensGetError>> {
		version(request)
			.exec(|latest::HostAuthTokensGetRequest { service }| async move {
				let AuthTokenSlot { platform, key, .. } = self.auth_token_slot(&service)?;
				if !is_valid_service(&service) {
					return Err(CallError::Domain(
						latest::HostAuthTokensLookupError::InvalidService,
					));
				}
				platform
					.auth_token(self.connection.product.product_id.as_str(), &key)
					.await
					.map(|token| latest::HostAuthTokensGetResponse { token })
					.map_err(lookup_failure)
			})
			.await
	}

	#[instrument(skip_all, fields(runtime.method = "auth_tokens.remove"))]
	async fn remove(
		&self,
		_cx: &CallContext,
		request: HostAuthTokensRemoveRequest,
	) -> Result<HostAuthTokensRemoveResponse, CallError<HostAuthTokensRemoveError>> {
		version(request)
			.exec(|latest::HostAuthTokensRemoveRequest { service }| async move {
				let AuthTokenSlot { platform, key, .. } = self.auth_token_slot(&service)?;
				if !is_valid_service(&service) {
					return Err(CallError::Domain(
						latest::HostAuthTokensLookupError::InvalidService,
					));
				}
				platform
					.remove_auth_token(self.connection.product.product_id.as_str(), &key)
					.await
					.map_err(lookup_failure)
			})
			.await
	}
}
