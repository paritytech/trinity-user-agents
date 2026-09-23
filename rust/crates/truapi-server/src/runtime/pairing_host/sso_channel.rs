//! SSO statement-store channel to the paired remote signing host.

use super::super::authority::{
    AuthorityCancelError, AuthorityError, BulletinAllowanceKey, CreateTransactionAuthorityRequest,
    PaymentTopUpAuthorityError, ProductDeviceChatAuthorityError, ProductDeviceChatAuthorityRequest,
    SignPayloadAuthorityRequest, SignRawAuthorityRequest, StatementStoreAllowanceKey,
};
use super::super::sso_remote::{
    RemoteResponseWait, SSO_LOCAL_DISCONNECT_REASON, SSO_PEER_DISCONNECT_REASON,
    SsoRemoteResponseError, SsoSessionKey, fresh_statement_expiry, reply_matcher, sso_message_id,
    statement_subscription_stream, subscribe_statement_topic, wait_for_sso_remote_response,
};
use super::super::statement_store_rpc::{self, StatementStoreRpc};
use super::PairingHost;
use crate::host_logic::session::{SessionInfo, SessionState, SsoSessionInfo};
use crate::host_logic::sso::messages::{
    CreateTransactionLegacyPayload, CreateTransactionPayload, CreateTransactionRequest,
    CreateTransactionWithLegacyAccountRequest, OnExistingAllowancePolicy, PaymentTopUpRequest,
    ProductRequest, ProductSubtreeRequest, RemoteMessage, RemoteMessageData,
    ResourceAllocationRequest, RingVrfError, SignRawWithLegacyAccountRequest, SignRequest,
    SsoAllocatedResource, SsoAllocationOutcome, SsoProductDeviceChatOperation, SsoSessionStatement,
    build_outgoing_request_statement, decode_sso_session_statement, v1,
};
use crate::host_logic::sso::wire::SsoRequest;
use crate::host_logic::statement_store::parse_new_statements_result;

use futures::FutureExt;
use futures::future::{AbortHandle, Abortable};
use tracing::{debug, instrument, warn};
use truapi::{CallContext, latest};

/// Active peer-disconnect watcher for one SSO session; aborts on drop.
pub(super) struct SsoDisconnectMonitor {
    key: SsoSessionKey,
    abort: AbortHandle,
}

impl Drop for SsoDisconnectMonitor {
    fn drop(&mut self) {
        self.abort.abort();
    }
}

impl PairingHost {
    pub(super) async fn remote_payment_top_up(
        &self,
        cx: &CallContext,
        session: &SessionInfo,
        request: PaymentTopUpRequest,
    ) -> Result<(), PaymentTopUpAuthorityError> {
        self.call(cx, session, request)
            .await
            .map_err(remote_authority_error)?
            .map_err(|error| PaymentTopUpAuthorityError::Domain(error.0))
    }

    fn stop_disconnect_monitor(&self) {
        self.disconnect_monitor
            .lock()
            .expect("SSO disconnect monitor mutex poisoned")
            .take();
    }

    /// Watch the session's topics for a peer disconnect statement, replacing
    /// any monitor for a different session. No-op when one is already running
    /// for this session.
    pub(super) fn start_disconnect_monitor(&self, session: &SessionInfo) {
        let Some(sso) = session.sso.clone() else {
            self.stop_disconnect_monitor();
            return;
        };
        let key = SsoSessionKey::from_session(&sso);

        let (registration, spawner) = {
            let mut current = self
                .disconnect_monitor
                .lock()
                .expect("SSO disconnect monitor mutex poisoned");
            if current.as_ref().is_some_and(|active| active.key == key) {
                return;
            }
            let (abort, registration) = AbortHandle::new_pair();
            *current = Some(SsoDisconnectMonitor { key, abort });
            (registration, self.spawner.clone())
        };

        let statement_store = self.statement_store.clone();
        let pairing_host = self.weak_self.clone();
        let future = async move {
            let result = wait_for_sso_peer_disconnect(statement_store, sso).await;
            let Some(pairing_host) = pairing_host.upgrade() else {
                return;
            };
            {
                let mut active = pairing_host
                    .disconnect_monitor
                    .lock()
                    .expect("SSO disconnect monitor mutex poisoned");
                if active.as_ref().is_some_and(|active| active.key == key) {
                    *active = None;
                }
            }
            match result {
                Ok(()) => {
                    pairing_host.handle_signing_host_disconnected(key).await;
                }
                Err(reason) => {
                    warn!(%reason, "SSO peer disconnect monitor stopped");
                }
            }
        };
        spawner(Box::pin(Abortable::new(future, registration).map(|_| ())));
    }

