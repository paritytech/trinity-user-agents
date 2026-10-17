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

use super::from_latest_error;
use truapi::versioned::{FromLatest, IntoLatest, Versioned};

use crate::runtime::{
	AccountHolder, ContactResolutionError, LEGACY_ACCOUNT_UNAVAILABLE_REASON,
	LEGACY_PRODUCT_ACCOUNT_MISMATCH_REASON, LegacySigner, ProductRuntimeHost,
	authority::{
		AuthorityError, AuthoritySession, CreateTransactionAuthorityRequest,
		SignPayloadAuthorityRequest, SignRawAuthorityRequest,
	},
	signing_call_error, transaction_call_error,
};

#[truapi::async_trait]
impl<H: AccountHolder> Signing for ProductRuntimeHost<H> {
	#[instrument(skip_all, fields(runtime.method = "signing.sign_payload"))]
	async fn sign_payload(
		&self,
		cx: &CallContext,
		request: HostSignPayloadRequest,
	) -> Result<HostSignPayloadResponse, CallError<HostSignPayloadError>> {
		let version = request.version();
		let mut inner = request.into_latest();
		let result = async {
			debug!("sign_payload: requesting signing-host signature");
			let (authority_session, account) = self
				.product_signer(
					cx,
					inner.account,
					|| latest::HostSignPayloadError::PermissionDenied,
					latest::HostSignPayloadError::Rejected,
				)
				.await?;
			inner.account = account;
			self.accounts
				.sign_payload(
					&authority_session,
					cx,
					&self.connection.temporary_permissions,
					SignPayloadAuthorityRequest::Product(inner),
				)
				.await
				.map_err(signing_call_error)
		}
		.await;
		result
			.map(|response| HostSignPayloadResponse::from_latest(response, version))
			.map_err(|error| from_latest_error::<HostSignPayloadError>(error, version))
	}

	#[instrument(skip_all, fields(runtime.method = "signing.sign_raw"))]
	async fn sign_raw(
		&self,
		cx: &CallContext,
		request: HostSignRawRequest,
	) -> Result<HostSignRawResponse, CallError<HostSignRawError>> {
		let version = request.version();
		let request = request.into_latest();
		let result = self.sign_raw_with_watermark(cx, request, true).await;
		result
			.map(|response| HostSignRawResponse::from_latest(response, version))
			.map_err(|error| from_latest_error::<HostSignRawError>(error, version))
	}

	#[instrument(skip_all, fields(runtime.method = "signing.sign_raw_unwatermarked_deprecated"))]
	async fn sign_raw_unwatermarked_deprecated(
		&self,
		cx: &CallContext,
		request: HostSignRawRequest,
	) -> Result<HostSignRawResponse, CallError<HostSignRawError>> {
		let version = request.version();
		let request = request.into_latest();
		tracing::warn!(
			"Temporary unwatermarked signing API is deprecated and will be removed: https://github.com/paritytech/trinity-user-agents/issues/612"
		);
		let result = self.sign_raw_with_watermark(cx, request, false).await;
		result
			.map(|response| HostSignRawResponse::from_latest(response, version))
			.map_err(|error| from_latest_error::<HostSignRawError>(error, version))
	}

