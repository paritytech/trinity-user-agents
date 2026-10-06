//! Pairing-host role for inter-host account authority.
//!
//! A pairing host does not own the user's signing keys. It pairs with a signing
//! host, keeps the active inter-host session, and sends authority requests to
//! that signing host over the SSO channel in [`sso_channel`].

mod sso_channel;
#[cfg(test)]
mod tests;

#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, Weak};
use truapi::latest::{
    HostAccountCreateProofRequest, HostAccountGetAliasRequest, HostAccountListRingVrfKeysRequest,
    HostAccountRegisterRingVrfKeyRequest, HostAccountRingVrfSignRequest,
};

use futures::channel::oneshot;
use sso_channel::SsoDisconnectMonitor;

use super::auth_state::AuthStateMachine;
use super::authority::{
    AccountCaller, AccountHolder, AccountInvocation, AuthorityError, AuthoritySession,
    AutoSigningKey, BulletinAllowanceKey, CreateTransactionAuthorityRequest, HostOperation,
    ProductAuthority, SignPayloadAuthorityRequest, SignRawAuthorityRequest,
    StatementStoreAllowanceKey, authority_session, require_current_session,
};
use super::connected_session_ui_info;
use super::host_grants::{HostGrantPersistence, HostGrantStore};
use super::identity::resolve_session_identity_with_chain;
use super::services::RuntimeServices;
use super::sso_pairing::{SsoPairingFlow, SsoPairingOutcome};
use super::sso_remote::{
    SSO_PEER_DISCONNECT_REASON, SessionDisconnects, SsoSessionKey, sso_message_id,
};
use super::statement_store_rpc::StatementStoreRpc;
use crate::chain_runtime::ChainRuntime;
use crate::host_internal::extrinsic::{
    Sr25519Signer, build_signed_transaction, local_transaction_metadata,
};
use crate::host_internal::sso_messages::{
    ProductRequest, RingVrfError, SsoAllocatedResource, SsoAllocationOutcome,
};
use crate::host_internal::transaction::sign_extrinsic_payload;
use crate::host_logic::entropy::derive_product_entropy_from_source;
use crate::host_logic::product_account::{
    SR25519_SIGNING_CONTEXT, derivation_index_bytes, derive_product_keypair_from_subtree_secret,
    derive_ring_vrf_entropy_from_domain,
};
use crate::host_logic::raw_signing::raw_payload_bytes;
use crate::host_logic::session::{SessionInfo, SessionState, encode_persisted_session};
use crate::host_logic::session_store::SessionStoreChangeNotifier;
use crate::runtime::vrf;
use crate::session_usernames::SessionUsernames;
use crate::subscription::Spawner;

use crate::platform::{
    CoreStorageKey, PairingHostConfig, Platform, ProductContext, SignVrfReview,
    UserConfirmationReview, normalize_product_identifier,
};
use futures::StreamExt;
use tracing::{instrument, warn};
use truapi::versioned::account::{HostRequestLoginError, HostRequestLoginResponse};
use truapi::{CallContext, CallError, v01};
use zeroize::Zeroizing;

use super::ring_vrf_registry::{RingVrfRegistryStore, validate_owner_listing};
use super::signing_host::ring_vrf::{
    ChainRingResolver, MemberCandidate, RingResolver, create_proof, development_context_bytes,
};

struct LoginInFlight {
    waiters: Vec<oneshot::Sender<Result<(), String>>>,
}

struct LoginInFlightOwner<'a> {
    host: &'a PairingHost,
    active: bool,
}

impl<'a> LoginInFlightOwner<'a> {
    fn new(host: &'a PairingHost) -> Self {
        Self { host, active: true }
    }

    fn finish(&mut self, result: Result<(), String>) {
        if self.active {
            self.active = false;
            self.host.finish_login_in_flight(result);
        }
    }
}

impl Drop for LoginInFlightOwner<'_> {
    fn drop(&mut self) {
        if self.active {
            self.host
                .finish_login_in_flight(Err("login request aborted".to_string()));
        }
    }
}

#[derive(Default)]
struct Selection {
    login_generation: u64,
    external_session_active: bool,
}

#[derive(Debug, derive_more::Display)]
enum StoredSessionActivationError {
    #[display("stored auth session is absent")]
    Missing,
    #[display("invalid stored auth session: {_0}")]
    Invalid(String),
    #[display("failed to read stored auth session: {_0}")]
    Read(String),
    #[display("stored auth session changed during activation")]
    Changed,
    #[display("failed to clear previous session: {_0}")]
    Cleanup(String),
}

/// State carried across the reconciles of one session store sync task.
#[derive(Default)]
struct SessionStoreSync {
    /// Clearing the store can itself notify the sync subscription; clear at
    /// most once per read-error streak so a persistently failing read cannot
    /// spin the task through its own clear notifications.
    cleared_after_read_error: bool,
}

impl SessionStoreSync {
    /// Re-read the persisted auth session and reconcile the in-memory one.
    async fn reconcile(&mut self, pairing_host: &PairingHost) {
        self.cleared_after_read_error = matches!(
            pairing_host
                .reconcile_stored_session(!self.cleared_after_read_error, true)
                .await,
            Err(StoredSessionActivationError::Read(_))
        );
    }
}

/// Remote account authority for a pairing host.
pub struct PairingHost {
    /// Shared runtime services. Held, not just borrowed at construction, so
    /// this role can resolve a product manifest for itself when it adjudicates
    /// a cross-product grant. `RuntimeServices` does not hold the pairing host
    /// back (`host_core` owns both), so this is not a cycle.
    services: Arc<RuntimeServices>,
    /// Host platform backing all syscalls.
    pub platform: Arc<dyn Platform>,
    /// Pairing configuration supplied by the embedding host.
    pub host_config: PairingHostConfig,
    /// Shared chain runtime, used to resolve session identity.
    pub chain: ChainRuntime,
    /// Active inter-host session with a signing host.
    session_state: Arc<SessionState>,
    session_store_changes: Arc<SessionStoreChangeNotifier>,
    /// Core-owned auth-state machine emitting to the host.
    pub auth_state: AuthStateMachine,
    /// People-chain statement store RPC client.
    pub statement_store: StatementStoreRpc,
    session_disconnects: Arc<SessionDisconnects>,
    /// `message_id` of the request the session's request channel carries,
    /// the only one a `Cancel` may name without replacing another request.
    newest_request: Mutex<Option<String>>,
    disconnect_monitor: Mutex<Option<SsoDisconnectMonitor>>,
    login_in_flight: Mutex<Option<LoginInFlight>>,
    selection: Mutex<Selection>,
    grants: Arc<HostGrantStore>,
    ring_resolver: Arc<dyn RingResolver>,
    ring_vrf_registry: Arc<RingVrfRegistryStore>,
    session_store_activation: futures::lock::Mutex<()>,
    #[cfg(test)]
    external_session_activation_pause: Mutex<Option<(oneshot::Sender<()>, oneshot::Receiver<()>)>>,
    /// Change notifications the sync task has finished reconciling.
    #[cfg(test)]
    session_store_change_ticks: AtomicUsize,
    /// Self-reference captured by the spawned disconnect-monitor task.
    weak_self: Weak<PairingHost>,
    /// Task spawner for background monitors.
    pub spawner: Spawner,
}

impl PairingHost {
    /// Build a pairing host over the shared runtime services.
    pub fn new(services: Arc<RuntimeServices>, host_config: PairingHostConfig) -> Arc<Self> {
        if services.asset_hub_chain_genesis_hash().is_none() {
            // Said once at startup rather than inferred from every grant
            // refusing, matching the signing role. Cross-product refusals are
            // deliberately indistinguishable from a product that granted
            // nothing, so a pairing host with no Asset Hub resolves no manifest
            // and refuses every grant while looking exactly like one whose
            // publishers granted nothing. The hash itself now arrives by
            // construction, so there is nothing to install here, only to report.
            tracing::warn!(
                "no Asset Hub configured on the pairing role: no product manifest \
                 will resolve, so every cross-product grant is refused"
            );
        }
        let platform = services.platform.clone();
        let auth_state = AuthStateMachine::new(platform.clone());
        Arc::new_cyclic(|weak_self| Self {
            services: services.clone(),
            platform,
            host_config,
            chain: services.chain.clone(),
            session_state: SessionState::new(),
            session_store_changes: SessionStoreChangeNotifier::new(),
            auth_state,
            statement_store: services.statement_store.clone(),
            session_disconnects: Arc::new(SessionDisconnects::default()),
            newest_request: Mutex::new(None),
            disconnect_monitor: Mutex::new(None),
            login_in_flight: Mutex::new(None),
            selection: Mutex::new(Selection::default()),
            grants: Arc::new(HostGrantStore::new(services.platform.clone())),
            ring_resolver: ChainRingResolver::new(services.chain.clone()),
            ring_vrf_registry: RingVrfRegistryStore::new(services.platform.clone()),
            session_store_activation: futures::lock::Mutex::new(()),
            #[cfg(test)]
            external_session_activation_pause: Mutex::new(None),
            #[cfg(test)]
            session_store_change_ticks: AtomicUsize::new(0),
            weak_self: weak_self.clone(),
            spawner: services.spawner.clone(),
        })
    }

