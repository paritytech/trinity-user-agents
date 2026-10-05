//! Pairing-host role for inter-host account authority.
//!
//! A pairing host does not own the user's signing keys. It pairs with a signing
//! host, keeps the active inter-host session, and sends authority requests to
//! that signing host over the SSO channel in [`sso_channel`].

mod sso_channel;

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
    AccountHolder, AuthorityError, AuthoritySession, CreateTransactionAuthorityRequest,
    SignPayloadAuthorityRequest, SignRawAuthorityRequest, authority_session,
};
use super::connected_session_ui_info;
use super::identity::resolve_session_identity_with_chain;
use super::services::RuntimeServices;
use super::sso_pairing::{SsoPairingFlow, SsoPairingOutcome};
use super::sso_remote::{SSO_PEER_DISCONNECT_REASON, SessionDisconnects, SsoSessionKey};
use super::statement_store_rpc::StatementStoreRpc;
use crate::chain_runtime::ChainRuntime;
use crate::host_internal::sso_messages::{ProductRequest, RingVrfError};
use crate::host_logic::entropy::derive_product_entropy_from_source;
use crate::host_logic::session::{SessionInfo, SessionState, encode_persisted_session};
use crate::host_logic::session_store::SessionStoreChangeNotifier;
use crate::runtime::vrf;
use crate::session_usernames::SessionUsernames;
use crate::subscription::Spawner;

use crate::platform::{
    CoreStorageKey, PairingHostConfig, Platform, ProductContext, SecretCoreStorageKey,
};
use futures::StreamExt;
use tracing::{instrument, warn};
use truapi::versioned::account::{HostRequestLoginError, HostRequestLoginResponse};
use truapi::{CallContext, CallError, v01};

struct LoginInFlight {
    waiters: Vec<oneshot::Sender<Result<(), String>>>,
}

struct LoginInFlightOwner<'a> {
    host: &'a SsoAccountHolderClient,
    active: bool,
}

