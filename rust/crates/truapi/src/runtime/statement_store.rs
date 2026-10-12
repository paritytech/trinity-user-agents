//! `StatementStore` surface: session-key statement proofs plus submit and
//! subscribe flows over the people-chain statement store.

use core::pin::Pin;
use core::task::{Context, Poll};

use futures::StreamExt as _;

use super::authority::{AuthorityError, StatementStoreAllowanceKey};
use super::statement_store_rpc::{self, StatementStoreRpc};
use super::{
    PERMISSION_DENIED_REASON, ProductRuntimeHost, remote_authority_call, remote_authority_context,
};
use crate::host_logic::statement_store::{
    MAX_MATCH_ALL_TOPICS, MAX_MATCH_ANY_TOPICS, TopicFilterKind, decode_signed_statement,
    parse_new_statements_result, sign_statement_fields, signed_statement_to_scale,
    statement_fields_from_v01, statement_proof_to_v01, unsigned_statement_signing_payload,
};

use crate::platform::{StatementStoreProductSignReview, UserConfirmationReview};
use serde_json::Value;
use subxt_rpcs::client::RpcSubscription;
use tracing::instrument;
use truapi::api::StatementStore;
use truapi::latest;
use truapi::versioned::statement_store::{
    RemoteStatementStoreCreateProofAuthorizedError,
    RemoteStatementStoreCreateProofAuthorizedRequest,
    RemoteStatementStoreCreateProofAuthorizedResponse, RemoteStatementStoreCreateProofError,
    RemoteStatementStoreCreateProofRequest, RemoteStatementStoreCreateProofResponse,
    RemoteStatementStoreSubmitError, RemoteStatementStoreSubmitRequest,
    RemoteStatementStoreSubmitResponse, RemoteStatementStoreSubscribeError,
    RemoteStatementStoreSubscribeItem, RemoteStatementStoreSubscribeRequest,
};
use truapi::{CallContext, CallError, Subscription};

#[truapi::async_trait]
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
        match self.open_statement_subscription(request).await {
            Ok(subscription) => subscription,
            Err(error) => Subscription::interrupted(error),
        }
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
        let RemoteStatementStoreCreateProofRequest::V1(mut inner) = request;
        inner.product_account_id = Self::normalize_product_account_id(inner.product_account_id)
            .map_err(|()| {
                CallError::Domain(RemoteStatementStoreCreateProofError::V1(
                    latest::RemoteStatementStoreCreateProofError::UnknownAccount,
                ))
            })?;
        let Some(owner) = self
            .authorized_product_account(&inner.product_account_id.dot_ns_identifier, cx)
            .await
        else {
            return Err(CallError::Domain(RemoteStatementStoreCreateProofError::V1(
                latest::RemoteStatementStoreCreateProofError::UnknownAccount,
            )));
        };
        inner.product_account_id.dot_ns_identifier = owner;
        let proof = self
            .create_product_statement_proof(cx, inner.product_account_id, inner.statement)
            .await
            .map_err(statement_proof_error)?;
        Ok(RemoteStatementStoreCreateProofResponse::V1(
            latest::RemoteStatementStoreCreateProofResponse { proof },
        ))
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
        let RemoteStatementStoreCreateProofAuthorizedRequest::V1(statement) = request;
        let proof = self
            .create_authorized_statement_proof(cx, statement)
            .await
            .map_err(statement_proof_authorized_error)?;
        Ok(RemoteStatementStoreCreateProofAuthorizedResponse::V1(
            latest::RemoteStatementStoreCreateProofResponse { proof },
        ))
    }

    #[instrument(skip_all, fields(runtime.method = "statement_store.submit"))]
    async fn submit(
        &self,
        cx: &CallContext,
        request: RemoteStatementStoreSubmitRequest,
    ) -> Result<RemoteStatementStoreSubmitResponse, CallError<RemoteStatementStoreSubmitError>>
    {
        let RemoteStatementStoreSubmitRequest::V1(statement) = request;
        self.require_remote_permission(
            latest::RemotePermission::StatementSubmit,
            RemoteStatementStoreSubmitError::V1(latest::GenericError {
                reason: PERMISSION_DENIED_REASON.to_string(),
            }),
        )
        .await?;
        if let Some(reason) = cx.cancel().reason() {
            return Err(CallError::Domain(RemoteStatementStoreSubmitError::V1(
                latest::GenericError {
                    reason: format!("statement submit {reason}"),
                },
            )));
        }
        let encoded = signed_statement_to_scale(statement.clone()).map_err(|reason| {
            CallError::Domain(RemoteStatementStoreSubmitError::V1(latest::GenericError {
                reason,
            }))
        })?;
        self.statement_store_rpc()
            .submit_sso(encoded, "statement-store")
            .await
            .map_err(|reason| {
                if let latest::StatementProof::Sr25519 { signer, .. } = statement.proof
                    && statement_store_rpc::is_no_allowance_rejection(&reason)
                {
                    self.authority
                        .forget_statement_store_allowance_key(&self.product_id(), signer);
                }
                CallError::Domain(RemoteStatementStoreSubmitError::V1(latest::GenericError {
                    reason: format!("statement-store submit failed: {reason}"),
                }))
            })?;
        self.services.cache_statement(statement);
        Ok(RemoteStatementStoreSubmitResponse::V1)
    }
}

