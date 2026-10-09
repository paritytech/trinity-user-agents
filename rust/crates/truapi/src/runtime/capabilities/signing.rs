//! Product-facing signing capability adapters.

use tracing::{debug, instrument};
use truapi::api::Signing;
use truapi::versioned::signing::{
    HostCreateTransactionError, HostCreateTransactionRequest, HostCreateTransactionResponse,
    HostCreateTransactionWithLegacyAccountError, HostCreateTransactionWithLegacyAccountRequest,
    HostCreateTransactionWithLegacyAccountResponse, HostSignPayloadError, HostSignPayloadRequest,
    HostSignPayloadResponse, HostSignPayloadWithLegacyAccountError,
    HostSignPayloadWithLegacyAccountRequest, HostSignPayloadWithLegacyAccountResponse,
    HostSignRawError, HostSignRawRequest, HostSignRawResponse, HostSignRawWithLegacyAccountError,
    HostSignRawWithLegacyAccountRequest, HostSignRawWithLegacyAccountResponse,
};
use truapi::{CallContext, CallError, v01};

use crate::runtime::authority::{
    AccountCaller, AuthorityError, CreateTransactionAuthorityRequest, SignPayloadAuthorityRequest,
    SignRawAuthorityRequest,
};
use crate::runtime::{
    AccountHolder, ContactResolutionError, LEGACY_ACCOUNT_UNAVAILABLE_REASON,
    LEGACY_PRODUCT_ACCOUNT_MISMATCH_REASON, LegacySigner, ProductRuntimeHost, signing_call_error,
    transaction_call_error,
};

#[truapi::async_trait]
impl<H: AccountHolder> Signing for ProductRuntimeHost<H> {
    #[instrument(skip_all, fields(runtime.method = "signing.sign_payload"))]
    async fn sign_payload(
        &self,
        cx: &CallContext,
        request: HostSignPayloadRequest,
    ) -> Result<HostSignPayloadResponse, CallError<HostSignPayloadError>> {
        debug!("sign_payload: requesting signing-host signature");
        let HostSignPayloadRequest::V1(mut inner) = request;
        inner.account = Self::normalize_product_account_id(inner.account).map_err(|()| {
            CallError::Domain(HostSignPayloadError::V1(
                v01::HostSignPayloadError::PermissionDenied,
            ))
        })?;
        let authority_session = self.accounts.current_session();
        self.connection
            .require_chain_submit(HostSignPayloadError::V1(
                v01::HostSignPayloadError::PermissionDenied,
            ))
            .await?;
        let Some(authority_session) = authority_session else {
            return Err(CallError::Domain(HostSignPayloadError::V1(
                v01::HostSignPayloadError::Rejected,
            )));
        };
        let Some(owner) = self
            .accounts
            .authorized_product_account(
                &self.connection.product,
                &inner.account.dot_ns_identifier,
                cx,
            )
            .await
        else {
            return Err(CallError::Domain(HostSignPayloadError::V1(
                v01::HostSignPayloadError::PermissionDenied,
            )));
        };
        inner.account.dot_ns_identifier = owner;
        self.accounts
            .sign_payload(
                &authority_session,
                cx,
                AccountCaller::Local {
                    product: &self.connection.product,
                },
                SignPayloadAuthorityRequest::Product(inner),
            )
            .await
            .map(HostSignPayloadResponse::V1)
            .map_err(|reason| signing_call_error(HostSignPayloadError::V1, reason))
    }

    #[instrument(skip_all, fields(runtime.method = "signing.sign_raw"))]
    async fn sign_raw(
        &self,
        cx: &CallContext,
        request: HostSignRawRequest,
    ) -> Result<HostSignRawResponse, CallError<HostSignRawError>> {
        self.sign_raw_with_watermark(cx, request, true).await
    }

    #[instrument(skip_all, fields(runtime.method = "signing.sign_raw_unwatermarked_deprecated"))]
    async fn sign_raw_unwatermarked_deprecated(
        &self,
        cx: &CallContext,
        request: HostSignRawRequest,
    ) -> Result<HostSignRawResponse, CallError<HostSignRawError>> {
        tracing::warn!(
            "Temporary unwatermarked signing API is deprecated and will be removed: https://github.com/paritytech/trinity-user-agents/issues/612"
        );
        self.sign_raw_with_watermark(cx, request, false).await
    }

