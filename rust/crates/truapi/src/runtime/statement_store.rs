//! `StatementStore` surface: session-key statement proofs plus submit and
//! subscribe flows over the people-chain statement store.

use core::{
	pin::Pin,
	task::{Context, Poll},
};

use futures::StreamExt as _;

use super::{
	PERMISSION_DENIED_REASON, ProductRuntimeHost,
	authority::{AuthorityError, StatementStoreAllowanceKey},
	remote_authority_call, remote_authority_context,
	statement_store_rpc::{self, StatementStoreRpc},
	versioning::version,
};
use crate::host_logic::statement_store::{
	decode_signed_statement, parse_new_statements_result, sign_statement_fields,
	signed_statement_to_scale, statement_fields_from_v01, statement_proof_to_v01,
	unsigned_statement_signing_payload, validated_topic_filter,
};

use crate::platform::{
	CrossProductSignatureKind, ProductId, StatementStoreProductSignReview, UserConfirmationReview,
};
use serde_json::Value;
use subxt_rpcs::client::RpcSubscription;
use tracing::instrument;
use truapi::{
	CallContext, CallError, Subscription,
	api::StatementStore,
	latest,
	versioned::statement_store::{
		RemoteStatementStoreCreateProofAuthorizedError,
		RemoteStatementStoreCreateProofAuthorizedRequest,
		RemoteStatementStoreCreateProofAuthorizedResponse, RemoteStatementStoreCreateProofError,
		RemoteStatementStoreCreateProofRequest, RemoteStatementStoreCreateProofResponse,
		RemoteStatementStoreSubmitError, RemoteStatementStoreSubmitRequest,
		RemoteStatementStoreSubmitResponse, RemoteStatementStoreSubscribeError,
		RemoteStatementStoreSubscribeItem, RemoteStatementStoreSubscribeRequest,
	},
};

#[async_trait::async_trait]
impl StatementStore for ProductRuntimeHost {
	#[instrument(skip_all, fields(runtime.method = "statement_store.subscribe"))]
	async fn subscribe(
		&self,
		_cx: &CallContext,
		request: RemoteStatementStoreSubscribeRequest,
	) -> Subscription<
		RemoteStatementStoreSubscribeItem,
		CallError<RemoteStatementStoreSubscribeError>,
	> {
		version(request)
			.exec(|request| async move {
				match self.open_statement_subscription(request).await {
					Ok(subscription) => subscription,
					Err(error) => Subscription::interrupted(error),
				}
			})
			.await
	}

	#[instrument(skip_all, fields(runtime.method = "statement_store.create_proof"))]
	async fn create_proof(
		&self,
		cx: &CallContext,
		request: RemoteStatementStoreCreateProofRequest,
	) -> Result<
		RemoteStatementStoreCreateProofResponse,
		CallError<RemoteStatementStoreCreateProofError>,
	> {
		version(request)
			.exec(|request| async move {
				let owner = request.product_account_id.dot_ns_identifier.clone();
				if !self.may_use_product_account(&owner, cx).await {
					return Err(CallError::Domain(
						latest::RemoteStatementStoreCreateProofError::UnknownAccount,
					));
				}
				let proof = self
					.create_product_statement_proof(
						cx,
						owner,
						request.product_account_id.derivation_index,
						request.statement,
					)
					.await
					.map_err(statement_proof_error)?;
				Ok(latest::RemoteStatementStoreCreateProofResponse { proof })
			})
			.await
	}

	#[instrument(skip_all, fields(runtime.method = "statement_store.create_proof_authorized"))]
	async fn create_proof_authorized(
		&self,
		cx: &CallContext,
		request: RemoteStatementStoreCreateProofAuthorizedRequest,
	) -> Result<
		RemoteStatementStoreCreateProofAuthorizedResponse,
		CallError<RemoteStatementStoreCreateProofAuthorizedError>,
	> {
		version(request)
			.exec(|statement| async move {
				let proof = self
					.create_authorized_statement_proof(cx, statement)
					.await
					.map_err(statement_proof_error)?;
				Ok(latest::RemoteStatementStoreCreateProofResponse { proof })
			})
			.await
	}