    /// Shared session holder for connection-status subscriptions.
    pub fn session_state(&self) -> Arc<SessionState> {
        self.session_state.clone()
    }

    /// Signal that the persisted auth session may have changed; the sync task
    /// re-reads it.
    pub fn notify_session_store_changed(&self) {
        self.advance_session_lifecycle();
        self.session_store_changes.notify();
    }

    fn advance_session_lifecycle(&self) -> u64 {
        let mut selection = self
            .selection
            .lock()
            .expect("session selection mutex poisoned");
        let mut lifecycle = self.grants.lifecycle();
        selection.external_session_active = false;
        lifecycle.advance()
    }

    /// Test hook for [`Self::start_session_store_sync`].
    #[cfg(test)]
    pub fn start_session_store_sync_for_tests(self: Arc<Self>, spawner: Spawner) {
        self.start_session_store_sync(spawner);
    }

    /// `message_id` of the request the session's request channel carries.
    #[cfg(test)]
    pub fn newest_request_for_tests(&self) -> Option<String> {
        self.newest_request
            .lock()
            .expect("newest request mutex poisoned")
            .clone()
    }

    /// Change notifications the sync task has finished reconciling.
    #[cfg(test)]
    pub fn session_store_change_ticks_for_tests(&self) -> usize {
        self.session_store_change_ticks.load(Ordering::SeqCst)
    }

    /// Test alias for [`Self::start_remote_monitor_for_current_session`].
    #[cfg(test)]
    pub fn start_session_supervision_for_current_session(&self) {
        self.start_remote_monitor_for_current_session();
    }

    #[cfg(test)]
    pub fn pause_external_session_activation_for_tests(
        &self,
    ) -> (oneshot::Receiver<()>, oneshot::Sender<()>) {
        let (entered_tx, entered_rx) = oneshot::channel();
        let (resume_tx, resume_rx) = oneshot::channel();
        *self
            .external_session_activation_pause
            .lock()
            .expect("external activation pause mutex poisoned") = Some((entered_tx, resume_rx));
        (entered_rx, resume_tx)
    }

    #[cfg(test)]
    async fn wait_at_external_session_activation_pause(&self) {
        let pause = self
            .external_session_activation_pause
            .lock()
            .expect("external activation pause mutex poisoned")
            .take();
        if let Some((entered, resume)) = pause {
            let _ = entered.send(());
            let _ = resume.await;
        }
    }

    fn current_session(&self) -> Option<AuthoritySession> {
        self.session_state.current().as_ref().map(authority_session)
    }

    pub async fn ring_vrf_providers(
        &self,
        ring: &v01::RingLocation,
    ) -> Result<Vec<v01::ProductAccountId>, RingVrfError> {
        let session = self.session_state.current().ok_or(RingVrfError::Unknown {
            reason: "no active session".to_string(),
        })?;
        self.ring_vrf_registry
            .providers(session.public_key, ring)
            .await
    }

    pub async fn selected_ring_vrf_provider(
        &self,
        ring: &v01::RingLocation,
    ) -> Result<Option<v01::ProductAccountId>, RingVrfError> {
        let session = self.session_state.current().ok_or(RingVrfError::Unknown {
            reason: "no active session".to_string(),
        })?;
        self.ring_vrf_registry
            .selected_provider(session.public_key, ring)
            .await
    }

    pub async fn select_ring_vrf_provider(
        &self,
        ring: v01::RingLocation,
        handle: v01::ProductAccountId,
    ) -> Result<(), RingVrfError> {
        let session = self.session_state.current().ok_or(RingVrfError::Unknown {
            reason: "no active session".to_string(),
        })?;
        self.ring_vrf_registry
            .select_provider(session.public_key, ring, handle)
            .await
    }

    /// Start the disconnect monitor when a session is already active.
    #[cfg(test)]
    pub fn start_remote_monitor_for_current_session(&self) {
        if let Some(session) = self.session_state.current() {
            self.start_disconnect_monitor(&session);
        }
    }

    /// Validate, resolve, and install an externally persisted canonical
    /// session blob without copying it into core storage. Reports the resulting
    /// auth state to the host, including when the blob failed to decode and the
    /// active session was therefore left alone.
    pub async fn activate_external_session(&self, blob: &[u8]) -> Result<(), String> {
        let installed = self.install_external_session(blob).await;
        self.auth_state.announce_current();
        installed
    }

    async fn install_external_session(&self, blob: &[u8]) -> Result<(), String> {
        let _activation = self.session_store_activation.lock().await;
        let session = crate::host_logic::session::decode_persisted_session(blob)?;
        self.invalidate_login_attempts();
        let activation_epoch = self.advance_session_lifecycle();
        let resolved = resolve_session_identity_with_chain(
            &self.chain,
            self.host_config.asset_hub_chain_genesis_hash,
            session,
        )
        .await;
        #[cfg(test)]
        self.wait_at_external_session_activation_pause().await;
        self.set_connected_session_if_current(resolved, activation_epoch, true)
            .await?;
        Ok(())
    }

    /// Read, validate, resolve, and install the persisted auth session before
    /// returning. Product frames may use the connected session once this
    /// future resolves. Reports the resulting auth state to the host, including
    /// when there was no session to restore.
    pub async fn activate_stored_session(&self) -> Result<(), String> {
        let restored = self
            .reconcile_stored_session(true, false)
            .await
            .map_err(|error| error.to_string());
        self.auth_state.announce_current();
        restored
    }

    async fn reconcile_stored_session(
        &self,
        clear_after_read_error: bool,
        preserve_external_session: bool,
    ) -> Result<(), StoredSessionActivationError> {
        let _activation = self.session_store_activation.lock().await;
        if preserve_external_session
            && self
                .selection
                .lock()
                .expect("session selection mutex poisoned")
                .external_session_active
        {
            return Ok(());
        }
        let (activation_epoch, read) = {
            let persistence = self.grants.persistence().await;
            self.drain_session_deletions(&persistence)
                .await
                .map_err(StoredSessionActivationError::Cleanup)?;
            let epoch = self.advance_session_lifecycle();
            let read = persistence.read_auth_session().await;
            (epoch, read)
        };
        let blob = match read {
            Ok(Some(blob)) => blob,
            Ok(None) => {
                self.clear_disconnected_session(false, Some(activation_epoch))
                    .await;
                return Err(StoredSessionActivationError::Missing);
            }
            Err(error) => {
                self.clear_disconnected_session(clear_after_read_error, Some(activation_epoch))
                    .await;
                return Err(StoredSessionActivationError::Read(error.reason));
            }
        };
        let session = match crate::host_logic::session::decode_persisted_session(&blob) {
            Ok(session) => session,
            Err(error) => {
                self.clear_disconnected_session(true, Some(activation_epoch))
                    .await;
                return Err(StoredSessionActivationError::Invalid(error));
            }
        };
        let resolved = resolve_session_identity_with_chain(
            &self.chain,
            self.host_config.asset_hub_chain_genesis_hash,
            session,
        )
        .await;
        let persistence = self.grants.persistence().await;
        self.drain_session_deletions(&persistence)
            .await
            .map_err(StoredSessionActivationError::Cleanup)?;
        if self.grants.lifecycle().revision() != activation_epoch {
            return Err(StoredSessionActivationError::Changed);
        }
        let latest = persistence.read_auth_session().await;
        let error = match latest {
            Ok(latest) if latest.as_deref() == Some(blob.as_slice()) => None,
            Ok(_) => Some((false, StoredSessionActivationError::Changed)),
            Err(error) => Some((
                clear_after_read_error,
                StoredSessionActivationError::Read(error.reason),
            )),
        };
        if let Some((clear_auth, error)) = error {
            if self.begin_session_clear(clear_auth, Some(activation_epoch), None)
                && let Err(reason) = self.drain_session_deletions(&persistence).await
            {
                warn!(%reason, "session cleanup remains pending");
            }
            return Err(error);
        }
        self.prepare_session_installation(&persistence, &resolved)
            .await
            .map_err(StoredSessionActivationError::Cleanup)?;
        if self.grants.lifecycle().revision() != activation_epoch {
            return Err(StoredSessionActivationError::Changed);
        }
        let resolved_blob = encode_persisted_session(&resolved);
        if !self.install_session_if_current(resolved, activation_epoch, false, None) {
            return Err(StoredSessionActivationError::Changed);
        }
        if resolved_blob != blob && self.grants.lifecycle().revision() == activation_epoch {
            let _ = persistence.write_auth_session(resolved_blob).await;
        }
        if let Err(reason) = self.drain_session_deletions(&persistence).await {
            warn!(%reason, "session cleanup remains pending");
        }
        Ok(())
    }