    /// Stop channel work for a cleared session: wake its in-flight waiters
    /// with a local disconnect, then drop the peer-disconnect monitor.
    pub(super) fn stop_session_channel(&self, session: Option<&SessionInfo>) {
        if let Some(sso) = session.and_then(|session| session.sso.as_ref()) {
            self.session_disconnects
                .notify(sso, SSO_LOCAL_DISCONNECT_REASON);
        }
        self.clear_statement_store_allowance_keys(session);
        self.clear_bulletin_allowance_keys(session);
        self.stop_disconnect_monitor();
        self.clear_product_subtrees(session);
    }

    /// Best-effort `Disconnected` notification to the SSO peer.
    #[instrument(skip_all, fields(runtime.method = "sso.disconnect.submit"))]
    pub(super) async fn submit_disconnected_message(
        &self,
        session: &SessionInfo,
    ) -> Result<(), String> {
        let sso = session
            .sso
            .as_ref()
            .ok_or_else(|| "No SSO session state".to_string())?;
        let message_id = sso_message_id();
        let message = RemoteMessage {
            message_id: message_id.clone(),
            data: RemoteMessageData::V1(v1::RemoteMessage::Disconnected),
        };
        let statement = build_outgoing_request_statement(
            sso,
            message_id,
            vec![message],
            fresh_statement_expiry(),
        )?;
        self.statement_store
            .submit_fire_and_forget(statement, "SSO statement-store")
            .await
            .map_err(|err| format!("SSO statement submit failed: {err}"))?;
        Ok(())
    }