	#[instrument(skip_all, fields(runtime.method = "statement_store.submit"))]
	async fn submit(
		&self,
		cx: &CallContext,
		request: RemoteStatementStoreSubmitRequest,
	) -> Result<RemoteStatementStoreSubmitResponse, CallError<RemoteStatementStoreSubmitError>> {
		version(request)
			.exec(|statement| async move {
				self.connection
					.require_remote_permission(
						latest::RemotePermission::StatementSubmit,
						latest::GenericError { reason: PERMISSION_DENIED_REASON.to_string() },
					)
					.await?;
				if let Some(reason) = cx.cancel().reason() {
					return Err(CallError::Domain(latest::GenericError {
						reason: format!("statement submit {reason}"),
					}));
				}
				let encoded = signed_statement_to_scale(statement.clone())
					.map_err(|reason| CallError::Domain(latest::GenericError { reason }))?;
				self.statement_store_rpc()
					.submit_sso(&encoded, "statement-store")
					.await
					.map_err(|error| {
						if let latest::StatementProof::Sr25519 { signer, .. } = statement.proof &&
							error.is_no_allowance()
						{
							self.accounts.forget_statement_store_allowance_key(signer);
						}
						CallError::Domain(latest::GenericError {
							reason: format!("statement-store submit failed: {error}"),
						})
					})?;
				self.connection.services.cache_statement(statement);
				Ok(())
			})
			.await
	}
}

#[instrument(skip_all, fields(runtime.method = "statement_store.subscription_stream"))]
fn statement_store_subscription_stream(
	subscription: RpcSubscription<Value>,
	remote_subscription_id: String,
) -> impl futures::Stream<Item = latest::RemoteStatementStoreSubscribeItem> + Send {
	StatementStoreSubscriptionStream { subscription, remote_subscription_id, is_complete: false }
}

struct StatementStoreSubscriptionStream {
	subscription: RpcSubscription<Value>,
	remote_subscription_id: String,
	is_complete: bool,
}

impl futures::Stream for StatementStoreSubscriptionStream {
	type Item = latest::RemoteStatementStoreSubscribeItem;

	fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
		let state = self.get_mut();
		loop {
			let value = match Pin::new(&mut state.subscription).poll_next(cx) {
				Poll::Pending => return Poll::Pending,
				Poll::Ready(Some(Ok(value))) => value,
				Poll::Ready(Some(Err(_))) | Poll::Ready(None) => {
					return Poll::Ready(None);
				},
			};
			let page =
				match parse_new_statements_result(state.remote_subscription_id.clone(), &value) {
					Ok(page) => page,
					Err(_) => continue,
				};

			let was_complete = state.is_complete;
			let is_complete = was_complete || page.remaining == Some(0);
			state.is_complete = is_complete;
			let statements = page
				.statements
				.into_iter()
				.filter_map(|statement| decode_signed_statement(&statement).ok())
				.collect::<Vec<_>>();
			if statements.is_empty() {
				if is_complete && !was_complete {
					return Poll::Ready(Some(latest::RemoteStatementStoreSubscribeItem {
						statements,
						is_complete,
					}));
				}
				continue;
			}

			return Poll::Ready(Some(latest::RemoteStatementStoreSubscribeItem {
				statements,
				is_complete,
			}));
		}
	}
}

impl ProductRuntimeHost {
	/// Open the remote statement-store subscription, reporting a failure
	/// before its first item as the interrupt the subscription ends with.
	async fn open_statement_subscription(
		&self,
		request: latest::RemoteStatementStoreSubscribeRequest,
	) -> Result<
		Subscription<latest::RemoteStatementStoreSubscribeItem, CallError<latest::GenericError>>,
		CallError<latest::GenericError>,
	> {
		let (kind, topics) = validated_topic_filter(request.into())
			.map_err(|reason| CallError::Domain(latest::GenericError { reason }))?;
		let statement_store = self.statement_store_rpc();
		let rpc_client = statement_store.client("statement-store").await.map_err(|reason| {
			CallError::Domain(latest::GenericError { reason: reason.to_string() })
		})?;
		let subscription = statement_store_rpc::subscribe(&rpc_client, kind, &topics)
			.await
			.map_err(|err| {
				CallError::Domain(latest::GenericError {
					reason: format!("statement-store subscribe failed: {err}"),
				})
			})?;
		let Some(remote_subscription_id) = subscription.subscription_id().map(ToString::to_string)
		else {
			return Err(CallError::Domain(latest::GenericError {
				reason: "statement-store subscribe returned no subscription id".to_string(),
			}));
		};
		let remote_stream =
			statement_store_subscription_stream(subscription, remote_subscription_id);
		let cached = self.connection.services.cached_statements(kind, &topics);
		let cached_page = (!cached.is_empty()).then(|| latest::RemoteStatementStoreSubscribeItem {
			statements: cached.clone(),
			is_complete: false,
		});
		let services = self.connection.services.clone();
		let mut pending_local = cached;
		let remote_stream = remote_stream.filter_map(move |mut page| {
			let mut visible_local = Vec::new();
			page.statements.retain(|statement| {
				if pending_local.contains(statement) {
					visible_local.push(statement.clone());
					false
				} else {
					true
				}
			});
			pending_local.retain(|statement| !visible_local.contains(statement));
			services.mark_statements_visible(&visible_local);
			futures::future::ready(
				(!page.statements.is_empty() || page.is_complete).then_some(page),
			)
		});
		let stream = futures::stream::iter(cached_page).chain(remote_stream);
		Ok(Subscription::new(stream.map(Ok)))
	}