    /// Spawn the background task that keeps the in-memory session in step
    /// with the persisted auth session. It reconciles once at boot and
    /// announces the outcome, so the host always receives an opening auth
    /// state, then reconciles again on every change notification.
    #[instrument(skip_all, fields(runtime.method = "session_store.sync"))]
    pub fn start_session_store_sync(self: Arc<Self>, spawner: Spawner) {
        let pairing_host = Arc::downgrade(&self);
        drop(self);
        spawner(Box::pin(async move {
            let Some(booting) = pairing_host.upgrade() else {
                return;
            };
            let mut ticks = booting.session_store_changes.subscribe();
            let mut sync = SessionStoreSync::default();
            sync.reconcile(&booting).await;
            booting.auth_state.announce_current();
            drop(booting);
            while ticks.next().await.is_some() {
                let Some(pairing_host) = pairing_host.upgrade() else {
                    break;
                };
                sync.reconcile(&pairing_host).await;
                #[cfg(test)]
                pairing_host
                    .session_store_change_ticks
                    .fetch_add(1, Ordering::SeqCst);
            }
        }));
    }

    #[instrument(skip_all, fields(runtime.method = "account.request_login", product = %product.product_id))]
    async fn request_login(
        &self,
        product: &ProductContext,
    ) -> Result<HostRequestLoginResponse, CallError<HostRequestLoginError>> {
        let _ = product;
        if let Some(session) = self.session_state.current() {
            self.auth_state
                .connected(&connected_session_ui_info(&session));
            return Ok(HostRequestLoginResponse::V1(
                v01::HostRequestLoginResponse::AlreadyConnected,
            ));
        }

        if let Some(waiter) = self.login_waiter() {
            match waiter.await {
                Ok(Ok(())) => {
                    return Ok(HostRequestLoginResponse::V1(
                        if self.session_state.current().is_some() {
                            v01::HostRequestLoginResponse::AlreadyConnected
                        } else {
                            v01::HostRequestLoginResponse::Rejected
                        },
                    ));
                }
                Ok(Err(reason)) => {
                    return Err(CallError::Domain(HostRequestLoginError::V1(
                        v01::HostRequestLoginError::Unknown { reason },
                    )));
                }
                Err(_) => {
                    return Err(CallError::Domain(HostRequestLoginError::V1(
                        v01::HostRequestLoginError::Unknown {
                            reason: "login waiter dropped".to_string(),
                        },
                    )));
                }
            }
        }

        let mut login_owner = LoginInFlightOwner::new(self);
        let login_generation = self.begin_login_attempt();
        let outcome = match SsoPairingFlow::new(self, login_generation)
            .request_session()
            .await
        {
            Ok(outcome) => outcome,
            Err(err) => {
                login_owner.finish(Err(login_error_reason(&err)));
                return Err(err);
            }
        };
        match outcome {
            SsoPairingOutcome::Cancelled => {
                login_owner.finish(Ok(()));
                if self.session_state.current().is_some() {
                    Ok(HostRequestLoginResponse::V1(
                        v01::HostRequestLoginResponse::AlreadyConnected,
                    ))
                } else {
                    Ok(HostRequestLoginResponse::V1(
                        v01::HostRequestLoginResponse::Rejected,
                    ))
                }
            }
            SsoPairingOutcome::Success => {
                login_owner.finish(Ok(()));
                Ok(HostRequestLoginResponse::V1(
                    v01::HostRequestLoginResponse::Success,
                ))
            }
        }
    }

    /// Persist and install the selected login under the storage guard.
    pub async fn commit_login_session(
        &self,
        session: &SessionInfo,
        generation: u64,
    ) -> Result<bool, truapi::latest::GenericError> {
        let persistence = self.grants.persistence().await;
        if !self.is_current_login_attempt(generation) {
            return Ok(false);
        }
        self.prepare_session_installation(&persistence, session)
            .await
            .map_err(|reason| truapi::latest::GenericError { reason })?;
        let mut epoch = {
            let mut selection = self
                .selection
                .lock()
                .expect("session selection mutex poisoned");
            let mut lifecycle = self.grants.lifecycle();
            if selection.login_generation != generation {
                return Ok(false);
            }
            lifecycle.queue_auth_deletion();
            selection.external_session_active = false;
            lifecycle.advance()
        };
        let blob = encode_persisted_session(session);
        persistence.write_auth_session(blob.clone()).await?;
        while self.is_current_login_attempt(generation) {
            let latest_epoch = self.grants.lifecycle().revision();
            if latest_epoch != epoch {
                let latest = persistence.read_auth_session().await?;
                let selection = self
                    .selection
                    .lock()
                    .expect("session selection mutex poisoned");
                let mut lifecycle = self.grants.lifecycle();
                if selection.login_generation != generation {
                    break;
                }
                if lifecycle.revision() != latest_epoch {
                    continue;
                }
                if latest.as_deref() != Some(blob.as_slice()) {
                    lifecycle.forget_auth_deletion();
                    return Ok(false);
                }
                epoch = latest_epoch;
            }
            if self.install_session_if_current(session.clone(), epoch, false, Some(generation)) {
                return Ok(true);
            }
        }
        self.drain_session_deletions(&persistence)
            .await
            .map_err(|reason| truapi::latest::GenericError { reason })?;
        Ok(false)
    }

    /// Finish durable cleanup left by a cancelled login write.
    pub async fn discard_login_session(&self) {
        let persistence = self.grants.persistence().await;
        if let Err(reason) = self.drain_session_deletions(&persistence).await {
            warn!(%reason, "cancelled login cleanup remains pending");
        }
    }

    #[instrument(skip_all, fields(runtime.method = "account.disconnect"))]
    async fn disconnect(&self) {
        self.cancel_login();
        let session = self.session_state.current();
        self.clear_disconnected_session(true, None).await;
        if let Some(session) = session {
            let weak_self = self.weak_self.clone();
            (self.spawner)(Box::pin(async move {
                if let Some(host) = weak_self.upgrade() {
                    let _ = host.submit_disconnected_message(&session).await;
                }
            }));
        }
    }

    /// Disconnect and discard pairing bootstrap material so the next login
    /// generates a new device keypair and topic.
    pub async fn logout_and_reset_pairing(&self) -> Result<(), String> {
        self.disconnect().await;
        self.grants
            .persistence()
            .await
            .clear_auto_signing_keys()
            .await
            .map_err(|reason| {
                format!("session disconnected, but AutoSigning reset failed: {reason}")
            })?;
        self.platform
            .clear_core_storage(CoreStorageKey::PairingDeviceIdentity)
            .await
            .map_err(|error| {
                format!(
                    "session disconnected, but pairing identity reset failed: {}",
                    error.reason
                )
            })?;
        self.platform
            .clear_core_storage(CoreStorageKey::LastProcessedPairingStatement)
            .await
            .map_err(|error| {
                format!(
                    "session disconnected and pairing identity reset, but pairing history reset failed: {}",
                    error.reason
                )
            })
    }

    /// Clear all capability material owned by one product while preserving the
    /// active session and unrelated products.
    pub async fn clear_product_state(&self, product_id: &str) -> Result<(), String> {
        let product_id =
            normalize_product_identifier(product_id).map_err(|error| error.to_string())?;
        let session = {
            let mut lifecycle = self.grants.lifecycle();
            lifecycle.advance();
            self.session_state.current()
        };
        self.grants
            .persistence()
            .await
            .clear_product(session.as_ref(), &product_id)
            .await
    }

    /// Clear the canonical local session and all session capabilities without
    /// sending a peer-disconnect statement. Reports the resulting auth state to
    /// the host, including when there was no session to clear.
    pub async fn reset_session_state(&self) {
        self.cancel_login();
        self.clear_disconnected_session(true, None).await;
        self.auth_state.announce_current();
    }

    /// Invalidate in-flight login attempts and emit the cancelled auth state.
    #[instrument(skip_all, fields(runtime.method = "account.cancel_login"))]
    pub fn cancel_login(&self) {
        self.invalidate_login_attempts();
        self.auth_state.login_cancelled();
    }

