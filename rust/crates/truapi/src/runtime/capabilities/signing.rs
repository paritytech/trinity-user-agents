//! Product-facing signing capability adapters.

use tracing::{debug, instrument};
use truapi::{
	CallContext, CallError,
	api::Signing,
	latest,
	versioned::signing::{
		HostCreateTransactionError, HostCreateTransactionRequest, HostCreateTransactionResponse,
		HostCreateTransactionWithLegacyAccountError, HostCreateTransactionWithLegacyAccountRequest,
		HostCreateTransactionWithLegacyAccountResponse, HostSignPayloadError,
		HostSignPayloadRequest, HostSignPayloadResponse, HostSignPayloadWithLegacyAccountError,
		HostSignPayloadWithLegacyAccountRequest, HostSignPayloadWithLegacyAccountResponse,
		HostSignRawError, HostSignRawRequest, HostSignRawResponse,
		HostSignRawWithLegacyAccountError, HostSignRawWithLegacyAccountRequest,
		HostSignRawWithLegacyAccountResponse,
	},
};

use crate::runtime::{
	AccountHolder, ContactResolutionError, LEGACY_ACCOUNT_UNAVAILABLE_REASON,
	LEGACY_PRODUCT_ACCOUNT_MISMATCH_REASON, LegacySigner, ProductRuntimeHost,
	authority::{
		AuthorityError, CreateTransactionAuthorityRequest, SignPayloadAuthorityRequest,
		SignRawAuthorityRequest,
	},
	legacy_transaction_call_error, signing_call_error, transaction_call_error,
	versioning::version,
};

#[async_trait::async_trait]
impl<H: AccountHolder> Signing for ProductRuntimeHost<H> {
	#[instrument(skip_all, fields(runtime.method = "signing.sign_payload"))]
	async fn sign_payload(
		&self,
		cx: &CallContext,
		request: HostSignPayloadRequest,
	) -> Result<HostSignPayloadResponse, CallError<HostSignPayloadError>> {
		version(request)
			.exec(|inner| async move {
				debug!("sign_payload: requesting signing-host signature");
				let owner = inner.account.dot_ns_identifier.clone();
				self.connection
					.require_chain_submit(latest::HostSignPayloadError::PermissionDenied)
					.await?;
				let session = self
					.accounts
					.current_session()
					.ok_or(CallError::Domain(latest::HostSignPayloadError::Rejected))?;
				if !self.may_use_product_account(&owner, cx).await {
					return Err(CallError::Domain(latest::HostSignPayloadError::PermissionDenied));
				}
				self.accounts
					.sign_payload(&session, cx, SignPayloadAuthorityRequest::Product(inner))
					.await
					.map_err(signing_call_error)
			})
			.await
	}

	#[instrument(skip_all, fields(runtime.method = "signing.sign_raw"))]
	async fn sign_raw(
		&self,
		cx: &CallContext,
		request: HostSignRawRequest,
	) -> Result<HostSignRawResponse, CallError<HostSignRawError>> {
		version(request)
			.exec(|request| async move { self.sign_raw_with_watermark(cx, request, true).await })
			.await
	}

	#[instrument(skip_all, fields(runtime.method = "signing.sign_raw_unwatermarked_deprecated"))]
	async fn sign_raw_unwatermarked_deprecated(
		&self,
		cx: &CallContext,
		request: HostSignRawRequest,
	) -> Result<HostSignRawResponse, CallError<HostSignRawError>> {
		version(request)
			.exec(|request| async move {
				tracing::info!(
					"Temporary unwatermarked signing API is deprecated and will be removed: https://github.com/paritytech/trinity-user-agents/issues/612"
				);
				self.sign_raw_with_watermark(cx, request, false).await
			})
			.await
	}

