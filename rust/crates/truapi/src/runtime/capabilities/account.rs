//! Product-facing account capability adapters.
//!
//! Account operations use shared host accounts; login uses the session lifecycle.

use crate::platform::{PermissionAuthorizationStatus, normalize_product_identifier};
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

use super::from_latest_error;
use truapi::versioned::{FromLatest, IntoLatest, Versioned};

use crate::runtime::{
	AccountHolder, ProductRuntimeHost, remote_authority_call, remote_authority_context,
	ring_vrf_alias_error, ring_vrf_list_error, ring_vrf_proof_error, ring_vrf_register_error,
	ring_vrf_sign_error, validate_vrf_transcript, vrf_call_error,
};

#[truapi::async_trait]
impl<H: AccountHolder> Account for ProductRuntimeHost<H> {
	#[instrument(skip_all, fields(runtime.method = "account.get_account"))]
	async fn get_account(
		&self,
		cx: &CallContext,
		request: HostAccountGetRequest,
	) -> Result<HostAccountGetResponse, CallError<HostAccountGetError>> {
		let version = request.version();
		let latest::HostAccountGetRequest { product_account_id } = request.into_latest();
		let result = async {
			let public_key = self.accounts.get_account(cx, product_account_id).await?;

			Ok(latest::HostAccountGetResponse {
				account: latest::ProductAccount { public_key: public_key.to_vec() },
			})
		}
		.await;
		result
			.map(|response| HostAccountGetResponse::from_latest(response, version))
			.map_err(|error| from_latest_error::<HostAccountGetError>(error, version))
	}

	#[instrument(skip_all, fields(runtime.method = "account.get_account_alias"))]
	async fn get_account_alias(
		&self,
		cx: &CallContext,
		request: HostAccountGetAliasRequest,
	) -> Result<HostAccountGetAliasResponse, CallError<HostAccountGetAliasError>> {
		let version = request.version();
		let mut request = request.into_latest();
		let result = async {
			request.key_handle =
				Self::normalize_product_account_id(request.key_handle).map_err(|()| {
					CallError::Domain(latest::HostAccountGetAliasError::Unknown {
						reason: "Invalid key handle".to_string(),
					})
				})?;
			let Some(authority_session) = self.accounts.current_session() else {
				return Err(CallError::Domain(latest::HostAccountGetAliasError::Rejected));
			};

			let cx = remote_authority_context(cx);
			remote_authority_call(
				&cx,
				self.accounts.account_alias(&authority_session, &cx, request),
			)
			.await
			.map_err(|err| CallError::Domain(ring_vrf_alias_error(err)))
		}
		.await;
		result
			.map(|response| HostAccountGetAliasResponse::from_latest(response, version))
			.map_err(|error| from_latest_error::<HostAccountGetAliasError>(error, version))
	}

	#[instrument(skip_all, fields(runtime.method = "account.create_account_proof"))]
	async fn create_account_proof(
		&self,
		cx: &CallContext,
		request: HostAccountCreateProofRequest,
	) -> Result<HostAccountCreateProofResponse, CallError<HostAccountCreateProofError>> {
		let version = request.version();
		let mut request = request.into_latest();
		let result = async {
			request.key_handle =
				Self::normalize_product_account_id(request.key_handle).map_err(|()| {
					CallError::Domain(latest::HostAccountCreateProofError::Unknown {
						reason: "Invalid key handle".to_string(),
					})
				})?;
			let Some(authority_session) = self.accounts.current_session() else {
				return Err(CallError::Domain(latest::HostAccountCreateProofError::Rejected));
			};

			let cx = remote_authority_context(cx);
			self.accounts
				.create_proof(&authority_session, &cx, request)
				.await
				.map_err(|err| CallError::Domain(ring_vrf_proof_error(err)))
		}
		.await;
		result
			.map(|response| HostAccountCreateProofResponse::from_latest(response, version))
			.map_err(|error| from_latest_error::<HostAccountCreateProofError>(error, version))
	}