    /// Send `request` to the paired signing host and await its typed answer.
    ///
    /// The outer error is the transport's; the inner result is the peer's
    /// payload for this request type.
    #[instrument(skip_all, fields(runtime.method = "sso.remote_message.submit", action = R::NAME))]
    async fn call<R: SsoRequest>(
        &self,
        cx: &CallContext,
        session: &SessionInfo,
        request: R,
    ) -> Result<R::Response, SsoRemoteResponseError> {
        let sso = session
            .sso
            .as_ref()
            .ok_or_else(|| SsoRemoteResponseError::Failure("No SSO session state".to_string()))?;
        let key = SsoSessionKey::from_session(sso);
        let (_disconnect_guard, disconnect) = self.session_disconnects.subscribe(sso);
        if !session_matches_key(&self.session_state, key) {
            return Err(SsoRemoteResponseError::LocalDisconnected);
        }
        let message_id = sso_message_id();
        let statement = build_outgoing_request_statement(
            sso,
            message_id.clone(),
            vec![RemoteMessage::request(message_id.clone(), request)],
            fresh_statement_expiry(),
        )
        .map_err(SsoRemoteResponseError::Failure)?;
        let rpc_client = self
            .statement_store
            .client("SSO statement-store")
            .await
            .map_err(|err| SsoRemoteResponseError::Failure(err.to_string()))?;
        let own_subscription = subscribe_statement_topic(&rpc_client, sso.session_id_own)
            .await
            .map_err(|err| {
                SsoRemoteResponseError::Failure(format!(
                    "SSO own statement-store subscribe failed: {err}"
                ))
            })?;
        let peer_subscription = subscribe_statement_topic(&rpc_client, sso.session_id_peer)
            .await
            .map_err(|err| {
                SsoRemoteResponseError::Failure(format!(
                    "SSO peer statement-store subscribe failed: {err}"
                ))
            })?;
        let submit_client = rpc_client.clone();
        let session_state = self.session_state.clone();
        let submit = async move {
            if !session_matches_key(&session_state, key) {
                return Err(SsoRemoteResponseError::LocalDisconnected);
            }
            statement_store_rpc::submit_sso(&submit_client, statement, "pairing-host request")
                .await
                .map_err(|err| {
                    SsoRemoteResponseError::Failure(format!("SSO statement submit failed: {err}"))
                })
        }
        .boxed();
        let action = R::NAME;
        debug!(action, %message_id, "submitted SSO remote message, awaiting response");
        let result = wait_for_sso_remote_response(
            RemoteResponseWait {
                own_statements: statement_subscription_stream(own_subscription, "own"),
                peer_statements: statement_subscription_stream(peer_subscription, "peer"),
                submit,
                session: sso,
                statement_request_id: &message_id,
                remote_message_id: &message_id,
                cancel: cx.cancel(),
                disconnect: Some(disconnect),
            },
            reply_matcher::<R>(&message_id),
        )
        .await;
        let result = result.map_err(|reason| match reason {
            SsoRemoteResponseError::Cancelled(err) if !cx.request_id().is_empty() => {
                SsoRemoteResponseError::Cancelled(err.with_remote_message_id(cx.request_id()))
            }
            reason => reason,
        });
        match &result {
            Ok(_) => debug!(action, %message_id, "SSO remote response received"),
            Err(reason) => warn!(action, %message_id, %reason, "SSO remote message failed"),
        }
        if matches!(&result, Err(SsoRemoteResponseError::PeerDisconnected)) {
            self.handle_signing_host_disconnected(key).await;
        }
        result.map(|response| response.payload)
    }

    /// Resolve a product's hard-subtree public key, asking the Account Holder
    /// only when neither the memory cache nor storage already holds it.
    pub(super) async fn remote_product_subtree_public_key(
        &self,
        cx: &CallContext,
        session: &SessionInfo,
        product_id: String,
    ) -> Result<[u8; 32], AuthorityError> {
        let sso = session.sso.as_ref().ok_or(AuthorityError::Disconnected)?;
        let lifecycle_epoch = self.current_session_lifecycle_epoch();
        let cache_key = (SsoSessionKey::from_session(sso), product_id.clone());
        if let Some(public_key) = self.known_product_subtree(session, cache_key.clone()).await {
            return Ok(public_key);
        }
        let public_key = self
            .call(cx, session, ProductSubtreeRequest { product_id })
            .await
            .map_err(remote_authority_error)?
            .map_err(remote_authority_error)?;
        if !self
            .persist_product_subtree_if_current(session, lifecycle_epoch, cache_key, public_key)
            .await
        {
            return Err(AuthorityError::Disconnected);
        }
        Ok(public_key)
    }

    /// Forward RFC-0023 VRF signing to the paired Account Holder.
    pub(super) async fn remote_sign_vrf(
        &self,
        cx: &CallContext,
        session: &SessionInfo,
        calling_product_id: String,
        request: latest::HostAccountSignVrfRequest,
    ) -> Result<latest::VrfSignature, AuthorityError> {
        self.call(
            cx,
            session,
            ProductRequest {
                calling_product_id,
                payload: request,
            },
        )
        .await
        .map_err(remote_authority_error)?
        .map_err(|err| match err {
            latest::HostAccountSignVrfError::NotConnected => AuthorityError::Disconnected,
            latest::HostAccountSignVrfError::Rejected => AuthorityError::Rejected,
            latest::HostAccountSignVrfError::Unknown { reason } => {
                AuthorityError::Unknown { reason }
            }
        })
    }