fn statement_store_topic_filter(
    request: RemoteStatementStoreSubscribeRequest,
) -> Result<(TopicFilterKind, Vec<[u8; 32]>), String> {
    match request {
        RemoteStatementStoreSubscribeRequest::V1(
            latest::RemoteStatementStoreSubscribeRequest::MatchAll(topics),
        ) => {
            if topics.len() > MAX_MATCH_ALL_TOPICS {
                return Err(format!(
                    "MatchAll has {} topics, maximum is {}",
                    topics.len(),
                    MAX_MATCH_ALL_TOPICS
                ));
            }
            Ok((TopicFilterKind::MatchAll, topics))
        }
        RemoteStatementStoreSubscribeRequest::V1(
            latest::RemoteStatementStoreSubscribeRequest::MatchAny(topics),
        ) => {
            if topics.len() > MAX_MATCH_ANY_TOPICS {
                let topic_count = topics.len();
                return Err(format!(
                    "MatchAny has {topic_count} topics, maximum is {MAX_MATCH_ANY_TOPICS}"
                ));
            }
            Ok((TopicFilterKind::MatchAny, topics))
        }
    }
}

#[instrument(skip_all, fields(runtime.method = "statement_store.subscription_stream"))]
fn statement_store_subscription_stream(
    subscription: RpcSubscription<Value>,
    remote_subscription_id: String,
) -> impl futures::Stream<Item = RemoteStatementStoreSubscribeItem> + Send {
    StatementStoreSubscriptionStream {
        subscription,
        remote_subscription_id,
        is_complete: false,
    }
}

struct StatementStoreSubscriptionStream {
    subscription: RpcSubscription<Value>,
    remote_subscription_id: String,
    is_complete: bool,
}

impl futures::Stream for StatementStoreSubscriptionStream {
    type Item = RemoteStatementStoreSubscribeItem;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let state = self.get_mut();
        loop {
            let value = match Pin::new(&mut state.subscription).poll_next(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Some(Ok(value))) => value,
                Poll::Ready(Some(Err(_))) | Poll::Ready(None) => {
                    return Poll::Ready(None);
                }
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
                    return Poll::Ready(Some(RemoteStatementStoreSubscribeItem::V1(
                        latest::RemoteStatementStoreSubscribeItem {
                            statements,
                            is_complete,
                        },
                    )));
                }
                continue;
            }

            return Poll::Ready(Some(RemoteStatementStoreSubscribeItem::V1(
                latest::RemoteStatementStoreSubscribeItem {
                    statements,
                    is_complete,
                },
            )));
        }
    }
}