	#[instrument(skip_all, fields(runtime.method = "signing.create_transaction"))]
	async fn create_transaction(
		&self,
		cx: &CallContext,
		request: HostCreateTransactionRequest,
	) -> Result<HostCreateTransactionResponse, CallError<HostCreateTransactionError>> {
		version(request)
			.exec(|mut inner| async move {
				debug!("create_transaction: requesting signing-host signature");
				let owner = inner.signer.dot_ns_identifier.clone();
				self.connection
					.require_chain_submit(latest::HostCreateTransactionError::PermissionDenied)
					.await?;
				let session = self
					.accounts
					.current_session()
					.ok_or(CallError::Domain(latest::HostCreateTransactionError::Rejected))?;
				if !self.may_use_product_account(&owner, cx).await {
					return Err(CallError::Domain(
						latest::HostCreateTransactionError::PermissionDenied,
					));
				}
				inner.call_data = self
					.substitute_declared_contacts(inner.call_data, &inner.contacts)
					.await
					.map_err(|error| {
						let error = match error {
							ContactResolutionError::Unsupported => {
								latest::HostCreateTransactionError::NotSupported {
									reason: "this host resolves no contacts".to_string(),
								}
							},
							ContactResolutionError::NotConnected => {
								latest::HostCreateTransactionError::PermissionDenied
							},
							ContactResolutionError::Host(reason) => {
								latest::HostCreateTransactionError::Unknown { reason }
							},
							ContactResolutionError::UnknownContact => {
								latest::HostCreateTransactionError::UnknownContact
							},
						};
						CallError::Domain(error)
					})?;
				self.accounts
					.create_transaction(
						&session,
						cx,
						CreateTransactionAuthorityRequest::Product(inner),
					)
					.await
					.map_err(transaction_call_error)
			})
			.await
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
		version(request)
			.exec(|inner| async move {
				let session = self
					.accounts
					.current_session()
					.ok_or(CallError::Domain(latest::HostSignPayloadError::Rejected))?;
				let signer = self
					.classify_legacy_address_signer(cx, &session, &inner.signer)
					.await
					.map_err(|err| {
						CallError::Domain(
							err.into_host_error(LEGACY_PRODUCT_ACCOUNT_MISMATCH_REASON),
						)
					})?;
				let LegacySigner::Product(product_account) = signer else {
					return Err(CallError::Domain(latest::HostSignPayloadError::Unknown {
						reason: LEGACY_PRODUCT_ACCOUNT_MISMATCH_REASON.to_string(),
					}));
				};
				self.connection
					.require_chain_submit(latest::HostSignPayloadError::PermissionDenied)
					.await?;
				self.accounts
					.sign_payload(
						&session,
						cx,
						SignPayloadAuthorityRequest::LegacyAccount {
							product_account,
							request: inner,
						},
					)
					.await
					.map_err(signing_call_error)
			})
			.await
	}

	#[instrument(skip_all, fields(runtime.method = "signing.sign_raw_with_legacy_account"))]
	async fn sign_raw_with_legacy_account(
		&self,
		cx: &CallContext,
		request: HostSignRawWithLegacyAccountRequest,
	) -> Result<HostSignRawWithLegacyAccountResponse, CallError<HostSignRawWithLegacyAccountError>>
	{
		version(request)
			.exec(|request| async move {
				self.sign_raw_with_legacy_account_with_watermark(cx, request, true).await
			})
			.await
	}

