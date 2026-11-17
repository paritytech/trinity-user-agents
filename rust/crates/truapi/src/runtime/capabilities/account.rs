//! Product-facing account operations and login lifecycle.

use futures::StreamExt;
use tracing::instrument;
use truapi::{
	CallContext, CallError, Subscription,
	api::Account,
	latest,
	versioned::account::{
		HostAccountConnectionStatusSubscribeError, HostAccountConnectionStatusSubscribeItem,
		HostAccountConnectionStatusSubscribeRequest, HostAccountCreateProofError,
		HostAccountCreateProofRequest, HostAccountCreateProofResponse, HostAccountGetAliasError,
		HostAccountGetAliasRequest, HostAccountGetAliasResponse, HostAccountGetError,
		HostAccountGetRequest, HostAccountGetResponse, HostAccountListRingVrfKeysError,
		HostAccountListRingVrfKeysRequest, HostAccountListRingVrfKeysResponse,
		HostAccountRegisterRingVrfKeyError, HostAccountRegisterRingVrfKeyRequest,
		HostAccountRegisterRingVrfKeyResponse, HostAccountRingVrfSignError,
		HostAccountRingVrfSignRequest, HostAccountRingVrfSignResponse, HostAccountSignVrfError,
		HostAccountSignVrfRequest, HostAccountSignVrfResponse, HostGetLegacyAccountsError,
		HostGetLegacyAccountsRequest, HostGetLegacyAccountsResponse, HostGetUserIdError,
		HostGetUserIdRequest, HostGetUserIdResponse, HostRequestLoginError,
		HostRequestLoginRequest, HostRequestLoginResponse,
	},
};

use truapi::versioned::IntoLatest;

use crate::runtime::{
	AccountHolder, ProductRuntimeHost, account_get_authority_error, ring_vrf_alias_error,
	ring_vrf_list_error, ring_vrf_proof_error, ring_vrf_register_error, ring_vrf_sign_error,
	validate_vrf_transcript, versioning::version, vrf_call_error,
};

#[async_trait::async_trait]
impl<H: AccountHolder> Account for ProductRuntimeHost<H> {
	#[instrument(skip_all, fields(runtime.method = "account.get_account"))]
	async fn get_account(
		&self,
		cx: &CallContext,
		request: HostAccountGetRequest,
	) -> Result<HostAccountGetResponse, CallError<HostAccountGetError>> {
		version(request)
			.exec(|latest::HostAccountGetRequest { product_account_id }| async move {
				let public_key = self
					.accounts
					.get_account(cx, product_account_id)
					.await
					.map_err(account_get_authority_error)?;

				Ok(latest::HostAccountGetResponse {
					account: latest::ProductAccount { public_key: public_key.to_vec() },
				})
			})
			.await
	}

	#[instrument(skip_all, fields(runtime.method = "account.get_account_alias"))]
	async fn get_account_alias(
		&self,
		cx: &CallContext,
		request: HostAccountGetAliasRequest,
	) -> Result<HostAccountGetAliasResponse, CallError<HostAccountGetAliasError>> {
		version(request)
			.exec(|request| async move {
				let session = self
					.accounts
					.current_session()
					.ok_or(CallError::Domain(latest::HostAccountGetAliasError::Rejected))?;

				self.accounts
					.account_alias(&session, cx, request)
					.await
					.map_err(|err| CallError::Domain(ring_vrf_alias_error(err)))
			})
			.await
	}

	#[instrument(skip_all, fields(runtime.method = "account.create_account_proof"))]
	async fn create_account_proof(
		&self,
		cx: &CallContext,
		request: HostAccountCreateProofRequest,
	) -> Result<HostAccountCreateProofResponse, CallError<HostAccountCreateProofError>> {
		version(request)
			.exec(|request| async move {
				let session = self
					.accounts
					.current_session()
					.ok_or(CallError::Domain(latest::HostAccountCreateProofError::Rejected))?;

				self.accounts
					.create_proof(&session, cx, request)
					.await
					.map_err(|err| CallError::Domain(ring_vrf_proof_error(err)))
			})
			.await
	}

	#[instrument(skip_all, fields(runtime.method = "account.register_ring_vrf_key"))]
	async fn register_ring_vrf_key(
		&self,
		cx: &CallContext,
		request: HostAccountRegisterRingVrfKeyRequest,
	) -> Result<HostAccountRegisterRingVrfKeyResponse, CallError<HostAccountRegisterRingVrfKeyError>>
	{
		version(request)
			.exec(|request| async move {
				let session = self.accounts.current_session().ok_or(CallError::Domain(
					latest::HostAccountRegisterRingVrfKeyError::NotConnected,
				))?;
				self.accounts
					.register_ring_vrf_key(&session, cx, request)
					.await
					.map_err(|err| CallError::Domain(ring_vrf_register_error(err)))
			})
			.await
	}

