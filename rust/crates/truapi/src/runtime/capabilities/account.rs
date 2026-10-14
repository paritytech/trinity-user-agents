//! Product-facing account operations and login lifecycle.

use crate::platform::{
	PermissionAuthorizationStatus, ProductSubtreeReview, UserConfirmationReview,
	normalize_product_identifier,
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

use super::from_latest_error;
use truapi::versioned::{FromLatest, IntoLatest, Versioned};

use crate::{
	host_internal::{product_manifest::Granted, sso_messages::ProductRequest},
	runtime::{
		ProductRuntimeHost, account_access_authorization, account_get_authority_error,
		remote_authority_call, remote_authority_context, ring_vrf_alias_error, ring_vrf_list_error,
		ring_vrf_proof_error, ring_vrf_register_error, ring_vrf_sign_error, until_cancelled,
		validate_vrf_transcript, vrf_call_error,
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
		let version = request.version();
		let latest::HostAccountGetRequest { product_account_id } = request.into_latest();
		let result = async {
			let product_account_id = Self::normalize_product_account_id(product_account_id)
				.map_err(|()| CallError::Domain(latest::HostAccountGetError::DomainNotValid))?;
			let Some(session) = self.authority.current_session() else {
				return Err(CallError::Domain(latest::HostAccountGetError::NotConnected));
			};

			let product_id = self.product_id();
			if product_account_id.dot_ns_identifier != product_id {
				match account_access_authorization(
					self.platform.as_ref(),
					&product_id,
					&product_account_id.dot_ns_identifier,
				)
				.await
				{
					Ok(PermissionAuthorizationStatus::Authorized) => {},
					Ok(
						PermissionAuthorizationStatus::Denied |
						PermissionAuthorizationStatus::NotDetermined,
					) => {
						return Err(CallError::Domain(latest::HostAccountGetError::Rejected));
					},
					Err(err) => {
						return Err(CallError::HostFailure { reason: err.to_string() });
					},
				}
			} else if self
				.authority
				.subtree_resolution_reaches_account_holder(
					&session,
					&product_account_id.dot_ns_identifier,
				)
				.await
			{
				let approved = until_cancelled(
					cx,
					self.confirm_product_action(UserConfirmationReview::ProductSubtree(
						ProductSubtreeReview {
							product_id: product_account_id.dot_ns_identifier.clone(),
						},
					)),
				)
				.await
				.map_err(account_get_authority_error)?
				.map_err(|err| CallError::HostFailure { reason: err.reason })?;
				if !approved {
					return Err(CallError::Domain(latest::HostAccountGetError::Rejected));
				}
			}

			let public_key = self
				.product_account_public_key(cx, &session, &product_account_id)
				.await
				.map_err(account_get_authority_error)?;

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
			let Some(session) = self.authority.current_session() else {
				return Err(CallError::Domain(latest::HostAccountGetAliasError::Rejected));
			};

			let calling_product_id = self.product_id();
			let cx = remote_authority_context(cx);
			remote_authority_call(
				&cx,
				self.authority.account_alias(
					&cx,
					&session,
					ProductRequest { calling_product_id, payload: request },
				),
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
			let Some(session) = self.authority.current_session() else {
				return Err(CallError::Domain(latest::HostAccountCreateProofError::Rejected));
			};

			let calling_product_id = self.product_id();
			let cx = remote_authority_context(cx);
			let Some(owner) = self
				.bounded_cross_product_scope_target(
					&request.key_handle.dot_ns_identifier,
					Granted::Context,
					&cx,
				)
				.await
			else {
				tracing::info!(
					caller = %calling_product_id,
					owner = %request.key_handle.dot_ns_identifier,
					"cross-product ring-VRF access refused at the runtime frontend"
				);
				return Err(CallError::Domain(latest::HostAccountCreateProofError::NotAllowlisted));
			};
			request.key_handle.dot_ns_identifier = owner;
			remote_authority_call(
				&cx,
				self.authority.create_proof(
					&cx,
					&session,
					ProductRequest { calling_product_id, payload: request },
				),
			)
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
			let Some(session) = self.authority.current_session() else {
				return Err(CallError::Domain(
					latest::HostAccountRegisterRingVrfKeyError::NotConnected,
				));
			};
			let calling_product_id = self.product_id();
			let cx = remote_authority_context(cx);
			remote_authority_call(
				&cx,
				self.authority.register_ring_vrf_key(
					&cx,
					&session,
					ProductRequest { calling_product_id, payload: request },
				),
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
			let Some(session) = self.authority.current_session() else {
				return Err(CallError::Domain(
					latest::HostAccountListRingVrfKeysError::NotConnected,
				));
			};
			request.owner = normalize_product_identifier(&request.owner).map_err(|err| {
				CallError::Domain(latest::HostAccountListRingVrfKeysError::Unknown {
					reason: err.to_string(),
				})
			})?;
			let calling_product_id = self.product_id();
			let cx = remote_authority_context(cx);
			remote_authority_call(
				&cx,
				self.authority.list_ring_vrf_keys(
					&cx,
					&session,
					ProductRequest { calling_product_id, payload: request },
				),
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
			let Some(session) = self.authority.current_session() else {
				return Err(CallError::Domain(latest::HostAccountRingVrfSignError::NotConnected));
			};
			let calling_product_id = self.product_id();
			let cx = remote_authority_context(cx);
			let Some(owner) = self
				.bounded_cross_product_scope_target(
					&request.key_handle.dot_ns_identifier,
					Granted::Context,
					&cx,
				)
				.await
			else {
				tracing::info!(
					caller = %calling_product_id,
					owner = %request.key_handle.dot_ns_identifier,
					"cross-product ring-VRF access refused at the runtime frontend"
				);
				return Err(CallError::Domain(latest::HostAccountRingVrfSignError::NotAllowlisted));
			};
			request.key_handle.dot_ns_identifier = owner;
			remote_authority_call(
				&cx,
				self.authority.ring_vrf_sign(
					&cx,
					&session,
					ProductRequest { calling_product_id, payload: request },
				),
			)
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
			let Some(session) = self.authority.current_session() else {
				return Err(CallError::Domain(latest::HostAccountSignVrfError::NotConnected));
			};
			let cx = remote_authority_context(cx);
			remote_authority_call(
				&cx,
				self.authority.sign_vrf(&cx, &session, self.product_id(), request),
			)
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
			let Some(session) = self.authority.current_session() else {
				return Err(CallError::Domain(latest::HostGetUserIdError::NotConnected));
			};

			match self.identity_disclosure_authorization().await {
				Ok(PermissionAuthorizationStatus::Authorized) => {},
				Ok(
					PermissionAuthorizationStatus::Denied |
					PermissionAuthorizationStatus::NotDetermined,
				) => {
					return Err(CallError::Domain(latest::HostGetUserIdError::PermissionDenied));
				},
				Err(reason) => return Err(CallError::HostFailure { reason }),
			}

			let session = if session.primary_username().is_some() {
				session
			} else {
				self.authority.refresh_session_identity().await.unwrap_or(session)
			};
			let primary_username = session.primary_username().ok_or_else(|| {
				CallError::Domain(latest::HostGetUserIdError::Unknown {
					reason: "No primary username for this session".to_string(),
				})
			})?;

			Ok(latest::HostGetUserIdResponse { primary_username: primary_username.to_string() })
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
		Subscription::new(self.authority.session_state().subscribe().map(move |item| {
			Ok(HostAccountConnectionStatusSubscribeItem::from_latest(item.into_latest(), version))
		}))
	}

	#[instrument(skip_all, fields(runtime.method = "account.request_login", product = %self.product.product_id))]
	async fn request_login(
		&self,
		_cx: &CallContext,
		request: HostRequestLoginRequest,
	) -> Result<HostRequestLoginResponse, CallError<HostRequestLoginError>> {
		let version = request.version();
		let _request = request.into_latest();
		let result = self
			.authority
			.request_login(&self.product)
			.await
			.map(IntoLatest::into_latest)
			.map_err(|error| error.map_domain(IntoLatest::into_latest));
		result
			.map(|response| HostRequestLoginResponse::from_latest(response, version))
			.map_err(|error| from_latest_error::<HostRequestLoginError>(error, version))
	}
}