	#[instrument(skip_all, fields(runtime.method = "signing.sign_raw_unwatermarked_deprecated_with_legacy_account"))]
	async fn sign_raw_unwatermarked_deprecated_with_legacy_account(
		&self,
		cx: &CallContext,
		request: HostSignRawWithLegacyAccountRequest,
	) -> Result<HostSignRawWithLegacyAccountResponse, CallError<HostSignRawWithLegacyAccountError>>
	{
		version(request)
			.exec(|request| async move {
				tracing::info!(
					"Temporary unwatermarked signing API is deprecated and will be removed: https://github.com/paritytech/trinity-user-agents/issues/612"
				);
				self.sign_raw_with_legacy_account_with_watermark(cx, request, false).await
			})
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
		version(request)
			.exec(|inner| async move {
				let session = self.accounts.current_session().ok_or(CallError::Domain(
					latest::HostCreateTransactionWithLegacyAccountError::Rejected,
				))?;
				let signer = self
					.classify_legacy_signer(cx, &session, inner.signer)
					.await
					.map_err(|err| {
						CallError::Domain(
							latest::HostCreateTransactionWithLegacyAccountError::Unknown {
								reason: err.into_reason(LEGACY_PRODUCT_ACCOUNT_MISMATCH_REASON),
							},
						)
					})?;
				self.connection
					.require_chain_submit(
						latest::HostCreateTransactionWithLegacyAccountError::PermissionDenied,
					)
					.await?;
				let authority_request = match signer {
					LegacySigner::Product(product_account) => {
						CreateTransactionAuthorityRequest::LegacyAccount {
							product_account,
							request: inner,
						}
					},
					LegacySigner::Identity(_) => {
						CreateTransactionAuthorityRequest::IdentityAccount(inner)
					},
				};
				self.accounts
					.create_transaction(&session, cx, authority_request)
					.await
					.map(|response| latest::HostCreateTransactionWithLegacyAccountResponse {
						transaction: response.transaction,
					})
					.map_err(legacy_transaction_call_error)
			})
			.await
	}
}

impl<H: AccountHolder> ProductRuntimeHost<H> {
	async fn sign_raw_with_watermark(
		&self,
		cx: &CallContext,
		request: latest::HostSignRawRequest,
		watermarked: bool,
	) -> Result<latest::HostSignPayloadResponse, CallError<latest::HostSignPayloadError>> {
		debug!("sign_raw: requesting signing-host signature");
		let inner = request;
		let owner = inner.account.dot_ns_identifier.clone();
		self.connection
			.require_chain_submit(latest::HostSignPayloadError::PermissionDenied)
			.await?;
		let session = self
			.accounts
			.current_session()
			.ok_or(CallError::Domain(latest::HostSignPayloadError::Rejected))?;
		if !self.may_use_product_account(&owner, cx).await {
			return Err(CallError::Domain(latest::HostSignPayloadError::PermissionDenied));
		}
		self.accounts
			.sign_raw(&session, cx, SignRawAuthorityRequest::Product(inner), watermarked)
			.await
			.map_err(raw_signing_call_error)
	}

	async fn sign_raw_with_legacy_account_with_watermark(
		&self,
		cx: &CallContext,
		request: latest::HostSignRawWithLegacyAccountRequest,
		watermarked: bool,
	) -> Result<latest::HostSignPayloadResponse, CallError<latest::HostSignPayloadError>> {
		let inner = request;
		let session = self
			.accounts
			.current_session()
			.ok_or(CallError::Domain(latest::HostSignPayloadError::Rejected))?;
		let signer =
			self.classify_legacy_address_signer(cx, &session, &inner.signer).await.map_err(
				|err| CallError::Domain(err.into_host_error(LEGACY_ACCOUNT_UNAVAILABLE_REASON)),
			)?;
		self.connection
			.require_chain_submit(latest::HostSignPayloadError::PermissionDenied)
			.await?;
		let authority_request = match signer {
			LegacySigner::Product(product_account) => {
				SignRawAuthorityRequest::LegacyAccount { product_account, request: inner }
			},
			LegacySigner::Identity(account) => {
				SignRawAuthorityRequest::IdentityAccount { account, request: inner }
			},
		};
		self.accounts
			.sign_raw(&session, cx, authority_request, watermarked)
			.await
			.map_err(raw_signing_call_error)
	}
}

fn raw_signing_call_error(error: AuthorityError) -> CallError<latest::HostSignPayloadError> {
	match error {
		AuthorityError::ConfirmationFailed(error) => {
			CallError::HostFailure { reason: format!("sign raw confirmation failed: {error:?}") }
		},
		error => signing_call_error(error),
	}
}