    /// Forward a payload-signing request to the paired signing host.
    #[instrument(skip_all, fields(account_kind = match &request {
        SignPayloadAuthorityRequest::Product(_) => "product",
        SignPayloadAuthorityRequest::LegacyAccount { .. } => "legacy",
    }))]
    pub(super) async fn remote_sign_payload(
        &self,
        cx: &CallContext,
        session: &SessionInfo,
        request: SignPayloadAuthorityRequest,
    ) -> Result<latest::HostSignPayloadResponse, AuthorityError> {
        let request = match request {
            SignPayloadAuthorityRequest::Product(request) => request,
            SignPayloadAuthorityRequest::LegacyAccount {
                product_account,
                request,
            } => latest::HostSignPayloadRequest {
                account: product_account,
                payload: request.payload,
            },
        };
        self.call(cx, session, SignRequest::Payload(Box::new(request)))
            .await
            .map_err(remote_authority_error)?
            .map_err(remote_authority_error)
    }

    /// Forward a raw-signing request to the paired signing host.
    #[instrument(skip_all, fields(account_kind = match &request {
        SignRawAuthorityRequest::Product(_) => "product",
        SignRawAuthorityRequest::LegacyAccount { .. } => "legacy",
    }))]
    pub(super) async fn remote_sign_raw(
        &self,
        cx: &CallContext,
        session: &SessionInfo,
        request: SignRawAuthorityRequest,
        watermarked: bool,
    ) -> Result<latest::HostSignPayloadResponse, AuthorityError> {
        match request {
            SignRawAuthorityRequest::Product(request) => self
                .call(
                    cx,
                    session,
                    if watermarked {
                        SignRequest::Raw(request)
                    } else {
                        SignRequest::RawUnwatermarkedDeprecated(request)
                    },
                )
                .await
                .map_err(remote_authority_error)?
                .map_err(remote_authority_error),
            SignRawAuthorityRequest::LegacyAccount { account, request } => {
                let request = SignRawWithLegacyAccountRequest {
                    account,
                    data: request.payload,
                };
                if !watermarked {
                    return self
                        .call(
                            cx,
                            session,
                            SignRequest::RawWithLegacyAccountUnwatermarkedDeprecated(request),
                        )
                        .await
                        .map_err(remote_authority_error)?
                        .map_err(remote_authority_error);
                }
                let signature = self
                    .call(cx, session, request)
                    .await
                    .map_err(remote_authority_error)?
                    .map_err(remote_authority_error)?;
                Ok(latest::HostSignPayloadResponse {
                    signature,
                    signed_transaction: None,
                })
            }
        }
    }

    /// Forward a transaction-creation request to the paired signing host.
    #[instrument(skip_all, fields(account_kind = match &request {
        CreateTransactionAuthorityRequest::Product(_) => "product",
        CreateTransactionAuthorityRequest::LegacyAccount { .. } => "legacy",
        CreateTransactionAuthorityRequest::IdentityAccount(_) => "identity",
    }))]
    pub(super) async fn remote_create_transaction(
        &self,
        cx: &CallContext,
        session: &SessionInfo,
        request: CreateTransactionAuthorityRequest,
    ) -> Result<latest::HostCreateTransactionResponse, AuthorityError> {
        let signed = match request {
            CreateTransactionAuthorityRequest::Product(payload) => {
                self.call(
                    cx,
                    session,
                    CreateTransactionRequest {
                        payload: CreateTransactionPayload::V1(payload),
                    },
                )
                .await
            }
            CreateTransactionAuthorityRequest::LegacyAccount {
                product_account,
                request,
            } => {
                self.call(
                    cx,
                    session,
                    CreateTransactionRequest {
                        payload: CreateTransactionPayload::V1(latest::ProductAccountTxPayload {
                            signer: product_account,
                            genesis_hash: request.genesis_hash,
                            call_data: request.call_data,
                            extensions: request.extensions,
                            tx_ext_version: request.tx_ext_version,
                        }),
                    },
                )
                .await
            }
            CreateTransactionAuthorityRequest::IdentityAccount(payload) => {
                self.call(
                    cx,
                    session,
                    CreateTransactionWithLegacyAccountRequest {
                        payload: CreateTransactionLegacyPayload::V1(payload),
                    },
                )
                .await
            }
        };
        signed
            .map_err(remote_authority_error)?
            .map(|transaction| latest::HostCreateTransactionResponse { transaction })
            .map_err(remote_authority_error)
    }

    /// Forward a contextual-alias request to the paired signing host.
    pub(super) async fn remote_account_alias(
        &self,
        cx: &CallContext,
        session: &SessionInfo,
        request: ProductRequest<latest::HostAccountGetAliasRequest>,
    ) -> Result<latest::HostAccountGetAliasResponse, RingVrfError> {
        self.call(cx, session, request)
            .await
            .map_err(ring_vrf_transport_error)?
    }

    /// Forward a ring-VRF proof request to the paired signing host.
    pub(super) async fn remote_create_proof(
        &self,
        cx: &CallContext,
        session: &SessionInfo,
        request: ProductRequest<latest::HostAccountCreateProofRequest>,
    ) -> Result<latest::HostAccountCreateProofResponse, RingVrfError> {
        self.call(cx, session, request)
            .await
            .map_err(ring_vrf_transport_error)?
    }

    /// Forward a ring-VRF key registration request to the paired signing host.
    pub(super) async fn remote_register_ring_vrf_key(
        &self,
        cx: &CallContext,
        session: &SessionInfo,
        request: ProductRequest<latest::HostAccountRegisterRingVrfKeyRequest>,
    ) -> Result<latest::RingVrfPublicKey, RingVrfError> {
        self.call(cx, session, request)
            .await
            .map_err(ring_vrf_transport_error)?
    }

    /// Forward a ring-VRF key listing request to the paired signing host.
    pub(super) async fn remote_list_ring_vrf_keys(
        &self,
        cx: &CallContext,
        session: &SessionInfo,
        request: ProductRequest<latest::HostAccountListRingVrfKeysRequest>,
    ) -> Result<Vec<latest::RegisteredRingVrfKey>, RingVrfError> {
        self.call(cx, session, request)
            .await
            .map_err(ring_vrf_transport_error)?
    }

    /// Forward a direct ring-VRF signing request to the paired signing host.
    pub(super) async fn remote_ring_vrf_sign(
        &self,
        cx: &CallContext,
        session: &SessionInfo,
        request: ProductRequest<latest::HostAccountRingVrfSignRequest>,
    ) -> Result<Vec<u8>, RingVrfError> {
        self.call(cx, session, request)
            .await
            .map_err(ring_vrf_transport_error)?
    }

    /// Forward a narrow Chat authority operation without exposing outgoing wallet keys.
    pub(super) async fn remote_product_device_chat(
        &self,
        cx: &CallContext,
        session: &SessionInfo,
        request: ProductDeviceChatAuthorityRequest,
    ) -> Result<latest::HostProductDeviceChatResponse, ProductDeviceChatAuthorityError> {
        let response = self
            .call(
                cx,
                session,
                ProductRequest {
                    calling_product_id: request.calling_product_id,
                    payload: SsoProductDeviceChatOperation::V3(request.operation),
                },
            )
            .await
            .map_err(|error| ProductDeviceChatAuthorityError::from(remote_authority_error(error)))?
            .map_err(|error| {
                let truapi::versioned::account::HostProductDeviceChatError::V1(error) = error;
                ProductDeviceChatAuthorityError::Domain(error)
            })?;
        let truapi::versioned::account::HostProductDeviceChatResponse::V2(response) = response;
        Ok(response)
    }

    /// Ask the paired signing host to allocate product resources, caching any
    /// returned allowance keys.
    pub(super) async fn remote_allocate_resources(
        &self,
        cx: &CallContext,
        session: &SessionInfo,
        product_id: String,
        request: latest::HostRequestResourceAllocationRequest,
    ) -> Result<latest::HostRequestResourceAllocationResponse, AuthorityError> {
        let lifecycle_epoch = self.current_session_lifecycle_epoch();
        let outcomes = self
            .call(
                cx,
                session,
                ResourceAllocationRequest {
                    calling_product_id: product_id.clone(),
                    resources: request.resources,
                    on_existing: OnExistingAllowancePolicy::Increase,
                },
            )
            .await
            .map_err(remote_authority_error)?
            .map_err(remote_authority_error)?;
        self.cache_allowance_outcomes(cx, session, lifecycle_epoch, &product_id, &outcomes)
            .await?;
        Ok(latest::HostRequestResourceAllocationResponse {
            outcomes: outcomes.into_iter().map(Into::into).collect(),
        })
    }

    /// Allocate exactly one allowance for the product and return its material.
    async fn remote_allowance_slot(
        &self,
        cx: &CallContext,
        session: &SessionInfo,
        product_id: &str,
        resource: latest::AllocatableResource,
        on_existing: OnExistingAllowancePolicy,
    ) -> Result<SsoAllocatedResource, AuthorityError> {
        let name = allowance_name(&resource);
        let outcomes = self
            .call(
                cx,
                session,
                ResourceAllocationRequest {
                    calling_product_id: product_id.to_string(),
                    resources: vec![resource],
                    on_existing,
                },
            )
            .await
            .map_err(remote_authority_error)?
            .map_err(remote_authority_error)?;
        match outcomes.into_iter().next() {
            Some(SsoAllocationOutcome::Allocated(resource)) => Ok(resource),
            Some(SsoAllocationOutcome::Rejected) => Err(AuthorityError::Rejected),
            Some(SsoAllocationOutcome::NotAvailable) => Err(AuthorityError::Unavailable {
                reason: format!("{name} is not available"),
            }),
            None => Err(AuthorityError::Unknown {
                reason: format!("Empty {name} response"),
            }),
        }
    }

    /// Statement-store allowance key for the product, served from the cache
    /// or allocated by the paired signing host.
    pub(super) async fn remote_statement_store_allowance_key(
        &self,
        cx: &CallContext,
        session: &SessionInfo,
        product_id: String,
    ) -> Result<StatementStoreAllowanceKey, AuthorityError> {
        let lifecycle_epoch = self.current_session_lifecycle_epoch();
        if let Some(cached) = self
            .cached_statement_store_allowance_key(session, lifecycle_epoch, &product_id)
            .await?
        {
            return Ok(cached);
        }
        match self
            .remote_allowance_slot(
                cx,
                session,
                &product_id,
                latest::AllocatableResource::StatementStoreAllowance,
                OnExistingAllowancePolicy::Ignore,
            )
            .await?
        {
            SsoAllocatedResource::StatementStoreAllowance { slot_account_key } => {
                self.cache_statement_store_allowance_key(
                    session,
                    lifecycle_epoch,
                    &product_id,
                    slot_account_key,
                )
                .await
            }
            other => Err(unexpected_resource("statement-store allowance", &other)),
        }
    }

    /// Bulletin allowance key for the product, served from the cache or
    /// allocated by the paired signing host.
    pub(super) async fn remote_bulletin_allowance_key(
        &self,
        cx: &CallContext,
        session: &SessionInfo,
        product_id: String,
    ) -> Result<BulletinAllowanceKey, AuthorityError> {
        let lifecycle_epoch = self.current_session_lifecycle_epoch();
        if let Some(cached) = self
            .cached_bulletin_allowance_key(session, lifecycle_epoch, &product_id)
            .await?
        {
            return Ok(cached);
        }
        self.allocate_bulletin_allowance_key(
            cx,
            session,
            lifecycle_epoch,
            product_id,
            OnExistingAllowancePolicy::Ignore,
        )
        .await
    }

    /// Evict the cached Bulletin allowance key and allocate a fresh one with
    /// an increased allowance, so a stale or exhausted slot is never reused.
    pub(super) async fn remote_refresh_bulletin_allowance_key(
        &self,
        cx: &CallContext,
        session: &SessionInfo,
        product_id: String,
    ) -> Result<BulletinAllowanceKey, AuthorityError> {
        let lifecycle_epoch = self.current_session_lifecycle_epoch();
        self.evict_bulletin_allowance_key(session, lifecycle_epoch, &product_id)
            .await?;
        self.allocate_bulletin_allowance_key(
            cx,
            session,
            lifecycle_epoch,
            product_id,
            OnExistingAllowancePolicy::Increase,
        )
        .await
    }

    async fn allocate_bulletin_allowance_key(
        &self,
        cx: &CallContext,
        session: &SessionInfo,
        lifecycle_epoch: u64,
        product_id: String,
        on_existing: OnExistingAllowancePolicy,
    ) -> Result<BulletinAllowanceKey, AuthorityError> {
        match self
            .remote_allowance_slot(
                cx,
                session,
                &product_id,
                latest::AllocatableResource::BulletinAllowance,
                on_existing,
            )
            .await?
        {
            SsoAllocatedResource::BulletinAllowance { slot_account_key } => {
                self.cache_bulletin_allowance_key(
                    session,
                    lifecycle_epoch,
                    &product_id,
                    slot_account_key,
                )
                .await
            }
            other => Err(unexpected_resource("bulletin allowance", &other)),
        }
    }

    async fn cache_allowance_outcomes(
        &self,
        cx: &CallContext,
        session: &SessionInfo,
        lifecycle_epoch: u64,
        product_id: &str,
        outcomes: &[SsoAllocationOutcome],
    ) -> Result<(), AuthorityError> {
        for outcome in outcomes {
            if let SsoAllocationOutcome::Allocated(resource) = outcome {
                match resource {
                    SsoAllocatedResource::StatementStoreAllowance { slot_account_key } => {
                        self.cache_statement_store_allowance_key(
                            session,
                            lifecycle_epoch,
                            product_id,
                            slot_account_key.clone(),
                        )
                        .await?;
                    }
                    SsoAllocatedResource::BulletinAllowance { slot_account_key } => {
                        self.cache_bulletin_allowance_key(
                            session,
                            lifecycle_epoch,
                            product_id,
                            slot_account_key.clone(),
                        )
                        .await?;
                    }
                    SsoAllocatedResource::SmartContractAllowance => {}
                    SsoAllocatedResource::ProductStatementStoreAllowance => {}
                    SsoAllocatedResource::AutoSigning {
                        product_root_private_key,
                        ring_vrf_domain_entropy,
                    } => {
                        let expected_product_subtree_public_key = self
                            .remote_product_subtree_public_key(cx, session, product_id.to_string())
                            .await?;
                        self.remember_auto_signing_key(
                            session,
                            lifecycle_epoch,
                            product_id,
                            expected_product_subtree_public_key,
                            *product_root_private_key,
                            *ring_vrf_domain_entropy,
                        )
                        .await?;
                    }
                }
            }
        }
        Ok(())
    }
}