    fn begin_login_attempt(&self) -> u64 {
        let mut selection = self
            .selection
            .lock()
            .expect("session selection mutex poisoned");
        selection.login_generation = selection.login_generation.wrapping_add(1);
        selection.login_generation
    }

    fn invalidate_login_attempts(&self) {
        let mut selection = self
            .selection
            .lock()
            .expect("session selection mutex poisoned");
        selection.login_generation = selection.login_generation.wrapping_add(1);
    }

    fn is_current_login_attempt(&self, generation: u64) -> bool {
        self.selection
            .lock()
            .expect("session selection mutex poisoned")
            .login_generation
            == generation
    }

    fn login_waiter(&self) -> Option<oneshot::Receiver<Result<(), String>>> {
        let mut in_flight = self
            .login_in_flight
            .lock()
            .expect("login in-flight mutex poisoned");
        if let Some(in_flight) = in_flight.as_mut() {
            let (tx, rx) = oneshot::channel();
            in_flight.waiters.push(tx);
            Some(rx)
        } else {
            *in_flight = Some(LoginInFlight {
                waiters: Vec::new(),
            });
            None
        }
    }

    fn finish_login_in_flight(&self, result: Result<(), String>) {
        let waiters = self
            .login_in_flight
            .lock()
            .expect("login in-flight mutex poisoned")
            .take()
            .map(|in_flight| in_flight.waiters)
            .unwrap_or_default();
        for waiter in waiters {
            let _ = waiter.send(result.clone());
        }
    }

    fn begin_session_clear(
        &self,
        clear_auth_session: bool,
        expected_epoch: Option<u64>,
        expected_peer: Option<SsoSessionKey>,
    ) -> bool {
        let mut selection = self
            .selection
            .lock()
            .expect("session selection mutex poisoned");
        let mut lifecycle = self.grants.lifecycle();
        if expected_epoch.is_some_and(|epoch| lifecycle.revision() != epoch)
            || expected_peer.is_some_and(|key| !self.current_sso_session_matches(key))
        {
            return false;
        }
        let previous = self.session_state.current();
        lifecycle.revoke_session(previous.as_ref(), clear_auth_session);
        selection.external_session_active = false;
        self.session_state.clear_session();
        let monitor = self.detach_session_channel(previous.as_ref());
        drop(lifecycle);
        drop(selection);
        self.stop_session_channel(previous.as_ref(), monitor);
        true
    }

    async fn drain_session_deletions(
        &self,
        persistence: &HostGrantPersistence<'_>,
    ) -> Result<(), String> {
        if !persistence.begin_cleanup() {
            return Ok(());
        }
        self.auth_state.store_disconnected();
        persistence.drain_cleanup().await
    }

    #[instrument(skip_all, fields(runtime.method = "session_store.clear_disconnected"))]
    async fn clear_disconnected_session(
        &self,
        clear_auth_session: bool,
        expected_epoch: Option<u64>,
    ) {
        if !self.begin_session_clear(clear_auth_session, expected_epoch, None) {
            return;
        }
        let persistence = self.grants.persistence().await;
        if let Err(reason) = self.drain_session_deletions(&persistence).await {
            warn!(%reason, "session cleanup remains pending");
        }
    }

    async fn prepare_session_installation(
        &self,
        persistence: &HostGrantPersistence<'_>,
        session: &SessionInfo,
    ) -> Result<(), String> {
        self.drain_session_deletions(persistence).await?;
        persistence
            .prepare_session(self.session_state.current().as_ref(), session)
            .await;
        Ok(())
    }

    async fn set_connected_session_if_current(
        &self,
        session: SessionInfo,
        activation_epoch: u64,
        external_session: bool,
    ) -> Result<bool, String> {
        let persistence = self.grants.persistence().await;
        if self.grants.lifecycle().revision() != activation_epoch {
            return Ok(false);
        }
        self.prepare_session_installation(&persistence, &session)
            .await?;
        Ok(self.install_session_if_current(session, activation_epoch, external_session, None))
    }

    fn install_session_if_current(
        &self,
        session: SessionInfo,
        activation_epoch: u64,
        external_session: bool,
        expected_login_generation: Option<u64>,
    ) -> bool {
        let detached = {
            let mut selection = self
                .selection
                .lock()
                .expect("session selection mutex poisoned");
            if expected_login_generation
                .is_some_and(|expected| expected != selection.login_generation)
            {
                return false;
            }
            let mut lifecycle = self.grants.lifecycle();
            if lifecycle.revision() != activation_epoch {
                return false;
            }
            if expected_login_generation.is_none() {
                selection.login_generation = selection.login_generation.wrapping_add(1);
            }
            let previous = self.session_state.current();
            let detached = (previous.as_ref() != Some(&session)).then(|| {
                let monitor = self.detach_session_channel(previous.as_ref());
                (previous, monitor)
            });
            lifecycle.forget_auth_deletion();
            self.session_state.set_session(session.clone());
            selection.external_session_active = external_session;
            detached
        };
        if let Some((previous, monitor)) = detached {
            self.stop_session_channel(previous.as_ref(), monitor);
        }
        self.start_disconnect_monitor(&session);
        vrf::prefetch(&self.spawner);
        self.auth_state
            .connected(&connected_session_ui_info(&session));
        true
    }

    /// Retained grants exercised by runtime lifecycle tests.
    #[cfg(test)]
    pub fn grants_for_tests(&self) -> &HostGrantStore {
        &self.grants
    }

    #[cfg(test)]
    pub async fn set_connected_session_for_tests(&self, session: SessionInfo) {
        let epoch = self.advance_session_lifecycle();
        self.set_connected_session_if_current(session, epoch, false)
            .await
            .expect("test session installs");
    }

    #[cfg(test)]
    pub async fn register_ring_vrf_key_for_tests(
        &self,
        session: &SessionInfo,
        handle: v01::ProductAccountId,
        ring: v01::RingLocation,
        public_key: [u8; 32],
    ) -> Result<(), RingVrfError> {
        self.ring_vrf_registry
            .register(session.public_key, handle, ring, public_key)
            .await
    }

    /// Single funnel for peer-initiated disconnects. Every detection source
    /// (monitor task, request-path error) must route here: it wakes in-flight
    /// waiters for `key`, then clears the session when `key` is still current,
    /// so stale notifications for replaced sessions only wake their own
    /// waiters.
    async fn handle_signing_host_disconnected(&self, key: SsoSessionKey) {
        self.session_disconnects
            .notify_key(key, SSO_PEER_DISCONNECT_REASON);
        if !self.begin_session_clear(true, None, Some(key)) {
            return;
        }
        let persistence = self.grants.persistence().await;
        if let Err(reason) = self.drain_session_deletions(&persistence).await {
            warn!(%reason, "session cleanup remains pending");
        }
    }

    fn current_sso_session_matches(&self, key: SsoSessionKey) -> bool {
        key.matches(&self.session_state)
    }

    /// Whether resolving `product_id`'s subtree would reach the Account Holder,
    /// i.e. neither the memory cache nor the persisted slot already holds it.
    ///
    /// A stale or missing session resolves as `false` so a broken state falls
    /// through to the resolution's own error rather than a spurious prompt.
    pub async fn subtree_reaches_account_holder(
        &self,
        session: &AuthoritySession,
        product_id: &str,
    ) -> bool {
        let Ok(session) = self.current_private_session(session) else {
            return false;
        };
        let Some(sso) = session.sso.as_ref() else {
            return false;
        };
        let cache_key = (SsoSessionKey::from_session(sso), product_id.to_string());
        self.grants
            .known_product_subtree(&self.session_state, &session, cache_key)
            .await
            .is_none()
    }

    fn current_private_session(
        &self,
        session: &AuthoritySession,
    ) -> Result<SessionInfo, AuthorityError> {
        require_current_session(&self.session_state, session)
    }

    fn operation_session(
        &self,
        operation: &HostOperation,
    ) -> Result<(SessionInfo, u64), AuthorityError> {
        let lifecycle = self.grants.lifecycle();
        lifecycle.require(operation)?;
        let session = self.current_private_session(&operation.session)?;
        Ok((session, lifecycle.revision()))
    }

