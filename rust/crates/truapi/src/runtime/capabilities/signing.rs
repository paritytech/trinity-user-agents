//! Product-facing signing capability adapters.

use crate::platform::{
	CreateTransactionReview, CrossProductSignatureKind, SignPayloadReview, SignRawReview,
	UserConfirmationReview,
};
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
	ContactResolutionError, LEGACY_ACCOUNT_UNAVAILABLE_REASON,
	LEGACY_PRODUCT_ACCOUNT_MISMATCH_REASON, LegacySigner, ProductRuntimeHost,
	authority::{
		AuthorityError, AuthoritySession, AutoSigningGrant, CreateTransactionAuthorityRequest,
		SignPayloadAuthorityRequest, SignRawAuthorityRequest,
	},
	legacy_transaction_call_error, remote_authority_call, remote_authority_context,
	signing_call_error, transaction_call_error,
	versioning::version,
};

#[truapi::async_trait]
impl Signing for ProductRuntimeHost {
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
				let grant = self
					.auto_signing_status(&session, &inner.account)
					.await
					.map_err(signing_call_error)?;
				if grant == AutoSigningGrant::Absent {
					let review = UserConfirmationReview::SignPayload(SignPayloadReview::Product {
						calling_product_id: Some(self.connection.product_id().clone()),
						request: inner.clone(),
					});
					self.confirm_signature(cx, CrossProductSignatureKind::Payload, &owner, review)
						.await
						.map_err(signing_call_error)?;
				}
				let cx = remote_authority_context(cx);
				remote_authority_call(
					&cx,
					self.accounts.sign_payload(
						&session,
						&cx,
						SignPayloadAuthorityRequest::Product(inner),
					),
				)
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
				tracing::warn!(
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
				let names_contacts = !inner.contacts.is_empty();
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
				let grant = self
					.auto_signing_status(&session, &inner.signer)
					.await
					.map_err(transaction_call_error)?;
				if grant == AutoSigningGrant::Absent || names_contacts {
					let review = UserConfirmationReview::CreateTransaction(
						CreateTransactionReview::Product {
							calling_product_id: Some(self.connection.product_id().clone()),
							payload: inner.clone(),
						},
					);
					if names_contacts {
						self.confirm(cx, review).await
					} else {
						self.confirm_signature(
							cx,
							CrossProductSignatureKind::Transaction,
							&owner,
							review,
						)
						.await
					}
					.map_err(transaction_call_error)?;
				}
				let cx = remote_authority_context(cx);
				remote_authority_call(
					&cx,
					self.accounts.create_transaction(
						&session,
						&cx,
						CreateTransactionAuthorityRequest::Product(inner),
					),
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
				if !matches!(signer, LegacySigner::Product) {
					return Err(CallError::Domain(latest::HostSignPayloadError::Unknown {
						reason: LEGACY_PRODUCT_ACCOUNT_MISMATCH_REASON.to_string(),
					}));
				}
				self.connection
					.require_chain_submit(latest::HostSignPayloadError::PermissionDenied)
					.await?;
				let review = UserConfirmationReview::SignPayload(SignPayloadReview::LegacyAccount(
					inner.clone(),
				));
				self.confirm(cx, review).await.map_err(signing_call_error)?;
				let cx = remote_authority_context(cx);
				remote_authority_call(
					&cx,
					self.accounts.sign_payload(
						&session,
						&cx,
						SignPayloadAuthorityRequest::LegacyAccount {
							product_account: latest::ProductAccountId {
								dot_ns_identifier: self.connection.product_id().clone(),
								derivation_index: latest::DerivationIndex::Index(0),
							},
							request: inner,
						},
					),
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
				tracing::warn!(
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
				let review = UserConfirmationReview::CreateTransaction(
					CreateTransactionReview::LegacyAccount(inner.clone()),
				);
				self.confirm(cx, review).await.map_err(legacy_transaction_call_error)?;
				let cx = remote_authority_context(cx);
				let authority_request = match signer {
					LegacySigner::Product => CreateTransactionAuthorityRequest::LegacyAccount {
						product_account: latest::ProductAccountId {
							dot_ns_identifier: self.connection.product_id().clone(),
							derivation_index: latest::DerivationIndex::Index(0),
						},
						request: inner,
					},
					LegacySigner::Identity(_) => {
						CreateTransactionAuthorityRequest::IdentityAccount(inner)
					},
				};
				remote_authority_call(
					&cx,
					self.accounts.create_transaction(&session, &cx, authority_request),
				)
				.await
				.map(|response| latest::HostCreateTransactionWithLegacyAccountResponse {
					transaction: response.transaction,
				})
				.map_err(legacy_transaction_call_error)
			})
			.await
	}
}

impl ProductRuntimeHost {
	/// Whether the local authority can sign without confirmation.
	async fn auto_signing_status(
		&self,
		session: &AuthoritySession,
		account: &latest::ProductAccountId,
	) -> Result<AutoSigningGrant, AuthorityError> {
		self.accounts.auto_signing_status(session, account).await
	}

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
		let grant = if watermarked {
			self.auto_signing_status(&session, &inner.account)
				.await
				.map_err(signing_call_error)?
		} else {
			AutoSigningGrant::Absent
		};
		if grant == AutoSigningGrant::Absent {
			let review = UserConfirmationReview::SignRaw(SignRawReview::Product {
				calling_product_id: Some(self.connection.product_id().clone()),
				request: inner.clone(),
				watermarked,
			});
			if watermarked {
				self.confirm_signature(cx, CrossProductSignatureKind::Raw, &owner, review).await
			} else {
				self.confirm(cx, review).await
			}
			.map_err(raw_signing_call_error)?;
		}
		let cx = remote_authority_context(cx);
		remote_authority_call(
			&cx,
			self.accounts.sign_raw(
				&session,
				&cx,
				SignRawAuthorityRequest::Product(inner),
				watermarked,
			),
		)
		.await
		.map_err(signing_call_error)
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
		let review = UserConfirmationReview::SignRaw(SignRawReview::LegacyAccount {
			request: inner.clone(),
			watermarked,
		});
		self.confirm(cx, review).await.map_err(raw_signing_call_error)?;
		let cx = remote_authority_context(cx);
		let authority_request = match signer {
			LegacySigner::Product => SignRawAuthorityRequest::Product(latest::HostSignRawRequest {
				account: latest::ProductAccountId {
					dot_ns_identifier: self.connection.product_id().clone(),
					derivation_index: latest::DerivationIndex::Index(0),
				},
				payload: inner.payload,
			}),
			LegacySigner::Identity(account) => {
				SignRawAuthorityRequest::LegacyAccount { account, request: inner }
			},
		};
		remote_authority_call(
			&cx,
			self.accounts.sign_raw(&session, &cx, authority_request, watermarked),
		)
		.await
		.map_err(signing_call_error)
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
