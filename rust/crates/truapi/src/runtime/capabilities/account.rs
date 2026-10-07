//! Product-facing account capability adapters.
//!
//! Account operations use shared host accounts; login uses the session lifecycle.

use crate::platform::{
    PermissionAuthorizationStatus, ProductSubtreeReview, UserConfirmationReview,
    normalize_product_identifier,
};
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
    HostGetUserIdRequest, HostGetUserIdResponse, HostRequestLoginError, HostRequestLoginRequest,
    HostRequestLoginResponse,
};
use truapi::{CallContext, CallError, Subscription, latest, v01};

use crate::host_internal::product_manifest::Granted;
use crate::runtime::{
    AccountCaller, AccountHolder, ProductRuntimeHost, account_access_authorization,
    account_get_authority_error, remote_authority_context, ring_vrf_alias_error,
    ring_vrf_list_error, ring_vrf_proof_error, ring_vrf_register_error, ring_vrf_sign_error,
    validate_vrf_transcript, vrf_call_error,
};

#[truapi::async_trait]
impl<H: AccountHolder> Account for ProductRuntimeHost<H> {
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
        let Some(operation) = self.accounts.current_operation() else {
            return Err(CallError::Domain(HostAccountGetError::V1(
                v01::HostAccountGetError::NotConnected,
            )));
        };
        let product_id = self.connection.product_id();
        if product_account_id.dot_ns_identifier != product_id {
            match account_access_authorization(
                self.connection.platform.as_ref(),
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
        }
        let outbound_review = (product_account_id.dot_ns_identifier == product_id).then(|| {
            UserConfirmationReview::ProductSubtree(ProductSubtreeReview {
                product_id: product_account_id.dot_ns_identifier.clone(),
            })
        });

        let public_key = self
            .product_account_public_key(
                cx,
                &operation,
                &product_account_id,
                outbound_review.as_ref(),
            )
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
        let Some(operation) = self.accounts.current_operation() else {
            return Err(CallError::Domain(HostAccountGetAliasError::V1(
                v01::HostAccountGetAliasError::Rejected,
            )));
        };

        let cx = remote_authority_context(cx);
        self.account_operation(
            &operation,
            &cx,
            self.accounts.account_alias(
                &operation,
                &cx,
                AccountCaller::Local {
                    product: &self.connection.product,
                    authorization: None,
                    outbound_review: None,
                },
                request,
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
        let Some(operation) = self.accounts.current_operation() else {
            return Err(CallError::Domain(HostAccountCreateProofError::V1(
                v01::HostAccountCreateProofError::Rejected,
            )));
        };

        let calling_product_id = self.connection.product_id();
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
            .connection
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
        self.account_operation(
            &operation,
            &cx,
            self.accounts.create_proof(
                &operation,
                &cx,
                AccountCaller::Local {
                    product: &self.connection.product,
                    authorization: None,
                    outbound_review: None,
                },
                request,
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
        let Some(operation) = self.accounts.current_operation() else {
            return Err(CallError::Domain(HostAccountRegisterRingVrfKeyError::V1(
                v01::HostAccountRegisterRingVrfKeyError::NotConnected,
            )));
        };
        let cx = remote_authority_context(cx);
        self.account_operation(
            &operation,
            &cx,
            self.accounts.register_ring_vrf_key(
                &operation,
                &cx,
                AccountCaller::Local {
                    product: &self.connection.product,
                    authorization: None,
                    outbound_review: None,
                },
                request,
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
        let Some(operation) = self.accounts.current_operation() else {
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
        let cx = remote_authority_context(cx);
        self.account_operation(
            &operation,
            &cx,
            self.accounts.list_ring_vrf_keys(
                &operation,
                &cx,
                AccountCaller::Local {
                    product: &self.connection.product,
                    authorization: None,
                    outbound_review: None,
                },
                request,
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
        let Some(operation) = self.accounts.current_operation() else {
            return Err(CallError::Domain(HostAccountRingVrfSignError::V1(
                v01::HostAccountRingVrfSignError::NotConnected,
            )));
        };
        let calling_product_id = self.connection.product_id();
        let cx = remote_authority_context(cx);
        // As in `create_account_proof`: the lookup is bounded before the authority
        // call rather than inside it, and the handle carried on is the normalized
        // owner the gate decided about rather than the spelling the caller sent.
        let Some(owner) = self
            .connection
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
        self.account_operation(
            &operation,
            &cx,
            self.accounts.ring_vrf_sign(
                &operation,
                &cx,
                AccountCaller::Local {
                    product: &self.connection.product,
                    authorization: None,
                    outbound_review: None,
                },
                request,
            ),
        )
        .await
        .map(HostAccountRingVrfSignResponse::V1)
        .map_err(|err| CallError::Domain(HostAccountRingVrfSignError::V1(ring_vrf_sign_error(err))))
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
        let Some(operation) = self.accounts.current_operation() else {
            return Err(CallError::Domain(HostAccountSignVrfError::V1(
                v01::HostAccountSignVrfError::NotConnected,
            )));
        };
        let authorization = self
            .accounts
            .wallet_authorization(&operation, &self.connection.product)
            .map_err(|error| CallError::Domain(HostAccountSignVrfError::V1(error.into())))?;
        let cx = remote_authority_context(cx);
        self.account_operation(
            &operation,
            &cx,
            self.accounts.sign_vrf(
                &operation,
                &cx,
                AccountCaller::Local {
                    product: &self.connection.product,
                    authorization: authorization.as_ref(),
                    outbound_review: None,
                },
                request,
            ),
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
        let Some(session) = self.accounts.current_session() else {
            return Err(CallError::Domain(HostGetUserIdError::V1(
                v01::HostGetUserIdError::NotConnected,
            )));
        };

        match self.connection.identity_disclosure_authorization().await {
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

        let primary_username = match session.primary_username() {
            Some(name) => Some(name.to_string()),
            None => self.host_session.primary_username().await,
        }
        .ok_or_else(|| {
            CallError::Domain(HostGetUserIdError::V1(v01::HostGetUserIdError::Unknown {
                reason: "No primary username for this session".to_string(),
            }))
        })?;

        Ok(HostGetUserIdResponse::V1(v01::HostGetUserIdResponse {
            primary_username,
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
        Subscription::new(self.connection.session_state.subscribe().map(Ok))
    }

    #[instrument(skip_all, fields(runtime.method = "account.request_login", product = %self.connection.product.product_id))]
    async fn request_login(
        &self,
        _cx: &CallContext,
        _request: HostRequestLoginRequest,
    ) -> Result<HostRequestLoginResponse, CallError<HostRequestLoginError>> {
        self.host_session
            .request_login(&self.connection.product)
            .await
            .map(HostRequestLoginResponse::V1)
            .map_err(|error| match error {
                CallError::Domain(error) => CallError::Domain(HostRequestLoginError::V1(error)),
                CallError::Denied => CallError::Denied,
                CallError::Unsupported => CallError::Unsupported,
                CallError::MalformedFrame { reason } => CallError::MalformedFrame { reason },
                CallError::HostFailure { reason } => CallError::HostFailure { reason },
                CallError::Cancelled => CallError::Cancelled,
            })
    }
}