    async fn refresh_current_session_identity(&self) -> Option<AuthoritySession> {
        let (current, epoch) = {
            let lifecycle = self.grants.lifecycle();
            (self.session_state.current()?, lifecycle.revision())
        };
        if current.has_username() || self.host_config.asset_hub_chain_genesis_hash == [0; 32] {
            return Some(authority_session(&current));
        }

        let resolved = resolve_session_identity_with_chain(
            &self.chain,
            self.host_config.asset_hub_chain_genesis_hash,
            current.clone(),
        )
        .await;
        if !resolved.has_username() || resolved == current {
            return self.current_session();
        }

        let persistence = self.grants.persistence().await;
        if let Err(reason) = self.drain_session_deletions(&persistence).await {
            warn!(%reason, "session cleanup remains pending");
            return self.current_session();
        }
        {
            let lifecycle = self.grants.lifecycle();
            if lifecycle.revision() != epoch
                || !self
                    .session_state
                    .replace_session_if_current(&current, resolved.clone())
            {
                return self.current_session();
            }
        }
        self.auth_state
            .connected(&connected_session_ui_info(&resolved));
        if let Err(err) = persistence
            .write_auth_session(encode_persisted_session(&resolved))
            .await
        {
            warn!(reason = %err.reason, "refreshed session identity persist failed");
        }
        if let Err(reason) = self.drain_session_deletions(&persistence).await {
            warn!(%reason, "session cleanup remains pending");
        }
        self.current_session()
    }

    /// Keep access checks and key derivation bound to the same canonical owner.
    async fn require_ring_vrf_key_access(
        &self,
        caller: AccountCaller<'_>,
        handle: &v01::ProductAccountId,
    ) -> Result<
        (
            v01::ProductAccountId,
            crate::runtime::product_manifest::AuthorizedAccess,
        ),
        RingVrfError,
    > {
        let access = crate::runtime::product_manifest::ring_vrf_key_access_granted(
            &self.services,
            self.platform.as_ref(),
            caller,
            handle,
        )
        .await?;
        Ok((
            v01::ProductAccountId {
                dot_ns_identifier: access.owner.clone(),
                derivation_index: handle.derivation_index.clone(),
            },
            access,
        ))
    }

    async fn local_ring_vrf_entropy(
        &self,
        session: &SessionInfo,
        handle: &v01::ProductAccountId,
    ) -> Result<Option<Zeroizing<[u8; 32]>>, RingVrfError> {
        let Some(auto_signing) = self
            .grants
            .auto_signing_key(session, &handle.dot_ns_identifier)
            .await
            .map_err(RingVrfError::from)?
        else {
            return Ok(None);
        };
        let entry = self
            .ring_vrf_registry
            .entry(session.public_key, handle)
            .await?
            .ok_or(RingVrfError::KeyNotRegistered)?;
        let entropy = Zeroizing::new(derive_ring_vrf_entropy_from_domain(
            auto_signing.ring_vrf_domain_entropy(),
            &handle.derivation_index,
        ));
        let vrf = vrf::load().await?;
        if entry.public_key != Some(vrf.member(&entropy)?) {
            return Err(RingVrfError::Unknown {
                reason: "registered ring-VRF public key does not match the AutoSigning capability"
                    .to_string(),
            });
        }
        Ok(Some(entropy))
    }

    async fn local_ring_vrf_entropy_for_ring(
        &self,
        session: &SessionInfo,
        handle: &v01::ProductAccountId,
        ring: &v01::RingLocation,
    ) -> Result<Option<Zeroizing<[u8; 32]>>, RingVrfError> {
        let Some(entropy) = self.local_ring_vrf_entropy(session, handle).await? else {
            return Ok(None);
        };
        let entry = self
            .ring_vrf_registry
            .entry(session.public_key, handle)
            .await?
            .ok_or(RingVrfError::KeyNotRegistered)?;
        if !entry.rings.contains(ring) {
            return Err(RingVrfError::KeyNotInRing);
        }
        Ok(Some(entropy))
    }

    fn mirror_ring_vrf_registration(
        &self,
        session: SessionInfo,
        request: ProductRequest<HostAccountRegisterRingVrfKeyRequest>,
    ) {
        let weak_self = self.weak_self.clone();
        (self.spawner)(Box::pin(async move {
            let Some(host) = weak_self.upgrade() else {
                return;
            };
            let cx = CallContext::with_request_id(format!(
                "ring-vrf-registration-mirror:{}",
                sso_message_id()
            ));
            if let Err(error) = host
                .remote_register_ring_vrf_key(&cx, &session, request)
                .await
            {
                warn!(?error, "ring-VRF registration mirror failed");
            }
        }));
    }

    async fn product_subtree_public_key(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        product_id: String,
    ) -> Result<[u8; 32], AuthorityError> {
        let session = self.current_private_session(session)?;
        self.remote_product_subtree_public_key(cx, &session, product_id)
            .await
    }

    /// The keypair that can serve `account` locally under an AutoSigning
    /// capability, when this host holds one.
    ///
    /// The single source of truth for local serviceability: the grant
    /// predicate and every local fast path call it, so a request can never be
    /// told "no prompt" and then relayed to a signing host that will prompt
    /// for it.
    ///
    /// A caller may name an account belonging to another product —
    /// `authorized_product_account` admits that for a `localhost` caller, and
    /// for one the owner granted `context` — and the grant covers the owning
    /// product's subtree, not the caller's. Serving one locally would sign with a key the caller was
    /// never granted and skip the confirmation the signing host raises for a
    /// relayed request, so the binding is checked here rather than only at the
    /// gate.
    async fn local_product_signing_key(
        &self,
        session: &SessionInfo,
        calling_product_id: Option<&str>,
        account: &v01::ProductAccountId,
    ) -> Result<Option<schnorrkel::Keypair>, AuthorityError> {
        if calling_product_id != Some(account.dot_ns_identifier.as_str()) {
            return Ok(None);
        }
        let Some(key) = self
            .grants
            .auto_signing_key(session, &account.dot_ns_identifier)
            .await?
        else {
            return Ok(None);
        };
        derive_product_keypair_from_subtree_secret(
            *key.as_secret_bytes(),
            derivation_index_bytes(&account.derivation_index),
        )
        .map(Some)
        .map_err(|err| AuthorityError::Unknown {
            reason: err.to_string(),
        })
    }

    async fn sign_vrf(
        &self,
        invocation: AccountInvocation<'_>,
        request: v01::HostAccountSignVrfRequest,
    ) -> Result<v01::VrfSignature, AuthorityError> {
        let session = invocation.session;
        let cx = invocation.call;
        let calling_product_id = invocation
            .caller
            .product_id()
            .ok_or(AuthorityError::Rejected)?;
        let session = self.current_private_session(session)?;
        if calling_product_id == request.account.dot_ns_identifier
            && let Some(auto_signing_key) = self
                .grants
                .auto_signing_key(&session, &request.account.dot_ns_identifier)
                .await?
        {
            let keypair = derive_product_keypair_from_subtree_secret(
                *auto_signing_key.as_secret_bytes(),
                derivation_index_bytes(&request.account.derivation_index),
            )
            .map_err(|err| AuthorityError::Unknown {
                reason: err.to_string(),
            })?;
            let (pre_output, proof) = crate::dynamic_vrf::sign_dynamic_vrf(
                &keypair,
                &request.transcript_label,
                request
                    .items
                    .iter()
                    .map(|item| (item.label.as_slice(), item.value.as_slice())),
            );
            return Ok(v01::VrfSignature { pre_output, proof });
        }
        if !super::authority::is_blessed_owner(
            calling_product_id,
            &request.account.dot_ns_identifier,
        ) {
            let confirmed = super::until_cancelled(
                cx,
                self.platform
                    .confirm_user_action(UserConfirmationReview::SignVrf(SignVrfReview {
                        calling_product_id: calling_product_id.to_string(),
                        request: request.clone(),
                    })),
            )
            .await?
            .map_err(|err| AuthorityError::Unknown {
                reason: format!("VRF signing confirmation failed: {err:?}"),
            })?;
            if !confirmed {
                return Err(AuthorityError::Rejected);
            }
        }
        self.remote_sign_vrf(cx, &session, calling_product_id.to_string(), request)
            .await
    }

    async fn account_alias(
        &self,
        invocation: AccountInvocation<'_>,
        request: HostAccountGetAliasRequest,
    ) -> Result<v01::ContextualAlias, RingVrfError> {
        let session = invocation.session;
        let cx = invocation.call;
        let calling_product_id = invocation
            .caller
            .product_id()
            .ok_or(RingVrfError::NotAllowlisted)?;
        let private_session = self.current_private_session(session)?;
        if calling_product_id == request.key_handle.dot_ns_identifier
            && let Some(entropy) = self
                .local_ring_vrf_entropy_for_ring(
                    &private_session,
                    &request.key_handle,
                    &request.ring_location,
                )
                .await?
        {
            self.ring_resolver.validate(&request.ring_location).await?;
            let vrf = vrf::load().await?;
            self.current_private_session(session)?;
            let context = development_context_bytes(&request.context);
            let alias = vrf.alias(&entropy, &context)?;
            return Ok(v01::ContextualAlias {
                context,
                alias: alias.to_vec(),
            });
        }
        self.remote_account_alias(
            cx,
            &private_session,
            ProductRequest {
                calling_product_id: calling_product_id.to_string(),
                payload: request,
            },
        )
        .await
    }