/// True when the current session's SSO channel matches `key`.
pub(super) fn session_matches_key(session_state: &SessionState, key: SsoSessionKey) -> bool {
    session_state.current().as_ref().is_some_and(|current| {
        current
            .sso
            .as_ref()
            .is_some_and(|sso| SsoSessionKey::from_session(sso) == key)
    })
}

fn ring_vrf_transport_error(reason: SsoRemoteResponseError) -> RingVrfError {
    remote_authority_error(reason).into()
}

fn allowance_name(resource: &latest::AllocatableResource) -> &'static str {
    match resource {
        latest::AllocatableResource::StatementStoreAllowance => "statement-store allowance",
        latest::AllocatableResource::BulletinAllowance => "bulletin allowance",
        _ => "resource",
    }
}

/// Reason for an allocation that came back as a different resource kind; names
/// only the kind so no key material reaches logs.
fn unexpected_resource(label: &str, resource: &SsoAllocatedResource) -> AuthorityError {
    AuthorityError::Unknown {
        reason: format!("Unexpected {label} response resource: {}", resource.kind()),
    }
}

fn remote_authority_error(reason: impl Into<SsoRemoteResponseError>) -> AuthorityError {
    match reason.into() {
        SsoRemoteResponseError::Cancelled(err) => AuthorityError::Cancelled(
            AuthorityCancelError::new(err.remote_message_id(), err.reason()),
        ),
        SsoRemoteResponseError::LocalDisconnected | SsoRemoteResponseError::PeerDisconnected => {
            AuthorityError::Disconnected
        }
        SsoRemoteResponseError::Failure(reason) => match reason.as_str() {
            "Rejected" | "User rejected" => AuthorityError::Rejected,
            SSO_LOCAL_DISCONNECT_REASON | SSO_PEER_DISCONNECT_REASON => {
                AuthorityError::Disconnected
            }
            _ => AuthorityError::Unknown { reason },
        },
    }
}