impl ProductRuntimeHost {
    /// Open the remote statement-store subscription, reporting a failure
    /// before its first item as the interrupt the subscription ends with.
    async fn open_statement_subscription(
        &self,
        request: RemoteStatementStoreSubscribeRequest,
    ) -> Result<
        Subscription<
            RemoteStatementStoreSubscribeItem,
            CallError<RemoteStatementStoreSubscribeError>,
        >,
        CallError<RemoteStatementStoreSubscribeError>,
    > {
        let (kind, topics) = match statement_store_topic_filter(request) {
            Ok(value) => value,
            Err(reason) => {
                return Err(CallError::Domain(RemoteStatementStoreSubscribeError::V1(
                    latest::GenericError { reason },
                )));
            }
        };
        let statement_store = self.statement_store_rpc();
        let rpc_client = statement_store
            .client("statement-store")
            .await
            .map_err(|reason| {
                CallError::Domain(RemoteStatementStoreSubscribeError::V1(
                    latest::GenericError {
                        reason: reason.to_string(),
                    },
                ))
            })?;
        let subscription = statement_store_rpc::subscribe(&rpc_client, kind, &topics)
            .await
            .map_err(|err| {
                CallError::Domain(RemoteStatementStoreSubscribeError::V1(
                    latest::GenericError {
                        reason: format!("statement-store subscribe failed: {err}"),
                    },
                ))
            })?;
        let Some(remote_subscription_id) = subscription.subscription_id().map(ToString::to_string)
        else {
            return Err(CallError::Domain(RemoteStatementStoreSubscribeError::V1(
                latest::GenericError {
                    reason: "statement-store subscribe returned no subscription id".to_string(),
                },
            )));
        };
        let remote_stream =
            statement_store_subscription_stream(subscription, remote_subscription_id);
        let cached = self.services.cached_statements(kind, &topics);
        let cached_page = (!cached.is_empty()).then(|| {
            RemoteStatementStoreSubscribeItem::V1(latest::RemoteStatementStoreSubscribeItem {
                statements: cached.clone(),
                is_complete: false,
            })
        });
        let services = self.services.clone();
        let mut pending_local = cached;
        let remote_stream = remote_stream.filter_map(move |item| {
            let RemoteStatementStoreSubscribeItem::V1(mut page) = item;
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
                (!page.statements.is_empty() || page.is_complete)
                    .then_some(RemoteStatementStoreSubscribeItem::V1(page)),
            )
        });
        let stream = futures::stream::iter(cached_page).chain(remote_stream);
        Ok(Subscription::new(stream.map(Ok)))
    }

    /// `StatementStoreRpc` bound to this runtime's people chain.
    pub fn statement_store_rpc(&self) -> StatementStoreRpc {
        self.services.statement_store.clone()
    }

    async fn create_product_statement_proof(
        &self,
        cx: &CallContext,
        product_account_id: latest::ProductAccountId,
        statement: latest::Statement,
    ) -> Result<latest::StatementProof, StatementProofFailure> {
        let session = self
            .authority
            .current_session()
            .ok_or(StatementProofFailure::NoSession)?;
        let signer = self
            .product_account_public_key(cx, &session, &product_account_id)
            .await
            .map_err(|err| StatementProofFailure::UnableToSign(err.to_string()))?;
        let fields = statement_fields_from_v01(statement)
            .map_err(StatementProofFailure::InvalidStatement)?;
        let payload = unsigned_statement_signing_payload(fields)
            .map_err(StatementProofFailure::UnableToSign)?;
        // A publisher's grant does not replace an ordinary caller's signature approval.
        if product_account_id.dot_ns_identifier != self.product_id() {
            let confirmed = self
                .confirm_product_action(UserConfirmationReview::StatementStoreProductSign(
                    StatementStoreProductSignReview {
                        calling_product_id: Some(self.product_id()),
                        account: product_account_id.clone(),
                        payload: payload.clone(),
                    },
                ))
                .await
                .map_err(|err| StatementProofFailure::UnableToSign(err.reason))?;
            if !confirmed {
                return Err(StatementProofFailure::Refused);
            }
        }
        let cx = remote_authority_context(cx);
        let signature = remote_authority_call(
            &cx,
            self.authority.sign_statement_store_product_payload(
                &cx,
                &session,
                Some(self.product_id().as_str()),
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
        let session = self
            .authority
            .current_session()
            .ok_or(StatementProofFailure::NoSession)?;
        let cx = remote_authority_context(cx);
        let allowance = remote_authority_call(
            &cx,
            self.authority
                .statement_store_allowance_key(&cx, &session, self.product_id()),
        )
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
    let signed = sign_statement_fields(key.secret, key.public_key, fields)
        .map_err(StatementProofFailure::UnableToSign)?;
    signed
        .into_iter()
        .find_map(|field| match field {
            crate::host_logic::statement_store::StatementField::Proof(proof) => {
                Some(statement_proof_to_v01(proof))
            }
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
        err => StatementProofFailure::UnableToSign(err.to_string()),
    }
}

fn statement_proof_domain_error(
    failure: StatementProofFailure,
) -> latest::RemoteStatementStoreCreateProofError {
    match failure {
        StatementProofFailure::NoSession | StatementProofFailure::Refused => {
            latest::RemoteStatementStoreCreateProofError::UnableToSign
        }
        StatementProofFailure::UnableToSign(_reason) => {
            latest::RemoteStatementStoreCreateProofError::UnableToSign
        }
        StatementProofFailure::InvalidStatement(reason) => {
            latest::RemoteStatementStoreCreateProofError::Unknown { reason }
        }
    }
}

fn statement_proof_error(
    failure: StatementProofFailure,
) -> CallError<RemoteStatementStoreCreateProofError> {
    CallError::Domain(RemoteStatementStoreCreateProofError::V1(
        statement_proof_domain_error(failure),
    ))
}

fn statement_proof_authorized_error(
    failure: StatementProofFailure,
) -> CallError<RemoteStatementStoreCreateProofAuthorizedError> {
    CallError::Domain(RemoteStatementStoreCreateProofAuthorizedError::V1(
        statement_proof_domain_error(failure),
    ))
}

#[cfg(test)]
mod tests {
    use super::super::{LocalActivation, RuntimeServices, SigningHostRole};
    use super::*;
    use crate::host_logic::product_account::{
        SR25519_SIGNING_CONTEXT, derive_product_keypair, derive_root_keypair_from_entropy,
        index_bytes,
    };
    use crate::platform::ProductContext;
    use crate::test_support::{
        StubPlatform, account_id, new_statements_frame, runtime_config, signed_statement,
        sso_session_info, sso_success_response_script, statement, stub_platform,
        submitted_remote_message, subscribe_ack_frame, test_spawner,
    };
    use futures::StreamExt;
    use parity_scale_codec::Encode;
    use schnorrkel::{ExpansionMode, MiniSecretKey, PublicKey, Signature};
    use std::sync::Arc;

    const ENTROPY: [u8; 16] = [0xAB; 16];

    fn statement_payload(statement: latest::Statement) -> Vec<u8> {
        unsigned_statement_signing_payload(statement_fields_from_v01(statement).unwrap()).unwrap()
    }

    fn allowance_key(seed: u8) -> ([u8; 64], [u8; 32]) {
        let mini_secret = MiniSecretKey::from_bytes(&[seed; 32]).unwrap();
        let keypair = mini_secret.expand_to_keypair(ExpansionMode::Ed25519);
        (keypair.secret.to_bytes(), keypair.public.to_bytes())
    }

    fn assert_sr25519_signature(signer: [u8; 32], signature: [u8; 64], payload: &[u8]) {
        let public = PublicKey::from_bytes(&signer).unwrap();
        let signature = Signature::from_bytes(&signature).unwrap();
        public
            .verify_simple(SR25519_SIGNING_CONTEXT, payload, &signature)
            .unwrap();
    }

    /// Seed `owner`'s manifest cache so its `context` grant to `grantee`
    /// resolves without a chain.
    fn cache_context_grant(platform: &StubPlatform, owner: &str, grantee: &str) {
        let entry = crate::runtime::product_manifest::CachedManifest {
            fetched_at_secs: crate::unix_time::current_unix_secs(),
            json: Some(format!(
                r#"{{"$v":1,"trustedProducts":{{"{grantee}":["context"]}}}}"#
            )),
        };
        futures::executor::block_on(crate::platform::CoreStorage::write_core_storage(
            platform,
            crate::runtime::product_manifest::manifest_cache_key(owner),
            entry.encode(),
        ))
        .expect("stub core storage accepts the entry");
    }

    fn signing_host_runtime(product_id: &str) -> (ProductRuntimeHost, Arc<SigningHostRole>) {
        signing_host_runtime_on(product_id, Arc::new(StubPlatform::default()))
    }

    fn signing_host_runtime_on(
        product_id: &str,
        platform: Arc<StubPlatform>,
    ) -> (ProductRuntimeHost, Arc<SigningHostRole>) {
        let platform: Arc<dyn crate::platform::Platform> = platform;
        let services = RuntimeServices::new(
            platform.clone(),
            crate::platform::HostInfo {
                name: "Polkadot Mobile".to_string(),
                icon: None,
                version: None,
                platform: truapi::latest::HostPlatform::Unknown,
            },
            [0; 32],
            [0xbb; 32],
            [0xcc; 32],
            test_spawner(),
        );
        let signing_host = SigningHostRole::new(services.clone(), "paseo".to_string());
        futures::executor::block_on(signing_host.activate_local_session(ENTROPY.to_vec()))
            .expect("activation succeeds");
        let host = ProductRuntimeHost::from_services(
            services.clone(),
            crate::host_core::ConnectionAdapters::from_services(&services),
            signing_host.clone(),
            ProductContext::new(product_id.to_string()).expect("valid product id"),
        );
        (host, signing_host)
    }

    #[test]
    fn statement_store_create_proof_pairing_host_does_not_use_session_key() {
        let host =
            ProductRuntimeHost::new(stub_platform(), runtime_config("myapp.dot"), test_spawner());
        let session = sso_session_info();
        host.test_cache_product_subtree(&session, "myapp.dot", session.public_key);
        host.test_session_state().set_session(session);
        let cx = CallContext::default();
        let request = RemoteStatementStoreCreateProofRequest::V1(
            latest::RemoteStatementStoreCreateProofRequest {
                product_account_id: account_id("myapp.dot", 0),
                statement: statement(),
            },
        );

        let err = futures::executor::block_on(StatementStore::create_proof(&host, &cx, request))
            .unwrap_err();

        assert!(matches!(
            err,
            CallError::Domain(RemoteStatementStoreCreateProofError::V1(
                latest::RemoteStatementStoreCreateProofError::UnableToSign
            ))
        ));
    }

    #[test]
    fn statement_store_create_proof_signing_host_uses_product_key() {
        let (host, _signing_host) = signing_host_runtime("myapp.dot");
        let statement = statement();
        let payload = statement_payload(statement.clone());
        let root = derive_root_keypair_from_entropy(&ENTROPY).unwrap();
        let product_keypair = derive_product_keypair(&root, "myapp.dot", index_bytes(0)).unwrap();
        let expected_signer = product_keypair.public.to_bytes();
        let cx = CallContext::default();
        let request = RemoteStatementStoreCreateProofRequest::V1(
            latest::RemoteStatementStoreCreateProofRequest {
                product_account_id: account_id("myapp.dot", 0),
                statement,
            },
        );

        let response =
            futures::executor::block_on(StatementStore::create_proof(&host, &cx, request)).unwrap();

        let RemoteStatementStoreCreateProofResponse::V1(inner) = response;
        let latest::StatementProof::Sr25519 { signer, signature } = inner.proof else {
            panic!("expected sr25519 statement proof");
        };
        assert_eq!(signer, expected_signer);
        assert_sr25519_signature(signer, signature, &payload);
    }

    /// The three signing capabilities reach a user confirmation through their
    /// AutoSigning gate, which never covers a caller that is not the account's
    /// own product. This path has no such gate, so signing as another product
    /// raises the confirmation itself rather than going out silently.
    #[test]
    fn statement_store_create_proof_confirms_a_cross_product_account() {
        let platform = Arc::new(StubPlatform {
            sign_raw_confirmed: true,
            ..Default::default()
        });
        cache_context_grant(&platform, "dim2.paseo", "dim2next");
        let (host, _signing_host) = signing_host_runtime_on("dim2next.paseo", platform.clone());
        let statement = statement();
        let payload = statement_payload(statement.clone());
        let root = derive_root_keypair_from_entropy(&ENTROPY).unwrap();
        let owner = derive_product_keypair(&root, "dim2.paseo", index_bytes(0)).unwrap();
        let request = RemoteStatementStoreCreateProofRequest::V1(
            latest::RemoteStatementStoreCreateProofRequest {
                product_account_id: account_id("dim2.paseo", 0),
                statement,
            },
        );

        let response = futures::executor::block_on(StatementStore::create_proof(
            &host,
            &CallContext::default(),
            request,
        ))
        .expect("the grant admits the account");

        let RemoteStatementStoreCreateProofResponse::V1(inner) = response;
        let latest::StatementProof::Sr25519 { signer, signature } = inner.proof else {
            panic!("expected sr25519 statement proof");
        };
        assert_eq!(
            signer,
            owner.public.to_bytes(),
            "the owner's account signed"
        );
        assert_sr25519_signature(signer, signature, &payload);

        let reviews = platform
            .statement_store_product_sign_reviews
            .lock()
            .expect("statement store product sign review list mutex poisoned");
        assert_eq!(reviews.len(), 1, "the user saw the signature");
        assert_eq!(
            reviews[0].calling_product_id.as_deref(),
            Some("dim2next.paseo"),
            "and saw which product asked",
        );
    }

    #[test]
    fn statement_store_create_proof_refuses_a_cross_product_account_the_user_declines() {
        let platform = Arc::new(StubPlatform {
            sign_raw_confirmed: false,
            ..Default::default()
        });
        cache_context_grant(&platform, "dim2.paseo", "dim2next");
        let (host, _signing_host) = signing_host_runtime_on("dim2next.paseo", platform);
        let request = RemoteStatementStoreCreateProofRequest::V1(
            latest::RemoteStatementStoreCreateProofRequest {
                product_account_id: account_id("dim2.paseo", 0),
                statement: statement(),
            },
        );

        let err = futures::executor::block_on(StatementStore::create_proof(
            &host,
            &CallContext::default(),
            request,
        ))
        .unwrap_err();

        assert!(matches!(
            err,
            CallError::Domain(RemoteStatementStoreCreateProofError::V1(
                latest::RemoteStatementStoreCreateProofError::UnableToSign
            ))
        ));
    }

    #[test]
    fn statement_store_create_proof_rejects_wrong_product_account() {
        let host =
            ProductRuntimeHost::new(stub_platform(), runtime_config("myapp.dot"), test_spawner());
        host.test_session_state().set_session(sso_session_info());
        let cx = CallContext::default();
        let request = RemoteStatementStoreCreateProofRequest::V1(
            latest::RemoteStatementStoreCreateProofRequest {
                product_account_id: account_id("other.dot", 0),
                statement: statement(),
            },
        );

        let err = futures::executor::block_on(StatementStore::create_proof(&host, &cx, request))
            .unwrap_err();

        assert!(matches!(
            err,
            CallError::Domain(RemoteStatementStoreCreateProofError::V1(
                latest::RemoteStatementStoreCreateProofError::UnknownAccount
            ))
        ));
    }

    #[test]
    fn statement_store_create_proof_authorized_signs_with_allowance_key() {
        let session = sso_session_info();
        let statement = statement();
        let payload = statement_payload(statement.clone());
        let (allowance_secret, expected_signer) = allowance_key(11);
        let platform = Arc::new(StubPlatform {
            sso_response_script: Some(sso_success_response_script(
                &session,
                crate::host_internal::sso_messages::RemoteMessage {
                    message_id: "wallet-proof-auth-1".to_string(),
                    data: crate::host_internal::sso_messages::RemoteMessageData::V1(
                        crate::host_internal::sso_messages::v1::RemoteMessage::ResourceAllocationResponse(
                            crate::host_internal::sso_messages::Response {
                                responding_to: "proof-auth-1".to_string(),
                                payload: Ok(vec![
                                    crate::host_internal::sso_messages::SsoAllocationOutcome::Allocated(
                                        crate::host_internal::sso_messages::SsoAllocatedResource::StatementStoreAllowance {
                                            slot_account_key: allowance_secret.to_vec(),
                                        },
                                    ),
                                ]),
                            },
                        ),
                    ),
                },
            )),
            ..Default::default()
        });
        let host = ProductRuntimeHost::new(
            platform.clone(),
            runtime_config("myapp.dot"),
            test_spawner(),
        );
        host.test_session_state().set_session(session.clone());
        let cx = CallContext::with_request_id("proof-auth-1".to_string());
        let request = RemoteStatementStoreCreateProofAuthorizedRequest::V1(statement);

        let response = futures::executor::block_on(StatementStore::create_proof_authorized(
            &host, &cx, request,
        ))
        .unwrap();

        let RemoteStatementStoreCreateProofAuthorizedResponse::V1(inner) = response;
        let latest::StatementProof::Sr25519 { signer, signature } = inner.proof else {
            panic!("expected sr25519 statement proof");
        };
        assert_eq!(signer, expected_signer);
        assert_sr25519_signature(signer, signature, &payload);

        let message = submitted_remote_message(&platform, &session);
        let crate::host_internal::sso_messages::RemoteMessageData::V1(
            crate::host_internal::sso_messages::v1::RemoteMessage::ResourceAllocationRequest(
                request,
            ),
        ) = message.data
        else {
            panic!("expected resource allocation request");
        };
        assert_eq!(request.calling_product_id, "myapp.dot");
        assert_eq!(
            request.on_existing,
            crate::host_internal::sso_messages::OnExistingAllowancePolicy::Ignore
        );
        assert_eq!(
            request.resources,
            vec![truapi::latest::AllocatableResource::StatementStoreAllowance]
        );
    }

    #[test]
    fn statement_store_submit_posts_signed_statement_and_waits_for_ack() {
        let platform = Arc::new(StubPlatform {
            rpc_responses: vec![
                r#"{"jsonrpc":"2.0","id":"truapi:1","result":{"status":"new"}}"#.to_string(),
            ],
            ..Default::default()
        });
        let host = ProductRuntimeHost::new(
            platform.clone(),
            runtime_config("myapp.dot"),
            test_spawner(),
        );
        let cx = CallContext::with_request_id("submit-1".to_string());
        let request = RemoteStatementStoreSubmitRequest::V1(signed_statement([7; 32]));

        futures::executor::block_on(StatementStore::submit(&host, &cx, request)).unwrap();

        let sent = platform.sent_rpc.lock().expect("rpc list mutex poisoned");
        assert_eq!(sent.len(), 1);
        let request: serde_json::Value = serde_json::from_str(&sent[0]).unwrap();
        assert_eq!(request["method"], "statement_submit");
        let statement_hex = request["params"][0].as_str().unwrap();
        let statement =
            hex::decode(statement_hex.strip_prefix("0x").unwrap_or(statement_hex)).unwrap();
        assert_eq!(
            crate::host_logic::statement_store::decode_signed_statement(&statement).unwrap(),
            signed_statement([7; 32])
        );
        assert_eq!(
            host.services
                .cached_statements(TopicFilterKind::MatchAll, &[[7; 32]]),
            vec![signed_statement([7; 32])]
        );
    }

    /// The permission prompt ends on its own terms, but a call withdrawn while
    /// it was up must not go on to publish the statement.
    #[test]
    fn statement_store_submit_withdrawn_before_it_is_sent_publishes_nothing() {
        let platform = Arc::new(StubPlatform {
            rpc_responses: vec![
                r#"{"jsonrpc":"2.0","id":"truapi:1","result":{"status":"new"}}"#.to_string(),
            ],
            ..Default::default()
        });
        let host = ProductRuntimeHost::new(
            platform.clone(),
            runtime_config("myapp.dot"),
            test_spawner(),
        );
        let cancel = truapi::CancellationToken::default();
        cancel.cancel();
        let cx = CallContext::with_parts("submit-withdrawn".to_string(), cancel);
        let request = RemoteStatementStoreSubmitRequest::V1(signed_statement([7; 32]));

        let result = futures::executor::block_on(StatementStore::submit(&host, &cx, request));

        assert_eq!(
            result,
            Err(CallError::Domain(RemoteStatementStoreSubmitError::V1(
                latest::GenericError {
                    reason: "statement submit cancelled".to_string(),
                }
            )))
        );
        assert!(platform.sent_rpc.lock().unwrap().is_empty());
    }

    #[test]
    fn statement_store_submit_requires_remote_permission_before_rpc() {
        let platform = Arc::new(StubPlatform {
            remote_permission_denied: true,
            ..Default::default()
        });
        let host = ProductRuntimeHost::new(
            platform.clone(),
            runtime_config("myapp.dot"),
            test_spawner(),
        );
        let cx = CallContext::with_request_id("submit-1".to_string());
        let request = RemoteStatementStoreSubmitRequest::V1(signed_statement([7; 32]));

        let err =
            futures::executor::block_on(StatementStore::submit(&host, &cx, request)).unwrap_err();

        match err {
            CallError::Domain(RemoteStatementStoreSubmitError::V1(latest::GenericError {
                reason,
            })) => assert_eq!(reason, PERMISSION_DENIED_REASON),
            other => panic!("expected statement-store permission denial, got {other:?}"),
        }
        assert!(platform.sent_rpc.lock().unwrap().is_empty());
    }

    #[test]
    fn statement_store_subscribe_maps_signed_pages() {
        let signed = crate::host_logic::statement_store::signed_statement_to_scale(
            signed_statement([7; 32]),
        )
        .unwrap();
        let unsigned = vec![crate::host_logic::statement_store::StatementField::Data(
            vec![1],
        )]
        .encode();
        let platform = Arc::new(StubPlatform {
            rpc_responses: vec![
                subscribe_ack_frame("truapi:1", "remote-sub"),
                new_statements_frame("remote-sub", vec![unsigned, signed]),
            ],
            ..Default::default()
        });
        let host = ProductRuntimeHost::new(
            platform.clone(),
            runtime_config("myapp.dot"),
            test_spawner(),
        );
        let cx = CallContext::with_request_id("sub-1".to_string());
        let mut subscription = futures::executor::block_on(StatementStore::subscribe(
            &host,
            &cx,
            RemoteStatementStoreSubscribeRequest::V1(
                latest::RemoteStatementStoreSubscribeRequest::MatchAny(vec![[7; 32]]),
            ),
        ));

        let item = futures::executor::block_on(subscription.next()).expect("statement page");

        let Ok(RemoteStatementStoreSubscribeItem::V1(inner)) = item else {
            panic!("expected a statement page")
        };
        assert!(inner.is_complete);
        assert_eq!(inner.statements, vec![signed_statement([7; 32])]);
        let sent = platform.sent_rpc.lock().expect("rpc list mutex poisoned");
        let request: serde_json::Value = serde_json::from_str(&sent[0]).unwrap();
        assert_eq!(request["method"], "statement_subscribeStatement");
        assert_eq!(
            request["params"][0]["matchAny"][0],
            "0x0707070707070707070707070707070707070707070707070707070707070707"
        );
    }

    #[test]
    fn statement_store_subscription_bridges_then_suppresses_remote_echo() {
        let cached = signed_statement([7; 32]);
        let encoded =
            crate::host_logic::statement_store::signed_statement_to_scale(cached.clone()).unwrap();
        let platform = Arc::new(StubPlatform {
            rpc_responses: vec![
                subscribe_ack_frame("truapi:1", "remote-sub-cached"),
                new_statements_frame("remote-sub-cached", vec![encoded]),
            ],
            ..Default::default()
        });
        let host = ProductRuntimeHost::new(platform, runtime_config("myapp.dot"), test_spawner());
        host.services.cache_statement(cached.clone());
        let cx = CallContext::with_request_id("sub-cached".to_string());
        let mut subscription = futures::executor::block_on(StatementStore::subscribe(
            &host,
            &cx,
            RemoteStatementStoreSubscribeRequest::V1(
                latest::RemoteStatementStoreSubscribeRequest::MatchAny(vec![[7; 32]]),
            ),
        ));

        assert_eq!(
            futures::executor::block_on(subscription.next()),
            Some(Ok(RemoteStatementStoreSubscribeItem::V1(
                latest::RemoteStatementStoreSubscribeItem {
                    statements: vec![cached],
                    is_complete: false,
                }
            )))
        );
        assert_eq!(
            futures::executor::block_on(subscription.next()),
            Some(Ok(RemoteStatementStoreSubscribeItem::V1(
                latest::RemoteStatementStoreSubscribeItem {
                    statements: vec![],
                    is_complete: true,
                }
            )))
        );
        assert!(
            host.services
                .cached_statements(TopicFilterKind::MatchAny, &[[7; 32]])
                .is_empty()
        );
    }

    /// Pages that arrive before the subscribe ack are buffered by remote
    /// subscription id and replayed once the ack confirms the subscription.
    #[test]
    fn statement_store_subscribe_buffers_pages_before_subscribe_ack() {
        let rogue = crate::host_logic::statement_store::signed_statement_to_scale(
            signed_statement([9; 32]),
        )
        .unwrap();
        let signed = crate::host_logic::statement_store::signed_statement_to_scale(
            signed_statement([7; 32]),
        )
        .unwrap();
        let platform = Arc::new(StubPlatform {
            rpc_responses: vec![
                new_statements_frame("remote-sub-pre", vec![rogue]),
                subscribe_ack_frame("truapi:1", "remote-sub-pre"),
                new_statements_frame("remote-sub-pre", vec![signed]),
            ],
            ..Default::default()
        });
        let host = ProductRuntimeHost::new(platform, runtime_config("myapp.dot"), test_spawner());
        let cx = CallContext::with_request_id("sub-pre".to_string());
        let mut subscription = futures::executor::block_on(StatementStore::subscribe(
            &host,
            &cx,
            RemoteStatementStoreSubscribeRequest::V1(
                latest::RemoteStatementStoreSubscribeRequest::MatchAny(vec![[7; 32]]),
            ),
        ));

        let item = futures::executor::block_on(subscription.next()).expect("statement page");

        assert_eq!(
            item,
            Ok(RemoteStatementStoreSubscribeItem::V1(
                latest::RemoteStatementStoreSubscribeItem {
                    statements: vec![signed_statement([9; 32])],
                    is_complete: true,
                }
            ))
        );
    }

    #[test]
    fn statement_store_subscribe_unsubscribes_remote_subscription_on_drop() {
        let signed = crate::host_logic::statement_store::signed_statement_to_scale(
            signed_statement([7; 32]),
        )
        .unwrap();
        let platform = Arc::new(StubPlatform {
            rpc_responses: vec![
                subscribe_ack_frame("truapi:1", "remote-sub-drop"),
                new_statements_frame("remote-sub-drop", vec![signed]),
            ],
            ..Default::default()
        });
        let host = ProductRuntimeHost::new(
            platform.clone(),
            runtime_config("myapp.dot"),
            test_spawner(),
        );
        let cx = CallContext::with_request_id("sub-drop".to_string());
        let mut subscription = futures::executor::block_on(StatementStore::subscribe(
            &host,
            &cx,
            RemoteStatementStoreSubscribeRequest::V1(
                latest::RemoteStatementStoreSubscribeRequest::MatchAny(vec![[7; 32]]),
            ),
        ));

        let _ = futures::executor::block_on(subscription.next()).expect("statement page");
        drop(subscription);

        let sent = platform.sent_rpc.lock().expect("rpc list mutex poisoned");
        assert_eq!(sent.len(), 2);
        let unsubscribe: serde_json::Value = serde_json::from_str(&sent[1]).unwrap();
        assert_eq!(unsubscribe["method"], "statement_unsubscribeStatement");
        assert_eq!(unsubscribe["params"][0], "remote-sub-drop");
    }

    #[test]
    fn statement_store_subscribe_emits_empty_completion_page_after_filtering() {
        let unsigned = vec![crate::host_logic::statement_store::StatementField::Data(
            vec![1],
        )]
        .encode();
        let platform = Arc::new(StubPlatform {
            rpc_responses: vec![
                subscribe_ack_frame("truapi:1", "remote-sub-empty"),
                new_statements_frame("remote-sub-empty", vec![unsigned]),
            ],
            ..Default::default()
        });
        let host = ProductRuntimeHost::new(platform, runtime_config("myapp.dot"), test_spawner());
        let cx = CallContext::with_request_id("sub-empty-complete".to_string());
        let mut subscription = futures::executor::block_on(StatementStore::subscribe(
            &host,
            &cx,
            RemoteStatementStoreSubscribeRequest::V1(
                latest::RemoteStatementStoreSubscribeRequest::MatchAny(vec![[7; 32]]),
            ),
        ));

        let item = futures::executor::block_on(subscription.next()).expect("completion page");

        let Ok(RemoteStatementStoreSubscribeItem::V1(inner)) = item else {
            panic!("expected a statement page")
        };
        assert!(inner.is_complete);
        assert!(inner.statements.is_empty());
    }

    #[test]
    fn statement_store_subscribe_rejects_topic_limit_violations() {
        let platform = stub_platform();
        let host = ProductRuntimeHost::new(
            platform.clone(),
            runtime_config("myapp.dot"),
            test_spawner(),
        );
        let cx = CallContext::with_request_id("sub-too-many".to_string());
        let topics = vec![[7; 32]; MAX_MATCH_ANY_TOPICS + 1];

        let mut subscription = futures::executor::block_on(StatementStore::subscribe(
            &host,
            &cx,
            RemoteStatementStoreSubscribeRequest::V1(
                latest::RemoteStatementStoreSubscribeRequest::MatchAny(topics),
            ),
        ));
        let err = match futures::executor::block_on(subscription.next()) {
            Some(Err(err)) => err,
            _ => panic!("topic limit violation should interrupt the subscription"),
        };

        let CallError::Domain(RemoteStatementStoreSubscribeError::V1(reason)) = err else {
            panic!("expected statement-store subscribe domain error");
        };
        assert_eq!(
            reason.reason,
            format!(
                "MatchAny has {} topics, maximum is {}",
                MAX_MATCH_ANY_TOPICS + 1,
                MAX_MATCH_ANY_TOPICS
            )
        );
        assert!(platform.sent_rpc.lock().unwrap().is_empty());
    }

    #[test]
    fn statement_store_subscribe_reports_chain_connect_failure() {
        let platform = Arc::new(StubPlatform {
            chain_connect_error: Some("chain unavailable"),
            ..Default::default()
        });
        let host = ProductRuntimeHost::new(
            platform.clone(),
            runtime_config("myapp.dot"),
            test_spawner(),
        );
        let cx = CallContext::with_request_id("sub-connect-fail".to_string());

        let mut subscription = futures::executor::block_on(StatementStore::subscribe(
            &host,
            &cx,
            RemoteStatementStoreSubscribeRequest::V1(
                latest::RemoteStatementStoreSubscribeRequest::MatchAny(vec![[7; 32]]),
            ),
        ));
        let err = match futures::executor::block_on(subscription.next()) {
            Some(Err(err)) => err,
            _ => panic!("chain connect failure should interrupt the subscription"),
        };

        let CallError::Domain(RemoteStatementStoreSubscribeError::V1(reason)) = err else {
            panic!("expected statement-store subscribe domain error");
        };
        assert!(
            reason.reason.contains("statement-store connect failed:"),
            "unexpected reason: {}",
            reason.reason
        );
        assert!(
            reason.reason.contains("chain unavailable"),
            "unexpected reason: {}",
            reason.reason
        );
        assert!(platform.sent_rpc.lock().unwrap().is_empty());
    }
}
