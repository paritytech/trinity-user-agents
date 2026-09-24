//! Product-facing account capability adapters.
//!
//! Account management uses shared session state and the account authority
//! for alias, proof, and login operations.

use futures::StreamExt;
use tracing::instrument;
use truapi::api::Account;
use truapi::versioned::account::{
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
    HostGetUserIdRequest, HostGetUserIdResponse, HostProductDeviceChatError,
    HostProductDeviceChatRequest, HostProductDeviceChatResponse, HostRequestLoginError,
    HostRequestLoginRequest, HostRequestLoginResponse,
};
use truapi::{CallContext, CallError, Subscription, latest, v01};
use truapi_platform::{
    PermissionAuthorizationStatus, ProductSubtreeReview, UserConfirmationReview,
    normalize_product_identifier,
};

use crate::host_logic::product_manifest::Granted;
use crate::host_logic::sso::messages::ProductRequest;
use crate::runtime::authority::{
    AuthorityError, ProductDeviceChatAuthorityRequest, chat_requires_preimage_submit,
};
use crate::runtime::{
    ProductRuntimeHost, account_access_authorization, account_get_authority_error,
    product_device_chat_authority_error, remote_authority_call, remote_authority_context,
    ring_vrf_alias_error, ring_vrf_list_error, ring_vrf_proof_error, ring_vrf_register_error,
    ring_vrf_sign_error, validate_vrf_transcript, vrf_call_error,
};

#[truapi::async_trait]
impl Account for ProductRuntimeHost {
    #[instrument(skip_all, fields(runtime.method = "account.get_account"))]
    async fn get_account(
        &self,
        cx: &CallContext,
        request: HostAccountGetRequest,
    ) -> Result<HostAccountGetResponse, CallError<HostAccountGetError>> {
        let HostAccountGetRequest::V1(v01::HostAccountGetRequest { product_account_id }) = request;
        let product_account_id =
            Self::normalize_product_account_id(product_account_id).map_err(|()| {
                CallError::Domain(HostAccountGetError::V1(
                    v01::HostAccountGetError::DomainNotValid,
                ))
            })?;
        let Some(session) = self.authority.current_session() else {
            return Err(CallError::Domain(HostAccountGetError::V1(
                v01::HostAccountGetError::NotConnected,
            )));
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
                Ok(PermissionAuthorizationStatus::Authorized) => {}
                Ok(
                    PermissionAuthorizationStatus::Denied
                    | PermissionAuthorizationStatus::NotDetermined,
                ) => {
                    return Err(CallError::Domain(HostAccountGetError::V1(
                        v01::HostAccountGetError::Rejected,
                    )));
                }
                Err(err) => {
                    return Err(CallError::HostFailure {
                        reason: err.to_string(),
                    });
                }
            }
        } else {
            // Own-account resolution walks two host callbacks before the
            // bounded SSO call: a persisted subtree read, then a confirmation.
            // Neither had a deadline, so a host that never answered its own
            // storage parked the request forever, with no response and no
            // error frame (host-rust-core#954). Both are bounded by the
            // caller's context, falling back to the same default the SSO call
            // uses, so an unresponsive host surfaces a typed error instead.
            let authority_cx = remote_authority_context(cx);
            let reaches_account_holder = remote_authority_call(&authority_cx, async {
                Ok::<_, AuthorityError>(
                    self.authority
                        .subtree_resolution_reaches_account_holder(
                            &session,
                            &product_account_id.dot_ns_identifier,
                        )
                        .await,
                )
            })
            .await
            .map_err(account_get_authority_error)?;

            if reaches_account_holder {
                // Own-account resolution has no access review, so a cold
                // subtree that must reach the Account Holder is the one point
                // a host can surface and reject before the SSO call.
                let approved = remote_authority_call(&authority_cx, async {
                    self.platform
                        .confirm_user_action(UserConfirmationReview::ProductSubtree(
                            ProductSubtreeReview {
                                product_id: product_account_id.dot_ns_identifier.clone(),
                            },
                        ))
                        .await
                        .map_err(|err| AuthorityError::Unavailable { reason: err.reason })
                })
                .await
                .map_err(account_get_authority_error)?;
                if !approved {
                    return Err(CallError::Domain(HostAccountGetError::V1(
                        v01::HostAccountGetError::Rejected,
                    )));
                }
            }
        }