    #[instrument(skip_all, fields(runtime.method = "signing.create_transaction"))]
    async fn create_transaction(
        &self,
        cx: &CallContext,
        request: HostCreateTransactionRequest,
    ) -> Result<HostCreateTransactionResponse, CallError<HostCreateTransactionError>> {
        debug!("create_transaction: requesting signing-host signature");
        let HostCreateTransactionRequest::V1(mut inner) = request;
        inner.signer = Self::normalize_product_account_id(inner.signer).map_err(|()| {
            CallError::Domain(HostCreateTransactionError::V1(
                v01::HostCreateTransactionError::PermissionDenied,
            ))
        })?;
        let authority_session = self.accounts.current_session();
        self.connection
            .require_chain_submit(HostCreateTransactionError::V1(
                v01::HostCreateTransactionError::PermissionDenied,
            ))
            .await?;
        let Some(authority_session) = authority_session else {
            return Err(CallError::Domain(HostCreateTransactionError::V1(
                v01::HostCreateTransactionError::Rejected,
            )));
        };
        let Some(owner) = self
            .accounts
            .authorized_product_account(
                &self.connection.product,
                &inner.signer.dot_ns_identifier,
                cx,
            )
            .await
        else {
            return Err(CallError::Domain(HostCreateTransactionError::V1(
                v01::HostCreateTransactionError::PermissionDenied,
            )));
        };
        inner.signer.dot_ns_identifier = owner;
        inner.call_data = self
            .substitute_declared_contacts(inner.call_data, &inner.contacts)
            .await
            .map_err(|error| {
                let error = match error {
                    ContactResolutionError::Unsupported => {
                        v01::HostCreateTransactionError::NotSupported {
                            reason: "this host resolves no contacts".to_string(),
                        }
                    }
                    ContactResolutionError::NotConnected => {
                        v01::HostCreateTransactionError::PermissionDenied
                    }
                    ContactResolutionError::Host(reason) => {
                        v01::HostCreateTransactionError::Unknown { reason }
                    }
                    ContactResolutionError::UnknownContact => {
                        v01::HostCreateTransactionError::UnknownContact
                    }
                };
                CallError::Domain(HostCreateTransactionError::V1(error))
            })?;
        self.accounts
            .create_transaction(
                &authority_session,
                cx,
                AccountCaller::Local {
                    product: &self.connection.product,
                },
                CreateTransactionAuthorityRequest::Product(inner),
            )
            .await
            .map(HostCreateTransactionResponse::V1)
            .map_err(|reason| transaction_call_error(HostCreateTransactionError::V1, reason))
    }

    #[instrument(skip_all, fields(runtime.method = "signing.sign_payload_with_legacy_account"))]
    async fn sign_payload_with_legacy_account(
        &self,
        cx: &CallContext,
        request: HostSignPayloadWithLegacyAccountRequest,
    ) -> Result<
        HostSignPayloadWithLegacyAccountResponse,
        CallError<HostSignPayloadWithLegacyAccountError>,
    > {
        let HostSignPayloadWithLegacyAccountRequest::V1(inner) = request;
        let Some(authority_session) = self.accounts.current_session() else {
            return Err(CallError::Domain(
                HostSignPayloadWithLegacyAccountError::V1(v01::HostSignPayloadError::Rejected),
            ));
        };
        let signer = self
            .classify_legacy_address_signer(cx, &authority_session, &inner.signer)
            .await
            .map_err(|err| {
                CallError::Domain(HostSignPayloadWithLegacyAccountError::V1(
                    err.into_host_error(LEGACY_PRODUCT_ACCOUNT_MISMATCH_REASON),
                ))
            })?;
        if !matches!(signer, LegacySigner::Product) {
            return Err(CallError::Domain(
                HostSignPayloadWithLegacyAccountError::V1(v01::HostSignPayloadError::Unknown {
                    reason: LEGACY_PRODUCT_ACCOUNT_MISMATCH_REASON.to_string(),
                }),
            ));
        }
        self.connection
            .require_chain_submit(HostSignPayloadWithLegacyAccountError::V1(
                v01::HostSignPayloadError::PermissionDenied,
            ))
            .await?;
        self.accounts
            .sign_payload(
                &authority_session,
                cx,
                AccountCaller::Local {
                    product: &self.connection.product,
                },
                SignPayloadAuthorityRequest::LegacyAccount {
                    product_account: v01::ProductAccountId {
                        dot_ns_identifier: self.connection.product_id(),
                        derivation_index: v01::DerivationIndex::Index(0),
                    },
                    request: inner,
                },
            )
            .await
            .map(HostSignPayloadWithLegacyAccountResponse::V1)
            .map_err(|reason| signing_call_error(HostSignPayloadWithLegacyAccountError::V1, reason))
    }