	#[instrument(skip_all, fields(runtime.method = "account.register_ring_vrf_key"))]
	async fn register_ring_vrf_key(
		&self,
		cx: &CallContext,
		request: HostAccountRegisterRingVrfKeyRequest,
	) -> Result<HostAccountRegisterRingVrfKeyResponse, CallError<HostAccountRegisterRingVrfKeyError>>
	{
		let version = request.version();
		let request = request.into_latest();
		let result = async {
			let Some(authority_session) = self.accounts.current_session() else {
				return Err(CallError::Domain(
					latest::HostAccountRegisterRingVrfKeyError::NotConnected,
				));
			};
			let cx = remote_authority_context(cx);
			remote_authority_call(
				&cx,
				self.accounts.register_ring_vrf_key(&authority_session, &cx, request),
			)
			.await
			.map_err(|err| CallError::Domain(ring_vrf_register_error(err)))
		}
		.await;
		result
			.map(|response| HostAccountRegisterRingVrfKeyResponse::from_latest(response, version))
			.map_err(|error| {
				from_latest_error::<HostAccountRegisterRingVrfKeyError>(error, version)
			})
	}

	#[instrument(skip_all, fields(runtime.method = "account.list_ring_vrf_keys"))]
	async fn list_ring_vrf_keys(
		&self,
		cx: &CallContext,
		request: HostAccountListRingVrfKeysRequest,
	) -> Result<HostAccountListRingVrfKeysResponse, CallError<HostAccountListRingVrfKeysError>> {
		let version = request.version();
		let mut request = request.into_latest();
		let result = async {
			let Some(authority_session) = self.accounts.current_session() else {
				return Err(CallError::Domain(
					latest::HostAccountListRingVrfKeysError::NotConnected,
				));
			};
			request.owner = normalize_product_identifier(&request.owner).map_err(|err| {
				CallError::Domain(latest::HostAccountListRingVrfKeysError::Unknown {
					reason: err.to_string(),
				})
			})?;
			let cx = remote_authority_context(cx);
			remote_authority_call(
				&cx,
				self.accounts.list_ring_vrf_keys(&authority_session, &cx, request),
			)
			.await
			.map_err(|err| CallError::Domain(ring_vrf_list_error(err)))
		}
		.await;
		result
			.map(|response| HostAccountListRingVrfKeysResponse::from_latest(response, version))
			.map_err(|error| from_latest_error::<HostAccountListRingVrfKeysError>(error, version))
	}

	#[instrument(skip_all, fields(runtime.method = "account.ring_vrf_sign"))]
	async fn ring_vrf_sign(
		&self,
		cx: &CallContext,
		request: HostAccountRingVrfSignRequest,
	) -> Result<HostAccountRingVrfSignResponse, CallError<HostAccountRingVrfSignError>> {
		let version = request.version();
		let mut request = request.into_latest();
		let result = async {
			request.key_handle =
				Self::normalize_product_account_id(request.key_handle).map_err(|()| {
					CallError::Domain(latest::HostAccountRingVrfSignError::Unknown {
						reason: "Invalid key handle".to_string(),
					})
				})?;
			let Some(authority_session) = self.accounts.current_session() else {
				return Err(CallError::Domain(latest::HostAccountRingVrfSignError::NotConnected));
			};
			let cx = remote_authority_context(cx);
			self.accounts
				.ring_vrf_sign(&authority_session, &cx, request)
				.await
				.map_err(|err| CallError::Domain(ring_vrf_sign_error(err)))
		}
		.await;
		result
			.map(|response| HostAccountRingVrfSignResponse::from_latest(response, version))
			.map_err(|error| from_latest_error::<HostAccountRingVrfSignError>(error, version))
	}