#[instrument(skip_all, fields(runtime.method = "sso.peer_disconnect.monitor"))]
async fn wait_for_sso_peer_disconnect(
    statement_store: StatementStoreRpc,
    session: SsoSessionInfo,
) -> Result<(), String> {
    let rpc_client = statement_store
        .client("SSO disconnect monitor")
        .await
        .map_err(|err| err.to_string())?;
    let mut subscription =
        statement_store_rpc::subscribe_match_all(&rpc_client, &[session.session_id_peer])
            .await
            .map_err(|err| format!("SSO disconnect monitor subscribe failed: {err}"))?;
    while let Some(item) = subscription.next().await {
        let value = item.map_err(|err| format!("SSO disconnect monitor item failed: {err}"))?;
        let page = parse_new_statements_result("sso-peer-disconnect-monitor".to_string(), &value)
            .map_err(|err| err.to_string())?;
        for statement in page.statements {
            let Some(SsoSessionStatement::RemoteMessages(messages)) = decode_sso_session_statement(
                &session,
                &statement,
                "truapi:sso-peer-disconnect-monitor",
            )?
            else {
                continue;
            };
            for message in messages {
                if message? == v1::RemoteMessage::Disconnected {
                    return Ok(());
                }
            }
        }
    }
    Err("SSO disconnect monitor response stream ended".to_string())
}

impl From<SsoAllocationOutcome> for latest::AllocationOutcome {
    fn from(outcome: SsoAllocationOutcome) -> Self {
        match outcome {
            SsoAllocationOutcome::Allocated(_) => Self::Allocated,
            SsoAllocationOutcome::Rejected => Self::Rejected,
            SsoAllocationOutcome::NotAvailable => Self::NotAvailable,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unexpected_resource_reasons_include_only_safe_discriminants() {
        let resource = SsoAllocatedResource::AutoSigning {
            product_root_private_key: [0xA5; 64],
            ring_vrf_domain_entropy: [0x5A; 32],
        };

        let AuthorityError::Unknown { reason } =
            unexpected_resource("statement-store allowance", &resource)
        else {
            panic!("expected an unknown authority error");
        };

        assert_eq!(
            reason,
            "Unexpected statement-store allowance response resource: auto-signing"
        );
        assert!(!reason.contains("165, 165"));
    }
}