    #[instrument(skip_all, fields(runtime.method = "signing.sign_raw_with_legacy_account"))]
    async fn sign_raw_with_legacy_account(
        &self,
        cx: &CallContext,
        request: HostSignRawWithLegacyAccountRequest,
    ) -> Result<HostSignRawWithLegacyAccountResponse, CallError<HostSignRawWithLegacyAccountError>>
    {
        self.sign_raw_with_legacy_account_with_watermark(cx, request, true)
            .await
    }

    #[instrument(skip_all, fields(runtime.method = "signing.sign_raw_unwatermarked_deprecated_with_legacy_account"))]
    async fn sign_raw_unwatermarked_deprecated_with_legacy_account(
        &self,
        cx: &CallContext,
        request: HostSignRawWithLegacyAccountRequest,
    ) -> Result<HostSignRawWithLegacyAccountResponse, CallError<HostSignRawWithLegacyAccountError>>
    {
        tracing::warn!(
            "Temporary unwatermarked signing API is deprecated and will be removed: https://github.com/paritytech/trinity-user-agents/issues/612"
        );
        self.sign_raw_with_legacy_account_with_watermark(cx, request, false)
            .await
    }

    #[instrument(skip_all, fields(runtime.method = "signing.create_transaction_with_legacy_account"))]
    async fn create_transaction_with_legacy_account(
        &self,
        cx: &CallContext,
        request: HostCreateTransactionWithLegacyAccountRequest,
    ) -> Result<
        HostCreateTransactionWithLegacyAccountResponse,
        CallError<HostCreateTransactionWithLegacyAccountError>,
    > {
        let HostCreateTransactionWithLegacyAccountRequest::V1(inner) = request;
        let Some(authority_session) = self.accounts.current_session() else {
            return Err(CallError::Domain(
                HostCreateTransactionWithLegacyAccountError::V1(
                    v01::HostCreateTransactionError::Rejected,
                ),
            ));
        };
        let signer = self
            .classify_legacy_signer(cx, &authority_session, inner.signer)
            .await
            .map_err(|err| {
                CallError::Domain(HostCreateTransactionWithLegacyAccountError::V1(
                    v01::HostCreateTransactionError::Unknown {
                        reason: err.into_reason(LEGACY_PRODUCT_ACCOUNT_MISMATCH_REASON),
                    },
                ))
            })?;
        self.connection
            .require_chain_submit(HostCreateTransactionWithLegacyAccountError::V1(
                v01::HostCreateTransactionError::PermissionDenied,
            ))
            .await?;
        let authority_request = match signer {
            LegacySigner::Product => CreateTransactionAuthorityRequest::LegacyAccount {
                product_account: v01::ProductAccountId {
                    dot_ns_identifier: self.connection.product_id(),
                    derivation_index: v01::DerivationIndex::Index(0),
                },
                request: inner,
            },
            LegacySigner::Identity(_) => CreateTransactionAuthorityRequest::IdentityAccount(inner),
        };
        self.accounts
            .create_transaction(
                &authority_session,
                cx,
                AccountCaller::Local {
                    product: &self.connection.product,
                },
                authority_request,
            )
            .await
            .map(|response| {
                HostCreateTransactionWithLegacyAccountResponse::V1(
                    v01::HostCreateTransactionWithLegacyAccountResponse {
                        transaction: response.transaction,
                    },
                )
            })
            .map_err(|reason| {
                transaction_call_error(HostCreateTransactionWithLegacyAccountError::V1, reason)
            })
    }
}

