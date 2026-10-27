//! Product-facing account operations and login lifecycle.

use crate::platform::{
	PermissionAuthorizationStatus, ProductId, ProductSubtreeReview, UserConfirmationReview,
};
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

use crate::{
	host_internal::permissions::ChatAuthorityConsent,
	platform::PermissionAuthorizationRequest,
	runtime::{authority::chat_requires_preimage_submit, product_device_chat_authority_error},
};
use truapi::versioned::{
	IntoLatest,
	account::{
		HostProductDeviceChatError, HostProductDeviceChatRequest, HostProductDeviceChatResponse,
	},
};

use crate::{
	host_internal::product_manifest::Granted,
	runtime::{
		ProductRuntimeHost, account_access_authorization, account_get_authority_error,
		remote_authority_call, remote_authority_context, ring_vrf_alias_error, ring_vrf_list_error,
		ring_vrf_proof_error, ring_vrf_register_error, ring_vrf_sign_error, until_cancelled,
		validate_vrf_transcript, versioning::version, vrf_call_error,
	},
};

#[truapi::async_trait]
impl Account for ProductRuntimeHost {
	#[instrument(skip_all, fields(runtime.method = "account.get_account"))]
	async fn get_account(
		&self,
		cx: &CallContext,
		request: HostAccountGetRequest,
	) -> Result<HostAccountGetResponse, CallError<HostAccountGetError>> {
		version(request)
			.exec(|latest::HostAccountGetRequest { product_account_id }| async move {
				let owner = product_account_id.dot_ns_identifier.clone();
				let session = self
					.accounts
					.current_session()
					.ok_or(CallError::Domain(latest::HostAccountGetError::NotConnected))?;

				if &owner != self.connection.product_id() {
					match account_access_authorization(
						self.connection.platform.as_ref(),
						&self.connection.services.permissions,
						self.connection.product_id(),
						&owner,
					)
					.await
					.map_err(|err| CallError::HostFailure { reason: err.to_string() })?
					{
						PermissionAuthorizationStatus::Authorized => {},
						PermissionAuthorizationStatus::Denied |
						PermissionAuthorizationStatus::NotDetermined => {
							return Err(CallError::Domain(latest::HostAccountGetError::Rejected));
						},
					}
				} else if remote_authority_call(&remote_authority_context(cx), async {
					Ok(self
						.accounts
						.subtree_resolution_reaches_account_holder(&session, &owner)
						.await)
				})
				.await
				.map_err(account_get_authority_error)?
				{
					let approved = until_cancelled(
						&remote_authority_context(cx),
						self.connection.confirm_user_action_if_needed(
							UserConfirmationReview::ProductSubtree(ProductSubtreeReview {
								product_id: owner.clone(),
							}),
						),
					)
					.await
					.map_err(account_get_authority_error)?
					.map_err(|err| CallError::HostFailure { reason: err.reason })?;
					if !approved {
						return Err(CallError::Domain(latest::HostAccountGetError::Rejected));
					}
				}

				let public_key = self
					.product_account_public_key(
						cx,
						&session,
						&owner,
						&product_account_id.derivation_index,
					)
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

				let cx = remote_authority_context(cx);
				remote_authority_call(&cx, self.accounts.account_alias(&session, &cx, request))
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
				let owner = request.key_handle.dot_ns_identifier.clone();
				let session = self
					.accounts
					.current_session()
					.ok_or(CallError::Domain(latest::HostAccountCreateProofError::Rejected))?;

				let cx = remote_authority_context(cx);
				if !self.holds_cross_product_scope_within(&owner, Granted::Context, &cx).await {
					tracing::info!(
						caller = %self.connection.product_id(),
						owner = %request.key_handle.dot_ns_identifier,
						"cross-product ring-VRF access refused at the runtime frontend"
					);
					return Err(CallError::Domain(
						latest::HostAccountCreateProofError::NotAllowlisted,
					));
				}
				remote_authority_call(&cx, self.accounts.create_proof(&session, &cx, request))
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
				let cx = remote_authority_context(cx);
				remote_authority_call(
					&cx,
					self.accounts.register_ring_vrf_key(&session, &cx, request),
				)
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
			.exec(|mut request| async move {
				let session = self.accounts.current_session().ok_or(CallError::Domain(
					latest::HostAccountListRingVrfKeysError::NotConnected,
				))?;
				request.owner = request.owner.parse::<ProductId>()?.into();
				let cx = remote_authority_context(cx);
				remote_authority_call(&cx, self.accounts.list_ring_vrf_keys(&session, &cx, request))
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
				let owner = request.key_handle.dot_ns_identifier.clone();
				let session = self
					.accounts
					.current_session()
					.ok_or(CallError::Domain(latest::HostAccountRingVrfSignError::NotConnected))?;
				let cx = remote_authority_context(cx);
				if !self.holds_cross_product_scope_within(&owner, Granted::Context, &cx).await {
					tracing::info!(
						caller = %self.connection.product_id(),
						owner = %request.key_handle.dot_ns_identifier,
						"cross-product ring-VRF access refused at the runtime frontend"
					);
					return Err(CallError::Domain(
						latest::HostAccountRingVrfSignError::NotAllowlisted,
					));
				}
				remote_authority_call(&cx, self.accounts.ring_vrf_sign(&session, &cx, request))
					.await
					.map_err(|err| CallError::Domain(ring_vrf_sign_error(err)))
			})
			.await
	}

	#[instrument(skip_all, fields(runtime.method = "account.product_device_chat"))]
	async fn product_device_chat(
		&self,
		cx: &CallContext,
		request: HostProductDeviceChatRequest,
	) -> Result<HostProductDeviceChatResponse, CallError<HostProductDeviceChatError>> {
		let operation = match request {
			HostProductDeviceChatRequest::V2(operation) => operation,
			HostProductDeviceChatRequest::V1(_) => {
				return Err(CallError::unavailable());
			},
		};
		let Some(session) = self.accounts.current_session() else {
			return Err(CallError::Domain(HostProductDeviceChatError::V1(
				latest::HostProductDeviceChatError::NotConnected,
			)));
		};
		// A session-only grant lives with the authority, so a new execution of
		// the same product reuses it without prompting again.
		let consent = if self.accounts.chat_session_granted(&session) &&
			self.connection
				.permission_authorization_status(PermissionAuthorizationRequest::ChatAuthority)
				.await
				.map_err(|error| CallError::HostFailure { reason: error.reason })? ==
				PermissionAuthorizationStatus::NotDetermined
		{
			ChatAuthorityConsent::Session
		} else {
			self.connection
				.chat_authority_authorization()
				.await
				.map_err(|reason| CallError::HostFailure { reason })?
		};
		if consent == ChatAuthorityConsent::Refused {
			return Err(CallError::Domain(HostProductDeviceChatError::V1(
				latest::HostProductDeviceChatError::AccessNotGranted,
			)));
		}
		if chat_requires_preimage_submit(&operation) {
			self.connection
				.require_remote_permission(
					latest::RemotePermission::PreimageSubmit,
					HostProductDeviceChatError::V1(
						latest::HostProductDeviceChatError::AccessNotGranted,
					),
				)
				.await?;
		}
		let cx = remote_authority_context(cx);
		remote_authority_call(
			&cx,
			self.accounts.product_device_chat(
				&cx,
				&session,
				operation,
				consent == ChatAuthorityConsent::Session,
			),
		)
		.await
		.map(HostProductDeviceChatResponse::V2)
		.map_err(product_device_chat_authority_error)
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
				let cx = remote_authority_context(cx);
				remote_authority_call(&cx, self.accounts.sign_vrf(&session, &cx, request))
					.await
					.map_err(vrf_call_error)
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

				match self.connection.identity_disclosure_authorization().await {
					Ok(PermissionAuthorizationStatus::Authorized) => {},
					Ok(
						PermissionAuthorizationStatus::Denied |
						PermissionAuthorizationStatus::NotDetermined,
					) => {
						return Err(CallError::Domain(
							latest::HostGetUserIdError::PermissionDenied,
						));
					},
					Err(reason) => return Err(CallError::HostFailure { reason }),
				}

				if session.primary_username().is_none() {
					self.accounts
						.refresh_session_identity()
						.await
						.map_err(|reason| CallError::HostFailure { reason })?;
				}
				let current = self
					.accounts
					.current_session()
					.filter(|current| {
						current.public_key == session.public_key &&
							current.identity_account_id == session.identity_account_id &&
							self.accounts.session_is_current(&session)
					})
					.ok_or(CallError::Domain(latest::HostGetUserIdError::NotConnected))?;
				let primary_username =
					current.primary_username().map(str::to_string).ok_or_else(|| {
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