	/// `StatementStoreRpc` bound to this runtime's people chain.
	pub fn statement_store_rpc(&self) -> StatementStoreRpc {
		self.connection.services.statement_store.clone()
	}

	async fn create_product_statement_proof(
		&self,
		cx: &CallContext,
		owner: ProductId,
		derivation_index: latest::DerivationIndex,
		statement: latest::Statement,
	) -> Result<latest::StatementProof, StatementProofFailure> {
		let product_account_id =
			latest::ProductAccountId { dot_ns_identifier: owner.clone(), derivation_index };
		let session = self.accounts.current_session().ok_or(StatementProofFailure::NoSession)?;
		let signer = self
			.product_account_public_key(cx, &session, &owner, &product_account_id.derivation_index)
			.await
			.map_err(|err| StatementProofFailure::UnableToSign(err.to_string()))?;
		let fields = statement_fields_from_v01(statement)
			.map_err(StatementProofFailure::InvalidStatement)?;
		let payload = unsigned_statement_signing_payload(fields)
			.map_err(StatementProofFailure::UnableToSign)?;
		if &owner != self.connection.product_id() {
			let review = UserConfirmationReview::StatementStoreProductSign(
				StatementStoreProductSignReview {
					calling_product_id: Some(self.connection.product_id().clone()),
					account: product_account_id.clone(),
					payload: payload.clone(),
				},
			);
			self.confirm_signature(cx, CrossProductSignatureKind::StatementProof, &owner, review)
				.await
				.map_err(statement_authority_failure)?;
		}
		let cx = remote_authority_context(cx);
		let signature = remote_authority_call(
			&cx,
			self.accounts.sign_statement_store_product_payload(
				&session,
				&cx,
				product_account_id,
				payload,
			),
		)
		.await
		.map_err(statement_authority_failure)?;
		Ok(latest::StatementProof::Sr25519 { signature, signer })
	}

	async fn create_authorized_statement_proof(
		&self,
		cx: &CallContext,
		statement: latest::Statement,
	) -> Result<latest::StatementProof, StatementProofFailure> {
		let session = self.accounts.current_session().ok_or(StatementProofFailure::NoSession)?;
		let cx = remote_authority_context(cx);
		let allowance =
			remote_authority_call(&cx, self.accounts.statement_store_allowance_key(&cx, &session))
				.await
				.map_err(statement_authority_failure)?;
		create_statement_proof_with_key(statement, &allowance)
	}
}

fn create_statement_proof_with_key(
	statement: latest::Statement,
	key: &StatementStoreAllowanceKey,
) -> Result<latest::StatementProof, StatementProofFailure> {
	let fields =
		statement_fields_from_v01(statement).map_err(StatementProofFailure::InvalidStatement)?;
	let signed = sign_statement_fields(*key.as_secret_bytes(), key.public_key, fields)
		.map_err(StatementProofFailure::UnableToSign)?;
	signed
		.into_iter()
		.find_map(|field| match field {
			crate::host_logic::statement_store::StatementField::Proof(proof) => {
				Some(statement_proof_to_v01(proof))
			},
			_ => None,
		})
		.ok_or_else(|| StatementProofFailure::UnableToSign("missing proof".to_string()))
}

enum StatementProofFailure {
	NoSession,
	/// The user refused a signature made with another product's account.
	Refused,
	InvalidStatement(String),
	UnableToSign(String),
}

fn statement_authority_failure(err: AuthorityError) -> StatementProofFailure {
	match err {
		AuthorityError::Disconnected => StatementProofFailure::NoSession,
		AuthorityError::Rejected => StatementProofFailure::Refused,
		AuthorityError::ConfirmationFailed(error) => {
			StatementProofFailure::UnableToSign(error.reason)
		},
		err => StatementProofFailure::UnableToSign(err.to_string()),
	}
}

fn statement_proof_error(
	failure: StatementProofFailure,
) -> CallError<latest::RemoteStatementStoreCreateProofError> {
	CallError::Domain(match failure {
		StatementProofFailure::NoSession | StatementProofFailure::Refused => {
			latest::RemoteStatementStoreCreateProofError::UnableToSign
		},
		StatementProofFailure::UnableToSign(_reason) => {
			latest::RemoteStatementStoreCreateProofError::UnableToSign
		},
		StatementProofFailure::InvalidStatement(reason) => {
			latest::RemoteStatementStoreCreateProofError::Unknown { reason }
		},
	})
}

#[cfg(test)]
mod tests;