impl<H: AccountHolder> ProductRuntimeHost<H> {
    async fn sign_raw_with_watermark(
        &self,
        cx: &CallContext,
        request: HostSignRawRequest,
        watermarked: bool,
    ) -> Result<HostSignRawResponse, CallError<HostSignRawError>> {
        debug!("sign_raw: requesting signing-host signature");
        let HostSignRawRequest::V1(mut inner) = request;
        inner.account = Self::normalize_product_account_id(inner.account).map_err(|()| {
            CallError::Domain(HostSignRawError::V1(
                v01::HostSignPayloadError::PermissionDenied,
            ))
        })?;
        let authority_session = self.accounts.current_session();
        self.connection
            .require_chain_submit(HostSignRawError::V1(
                v01::HostSignPayloadError::PermissionDenied,
            ))
            .await?;
        let Some(authority_session) = authority_session else {
            return Err(CallError::Domain(HostSignRawError::V1(
                v01::HostSignPayloadError::Rejected,
            )));
        };
        let Some(owner) = self
            .accounts
            .authorized_product_account(
                &self.connection.product,
                &inner.account.dot_ns_identifier,
                cx,
            )
            .await
        else {
            return Err(CallError::Domain(HostSignRawError::V1(
                v01::HostSignPayloadError::PermissionDenied,
            )));
        };
        inner.account.dot_ns_identifier = owner;
        self.accounts
            .sign_raw(
                &authority_session,
                cx,
                AccountCaller::Local {
                    product: &self.connection.product,
                },
                SignRawAuthorityRequest::Product(inner),
                watermarked,
            )
            .await
            .map(HostSignRawResponse::V1)
            .map_err(|reason| raw_signing_call_error(HostSignRawError::V1, reason))
    }

    async fn sign_raw_with_legacy_account_with_watermark(
        &self,
        cx: &CallContext,
        request: HostSignRawWithLegacyAccountRequest,
        watermarked: bool,
    ) -> Result<HostSignRawWithLegacyAccountResponse, CallError<HostSignRawWithLegacyAccountError>>
    {
        let HostSignRawWithLegacyAccountRequest::V1(inner) = request;
        let Some(authority_session) = self.accounts.current_session() else {
            return Err(CallError::Domain(HostSignRawWithLegacyAccountError::V1(
                v01::HostSignPayloadError::Rejected,
            )));
        };
        let signer = self
            .classify_legacy_address_signer(cx, &authority_session, &inner.signer)
            .await
            .map_err(|err| {
                CallError::Domain(HostSignRawWithLegacyAccountError::V1(
                    err.into_host_error(LEGACY_ACCOUNT_UNAVAILABLE_REASON),
                ))
            })?;
        self.connection
            .require_chain_submit(HostSignRawWithLegacyAccountError::V1(
                v01::HostSignPayloadError::PermissionDenied,
            ))
            .await?;
        let authority_request = match signer {
            LegacySigner::Product => SignRawAuthorityRequest::LegacyAccount {
                product_account: v01::ProductAccountId {
                    dot_ns_identifier: self.connection.product_id(),
                    derivation_index: v01::DerivationIndex::Index(0),
                },
                request: inner,
            },
            LegacySigner::Identity(account) => SignRawAuthorityRequest::IdentityAccount {
                account,
                request: inner,
            },
        };
        self.accounts
            .sign_raw(
                &authority_session,
                cx,
                AccountCaller::Local {
                    product: &self.connection.product,
                },
                authority_request,
                watermarked,
            )
            .await
            .map(HostSignRawWithLegacyAccountResponse::V1)
            .map_err(|reason| raw_signing_call_error(HostSignRawWithLegacyAccountError::V1, reason))
    }
}

fn raw_signing_call_error<E>(
    wrap: fn(v01::HostSignPayloadError) -> E,
    error: AuthorityError,
) -> CallError<E> {
    match error {
        AuthorityError::ConfirmationFailed(error) => CallError::HostFailure {
            reason: format!("sign raw confirmation failed: {error:?}"),
        },
        error => signing_call_error(wrap, error),
    }
}