impl<'a> LoginInFlightOwner<'a> {
    fn new(host: &'a SsoAccountHolderClient) -> Self {
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
struct SessionLifecycle {
    epoch: u64,
    storage_revision: u64,
    validated_storage_revision: u64,
    external_session_active: bool,
}

impl SessionLifecycle {
    fn advance(&mut self) -> u64 {
        self.epoch = self
            .epoch
            .checked_add(1)
            .expect("session lifecycle epoch exhausted");
        self.external_session_active = false;
        self.epoch
    }
}

#[derive(Debug, derive_more::Display)]
enum StoredSessionActivationError {
    #[display("stored auth session is absent")]
    Missing,
    #[display("invalid stored auth session: {_0}")]
    Invalid(String),
    #[display("failed to read stored auth session: {_0}")]
    Read(String),
    #[display("failed to persist stored auth session: {_0}")]
    Write(String),
    #[display("stored auth session changed during activation")]
    Changed,
}

type SessionCleanup = Arc<
    dyn Fn(SessionInfo) -> futures::future::BoxFuture<'static, Result<(), AuthorityError>>
        + Send
        + Sync,
>;

/// Remote account authority for a pairing host.
pub struct SsoAccountHolderClient {
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
    login_generation: Mutex<u64>,
    session_store_activation: futures::lock::Mutex<()>,
    session_cleanup: Mutex<Option<SessionCleanup>>,
    session_lifecycle: Mutex<SessionLifecycle>,
    #[cfg(test)]
    external_session_activation_pause: Mutex<Option<(oneshot::Sender<()>, oneshot::Receiver<()>)>>,
    /// Change notifications the sync task has finished reconciling.
    #[cfg(test)]
    session_store_change_ticks: AtomicUsize,
    /// Self-reference captured by the spawned disconnect-monitor task.
    weak_self: Weak<SsoAccountHolderClient>,
    /// Task spawner for background monitors.
    pub spawner: Spawner,
}

impl SsoAccountHolderClient {
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
            login_generation: Mutex::new(0),
            session_store_activation: futures::lock::Mutex::new(()),
            session_cleanup: Mutex::new(None),
            session_lifecycle: Mutex::new(SessionLifecycle::default()),
            #[cfg(test)]
            external_session_activation_pause: Mutex::new(None),
            #[cfg(test)]
            session_store_change_ticks: AtomicUsize::new(0),
            weak_self: weak_self.clone(),
            spawner: services.spawner.clone(),
        })
    }

    /// Connect transport session invalidation to the composing host's capability cleanup.
    pub fn set_session_cleanup(&self, cleanup: SessionCleanup) {
        *self
            .session_cleanup
            .lock()
            .expect("session cleanup mutex poisoned") = Some(cleanup);
    }

    async fn cleanup_session(&self, session: Option<SessionInfo>) -> Result<(), AuthorityError> {
        let cleanup = self
            .session_cleanup
            .lock()
            .expect("session cleanup mutex poisoned")
            .clone();
        if let (Some(cleanup), Some(session)) = (cleanup, session) {
            cleanup(session).await?;
        }
        Ok(())
    }

    /// Shared session holder for connection-status subscriptions.
    pub fn session_state(&self) -> Arc<SessionState> {
        self.session_state.clone()
    }

    /// Signal that the persisted auth session may have changed; the sync task
    /// re-reads it.
    pub fn notify_session_store_changed(&self) {
        let mut lifecycle = self
            .session_lifecycle
            .lock()
            .expect("session lifecycle mutex poisoned");
        lifecycle.storage_revision = lifecycle
            .storage_revision
            .checked_add(1)
            .expect("session storage revision exhausted");
        lifecycle.external_session_active = false;
        drop(lifecycle);
        self.session_store_changes.notify();
    }

    fn session_store_revision(&self) -> u64 {
        self.session_lifecycle
            .lock()
            .expect("session lifecycle mutex poisoned")
            .storage_revision
    }

    async fn matching_session_store_revision(
        &self,
        expected: &[u8],
    ) -> Result<Option<u64>, crate::latest::GenericError> {
        loop {
            let revision = self.session_store_revision();
            let stored = self
                .platform
                .read_secret_core_storage(SecretCoreStorageKey::AuthSession)
                .await?;
            if self.session_store_revision() == revision {
                return Ok((stored.as_deref() == Some(expected)).then_some(revision));
            }
        }
    }

    fn advance_session_lifecycle(&self) -> u64 {
        let mut lifecycle = self
            .session_lifecycle
            .lock()
            .expect("session lifecycle mutex poisoned");
        lifecycle.advance()
    }

    /// Epoch fencing replies across external session changes.
    pub fn current_session_lifecycle_epoch(&self) -> u64 {
        self.session_lifecycle
            .lock()
            .expect("session lifecycle mutex poisoned")
            .epoch
    }

    /// Reject completions from a replaced pairing session.
    fn is_session_epoch_current(&self, epoch: u64) -> bool {
        self.current_session_lifecycle_epoch() == epoch
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
    /// Suspend identity resolution to exercise a concurrent session replacement.
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

    fn current_session_snapshot(&self) -> Option<(SessionInfo, AuthoritySession)> {
        let lifecycle = self
            .session_lifecycle
            .lock()
            .expect("session lifecycle mutex poisoned");
        if lifecycle.validated_storage_revision != lifecycle.storage_revision {
            return None;
        }
        let private = self.session_state.current()?;
        let mut session = authority_session(&private);
        session
            .validation_id
            .extend_from_slice(&lifecycle.epoch.to_le_bytes());
        Some((private, session))
    }

    fn current_session(&self) -> Option<AuthoritySession> {
        self.current_session_snapshot().map(|(_, session)| session)
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
        let activation_epoch = self.advance_session_lifecycle();
        let storage_revision = self.session_store_revision();
        let resolved = resolve_session_identity_with_chain(
            &self.chain,
            self.host_config.asset_hub_chain_genesis_hash,
            session,
        )
        .await;
        #[cfg(test)]
        self.wait_at_external_session_activation_pause().await;
        if !self
            .set_connected_session_if_current(
                resolved,
                activation_epoch,
                Some(storage_revision),
                true,
            )
            .await
            .map_err(|error| error.to_string())?
        {
            return Err(StoredSessionActivationError::Changed.to_string());
        }
        Ok(())
    }

    /// Read, validate, resolve, and install the persisted auth session before
    /// returning. Product frames may use the connected session once this
    /// future resolves. Reports the resulting auth state to the host, including
    /// when there was no session to restore.
    pub async fn activate_stored_session(&self) -> Result<(), String> {
        let restored = self
            .reconcile_stored_session(false)
            .await
            .map_err(|error| error.to_string());
        self.auth_state.announce_current();
        restored
    }

    async fn reconcile_stored_session(
        &self,
        preserve_external_session: bool,
    ) -> Result<(), StoredSessionActivationError> {
        let _activation = self.session_store_activation.lock().await;
        if preserve_external_session
            && self
                .session_lifecycle
                .lock()
                .expect("session lifecycle mutex poisoned")
                .external_session_active
        {
            return Ok(());
        }
        let starting_epoch = self.current_session_lifecycle_epoch();
        let starting_revision = self.session_store_revision();
        let blob = match self
            .platform
            .read_secret_core_storage(SecretCoreStorageKey::AuthSession)
            .await
        {
            Ok(Some(blob)) => blob,
            Ok(None) => {
                self.clear_disconnected_session(false)
                    .await
                    .map_err(|error| StoredSessionActivationError::Write(error.to_string()))?;
                return Err(StoredSessionActivationError::Missing);
            }
            Err(error) => {
                self.clear_disconnected_session(false)
                    .await
                    .map_err(|error| StoredSessionActivationError::Write(error.to_string()))?;
                return Err(StoredSessionActivationError::Read(error.reason));
            }
        };
        let session = match crate::host_logic::session::decode_persisted_session(&blob) {
            Ok(session) => session,
            Err(error) => {
                self.clear_disconnected_session(false)
                    .await
                    .map_err(|error| StoredSessionActivationError::Write(error.to_string()))?;
                return Err(StoredSessionActivationError::Invalid(error));
            }
        };
        let activation_epoch = {
            let mut lifecycle = self
                .session_lifecycle
                .lock()
                .expect("session lifecycle mutex poisoned");
            if lifecycle.epoch != starting_epoch {
                return Err(StoredSessionActivationError::Changed);
            }
            if lifecycle.storage_revision == starting_revision
                && self.session_state.current().as_ref() == Some(&session)
            {
                lifecycle.validated_storage_revision = starting_revision;
                lifecycle.external_session_active = false;
                return Ok(());
            }
            lifecycle.advance()
        };
        let resolved = resolve_session_identity_with_chain(
            &self.chain,
            self.host_config.asset_hub_chain_genesis_hash,
            session,
        )
        .await;

        // Identity resolution can await chain I/O. Re-read the slot before
        // installation so an older activation cannot overwrite or expose a
        // session replaced while that lookup was in flight.
        let mut storage_revision = match self.matching_session_store_revision(&blob).await {
            Ok(Some(revision)) => revision,
            result => {
                self.clear_disconnected_session(false)
                    .await
                    .map_err(|error| StoredSessionActivationError::Write(error.to_string()))?;
                return Err(match result {
                    Err(error) => StoredSessionActivationError::Read(error.reason),
                    _ => StoredSessionActivationError::Changed,
                });
            }
        };

        if !self.is_session_epoch_current(activation_epoch) {
            return Err(StoredSessionActivationError::Changed);
        }
        let resolved_blob = encode_persisted_session(&resolved);
        if resolved_blob != blob {
            self.platform
                .write_secret_core_storage(SecretCoreStorageKey::AuthSession, resolved_blob.clone())
                .await
                .map_err(|error| StoredSessionActivationError::Write(error.reason))?;
            storage_revision = match self.matching_session_store_revision(&resolved_blob).await {
                Ok(Some(revision)) => revision,
                result => {
                    self.clear_disconnected_session(false)
                        .await
                        .map_err(|error| StoredSessionActivationError::Write(error.to_string()))?;
                    return Err(match result {
                        Err(error) => StoredSessionActivationError::Read(error.reason),
                        _ => StoredSessionActivationError::Changed,
                    });
                }
            };
        }
        if !self
            .set_connected_session_if_current(
                resolved,
                activation_epoch,
                Some(storage_revision),
                false,
            )
            .await
            .map_err(|error| StoredSessionActivationError::Write(error.to_string()))?
        {
            return Err(StoredSessionActivationError::Changed);
        }
        Ok(())
    }

    async fn sync_stored_session(&self) {
        if let Err(error) = self.reconcile_stored_session(true).await {
            tracing::debug!(%error, "stored session reconciliation did not connect");
        }
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
            booting.sync_stored_session().await;
            booting.auth_state.announce_current();
            drop(booting);
            while ticks.next().await.is_some() {
                let Some(pairing_host) = pairing_host.upgrade() else {
                    break;
                };
                pairing_host.sync_stored_session().await;
                #[cfg(test)]
                pairing_host
                    .session_store_change_ticks
                    .fetch_add(1, Ordering::SeqCst);
            }
        }));
    }

    #[instrument(skip_all, fields(runtime.method = "account.request_login", product = %product.product_id))]
    /// Run or join the paired host login workflow.
    pub async fn request_login(
        &self,
        product: &ProductContext,
    ) -> Result<HostRequestLoginResponse, CallError<HostRequestLoginError>> {
        let _ = product;
        if let Some((session, _)) = self.current_session_snapshot() {
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
                        if self.current_session().is_some() {
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
        let activation_epoch = self.advance_session_lifecycle();
        let starting_revision = self.session_store_revision();
        let outcome = match SsoPairingFlow::new(self).request_session().await {
            Ok(outcome) => outcome,
            Err(err) => {
                login_owner.finish(Err(login_error_reason(&err)));
                return Err(err);
            }
        };
        match outcome {
            SsoPairingOutcome::Cancelled => {
                login_owner.finish(Ok(()));
                if self.current_session().is_some() {
                    Ok(HostRequestLoginResponse::V1(
                        v01::HostRequestLoginResponse::AlreadyConnected,
                    ))
                } else {
                    Ok(HostRequestLoginResponse::V1(
                        v01::HostRequestLoginResponse::Rejected,
                    ))
                }
            }
            SsoPairingOutcome::Success(session) => {
                let _activation = self.session_store_activation.lock().await;
                if !self.is_current_login_attempt(login_generation)
                    || !self.is_session_epoch_current(activation_epoch)
                    || self.session_store_revision() != starting_revision
                {
                    login_owner.finish(Ok(()));
                    return Ok(HostRequestLoginResponse::V1(
                        v01::HostRequestLoginResponse::Rejected,
                    ));
                }
                let blob = encode_persisted_session(&session);
                self.platform
                    .write_secret_core_storage(SecretCoreStorageKey::AuthSession, blob.clone())
                    .await
                    .map_err(|error| {
                        self.auth_state.login_failed(error.reason.clone());
                        CallError::HostFailure {
                            reason: error.reason,
                        }
                    })?;
                let storage_revision =
                    self.matching_session_store_revision(&blob)
                        .await
                        .map_err(|error| {
                            self.auth_state.login_failed(error.reason.clone());
                            CallError::HostFailure {
                                reason: error.reason,
                            }
                        })?;
                let installed = if self.is_current_login_attempt(login_generation)
                    && storage_revision.is_some()
                {
                    self.set_connected_session_if_current(
                        *session,
                        activation_epoch,
                        storage_revision,
                        false,
                    )
                    .await
                } else {
                    Ok(false)
                };
                if !matches!(installed, Ok(true)) {
                    if storage_revision == Some(self.session_store_revision()) {
                        self.platform
                            .clear_secret_core_storage(SecretCoreStorageKey::AuthSession)
                            .await
                            .map_err(|error| {
                                self.auth_state.login_failed(error.reason.clone());
                                CallError::HostFailure {
                                    reason: error.reason,
                                }
                            })?;
                    }
                    installed.map_err(|error| {
                        self.auth_state.login_failed(error.to_string());
                        CallError::HostFailure {
                            reason: error.to_string(),
                        }
                    })?;
                    login_owner.finish(Ok(()));
                    return Ok(HostRequestLoginResponse::V1(
                        v01::HostRequestLoginResponse::Rejected,
                    ));
                }
                login_owner.finish(Ok(()));
                Ok(HostRequestLoginResponse::V1(
                    v01::HostRequestLoginResponse::Success,
                ))
            }
        }
    }

    #[instrument(skip_all, fields(runtime.method = "account.disconnect"))]
    /// End the transport session and persist capability removal.
    pub async fn disconnect(&self) -> Result<(), AuthorityError> {
        self.cancel_login();
        let session = self.session_state.current();
        self.clear_disconnected_session(true).await?;
        if let Some(session) = session {
            let weak_self = self.weak_self.clone();
            (self.spawner)(Box::pin(async move {
                if let Some(host) = weak_self.upgrade() {
                    let _ = host.submit_disconnected_message(&session).await;
                }
            }));
        }
        Ok(())
    }

    /// Discard pairing bootstrap material after disconnect has completed.
    pub async fn reset_pairing_identity(&self) -> Result<(), String> {
        self.platform
            .clear_secret_core_storage(SecretCoreStorageKey::PairingDeviceIdentity)
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

    /// Clear authentication and session capabilities without notifying the peer.
    pub async fn reset_session_state(&self) -> Result<(), AuthorityError> {
        self.cancel_login();
        let result = self.clear_disconnected_session(true).await;
        self.auth_state.announce_current();
        result
    }

    /// Invalidate in-flight login attempts and emit the cancelled auth state.
    #[instrument(skip_all, fields(runtime.method = "account.cancel_login"))]
    pub fn cancel_login(&self) {
        self.invalidate_login_attempts();
        self.auth_state.login_cancelled();
    }

    fn begin_login_attempt(&self) -> u64 {
        let mut generation = self
            .login_generation
            .lock()
            .expect("login generation mutex poisoned");
        *generation = generation.wrapping_add(1);
        *generation
    }

    fn invalidate_login_attempts(&self) {
        let mut generation = self
            .login_generation
            .lock()
            .expect("login generation mutex poisoned");
        *generation = generation.wrapping_add(1);
    }

    fn is_current_login_attempt(&self, generation: u64) -> bool {
        *self
            .login_generation
            .lock()
            .expect("login generation mutex poisoned")
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

    #[instrument(skip_all, fields(runtime.method = "session_store.clear_disconnected"))]
    async fn clear_disconnected_session(
        &self,
        clear_auth_session: bool,
    ) -> Result<(), AuthorityError> {
        let previous = {
            let mut lifecycle = self
                .session_lifecycle
                .lock()
                .expect("session lifecycle mutex poisoned");
            lifecycle.advance();
            let previous = self.session_state.current();
            self.session_state.clear_session();
            previous
        };
        self.stop_session_channel(previous.as_ref());
        self.auth_state.store_disconnected();
        let _activation = if clear_auth_session {
            Some(self.session_store_activation.lock().await)
        } else {
            None
        };
        self.cleanup_session(previous).await?;
        if clear_auth_session {
            self.platform
                .clear_secret_core_storage(SecretCoreStorageKey::AuthSession)
                .await
                .map_err(|error| AuthorityError::Unavailable {
                    reason: error.reason,
                })?;
        }
        Ok(())
    }

    #[cfg(test)]
    async fn set_connected_session(&self, session: SessionInfo) -> Result<(), AuthorityError> {
        let activation_epoch = self.advance_session_lifecycle();
        self.set_connected_session_if_current(session, activation_epoch, None, false)
            .await?;
        Ok(())
    }

    async fn set_connected_session_if_current(
        &self,
        session: SessionInfo,
        activation_epoch: u64,
        storage_revision: Option<u64>,
        external_session: bool,
    ) -> Result<bool, AuthorityError> {
        if !self.is_session_epoch_current(activation_epoch)
            || storage_revision.is_some_and(|revision| revision != self.session_store_revision())
        {
            return Ok(false);
        }
        let prior = self.session_state.current();
        if let Some(previous) = prior.as_ref()
            && (previous.public_key != session.public_key
                || previous.sso.as_ref().map(SsoSessionKey::from_session)
                    != session.sso.as_ref().map(SsoSessionKey::from_session))
        {
            self.cleanup_session(prior.clone()).await?;
        }
        let previous = {
            let mut lifecycle = self
                .session_lifecycle
                .lock()
                .expect("session lifecycle mutex poisoned");
            if lifecycle.epoch != activation_epoch
                || storage_revision.is_some_and(|revision| revision != lifecycle.storage_revision)
            {
                return Ok(false);
            }
            let previous = self.session_state.current();
            self.session_state.set_session(session.clone());
            lifecycle.validated_storage_revision = lifecycle.storage_revision;
            lifecycle.external_session_active = external_session;
            previous
        };
        if previous.as_ref() != Some(&session) {
            self.stop_session_channel(previous.as_ref());
        }
        self.start_disconnect_monitor(&session);
        vrf::prefetch(&self.spawner);
        self.auth_state
            .connected(&connected_session_ui_info(&session));
        Ok(true)
    }

    #[cfg(test)]
    /// Install a transport session through its production lifecycle fence.
    pub async fn set_connected_session_for_tests(&self, session: SessionInfo) {
        self.set_connected_session(session).await.unwrap();
    }

    async fn handle_signing_host_disconnected(&self, key: SsoSessionKey) {
        self.session_disconnects
            .notify_key(key, SSO_PEER_DISCONNECT_REASON);
        if !self.current_sso_session_matches(key) {
            return;
        }

        if let Err(error) = self.clear_disconnected_session(true).await {
            tracing::error!(%error, "could not persist paired disconnect");
        }
    }

    fn current_sso_session_matches(&self, key: SsoSessionKey) -> bool {
        sso_channel::session_matches_key(&self.session_state, key)
    }

    fn current_private_session(
        &self,
        session: &AuthoritySession,
    ) -> Result<SessionInfo, AuthorityError> {
        let (private, current) = self
            .current_session_snapshot()
            .ok_or(AuthorityError::Disconnected)?;
        if current.validation_id != session.validation_id
            || current.public_key != session.public_key
        {
            return Err(AuthorityError::Disconnected);
        }
        Ok(private)
    }

    /// Refresh display identity without changing transport capabilities.
    pub async fn refresh_current_session_identity(
        &self,
    ) -> Result<Option<AuthoritySession>, AuthorityError> {
        let _activation = self.session_store_activation.lock().await;
        let (current, epoch, starting_revision) = {
            let lifecycle = self
                .session_lifecycle
                .lock()
                .expect("session lifecycle mutex poisoned");
            if lifecycle.validated_storage_revision != lifecycle.storage_revision {
                return Ok(None);
            }
            let Some(current) = self.session_state.current() else {
                return Ok(None);
            };
            (current, lifecycle.epoch, lifecycle.storage_revision)
        };
        if current.has_username() || self.host_config.asset_hub_chain_genesis_hash == [0; 32] {
            return Ok(self.current_session());
        }
        let resolved = resolve_session_identity_with_chain(
            &self.chain,
            self.host_config.asset_hub_chain_genesis_hash,
            current.clone(),
        )
        .await;
        if !self.is_session_epoch_current(epoch)
            || self.session_store_revision() != starting_revision
            || !resolved.has_username()
            || resolved == current
        {
            return Ok(self.current_session());
        }
        let blob = encode_persisted_session(&resolved);
        self.platform
            .write_secret_core_storage(SecretCoreStorageKey::AuthSession, blob.clone())
            .await
            .map_err(|error| AuthorityError::Unavailable {
                reason: error.reason,
            })?;
        let Some(storage_revision) =
            self.matching_session_store_revision(&blob)
                .await
                .map_err(|error| AuthorityError::Unavailable {
                    reason: error.reason,
                })?
        else {
            return Ok(self.current_session());
        };
        let mut lifecycle = self
            .session_lifecycle
            .lock()
            .expect("session lifecycle mutex poisoned");
        if lifecycle.epoch != epoch
            || lifecycle.storage_revision != storage_revision
            || !self
                .session_state
                .replace_session_if_current(&current, resolved.clone())
        {
            drop(lifecycle);
            return Ok(self.current_session());
        }
        lifecycle.validated_storage_revision = storage_revision;
        drop(lifecycle);
        self.auth_state
            .connected(&connected_session_ui_info(&resolved));
        Ok(self.current_session())
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
impl AccountHolder for SsoAccountHolderClient {
    fn current_session(&self) -> Option<AuthoritySession> {
        SsoAccountHolderClient::current_session(self)
    }

    async fn product_subtree_public_key(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        product_id: String,
    ) -> Result<[u8; 32], AuthorityError> {
        let private_session = self.current_private_session(session)?;
        self.remote_product_subtree_public_key(cx, &private_session, product_id)
            .await
    }

    async fn sign_vrf(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        calling_product_id: String,
        request: v01::HostAccountSignVrfRequest,
    ) -> Result<v01::VrfSignature, AuthorityError> {
        let private_session = self.current_private_session(session)?;
        self.remote_sign_vrf(cx, &private_session, calling_product_id, request)
            .await
    }

    async fn sign_payload(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        _calling_product_id: Option<&str>,
        request: SignPayloadAuthorityRequest,
    ) -> Result<v01::HostSignPayloadResponse, AuthorityError> {
        let private_session = self.current_private_session(session)?;
        self.remote_sign_payload(cx, &private_session, request)
            .await
    }

    async fn sign_raw(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        _calling_product_id: Option<&str>,
        request: SignRawAuthorityRequest,
        watermarked: bool,
    ) -> Result<v01::HostSignPayloadResponse, AuthorityError> {
        let private_session = self.current_private_session(session)?;
        self.remote_sign_raw(cx, &private_session, request, watermarked)
            .await
    }

    async fn create_transaction(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        _calling_product_id: Option<&str>,
        request: CreateTransactionAuthorityRequest,
    ) -> Result<v01::HostCreateTransactionResponse, AuthorityError> {
        let private_session = self.current_private_session(session)?;
        self.remote_create_transaction(cx, &private_session, request)
            .await
    }

    async fn account_alias(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        request: ProductRequest<HostAccountGetAliasRequest>,
    ) -> Result<v01::ContextualAlias, RingVrfError> {
        let private_session = self.current_private_session(session)?;
        self.remote_account_alias(cx, &private_session, request)
            .await
    }

    async fn create_proof(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        request: ProductRequest<HostAccountCreateProofRequest>,
    ) -> Result<v01::HostAccountCreateProofResponse, RingVrfError> {
        let private_session = self.current_private_session(session)?;
        self.remote_create_proof(cx, &private_session, request)
            .await
    }

    async fn register_ring_vrf_key(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        request: ProductRequest<HostAccountRegisterRingVrfKeyRequest>,
    ) -> Result<[u8; 32], RingVrfError> {
        let private_session = self.current_private_session(session)?;
        self.remote_register_ring_vrf_key(cx, &private_session, request)
            .await
    }

    async fn list_ring_vrf_keys(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        request: ProductRequest<HostAccountListRingVrfKeysRequest>,
    ) -> Result<Vec<v01::RegisteredRingVrfKey>, RingVrfError> {
        let private_session = self.current_private_session(session)?;
        self.remote_list_ring_vrf_keys(cx, &private_session, request)
            .await
    }

    async fn ring_vrf_sign(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        request: ProductRequest<HostAccountRingVrfSignRequest>,
    ) -> Result<Vec<u8>, RingVrfError> {
        let private_session = self.current_private_session(session)?;
        self.remote_ring_vrf_sign(cx, &private_session, request)
            .await
    }

    async fn allocate_grants(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        product_id: String,
        request: v01::HostRequestResourceAllocationRequest,
        policy: crate::host_internal::sso_messages::OnExistingAllowancePolicy,
    ) -> Result<Vec<super::authority::AccountAllocationOutcome>, AuthorityError> {
        let private_session = self.current_private_session(session)?;
        self.remote_allocate_grants(cx, &private_session, product_id, request, policy)
            .await
    }

    async fn sign_statement_store_product_payload(
        &self,
        _cx: &CallContext,
        _session: &AuthoritySession,
        _calling_product_id: Option<&str>,
        _account: v01::ProductAccountId,
        _payload: Vec<u8>,
    ) -> Result<[u8; 64], AuthorityError> {
        Err(AuthorityError::NotSupported {
            reason: "SSO does not carry exact statement proof signing".to_string(),
        })
    }

    fn derive_entropy(
        &self,
        session: &AuthoritySession,
        product_id: &str,
        context: &[u8],
    ) -> Result<[u8; 32], AuthorityError> {
        SsoAccountHolderClient::derive_entropy(self, session, product_id, context)
    }

    fn contacts_handle_key(&self, session: &AuthoritySession) -> Result<[u8; 32], AuthorityError> {
        SsoAccountHolderClient::contacts_handle_key(self, session)
    }
}
