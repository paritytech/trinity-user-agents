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
use truapi::{CallContext, CallError, latest, v01};

use crate::runtime::authority::{
    AuthorityError, AuthoritySession, CreateTransactionAuthorityRequest,
    SignPayloadAuthorityRequest, SignRawAuthorityRequest,
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
        let (authority_session, account) = self
            .product_signer(
                cx,
                inner.account,
                || HostSignPayloadError::V1(v01::HostSignPayloadError::PermissionDenied),
                HostSignPayloadError::V1(v01::HostSignPayloadError::Rejected),
            )
            .await?;
        inner.account = account;
        self.accounts
            .sign_payload(
                &authority_session,
                cx,
                &self.connection.product,
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
        let (authority_session, signer) = self
            .product_signer(
                cx,
                inner.signer,
                || {
                    HostCreateTransactionError::V1(
                        v01::HostCreateTransactionError::PermissionDenied,
                    )
                },
                HostCreateTransactionError::V1(v01::HostCreateTransactionError::Rejected),
            )
            .await?;
        inner.signer = signer;
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
                &self.connection.product,
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
                &self.connection.product,
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
                &self.connection.product,
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
    /// The session to sign under and the product account a request names, once
    /// the caller may submit to chain and its manifest grants that account.
    async fn product_signer<E>(
        &self,
        cx: &CallContext,
        account: latest::ProductAccountId,
        denied: impl Fn() -> E,
        rejected: E,
    ) -> Result<(AuthoritySession, latest::ProductAccountId), CallError<E>> {
        let mut account = Self::normalize_product_account_id(account)
            .map_err(|()| CallError::Domain(denied()))?;
        let authority_session = self.accounts.current_session();
        self.connection.require_chain_submit(denied()).await?;
        let authority_session = authority_session.ok_or(CallError::Domain(rejected))?;
        account.dot_ns_identifier = self
            .accounts
            .authorized_product_account(&self.connection.product, &account.dot_ns_identifier, cx)
            .await
            .ok_or_else(|| CallError::Domain(denied()))?;
        Ok((authority_session, account))
    }

    async fn sign_raw_with_watermark(
        &self,
        cx: &CallContext,
        request: HostSignRawRequest,
        watermarked: bool,
    ) -> Result<HostSignRawResponse, CallError<HostSignRawError>> {
        debug!("sign_raw: requesting signing-host signature");
        let HostSignRawRequest::V1(mut inner) = request;
        let (authority_session, account) = self
            .product_signer(
                cx,
                inner.account,
                || HostSignRawError::V1(v01::HostSignPayloadError::PermissionDenied),
                HostSignRawError::V1(v01::HostSignPayloadError::Rejected),
            )
            .await?;
        inner.account = account;
        self.accounts
            .sign_raw(
                &authority_session,
                cx,
                &self.connection.product,
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
                &self.connection.product,
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