	#[instrument(skip_all, fields(runtime.method = "account.list_ring_vrf_keys"))]
	async fn list_ring_vrf_keys(
		&self,
		cx: &CallContext,
		request: HostAccountListRingVrfKeysRequest,
	) -> Result<HostAccountListRingVrfKeysResponse, CallError<HostAccountListRingVrfKeysError>> {
		version(request)
			.exec(|request| async move {
				let session = self.accounts.current_session().ok_or(CallError::Domain(
					latest::HostAccountListRingVrfKeysError::NotConnected,
				))?;
				self.accounts
					.list_ring_vrf_keys(&session, cx, request)
					.await
					.map_err(|err| CallError::Domain(ring_vrf_list_error(err)))
			})
			.await
	}

	#[instrument(skip_all, fields(runtime.method = "account.ring_vrf_sign"))]
	async fn ring_vrf_sign(
		&self,
		cx: &CallContext,
		request: HostAccountRingVrfSignRequest,
	) -> Result<HostAccountRingVrfSignResponse, CallError<HostAccountRingVrfSignError>> {
		version(request)
			.exec(|request| async move {
				let session = self
					.accounts
					.current_session()
					.ok_or(CallError::Domain(latest::HostAccountRingVrfSignError::NotConnected))?;
				self.accounts
					.ring_vrf_sign(&session, cx, request)
					.await
					.map_err(|err| CallError::Domain(ring_vrf_sign_error(err)))
			})
			.await
	}

	#[instrument(skip_all, fields(runtime.method = "account.sign_vrf"))]
	async fn sign_vrf(
		&self,
		cx: &CallContext,
		request: HostAccountSignVrfRequest,
	) -> Result<HostAccountSignVrfResponse, CallError<HostAccountSignVrfError>> {
		version(request)
			.exec(|request| async move {
				validate_vrf_transcript(&request).map_err(|reason| {
					CallError::Domain(latest::HostAccountSignVrfError::Unknown { reason })
				})?;
				let session = self
					.accounts
					.current_session()
					.ok_or(CallError::Domain(latest::HostAccountSignVrfError::NotConnected))?;
				if !self.may_use_product_account(&request.account.dot_ns_identifier, cx).await {
					return Err(CallError::Domain(latest::HostAccountSignVrfError::Rejected));
				}
				self.accounts.sign_vrf(&session, cx, request).await.map_err(vrf_call_error)
			})
			.await
	}

	#[instrument(skip_all, fields(runtime.method = "account.get_legacy_accounts"))]
	async fn get_legacy_accounts(
		&self,
		_cx: &CallContext,
		request: HostGetLegacyAccountsRequest,
	) -> Result<HostGetLegacyAccountsResponse, CallError<HostGetLegacyAccountsError>> {
		version(request)
			.exec(|()| async move {
				// Match the mobile hosts: compatibility signing accounts may be
				// addressed explicitly, but are not enumerated.
				Ok(latest::HostGetLegacyAccountsResponse { accounts: vec![] })
			})
			.await
	}

	#[instrument(skip_all, fields(runtime.method = "account.get_user_id"))]
	async fn get_user_id(
		&self,
		_cx: &CallContext,
		request: HostGetUserIdRequest,
	) -> Result<HostGetUserIdResponse, CallError<HostGetUserIdError>> {
		version(request)
			.exec(|()| async move {
				let session = self
					.accounts
					.current_session()
					.ok_or(CallError::Domain(latest::HostGetUserIdError::NotConnected))?;

				let denied = latest::HostGetUserIdError::PermissionDenied;
				self.connection.require_identity_disclosure(denied).await?;

				let primary_username = match session.primary_username() {
					Some(name) => Some(name.to_string()),
					None => self.host_session.primary_username().await,
				}
				.ok_or_else(|| {
					CallError::Domain(latest::HostGetUserIdError::Unknown {
						reason: "No primary username for this session".to_string(),
					})
				})?;

				Ok(latest::HostGetUserIdResponse { primary_username })
			})
			.await
	}

	#[instrument(skip_all, fields(runtime.method = "account.connection_status_subscribe"))]
	async fn connection_status_subscribe(
		&self,
		_cx: &CallContext,
		request: HostAccountConnectionStatusSubscribeRequest,
	) -> Subscription<
		HostAccountConnectionStatusSubscribeItem,
		CallError<HostAccountConnectionStatusSubscribeError>,
	> {
		version(request)
			.exec(|()| async move {
				Subscription::new(
					self.connection.session_state.subscribe().map(|item| Ok(item.into_latest())),
				)
			})
			.await
	}

	#[instrument(skip_all, fields(runtime.method = "account.request_login", product = %self.connection.product.product_id))]
	async fn request_login(
		&self,
		_cx: &CallContext,
		request: HostRequestLoginRequest,
	) -> Result<HostRequestLoginResponse, CallError<HostRequestLoginError>> {
		version(request)
			.exec(|_request| async move {
				self.host_session.request_login(&self.connection.product).await
			})
			.await
	}
}