        let public_key = self
            .product_account_public_key(cx, &session, &product_account_id)
            .await
            .map_err(account_get_authority_error)?;

        Ok(HostAccountGetResponse::V1(v01::HostAccountGetResponse {
            account: v01::ProductAccount {
                public_key: public_key.to_vec(),
            },
        }))
    }

    #[instrument(skip_all, fields(runtime.method = "account.get_account_alias"))]
    async fn get_account_alias(
        &self,
        cx: &CallContext,
        request: HostAccountGetAliasRequest,
    ) -> Result<HostAccountGetAliasResponse, CallError<HostAccountGetAliasError>> {
        let HostAccountGetAliasRequest::V1(mut request) = request;
        request.key_handle =
            Self::normalize_product_account_id(request.key_handle).map_err(|()| {
                CallError::Domain(HostAccountGetAliasError::V1(
                    v01::HostAccountGetAliasError::Unknown {
                        reason: "Invalid key handle".to_string(),
                    },
                ))
            })?;
        let Some(session) = self.authority.current_session() else {
            return Err(CallError::Domain(HostAccountGetAliasError::V1(
                v01::HostAccountGetAliasError::Rejected,
            )));
        };

        let calling_product_id = self.product_id();
        let cx = remote_authority_context(cx);
        remote_authority_call(
            &cx,
            self.authority.account_alias(
                &cx,
                &session,
                ProductRequest {
                    calling_product_id,
                    payload: request,
                },
            ),
        )
        .await
        .map(HostAccountGetAliasResponse::V1)
        .map_err(|err| CallError::Domain(HostAccountGetAliasError::V1(ring_vrf_alias_error(err))))
    }

    #[instrument(skip_all, fields(runtime.method = "account.create_account_proof"))]
    async fn create_account_proof(
        &self,
        cx: &CallContext,
        request: HostAccountCreateProofRequest,
    ) -> Result<HostAccountCreateProofResponse, CallError<HostAccountCreateProofError>> {
        let HostAccountCreateProofRequest::V1(mut request) = request;
        request.key_handle =
            Self::normalize_product_account_id(request.key_handle).map_err(|()| {
                CallError::Domain(HostAccountCreateProofError::V1(
                    v01::HostAccountCreateProofError::Unknown {
                        reason: "Invalid key handle".to_string(),
                    },
                ))
            })?;
        // The session is consulted before the grant, matching `ring_vrf_sign`.
        // The other order makes the pair of refusals a probe for who granted
        // whom: with no session a granting target answers `Rejected` and a
        // non-granting one `NotAllowlisted`, which is exactly what the uniform
        // cross-product refusal exists to prevent.
        let Some(session) = self.authority.current_session() else {
            return Err(CallError::Domain(HostAccountCreateProofError::V1(
                v01::HostAccountCreateProofError::Rejected,
            )));
        };

        let calling_product_id = self.product_id();
        let cx = remote_authority_context(cx);
        // The grant lookup runs *before* `remote_authority_call`, under a bound of
        // its own. It can reach dotNS on the Asset Hub, several sequential chain
        // operations each bounded only by `OPERATION_TIMEOUT`, so it needs a
        // deadline either way. Running it inside would arm two timers on one
        // budget, and whichever fired first would decide whether the caller sees
        // the uniform refusal or a transport error naming a reason, making the
        // refusal shape depend on scheduling. Decided here instead, a lookup that
        // runs out of time answers `NotAllowlisted` like every other refusal on
        // this path. The stages are bounded separately, so a caller asking for
        // one second can wait up to two.
        //
        // The gate returns the normalized owner it decided about and the handle
        // is rebuilt from it, so authorization and key derivation agree by
        // construction rather than by a registry lookup happening to miss.
        let Some(owner) = self
            .bounded_cross_product_scope_target(
                &request.key_handle.dot_ns_identifier,
                Granted::Context,
                &cx,
            )
            .await
        else {
            // Recorded here, because this door answers without reaching the
            // authority. Without this line a product probing which handles
            // exist on the device leaves no trace, while every success is
            // logged. The wire answers one refusal for every
            // reason; this is the operator's copy.
            tracing::info!(
                caller = %calling_product_id,
                owner = %request.key_handle.dot_ns_identifier,
                "cross-product ring-VRF access refused at the runtime frontend"
            );
            return Err(CallError::Domain(HostAccountCreateProofError::V1(
                v01::HostAccountCreateProofError::NotAllowlisted,
            )));
        };
        request.key_handle.dot_ns_identifier = owner;
        remote_authority_call(
            &cx,
            self.authority.create_proof(
                &cx,
                &session,
                ProductRequest {
                    calling_product_id,
                    payload: request,
                },
            ),
        )
        .await
        .map(HostAccountCreateProofResponse::V1)
        .map_err(|err| {
            CallError::Domain(HostAccountCreateProofError::V1(ring_vrf_proof_error(err)))
        })
    }

    #[instrument(skip_all, fields(runtime.method = "account.register_ring_vrf_key"))]
    async fn register_ring_vrf_key(
        &self,
        cx: &CallContext,
        request: HostAccountRegisterRingVrfKeyRequest,
    ) -> Result<HostAccountRegisterRingVrfKeyResponse, CallError<HostAccountRegisterRingVrfKeyError>>
    {
        let HostAccountRegisterRingVrfKeyRequest::V1(request) = request;
        let Some(session) = self.authority.current_session() else {
            return Err(CallError::Domain(HostAccountRegisterRingVrfKeyError::V1(
                v01::HostAccountRegisterRingVrfKeyError::NotConnected,
            )));
        };
        let calling_product_id = self.product_id();
        let cx = remote_authority_context(cx);
        remote_authority_call(
            &cx,
            self.authority.register_ring_vrf_key(
                &cx,
                &session,
                ProductRequest {
                    calling_product_id,
                    payload: request,
                },
            ),
        )
        .await
        .map(HostAccountRegisterRingVrfKeyResponse::V1)
        .map_err(|err| {
            CallError::Domain(HostAccountRegisterRingVrfKeyError::V1(
                ring_vrf_register_error(err),
            ))
        })
    }

    #[instrument(skip_all, fields(runtime.method = "account.list_ring_vrf_keys"))]
    async fn list_ring_vrf_keys(
        &self,
        cx: &CallContext,
        request: HostAccountListRingVrfKeysRequest,
    ) -> Result<HostAccountListRingVrfKeysResponse, CallError<HostAccountListRingVrfKeysError>>
    {
        let HostAccountListRingVrfKeysRequest::V1(mut request) = request;
        let Some(session) = self.authority.current_session() else {
            return Err(CallError::Domain(HostAccountListRingVrfKeysError::V1(
                v01::HostAccountListRingVrfKeysError::NotConnected,
            )));
        };
        request.owner = normalize_product_identifier(&request.owner).map_err(|err| {
            CallError::Domain(HostAccountListRingVrfKeysError::V1(
                v01::HostAccountListRingVrfKeysError::Unknown {
                    reason: err.to_string(),
                },
            ))
        })?;
        let calling_product_id = self.product_id();
        let cx = remote_authority_context(cx);
        remote_authority_call(
            &cx,
            self.authority.list_ring_vrf_keys(
                &cx,
                &session,
                ProductRequest {
                    calling_product_id,
                    payload: request,
                },
            ),
        )
        .await
        .map(HostAccountListRingVrfKeysResponse::V1)
        .map_err(|err| {
            CallError::Domain(HostAccountListRingVrfKeysError::V1(ring_vrf_list_error(
                err,
            )))
        })
    }

    #[instrument(skip_all, fields(runtime.method = "account.ring_vrf_sign"))]
    async fn ring_vrf_sign(
        &self,
        cx: &CallContext,
        request: HostAccountRingVrfSignRequest,
    ) -> Result<HostAccountRingVrfSignResponse, CallError<HostAccountRingVrfSignError>> {
        let HostAccountRingVrfSignRequest::V1(mut request) = request;
        request.key_handle =
            Self::normalize_product_account_id(request.key_handle).map_err(|()| {
                CallError::Domain(HostAccountRingVrfSignError::V1(
                    v01::HostAccountRingVrfSignError::Unknown {
                        reason: "Invalid key handle".to_string(),
                    },
                ))
            })?;
        let Some(session) = self.authority.current_session() else {
            return Err(CallError::Domain(HostAccountRingVrfSignError::V1(
                v01::HostAccountRingVrfSignError::NotConnected,
            )));
        };
        let calling_product_id = self.product_id();
        let cx = remote_authority_context(cx);
        // As in `create_account_proof`: the lookup is bounded before the authority
        // call rather than inside it, and the handle carried on is the normalized
        // owner the gate decided about rather than the spelling the caller sent.
        let Some(owner) = self
            .bounded_cross_product_scope_target(
                &request.key_handle.dot_ns_identifier,
                Granted::Context,
                &cx,
            )
            .await
        else {
            // Recorded here, because this door answers without reaching the
            // authority. Without this line a product probing which handles
            // exist on the device leaves no trace, while every success is
            // logged. The wire answers one refusal for every
            // reason; this is the operator's copy.
            tracing::info!(
                caller = %calling_product_id,
                owner = %request.key_handle.dot_ns_identifier,
                "cross-product ring-VRF access refused at the runtime frontend"
            );
            return Err(CallError::Domain(HostAccountRingVrfSignError::V1(
                v01::HostAccountRingVrfSignError::NotAllowlisted,
            )));
        };
        request.key_handle.dot_ns_identifier = owner;
        remote_authority_call(
            &cx,
            self.authority.ring_vrf_sign(
                &cx,
                &session,
                ProductRequest {
                    calling_product_id,
                    payload: request,
                },
            ),
        )
        .await
        .map(HostAccountRingVrfSignResponse::V1)
        .map_err(|err| CallError::Domain(HostAccountRingVrfSignError::V1(ring_vrf_sign_error(err))))
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
            }
        };
        let calling_product_id =
            normalize_product_identifier(&self.product_id()).map_err(|_| {
                CallError::Domain(HostProductDeviceChatError::V1(
                    latest::HostProductDeviceChatError::InvalidRequest,
                ))
            })?;
        let Some(session) = self.authority.current_session() else {
            return Err(CallError::Domain(HostProductDeviceChatError::V1(
                latest::HostProductDeviceChatError::NotConnected,
            )));
        };
        if self
            .chat_authority_authorization()
            .await
            .map_err(|reason| CallError::HostFailure { reason })?
            != PermissionAuthorizationStatus::Authorized
        {
            return Err(CallError::Domain(HostProductDeviceChatError::V1(
                latest::HostProductDeviceChatError::AccessNotGranted,
            )));
        }
        if chat_requires_preimage_submit(&operation) {
            self.require_remote_permission(
                v01::RemotePermission::PreimageSubmit,
                HostProductDeviceChatError::V1(
                    latest::HostProductDeviceChatError::AccessNotGranted,
                ),
            )
            .await?;
        }
        let cx = remote_authority_context(cx);
        let authority_request = ProductDeviceChatAuthorityRequest {
            calling_product_id,
            operation,
        };
        remote_authority_call(
            &cx,
            self.authority
                .product_device_chat(&cx, &session, authority_request),
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
        let HostAccountSignVrfRequest::V1(mut request) = request;
        request.account = Self::normalize_product_account_id(request.account).map_err(|()| {
            CallError::Domain(HostAccountSignVrfError::V1(
                v01::HostAccountSignVrfError::Unknown {
                    reason: "Invalid product account".to_string(),
                },
            ))
        })?;
        validate_vrf_transcript(&request).map_err(|reason| {
            CallError::Domain(HostAccountSignVrfError::V1(
                v01::HostAccountSignVrfError::Unknown { reason },
            ))
        })?;
        let Some(session) = self.authority.current_session() else {
            return Err(CallError::Domain(HostAccountSignVrfError::V1(
                v01::HostAccountSignVrfError::NotConnected,
            )));
        };
        let cx = remote_authority_context(cx);
        remote_authority_call(
            &cx,
            self.authority
                .sign_vrf(&cx, &session, self.product_id(), request),
        )
        .await
        .map(HostAccountSignVrfResponse::V1)
        .map_err(vrf_call_error)
    }

    #[instrument(skip_all, fields(runtime.method = "account.get_legacy_accounts"))]
    async fn get_legacy_accounts(
        &self,
        _cx: &CallContext,
        _request: HostGetLegacyAccountsRequest,
    ) -> Result<HostGetLegacyAccountsResponse, CallError<HostGetLegacyAccountsError>> {
        // Match the mobile hosts: compatibility signing accounts may be
        // addressed explicitly, but are not enumerated.
        Ok(HostGetLegacyAccountsResponse::V1(
            latest::HostGetLegacyAccountsResponse { accounts: vec![] },
        ))
    }

    #[instrument(skip_all, fields(runtime.method = "account.get_user_id"))]
    async fn get_user_id(
        &self,
        _cx: &CallContext,
        _request: HostGetUserIdRequest,
    ) -> Result<HostGetUserIdResponse, CallError<HostGetUserIdError>> {
        let Some(session) = self.authority.current_session() else {
            return Err(CallError::Domain(HostGetUserIdError::V1(
                v01::HostGetUserIdError::NotConnected,
            )));
        };

        match self.identity_disclosure_authorization().await {
            Ok(PermissionAuthorizationStatus::Authorized) => {}
            Ok(
                PermissionAuthorizationStatus::Denied
                | PermissionAuthorizationStatus::NotDetermined,
            ) => {
                return Err(CallError::Domain(HostGetUserIdError::V1(
                    v01::HostGetUserIdError::PermissionDenied,
                )));
            }
            Err(reason) => return Err(CallError::HostFailure { reason }),
        }

        let session = if session.primary_username().is_some() {
            session
        } else {
            self.authority
                .refresh_session_identity()
                .await
                .unwrap_or(session)
        };
        let primary_username = session.primary_username().ok_or_else(|| {
            CallError::Domain(HostGetUserIdError::V1(v01::HostGetUserIdError::Unknown {
                reason: "No primary username for this session".to_string(),
            }))
        })?;

        Ok(HostGetUserIdResponse::V1(v01::HostGetUserIdResponse {
            primary_username: primary_username.to_string(),
        }))
    }

    #[instrument(skip_all, fields(runtime.method = "account.connection_status_subscribe"))]
    async fn connection_status_subscribe(
        &self,
        _cx: &CallContext,
        _request: HostAccountConnectionStatusSubscribeRequest,
    ) -> Subscription<
        HostAccountConnectionStatusSubscribeItem,
        CallError<HostAccountConnectionStatusSubscribeError>,
    > {
        Subscription::new(self.authority.session_state().subscribe().map(Ok))
    }

    #[instrument(skip_all, fields(runtime.method = "account.request_login", product = %self.product.product_id))]
    async fn request_login(
        &self,
        _cx: &CallContext,
        _request: HostRequestLoginRequest,
    ) -> Result<HostRequestLoginResponse, CallError<HostRequestLoginError>> {
        self.authority.request_login(&self.product).await
    }
}