    async fn create_proof(
        &self,
        invocation: AccountInvocation<'_>,
        request: HostAccountCreateProofRequest,
    ) -> Result<v01::HostAccountCreateProofResponse, RingVrfError> {
        let session = invocation.session;
        let cx = invocation.call;
        let calling_product_id = invocation
            .caller
            .product_id()
            .ok_or(RingVrfError::NotAllowlisted)?;
        let (key_handle, access) = self
            .require_ring_vrf_key_access(invocation.caller, &request.key_handle)
            .await?;
        // A publisher's grant cannot expose its alias in an unrelated product's context.
        crate::runtime::product_manifest::require_own_context(&access, &request.context)?;
        let private_session = self.current_private_session(session)?;
        if let Some(entropy) = self
            .local_ring_vrf_entropy_for_ring(&private_session, &key_handle, &request.ring_location)
            .await?
        {
            let vrf = vrf::load().await?;
            let member = vrf.member(&entropy)?;
            let resolved = self
                .ring_resolver
                .resolve(&request.ring_location, &[MemberCandidate { member }])
                .await?;
            self.current_private_session(session)?;
            let context = development_context_bytes(&request.context);
            let (proof, alias) =
                create_proof(&vrf, &entropy, &resolved, &context, &request.message)?;
            return Ok(v01::HostAccountCreateProofResponse {
                proof,
                contextual_alias: v01::ContextualAlias {
                    context,
                    alias: alias.to_vec(),
                },
                ring_index: resolved.ring_index,
                ring_revision: resolved.ring_revision,
            });
        }
        self.remote_create_proof(
            cx,
            &private_session,
            ProductRequest {
                calling_product_id: calling_product_id.to_string(),
                payload: request,
            },
        )
        .await
    }

    async fn register_ring_vrf_key(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        request: ProductRequest<HostAccountRegisterRingVrfKeyRequest>,
    ) -> Result<[u8; 32], RingVrfError> {
        let private_session = self.current_private_session(session)?;
        let handle = v01::ProductAccountId {
            dot_ns_identifier: normalize_product_identifier(&request.calling_product_id).map_err(
                |error| RingVrfError::Unknown {
                    reason: error.to_string(),
                },
            )?,
            derivation_index: request.payload.index.clone(),
        };
        if let Some(auto_signing) = self
            .grants
            .auto_signing_key(&private_session, &request.calling_product_id)
            .await
            .map_err(RingVrfError::from)?
        {
            self.ring_resolver.validate(&request.payload.ring).await?;
            let vrf = vrf::load().await?;
            self.current_private_session(session)?;
            let entropy = Zeroizing::new(derive_ring_vrf_entropy_from_domain(
                auto_signing.ring_vrf_domain_entropy(),
                &request.payload.index,
            ));
            let public_key = vrf.member(&entropy)?;
            self.ring_vrf_registry
                .register(
                    private_session.public_key,
                    handle,
                    request.payload.ring.clone(),
                    public_key,
                )
                .await?;
            self.current_private_session(session)?;
            self.mirror_ring_vrf_registration(private_session, request);
            return Ok(public_key);
        }
        let public_key = self
            .remote_register_ring_vrf_key(cx, &private_session, request.clone())
            .await?;
        self.ring_vrf_registry
            .register(
                private_session.public_key,
                handle,
                request.payload.ring,
                public_key,
            )
            .await?;
        self.current_private_session(session)?;
        Ok(public_key)
    }

    async fn list_ring_vrf_keys(
        &self,
        invocation: AccountInvocation<'_>,
        request: HostAccountListRingVrfKeysRequest,
    ) -> Result<Vec<v01::RegisteredRingVrfKey>, RingVrfError> {
        let session = invocation.session;
        let cx = invocation.call;
        let calling_product_id = invocation
            .caller
            .product_id()
            .ok_or(RingVrfError::NotAllowlisted)?;
        let private_session = self.current_private_session(session)?;
        let owner = normalize_product_identifier(&request.owner).map_err(|error| {
            RingVrfError::Unknown {
                reason: error.to_string(),
            }
        })?;
        if calling_product_id == owner
            && let Some(mut entries) = self
                .ring_vrf_registry
                .complete_owner_entries(private_session.public_key, &owner)
                .await?
        {
            self.current_private_session(session)?;
            apply_ring_vrf_disclosure(&mut entries, request.disclosure);
            return Ok(entries);
        }
        let requested_disclosure = request.disclosure;
        let mut remote_request = ProductRequest {
            calling_product_id: calling_product_id.to_string(),
            payload: request,
        };
        if remote_request.calling_product_id == owner {
            remote_request.payload.disclosure = v01::RingVrfKeyDisclosure::PublicKey;
        }
        let mut entries = self
            .remote_list_ring_vrf_keys(cx, &private_session, remote_request)
            .await?;
        validate_owner_listing(&owner, &entries)?;
        if entries.iter().all(|entry| entry.public_key.is_some()) {
            entries = self
                .ring_vrf_registry
                .reconcile_owner(private_session.public_key, &owner, entries)
                .await?;
        }
        self.current_private_session(session)?;
        apply_ring_vrf_disclosure(&mut entries, requested_disclosure);
        Ok(entries)
    }

    async fn ring_vrf_sign(
        &self,
        invocation: AccountInvocation<'_>,
        request: HostAccountRingVrfSignRequest,
    ) -> Result<Vec<u8>, RingVrfError> {
        let session = invocation.session;
        let cx = invocation.call;
        let calling_product_id = invocation
            .caller
            .product_id()
            .ok_or(RingVrfError::NotAllowlisted)?;
        let (key_handle, _access) = self
            .require_ring_vrf_key_access(invocation.caller, &request.key_handle)
            .await?;
        let private_session = self.current_private_session(session)?;
        if let Some(entropy) = self
            .local_ring_vrf_entropy(&private_session, &key_handle)
            .await?
        {
            let vrf = vrf::load().await?;
            self.current_private_session(session)?;
            return vrf.sign(&entropy, &request.message);
        }
        self.remote_ring_vrf_sign(
            cx,
            &private_session,
            ProductRequest {
                calling_product_id: calling_product_id.to_string(),
                payload: request,
            },
        )
        .await
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
                        self.grants.cache_statement_store_allowance_key(&self.session_state,
                            session,
                            lifecycle_epoch,
                            product_id,
                            slot_account_key.clone(),
                        )
                        .await?;
                    }
                    SsoAllocatedResource::BulletinAllowance { slot_account_key } => {
                        self.grants.cache_bulletin_allowance_key(&self.session_state,
                            session,
                            lifecycle_epoch,
                            product_id,
                            slot_account_key.clone(),
                        )
                        .await?;
                    }
                    SsoAllocatedResource::SmartContractAllowance => {}
                    SsoAllocatedResource::AutoSigning {
                        product_root_private_key,
                        ring_vrf_domain_entropy,
                    } => {
                        let expected_product_subtree_public_key = self
                            .remote_product_subtree_public_key(cx, session, product_id.to_string())
                            .await?;
                        self.grants.remember_auto_signing_key(&self.session_state,
                            session,
                            lifecycle_epoch,
                            product_id,
                            expected_product_subtree_public_key,
                            AutoSigningKey::from_parts(*product_root_private_key, *ring_vrf_domain_entropy),
                        )
                        .await?;
                    }
                }
            }
        }
        Ok(())
    }

    async fn sign_statement_store_product_payload(
        &self,
        invocation: AccountInvocation<'_>,
        account: v01::ProductAccountId,
        payload: Vec<u8>,
    ) -> Result<[u8; 64], AuthorityError> {
        let (session, operation) = {
            let lifecycle = self.grants.lifecycle();
            (
                self.current_private_session(invocation.session)?,
                lifecycle.capture(invocation.session.clone()),
            )
        };
        if !matches!(invocation.caller, AccountCaller::Local { product, .. } if product.product_id == account.dot_ns_identifier)
        {
            invocation
                .confirm(
                    self.platform.as_ref(),
                    UserConfirmationReview::StatementStoreProductSign(
                        crate::platform::StatementStoreProductSignReview {
                            calling_product_id: invocation.caller.product_id().map(str::to_string),
                            account: account.clone(),
                            payload: payload.clone(),
                        },
                    ),
                )
                .await?;
        }
        let cx = match invocation.caller {
            AccountCaller::Local { .. } => super::remote_authority_context(invocation.call),
            AccountCaller::Remote { .. } => invocation.call.clone(),
        };
        super::remote_authority_call(&cx, async {
            let keypair = match invocation.caller {
                AccountCaller::Local { product, .. } => self.local_product_signing_key(&session, Some(&product.product_id), &account).await?,
                AccountCaller::Remote { .. } => None,
            };
            let Some(keypair) = keypair else {
                return Err(AuthorityError::Unavailable { reason: "pairing host: exact statement proof signing needs an AutoSigning capability; the current SSO raw-signing protocol cannot carry it".to_string() });
            };
            let lifecycle = self.grants.lifecycle();
            lifecycle.require(&operation)?;
            self.current_private_session(invocation.session)?;
            Ok(keypair.secret.sign_simple(SR25519_SIGNING_CONTEXT, &payload, &keypair.public).to_bytes())
        }).await
    }

    fn derive_entropy(
        &self,
        session: &AuthoritySession,
        product_id: &str,
        context: &[u8],
    ) -> Result<[u8; 32], AuthorityError> {
        let session = self.current_private_session(session)?;
        if session.sso.is_none() {
            return Err(AuthorityError::Disconnected);
        }
        let root_entropy_source =
            session
                .root_entropy_source
                .ok_or_else(|| AuthorityError::Unavailable {
                    reason: "Session secret missing".to_string(),
                })?;
        derive_product_entropy_from_source(&root_entropy_source, product_id, context).map_err(
            |err| AuthorityError::Unknown {
                reason: err.to_string(),
            },
        )
    }

    /// Contact-handle key from the wallet-provided root entropy source.
    ///
    /// Unlike `derive_entropy` this does not require a live SSO channel: it
    /// needs the key material the session already carries, not a round trip to
    /// the wallet.
    fn contacts_handle_key(&self, session: &AuthoritySession) -> Result<[u8; 32], AuthorityError> {
        let session = self.current_private_session(session)?;
        let root_entropy_source =
            session
                .root_entropy_source
                .ok_or_else(|| AuthorityError::Unavailable {
                    reason: "Session secret missing".to_string(),
                })?;
        Ok(crate::runtime::contacts::handle_key_from_root_source(
            &root_entropy_source,
        ))
    }
}