	#[instrument(skip_all, fields(runtime.method = "signing.create_transaction"))]
	async fn create_transaction(
		&self,
		cx: &CallContext,
		request: HostCreateTransactionRequest,
	) -> Result<HostCreateTransactionResponse, CallError<HostCreateTransactionError>> {
		let version = request.version();
		let mut inner = request.into_latest();
		let result = async {
			debug!("create_transaction: requesting signing-host signature");
			let (authority_session, signer) = self
				.product_signer(
					cx,
					inner.signer,
					|| latest::HostCreateTransactionError::PermissionDenied,
					latest::HostCreateTransactionError::Rejected,
				)
				.await?;
			inner.signer = signer;
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
					&authority_session,
					cx,
					&self.connection.temporary_permissions,
					CreateTransactionAuthorityRequest::Product(inner),
				)
				.await
				.map_err(transaction_call_error)
		}
		.await;
		result
			.map(|response| HostCreateTransactionResponse::from_latest(response, version))
			.map_err(|error| from_latest_error::<HostCreateTransactionError>(error, version))
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
		let version = request.version();
		let inner = request.into_latest();
		let result = async {
			let Some(authority_session) = self.accounts.current_session() else {
				return Err(CallError::Domain(latest::HostSignPayloadError::Rejected));
			};
			let signer = self
				.classify_legacy_address_signer(cx, &authority_session, &inner.signer)
				.await
				.map_err(|err| {
					CallError::Domain(err.into_host_error(LEGACY_PRODUCT_ACCOUNT_MISMATCH_REASON))
				})?;
			if !matches!(signer, LegacySigner::Product) {
				return Err(CallError::Domain(latest::HostSignPayloadError::Unknown {
					reason: LEGACY_PRODUCT_ACCOUNT_MISMATCH_REASON.to_string(),
				}));
			}
			self.connection
				.require_chain_submit(latest::HostSignPayloadError::PermissionDenied)
				.await?;
			self.accounts
				.sign_payload(
					&authority_session,
					cx,
					&self.connection.temporary_permissions,
					SignPayloadAuthorityRequest::LegacyAccount {
						product_account: latest::ProductAccountId {
							dot_ns_identifier: self.connection.product_id(),
							derivation_index: latest::DerivationIndex::Index(0),
						},
						request: inner,
					},
				)
				.await
				.map_err(signing_call_error)
		}
		.await;
		result
			.map(|response| {
				HostSignPayloadWithLegacyAccountResponse::from_latest(response, version)
			})
			.map_err(|error| {
				from_latest_error::<HostSignPayloadWithLegacyAccountError>(error, version)
			})
	}

	#[instrument(skip_all, fields(runtime.method = "signing.sign_raw_with_legacy_account"))]
	async fn sign_raw_with_legacy_account(
		&self,
		cx: &CallContext,
		request: HostSignRawWithLegacyAccountRequest,
	) -> Result<HostSignRawWithLegacyAccountResponse, CallError<HostSignRawWithLegacyAccountError>>
	{
		let version = request.version();
		let request = request.into_latest();
		let result = self.sign_raw_with_legacy_account_with_watermark(cx, request, true).await;
		result
			.map(|response| HostSignRawWithLegacyAccountResponse::from_latest(response, version))
			.map_err(|error| from_latest_error::<HostSignRawWithLegacyAccountError>(error, version))
	}

	#[instrument(skip_all, fields(runtime.method = "signing.sign_raw_unwatermarked_deprecated_with_legacy_account"))]
	async fn sign_raw_unwatermarked_deprecated_with_legacy_account(
		&self,
		cx: &CallContext,
		request: HostSignRawWithLegacyAccountRequest,
	) -> Result<HostSignRawWithLegacyAccountResponse, CallError<HostSignRawWithLegacyAccountError>>
	{
		let version = request.version();
		let request = request.into_latest();
		tracing::warn!(
			"Temporary unwatermarked signing API is deprecated and will be removed: https://github.com/paritytech/trinity-user-agents/issues/612"
		);
		let result = self.sign_raw_with_legacy_account_with_watermark(cx, request, false).await;
		result
			.map(|response| HostSignRawWithLegacyAccountResponse::from_latest(response, version))
			.map_err(|error| from_latest_error::<HostSignRawWithLegacyAccountError>(error, version))
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
		let version = request.version();
		let inner = request.into_latest();
		let result = async {
			let Some(authority_session) = self.accounts.current_session() else {
				return Err(CallError::Domain(latest::HostCreateTransactionError::Rejected));
			};
			let signer = self
				.classify_legacy_signer(cx, &authority_session, inner.signer)
				.await
				.map_err(|err| {
					CallError::Domain(latest::HostCreateTransactionError::Unknown {
						reason: err.into_reason(LEGACY_PRODUCT_ACCOUNT_MISMATCH_REASON),
					})
				})?;
			self.connection
				.require_chain_submit(latest::HostCreateTransactionError::PermissionDenied)
				.await?;
			let authority_request = match signer {
				LegacySigner::Product => CreateTransactionAuthorityRequest::LegacyAccount {
					product_account: latest::ProductAccountId {
						dot_ns_identifier: self.connection.product_id(),
						derivation_index: latest::DerivationIndex::Index(0),
					},
					request: inner,
				},
				LegacySigner::Identity(_) => {
					CreateTransactionAuthorityRequest::IdentityAccount(inner)
				},
			};
			self.accounts
				.create_transaction(
					&authority_session,
					cx,
					&self.connection.temporary_permissions,
					authority_request,
				)
				.await
				.map(|response| latest::HostCreateTransactionWithLegacyAccountResponse {
					transaction: response.transaction,
				})
				.map_err(transaction_call_error)
		}
		.await;
		result
			.map(|response| {
				HostCreateTransactionWithLegacyAccountResponse::from_latest(response, version)
			})
			.map_err(|error| {
				from_latest_error::<HostCreateTransactionWithLegacyAccountError>(error, version)
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
			.authorized_product_account(&account.dot_ns_identifier, cx)
			.await
			.ok_or_else(|| CallError::Domain(denied()))?;
		Ok((authority_session, account))
	}

	async fn sign_raw_with_watermark(
		&self,
		cx: &CallContext,
		request: latest::HostSignRawRequest,
		watermarked: bool,
	) -> Result<latest::HostSignPayloadResponse, CallError<latest::HostSignPayloadError>> {
		debug!("sign_raw: requesting signing-host signature");
		let mut inner = request;
		let (authority_session, account) = self
			.product_signer(
				cx,
				inner.account,
				|| latest::HostSignPayloadError::PermissionDenied,
				latest::HostSignPayloadError::Rejected,
			)
			.await?;
		inner.account = account;
		self.accounts
			.sign_raw(
				&authority_session,
				cx,
				&self.connection.temporary_permissions,
				SignRawAuthorityRequest::Product(inner),
				watermarked,
			)
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
		let Some(authority_session) = self.accounts.current_session() else {
			return Err(CallError::Domain(latest::HostSignPayloadError::Rejected));
		};
		let signer = self
			.classify_legacy_address_signer(cx, &authority_session, &inner.signer)
			.await
			.map_err(|err| {
				CallError::Domain(err.into_host_error(LEGACY_ACCOUNT_UNAVAILABLE_REASON))
			})?;
		self.connection
			.require_chain_submit(latest::HostSignPayloadError::PermissionDenied)
			.await?;
		let authority_request = match signer {
			LegacySigner::Product => SignRawAuthorityRequest::LegacyAccount {
				product_account: latest::ProductAccountId {
					dot_ns_identifier: self.connection.product_id(),
					derivation_index: latest::DerivationIndex::Index(0),
				},
				request: inner,
			},
			LegacySigner::Identity(account) => {
				SignRawAuthorityRequest::IdentityAccount { account, request: inner }
			},
		};
		self.accounts
			.sign_raw(
				&authority_session,
				cx,
				&self.connection.temporary_permissions,
				authority_request,
				watermarked,
			)
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
