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

use super::from_latest_error;
use truapi::versioned::{FromLatest, IntoLatest, Versioned};

use crate::runtime::{
	ContactResolutionError, LEGACY_ACCOUNT_UNAVAILABLE_REASON,
	LEGACY_PRODUCT_ACCOUNT_MISMATCH_REASON, LegacySigner, ProductRuntimeHost,
	authority::{
		AuthorityError, AuthoritySession, AutoSigningGrant, CreateTransactionAuthorityRequest,
		SignPayloadAuthorityRequest, SignRawAuthorityRequest,
	},
	remote_authority_call, remote_authority_context, signing_call_error, transaction_call_error,
	until_cancelled,
};

#[truapi::async_trait]
impl Signing for ProductRuntimeHost {
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
			inner.account = Self::normalize_product_account_id(inner.account)
				.map_err(|()| CallError::Domain(latest::HostSignPayloadError::PermissionDenied))?;
			self.require_chain_submit(latest::HostSignPayloadError::PermissionDenied)
				.await?;
			let Some(session) = self.authority.current_session() else {
				return Err(CallError::Domain(latest::HostSignPayloadError::Rejected));
			};
			let Some(owner) =
				self.authorized_product_account(&inner.account.dot_ns_identifier, cx).await
			else {
				return Err(CallError::Domain(latest::HostSignPayloadError::PermissionDenied));
			};
			inner.account.dot_ns_identifier = owner;
			let grant = self
				.auto_signing_status(&session, &inner.account)
				.await
				.map_err(signing_call_error)?;
			if grant == AutoSigningGrant::Absent {
				let confirmed = until_cancelled(
					cx,
					self.confirm_signature(
						CrossProductSignatureKind::Payload,
						&inner.account,
						|| {
							UserConfirmationReview::SignPayload(SignPayloadReview::Product {
								calling_product_id: Some(self.product_id()),
								request: inner.clone(),
							})
						},
					),
				)
				.await
				.map_err(signing_call_error)?
				.map_err(|err| CallError::HostFailure {
					reason: format!("sign payload confirmation failed: {err:?}"),
				})?;
				if !confirmed {
					return Err(CallError::Domain(latest::HostSignPayloadError::Rejected));
				}
			}
			let cx = remote_authority_context(cx);
			remote_authority_call(
				&cx,
				self.authority.sign_payload(
					&cx,
					&session,
					Some(self.product_id().as_str()),
					SignPayloadAuthorityRequest::Product(inner),
				),
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
			inner.signer = Self::normalize_product_account_id(inner.signer).map_err(|()| {
				CallError::Domain(latest::HostCreateTransactionError::PermissionDenied)
			})?;
			self.require_chain_submit(latest::HostCreateTransactionError::PermissionDenied)
				.await?;
			let Some(session) = self.authority.current_session() else {
				return Err(CallError::Domain(latest::HostCreateTransactionError::Rejected));
			};
			let Some(owner) =
				self.authorized_product_account(&inner.signer.dot_ns_identifier, cx).await
			else {
				return Err(CallError::Domain(
					latest::HostCreateTransactionError::PermissionDenied,
				));
			};
			inner.signer.dot_ns_identifier = owner;
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
				let review = || {
					UserConfirmationReview::CreateTransaction(CreateTransactionReview::Product {
						calling_product_id: Some(self.product_id()),
						payload: inner.clone(),
					})
				};
				let confirmed = until_cancelled(cx, async {
					if names_contacts {
						self.confirm_product_action(review()).await
					} else {
						self.confirm_signature(
							CrossProductSignatureKind::Transaction,
							&inner.signer,
							review,
						)
						.await
					}
				})
				.await
				.map_err(transaction_call_error)?
				.map_err(|err| CallError::HostFailure {
					reason: format!("create transaction confirmation failed: {err:?}"),
				})?;
				if !confirmed {
					return Err(CallError::Domain(latest::HostCreateTransactionError::Rejected));
				}
			}
			let cx = remote_authority_context(cx);
			remote_authority_call(
				&cx,
				self.authority.create_transaction(
					&cx,
					&session,
					Some(self.product_id().as_str()),
					CreateTransactionAuthorityRequest::Product(inner),
				),
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
			let Some(session) = self.authority.current_session() else {
				return Err(CallError::Domain(latest::HostSignPayloadError::Rejected));
			};
			let signer = self
				.classify_legacy_address_signer(cx, &session, &inner.signer)
				.await
				.map_err(|err| {
					CallError::Domain(err.into_host_error(LEGACY_PRODUCT_ACCOUNT_MISMATCH_REASON))
				})?;
			if !matches!(signer, LegacySigner::Product) {
				return Err(CallError::Domain(latest::HostSignPayloadError::Unknown {
					reason: LEGACY_PRODUCT_ACCOUNT_MISMATCH_REASON.to_string(),
				}));
			}
			self.require_chain_submit(latest::HostSignPayloadError::PermissionDenied)
				.await?;
			let confirmed = until_cancelled(
				cx,
				self.platform.confirm_user_action(UserConfirmationReview::SignPayload(
					SignPayloadReview::LegacyAccount(inner.clone()),
				)),
			)
			.await
			.map_err(signing_call_error)?
			.map_err(|err| CallError::HostFailure {
				reason: format!("sign payload confirmation failed: {err:?}"),
			})?;
			if !confirmed {
				return Err(CallError::Domain(latest::HostSignPayloadError::Rejected));
			}
			let cx = remote_authority_context(cx);
			remote_authority_call(
				&cx,
				self.authority.sign_payload(
					&cx,
					&session,
					Some(self.product_id().as_str()),
					SignPayloadAuthorityRequest::LegacyAccount {
						product_account: latest::ProductAccountId {
							dot_ns_identifier: self.product_id(),
							derivation_index: latest::DerivationIndex::Index(0),
						},
						request: inner,
					},
				),
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
			let Some(session) = self.authority.current_session() else {
				return Err(CallError::Domain(latest::HostCreateTransactionError::Rejected));
			};
			let signer =
				self.classify_legacy_signer(cx, &session, inner.signer).await.map_err(|err| {
					CallError::Domain(latest::HostCreateTransactionError::Unknown {
						reason: err.into_reason(LEGACY_PRODUCT_ACCOUNT_MISMATCH_REASON),
					})
				})?;
			self.require_chain_submit(latest::HostCreateTransactionError::PermissionDenied)
				.await?;
			let confirmed = until_cancelled(
				cx,
				self.platform.confirm_user_action(UserConfirmationReview::CreateTransaction(
					CreateTransactionReview::LegacyAccount(inner.clone()),
				)),
			)
			.await
			.map_err(transaction_call_error)?
			.map_err(|err| CallError::HostFailure {
				reason: format!("create transaction confirmation failed: {err:?}"),
			})?;
			if !confirmed {
				return Err(CallError::Domain(latest::HostCreateTransactionError::Rejected));
			}
			let cx = remote_authority_context(cx);
			let authority_request = match signer {
				LegacySigner::Product => CreateTransactionAuthorityRequest::LegacyAccount {
					product_account: latest::ProductAccountId {
						dot_ns_identifier: self.product_id(),
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
				self.authority.create_transaction(
					&cx,
					&session,
					Some(self.product_id().as_str()),
					authority_request,
				),
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

impl ProductRuntimeHost {
	/// Whether the local authority can sign without confirmation.
	async fn auto_signing_status(
		&self,
		session: &AuthoritySession,
		account: &latest::ProductAccountId,
	) -> Result<AutoSigningGrant, AuthorityError> {
		self.authority.auto_signing_status(session, &self.product_id(), account).await
	}

	async fn sign_raw_with_watermark(
		&self,
		cx: &CallContext,
		request: latest::HostSignRawRequest,
		watermarked: bool,
	) -> Result<latest::HostSignPayloadResponse, CallError<latest::HostSignPayloadError>> {
		debug!("sign_raw: requesting signing-host signature");
		let mut inner = request;
		inner.account = Self::normalize_product_account_id(inner.account)
			.map_err(|()| CallError::Domain(latest::HostSignPayloadError::PermissionDenied))?;
		self.require_chain_submit(latest::HostSignPayloadError::PermissionDenied)
			.await?;
		let Some(session) = self.authority.current_session() else {
			return Err(CallError::Domain(latest::HostSignPayloadError::Rejected));
		};
		let Some(owner) =
			self.authorized_product_account(&inner.account.dot_ns_identifier, cx).await
		else {
			return Err(CallError::Domain(latest::HostSignPayloadError::PermissionDenied));
		};
		inner.account.dot_ns_identifier = owner;
		let grant = if watermarked {
			self.auto_signing_status(&session, &inner.account)
				.await
				.map_err(signing_call_error)?
		} else {
			AutoSigningGrant::Absent
		};
		if grant == AutoSigningGrant::Absent {
			let review = || {
				UserConfirmationReview::SignRaw(SignRawReview::Product {
					calling_product_id: Some(self.product_id()),
					request: inner.clone(),
					watermarked,
				})
			};
			let confirmed = until_cancelled(cx, async {
				if watermarked {
					self.confirm_signature(CrossProductSignatureKind::Raw, &inner.account, review)
						.await
				} else {
					self.confirm_product_action(review()).await
				}
			})
			.await
			.map_err(signing_call_error)?
			.map_err(|err| CallError::HostFailure {
				reason: format!("sign raw confirmation failed: {err:?}"),
			})?;
			if !confirmed {
				return Err(CallError::Domain(latest::HostSignPayloadError::Rejected));
			}
		}
		let cx = remote_authority_context(cx);
		remote_authority_call(
			&cx,
			self.authority.sign_raw(
				&cx,
				&session,
				Some(self.product_id().as_str()),
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
		let Some(session) = self.authority.current_session() else {
			return Err(CallError::Domain(latest::HostSignPayloadError::Rejected));
		};
		let signer =
			self.classify_legacy_address_signer(cx, &session, &inner.signer).await.map_err(
				|err| CallError::Domain(err.into_host_error(LEGACY_ACCOUNT_UNAVAILABLE_REASON)),
			)?;
		self.require_chain_submit(latest::HostSignPayloadError::PermissionDenied)
			.await?;
		let confirmed = until_cancelled(
			cx,
			self.platform.confirm_user_action(UserConfirmationReview::SignRaw(
				SignRawReview::LegacyAccount { request: inner.clone(), watermarked },
			)),
		)
		.await
		.map_err(signing_call_error)?
		.map_err(|err| CallError::HostFailure {
			reason: format!("sign raw confirmation failed: {err:?}"),
		})?;
		if !confirmed {
			return Err(CallError::Domain(latest::HostSignPayloadError::Rejected));
		}
		let cx = remote_authority_context(cx);
		let authority_request = match signer {
			LegacySigner::Product => SignRawAuthorityRequest::Product(latest::HostSignRawRequest {
				account: latest::ProductAccountId {
					dot_ns_identifier: self.product_id(),
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
			self.authority.sign_raw(
				&cx,
				&session,
				Some(self.product_id().as_str()),
				authority_request,
				watermarked,
			),
		)
		.await
		.map_err(signing_call_error)
	}
}