fn apply_ring_vrf_disclosure(
    entries: &mut [v01::RegisteredRingVrfKey],
    disclosure: v01::RingVrfKeyDisclosure,
) {
    if disclosure == v01::RingVrfKeyDisclosure::Anonymized {
        for entry in entries {
            entry.public_key = None;
        }
    }
}

fn login_error_reason(err: &CallError<HostRequestLoginError>) -> String {
    match err {
        CallError::Domain(HostRequestLoginError::V1(v01::HostRequestLoginError::Unknown {
            reason,
        }))
        | CallError::HostFailure { reason } => reason.clone(),
        CallError::Unsupported => "login unsupported".to_string(),
        CallError::Denied => "login denied".to_string(),
        CallError::MalformedFrame { reason } => reason.clone(),
        CallError::Cancelled => "login cancelled".to_string(),
    }
}

#[async_trait::async_trait]
impl ProductAuthority for PairingHost {
    fn account_holder(&self) -> &dyn AccountHolder {
        self
    }

    fn current_operation(&self) -> Option<HostOperation> {
        let lifecycle = self.grants.lifecycle();
        self.current_session()
            .map(|session| lifecycle.capture(session))
    }

    fn require_current_operation(&self, operation: &HostOperation) -> Result<(), AuthorityError> {
        self.operation_session(operation).map(|_| ())
    }

    async fn allocate_resources(
        &self,
        cx: &CallContext,
        operation: &HostOperation,
        product: &ProductContext,
        request: v01::HostRequestResourceAllocationRequest,
    ) -> Result<v01::HostRequestResourceAllocationResponse, AuthorityError> {
        let (session, lifecycle_epoch) = self.operation_session(operation)?;
        let confirmed = super::until_cancelled(cx, async {
            if crate::platform::has_trusted_remote_permissions(&product.product_id) {
                return Ok(true);
            }
            self.platform
                .confirm_user_action(UserConfirmationReview::ResourceAllocation(
                    crate::platform::ResourceAllocationReview {
                        calling_product_id: product.product_id.clone(),
                        resources: request.resources.clone(),
                    },
                ))
                .await
        })
        .await?
        .map_err(AuthorityError::ConfirmationFailed)?;
        if !confirmed {
            return Err(AuthorityError::Unknown {
                reason: "User rejected resource allocation".to_string(),
            });
        }
        let cx = super::remote_authority_context_with_default(
            cx,
            super::RESOURCE_ALLOCATION_REMOTE_AUTHORITY_RESPONSE_TIMEOUT,
        );
        super::remote_authority_call(&cx, async {
            self.require_current_operation(operation)?;
            let outcomes = self
                .remote_allocate_resources(&cx, &session, product.product_id.clone(), request)
                .await?;
            self.cache_allowance_outcomes(
                &cx,
                &session,
                lifecycle_epoch,
                &product.product_id,
                &outcomes,
            )
            .await?;
            self.require_current_operation(operation)?;
            Ok(v01::HostRequestResourceAllocationResponse {
                outcomes: outcomes.into_iter().map(Into::into).collect(),
            })
        })
        .await
    }

    fn session_state(&self) -> Arc<SessionState> {
        PairingHost::session_state(self)
    }

    #[cfg(test)]
    fn cache_product_subtree_for_test(
        &self,
        session: &SessionInfo,
        product_id: &str,
        public_key: [u8; 32],
    ) {
        self.grants
            .cache_product_subtree_for_test(session, product_id, public_key);
    }

    async fn request_login(
        &self,
        product: &ProductContext,
    ) -> Result<HostRequestLoginResponse, CallError<HostRequestLoginError>> {
        PairingHost::request_login(self, product).await
    }

    async fn disconnect(&self) {
        PairingHost::disconnect(self).await;
    }

    async fn refresh_session_identity(&self) -> Option<AuthoritySession> {
        self.refresh_current_session_identity().await
    }

    async fn subtree_resolution_reaches_account_holder(
        &self,
        session: &AuthoritySession,
        product_id: &str,
    ) -> bool {
        PairingHost::subtree_reaches_account_holder(self, session, product_id).await
    }

    fn wallet_authorization(
        &self,
        operation: &HostOperation,
        _product: &ProductContext,
    ) -> Result<Option<super::WalletAuthorization>, AuthorityError> {
        self.require_current_operation(operation)?;
        Ok(None)
    }

    async fn statement_store_allowance_key(
        &self,
        cx: &CallContext,
        operation: &HostOperation,
        product_id: String,
    ) -> Result<StatementStoreAllowanceKey, AuthorityError> {
        let (session, lifecycle_epoch) = self.operation_session(operation)?;
        self.remote_statement_store_allowance_key(cx, &session, lifecycle_epoch, product_id)
            .await
    }

    async fn bulletin_allowance_key(
        &self,
        cx: &CallContext,
        operation: &HostOperation,
        product_id: String,
    ) -> Result<BulletinAllowanceKey, AuthorityError> {
        let (session, lifecycle_epoch) = self.operation_session(operation)?;
        self.remote_bulletin_allowance_key(cx, &session, lifecycle_epoch, product_id)
            .await
    }

    async fn refresh_bulletin_allowance_key(
        &self,
        cx: &CallContext,
        operation: &HostOperation,
        product_id: String,
    ) -> Result<BulletinAllowanceKey, AuthorityError> {
        let (session, lifecycle_epoch) = self.operation_session(operation)?;
        self.remote_refresh_bulletin_allowance_key(cx, &session, lifecycle_epoch, product_id)
            .await
    }
}

#[async_trait::async_trait]
impl AccountHolder for PairingHost {
    fn current_session(&self) -> Option<AuthoritySession> {
        PairingHost::current_session(self)
    }

    async fn product_subtree_public_key(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        product_id: String,
    ) -> Result<[u8; 32], AuthorityError> {
        PairingHost::product_subtree_public_key(self, cx, session, product_id).await
    }

    async fn sign_vrf(
        &self,
        invocation: AccountInvocation<'_>,
        request: v01::HostAccountSignVrfRequest,
    ) -> Result<v01::VrfSignature, AuthorityError> {
        PairingHost::sign_vrf(self, invocation, request).await
    }