	#[instrument(skip_all, fields(runtime.method = "account.sign_vrf"))]
	async fn sign_vrf(
		&self,
		cx: &CallContext,
		request: HostAccountSignVrfRequest,
	) -> Result<HostAccountSignVrfResponse, CallError<HostAccountSignVrfError>> {
		let version = request.version();
		let mut request = request.into_latest();
		let result = async {
			request.account =
				Self::normalize_product_account_id(request.account).map_err(|()| {
					CallError::Domain(latest::HostAccountSignVrfError::Unknown {
						reason: "Invalid product account".to_string(),
					})
				})?;
			validate_vrf_transcript(&request).map_err(|reason| {
				CallError::Domain(latest::HostAccountSignVrfError::Unknown { reason })
			})?;
			let Some(authority_session) = self.accounts.current_session() else {
				return Err(CallError::Domain(latest::HostAccountSignVrfError::NotConnected));
			};
			let cx = remote_authority_context(cx);
			remote_authority_call(&cx, self.accounts.sign_vrf(&authority_session, &cx, request))
				.await
				.map_err(vrf_call_error)
		}
		.await;
		result
			.map(|response| HostAccountSignVrfResponse::from_latest(response, version))
			.map_err(|error| from_latest_error::<HostAccountSignVrfError>(error, version))
	}

	#[instrument(skip_all, fields(runtime.method = "account.get_legacy_accounts"))]
	async fn get_legacy_accounts(
		&self,
		_cx: &CallContext,
		request: HostGetLegacyAccountsRequest,
	) -> Result<HostGetLegacyAccountsResponse, CallError<HostGetLegacyAccountsError>> {
		let version = request.version();
		let () = request.into_latest();
		// Match the mobile hosts: compatibility signing accounts may be
		// addressed explicitly, but are not enumerated.
		Ok(HostGetLegacyAccountsResponse::from_latest(
			latest::HostGetLegacyAccountsResponse { accounts: vec![] },
			version,
		))
	}

	#[instrument(skip_all, fields(runtime.method = "account.get_user_id"))]
	async fn get_user_id(
		&self,
		_cx: &CallContext,
		request: HostGetUserIdRequest,
	) -> Result<HostGetUserIdResponse, CallError<HostGetUserIdError>> {
		let version = request.version();
		let () = request.into_latest();
		let result = async {
			let Some(session) = self.accounts.current_session() else {
				return Err(CallError::Domain(latest::HostGetUserIdError::NotConnected));
			};

			match self.connection.identity_disclosure_authorization().await {
				Ok(PermissionAuthorizationStatus::Authorized) => {},
				Ok(
					PermissionAuthorizationStatus::Denied |
					PermissionAuthorizationStatus::NotDetermined,
				) => {
					return Err(CallError::Domain(latest::HostGetUserIdError::PermissionDenied));
				},
				Err(reason) => return Err(CallError::HostFailure { reason }),
			}

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
		}
		.await;
		result
			.map(|response| HostGetUserIdResponse::from_latest(response, version))
			.map_err(|error| from_latest_error::<HostGetUserIdError>(error, version))
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
		let version = request.version();
		let () = request.into_latest();
		Subscription::new(self.connection.session_state.subscribe().map(move |item| {
			Ok(HostAccountConnectionStatusSubscribeItem::from_latest(item.into_latest(), version))
		}))
	}

	#[instrument(skip_all, fields(runtime.method = "account.request_login", product = %self.connection.product.product_id))]
	async fn request_login(
		&self,
		_cx: &CallContext,
		request: HostRequestLoginRequest,
	) -> Result<HostRequestLoginResponse, CallError<HostRequestLoginError>> {
		let version = request.version();
		let _request = request.into_latest();
		let result = self.host_session.request_login(&self.connection.product).await;
		result
			.map(|response| HostRequestLoginResponse::from_latest(response, version))
			.map_err(|error| from_latest_error::<HostRequestLoginError>(error, version))
	}
}