    async fn sign_payload(
        &self,
        invocation: AccountInvocation<'_>,
        request: SignPayloadAuthorityRequest,
    ) -> Result<v01::HostSignPayloadResponse, AuthorityError> {
        let (session, operation) = {
            let lifecycle = self.grants.lifecycle();
            (
                self.current_private_session(invocation.session)?,
                lifecycle.capture(invocation.session.clone()),
            )
        };
        let keypair = if let AccountCaller::Local { product, .. } = invocation.caller
            && let SignPayloadAuthorityRequest::Product(payload) = &request
        {
            self.local_product_signing_key(&session, Some(&product.product_id), &payload.account)
                .await?
        } else {
            None
        };
        if keypair.is_none() && matches!(invocation.caller, AccountCaller::Local { .. }) {
            invocation
                .confirm(self.platform.as_ref(), request.review(invocation.caller))
                .await?;
        }
        let cx = match invocation.caller {
            AccountCaller::Local { .. } => super::remote_authority_context(invocation.call),
            AccountCaller::Remote { .. } => invocation.call.clone(),
        };
        super::remote_authority_call(&cx, async {
            self.require_current_operation(&operation)?;
            if let Some(keypair) = keypair
                && let SignPayloadAuthorityRequest::Product(payload) = request
            {
                let lifecycle = self.grants.lifecycle();
                lifecycle.require(&operation)?;
                self.current_private_session(invocation.session)?;
                return Ok(sign_extrinsic_payload(&keypair, payload.payload)?);
            }
            self.remote_sign_payload(&cx, &session, request).await
        })
        .await
    }

    async fn sign_raw(
        &self,
        invocation: AccountInvocation<'_>,
        request: SignRawAuthorityRequest,
        watermarked: bool,
    ) -> Result<v01::HostSignPayloadResponse, AuthorityError> {
        let (session, operation) = {
            let lifecycle = self.grants.lifecycle();
            (
                self.current_private_session(invocation.session)?,
                lifecycle.capture(invocation.session.clone()),
            )
        };
        if !matches!(request, SignRawAuthorityRequest::Product(_))
            && matches!(invocation.caller, AccountCaller::Local { .. })
        {
            invocation
                .confirm(
                    self.platform.as_ref(),
                    request.review(invocation.caller, watermarked),
                )
                .await?;
        }
        let keypair = if let AccountCaller::Local { product, .. } = invocation.caller
            && watermarked
        {
            let account = match &request {
                SignRawAuthorityRequest::Product(payload) => Some(&payload.account),
                SignRawAuthorityRequest::LegacyAccount {
                    product_account, ..
                } => Some(product_account),
                SignRawAuthorityRequest::IdentityAccount { .. } => None,
            };
            if let Some(account) = account {
                self.local_product_signing_key(&session, Some(&product.product_id), account)
                    .await?
            } else {
                None
            }
        } else {
            None
        };
        if keypair.is_none()
            && matches!(request, SignRawAuthorityRequest::Product(_))
            && matches!(invocation.caller, AccountCaller::Local { .. })
        {
            invocation
                .confirm(
                    self.platform.as_ref(),
                    request.review(invocation.caller, watermarked),
                )
                .await?;
        }
        let cx = match invocation.caller {
            AccountCaller::Local { .. } => super::remote_authority_context(invocation.call),
            AccountCaller::Remote { .. } => invocation.call.clone(),
        };
        super::remote_authority_call(&cx, async {
            self.require_current_operation(&operation)?;
            if let Some(keypair) = keypair {
                let payload = match request {
                    SignRawAuthorityRequest::Product(request) => request.payload,
                    SignRawAuthorityRequest::LegacyAccount { request, .. }
                    | SignRawAuthorityRequest::IdentityAccount { request, .. } => request.payload,
                };
                let message = raw_payload_bytes(payload, watermarked)?;
                let lifecycle = self.grants.lifecycle();
                lifecycle.require(&operation)?;
                self.current_private_session(invocation.session)?;
                let signature = keypair
                    .secret
                    .sign_simple(SR25519_SIGNING_CONTEXT, &message, &keypair.public)
                    .to_bytes();
                return Ok(v01::HostSignPayloadResponse {
                    signature: signature.to_vec(),
                    signed_transaction: None,
                });
            }
            self.remote_sign_raw(&cx, &session, request, watermarked)
                .await
        })
        .await
    }

    async fn create_transaction(
        &self,
        invocation: AccountInvocation<'_>,
        request: CreateTransactionAuthorityRequest,
    ) -> Result<v01::HostCreateTransactionResponse, AuthorityError> {
        let (session, operation) = {
            let lifecycle = self.grants.lifecycle();
            (
                self.current_private_session(invocation.session)?,
                lifecycle.capture(invocation.session.clone()),
            )
        };
        let keypair = if let AccountCaller::Local { product, .. } = invocation.caller
            && let CreateTransactionAuthorityRequest::Product(payload) = &request
        {
            self.local_product_signing_key(&session, Some(&product.product_id), &payload.signer)
                .await?
        } else {
            None
        };
        let names_contacts = matches!(&request, CreateTransactionAuthorityRequest::Product(payload) if !payload.contacts.is_empty());
        if (keypair.is_none() || names_contacts)
            && matches!(invocation.caller, AccountCaller::Local { .. })
        {
            invocation
                .confirm(self.platform.as_ref(), request.review(invocation.caller))
                .await?;
        }
        let cx = match invocation.caller {
            AccountCaller::Local { .. } => super::remote_authority_context(invocation.call),
            AccountCaller::Remote { .. } => invocation.call.clone(),
        };
        super::remote_authority_call(&cx, async {
            self.require_current_operation(&operation)?;
            if let Some(keypair) = keypair
                && let CreateTransactionAuthorityRequest::Product(payload) = request
            {
                let metadata =
                    local_transaction_metadata(&self.chain, payload.genesis_hash).await?;
                let lifecycle = self.grants.lifecycle();
                lifecycle.require(&operation)?;
                self.current_private_session(invocation.session)?;
                return Ok(v01::HostCreateTransactionResponse {
                    transaction: build_signed_transaction(
                        &Sr25519Signer::from_keypair(&keypair),
                        payload.genesis_hash,
                        &payload.call_data,
                        &payload.extensions,
                        payload.tx_ext_version,
                        metadata,
                    )?,
                });
            }
            self.remote_create_transaction(&cx, &session, request).await
        })
        .await
    }

    async fn account_alias(
        &self,
        invocation: AccountInvocation<'_>,
        request: HostAccountGetAliasRequest,
    ) -> Result<v01::ContextualAlias, RingVrfError> {
        PairingHost::account_alias(self, invocation, request).await
    }

    async fn create_proof(
        &self,
        invocation: AccountInvocation<'_>,
        request: HostAccountCreateProofRequest,
    ) -> Result<v01::HostAccountCreateProofResponse, RingVrfError> {
        PairingHost::create_proof(self, invocation, request).await
    }

    async fn register_ring_vrf_key(
        &self,
        invocation: AccountInvocation<'_>,
        request: HostAccountRegisterRingVrfKeyRequest,
    ) -> Result<[u8; 32], RingVrfError> {
        PairingHost::register_ring_vrf_key(
            self,
            invocation.call,
            invocation.session,
            ProductRequest {
                calling_product_id: invocation
                    .caller
                    .product_id()
                    .ok_or(RingVrfError::NotAllowlisted)?
                    .to_string(),
                payload: request,
            },
        )
        .await
    }

    async fn list_ring_vrf_keys(
        &self,
        invocation: AccountInvocation<'_>,
        request: HostAccountListRingVrfKeysRequest,
    ) -> Result<Vec<v01::RegisteredRingVrfKey>, RingVrfError> {
        PairingHost::list_ring_vrf_keys(self, invocation, request).await
    }

    async fn ring_vrf_sign(
        &self,
        invocation: AccountInvocation<'_>,
        request: HostAccountRingVrfSignRequest,
    ) -> Result<Vec<u8>, RingVrfError> {
        PairingHost::ring_vrf_sign(self, invocation, request).await
    }

    async fn sign_statement_store_product_payload(
        &self,
        invocation: AccountInvocation<'_>,
        account: v01::ProductAccountId,
        payload: Vec<u8>,
    ) -> Result<[u8; 64], AuthorityError> {
        PairingHost::sign_statement_store_product_payload(self, invocation, account, payload).await
    }

    fn derive_entropy(
        &self,
        session: &AuthoritySession,
        product_id: &str,
        context: &[u8],
    ) -> Result<[u8; 32], AuthorityError> {
        PairingHost::derive_entropy(self, session, product_id, context)
    }

    fn contacts_handle_key(&self, session: &AuthoritySession) -> Result<[u8; 32], AuthorityError> {
        PairingHost::contacts_handle_key(self, session)
    }
}
