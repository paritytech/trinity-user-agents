//! Paired session lifecycle and typed outbound SSO transport.

mod channel;
mod pairing;
#[cfg(test)]
mod tests;

use super::auth_state::AuthStateMachine;
use super::authority::{AuthoritySession, authority_session};
use super::host_grants::{HostGrantPersistence, HostGrantStore};
use super::identity::resolve_session_identity_with_chain;
use super::services::RuntimeServices;
use super::sso_remote::{SSO_PEER_DISCONNECT_REASON, SessionDisconnects, SsoSessionKey};
use super::statement_store_rpc::StatementStoreRpc;
use super::vrf;
use super::{HostSession, connected_session_ui_info};
use crate::chain_runtime::ChainRuntime;
use crate::host_logic::session::{SessionInfo, SessionState, encode_persisted_session};
use crate::host_logic::session_store::SessionStoreChangeNotifier;
use crate::platform::{CoreStorageKey, PairingHostConfig, Platform, ProductContext};
use crate::session_usernames::SessionUsernames;
use crate::subscription::Spawner;
use channel::SsoDisconnectMonitor;
use futures::StreamExt;
use futures::channel::oneshot;
use pairing::{SsoPairingFlow, SsoPairingOutcome};
#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, Weak};
use tracing::{instrument, warn};
use truapi::CallError;
use truapi::latest as api;
use truapi::latest::{HostRequestLoginError, HostRequestLoginResponse};

struct LoginInFlight {
    waiters: Vec<oneshot::Sender<Result<(), String>>>,
}

struct LoginInFlightOwner<'a> {
    host: &'a SsoRequestService,
    active: bool,
}

impl<'a> LoginInFlightOwner<'a> {
    fn new(host: &'a SsoRequestService) -> Self {
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

#[derive(Default)]
struct SessionStoreSync {
    // Storage clears can notify this subscription; avoid repeating a failing clear.
    cleared_after_read_error: bool,
}

impl SessionStoreSync {
    async fn reconcile(&mut self, service: &SsoRequestService) {
        self.cleared_after_read_error = matches!(
            service
                .reconcile_stored_session(!self.cleared_after_read_error, true)
                .await,
            Err(StoredSessionActivationError::Read(_))
        );
    }
}

/// Canonical paired session selection, cleanup and request transport.
pub struct SsoRequestService {
    platform: Arc<dyn Platform>,
    host_config: PairingHostConfig,
    chain: ChainRuntime,
    session_state: Arc<SessionState>,
    session_store_changes: Arc<SessionStoreChangeNotifier>,
    auth_state: AuthStateMachine,
    statement_store: StatementStoreRpc,
    session_disconnects: Arc<SessionDisconnects>,
    newest_request: Mutex<Option<String>>,
    disconnect_monitor: Mutex<Option<SsoDisconnectMonitor>>,
    login_in_flight: Mutex<Option<LoginInFlight>>,
    selection: Mutex<Selection>,
    grants: Arc<HostGrantStore>,
    session_store_activation: futures::lock::Mutex<()>,
    #[cfg(test)]
    external_session_activation_pause: Mutex<Option<(oneshot::Sender<()>, oneshot::Receiver<()>)>>,
    #[cfg(test)]
    session_store_change_ticks: AtomicUsize,
    weak_self: Weak<SsoRequestService>,
    spawner: Spawner,
}

impl SsoRequestService {
    /// Share the runtime's grant persistence barrier with session selection.
    pub fn new(
        services: Arc<RuntimeServices>,
        host_config: PairingHostConfig,
        grants: Arc<HostGrantStore>,
    ) -> Arc<Self> {
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
            selection: Mutex::new(Selection::default()),
            grants,
            session_store_activation: futures::lock::Mutex::new(()),
            #[cfg(test)]
            external_session_activation_pause: Mutex::new(None),
            #[cfg(test)]
            session_store_change_ticks: AtomicUsize::new(0),
            weak_self: weak_self.clone(),
            spawner: services.spawner.clone(),
        })
    }

    /// Invalidate stale work and request a persisted-session reread.
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

    /// Pause replacement before the guarded installation for race tests.
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

    /// Clear all capability material owned by one product while preserving the
    /// active session and unrelated products.
    pub async fn clear_product_state(&self, product_id: &str) -> Result<(), String> {
        let product_id = crate::platform::normalize_product_identifier(product_id)
            .map_err(|error| error.to_string())?;
        let session = {
            let mut lifecycle = self.grants.lifecycle();
            lifecycle.revoke_product(&product_id);
            self.session_state().current()
        };
        self.grants
            .persistence()
            .await
            .clear_product(session.as_ref(), &product_id)
            .await
    }

    /// Selected remote account identity.
    pub fn current_session(&self) -> Option<AuthoritySession> {
        self.session_state.current().as_ref().map(authority_session)
    }

    /// Start the disconnect monitor when a session is already active.
    #[cfg(test)]
    pub fn start_remote_monitor_for_current_session(&self) {
        if let Some(session) = self.session_state.current() {
            channel::start_disconnect_monitor(self, &session);
        }
    }

    /// Install an external canonical session without copying it into storage.
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

    /// Restore the persisted session and report the resulting connection state.
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

    /// Reconcile at boot and after storage notifications, reporting auth state.
    #[instrument(skip_all, fields(runtime.method = "session_store.sync"))]
    pub fn start_session_store_sync(self: Arc<Self>, spawner: Spawner) {
        let service = Arc::downgrade(&self);
        drop(self);
        spawner(Box::pin(async move {
            let Some(booting) = service.upgrade() else {
                return;
            };
            let mut ticks = booting.session_store_changes.subscribe();
            let mut sync = SessionStoreSync::default();
            sync.reconcile(&booting).await;
            booting.auth_state.announce_current();
            drop(booting);
            while ticks.next().await.is_some() {
                let Some(service) = service.upgrade() else {
                    break;
                };
                sync.reconcile(&service).await;
                #[cfg(test)]
                service
                    .session_store_change_ticks
                    .fetch_add(1, Ordering::SeqCst);
            }
        }));
    }

    async fn commit_login_session(
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

    async fn discard_login_session(&self) {
        let persistence = self.grants.persistence().await;
        if let Err(reason) = self.drain_session_deletions(&persistence).await {
            warn!(%reason, "cancelled login cleanup remains pending");
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

    /// Clear the paired session and capabilities without notifying the peer.
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
        let monitor = channel::detach_session_channel(self, previous.as_ref());
        drop(lifecycle);
        drop(selection);
        channel::stop_session_channel(self, previous.as_ref(), monitor);
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
                let monitor = channel::detach_session_channel(self, previous.as_ref());
                (previous, monitor)
            });
            lifecycle.forget_auth_deletion();
            self.session_state.set_session(session.clone());
            selection.external_session_active = external_session;
            detached
        };
        if let Some((previous, monitor)) = detached {
            channel::stop_session_channel(self, previous.as_ref(), monitor);
        }
        channel::start_disconnect_monitor(self, &session);
        vrf::prefetch(&self.spawner);
        self.auth_state
            .connected(&connected_session_ui_info(&session));
        true
    }

    /// Install a selected session through the real lifecycle.
    #[cfg(test)]
    pub async fn set_connected_session_for_tests(&self, session: SessionInfo) {
        let epoch = self.advance_session_lifecycle();
        self.set_connected_session_if_current(session, epoch, false)
            .await
            .expect("test session installs");
    }

    // Stale peer notifications wake only their own waiters.
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

    /// Resolve missing identity information for the selected session.
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
}

#[crate::platform::async_trait]
impl HostSession for SsoRequestService {
    /// Shared session holder for connection-status subscriptions.
    fn session_state(&self) -> Arc<SessionState> {
        self.session_state.clone()
    }

    /// Start or join the current pairing attempt.
    #[instrument(skip_all, fields(runtime.method = "account.request_login", product = %product.product_id))]
    async fn request_login(
        &self,
        product: &ProductContext,
    ) -> Result<HostRequestLoginResponse, CallError<HostRequestLoginError>> {
        let _ = product;
        if let Some(session) = self.session_state.current() {
            self.auth_state
                .connected(&connected_session_ui_info(&session));
            return Ok(api::HostRequestLoginResponse::AlreadyConnected);
        }

        if let Some(waiter) = self.login_waiter() {
            match waiter.await {
                Ok(Ok(())) => {
                    return Ok(if self.session_state.current().is_some() {
                        api::HostRequestLoginResponse::AlreadyConnected
                    } else {
                        api::HostRequestLoginResponse::Rejected
                    });
                }
                Ok(Err(reason)) => {
                    return Err(CallError::Domain(api::HostRequestLoginError::Unknown {
                        reason,
                    }));
                }
                Err(_) => {
                    return Err(CallError::Domain(api::HostRequestLoginError::Unknown {
                        reason: "login waiter dropped".to_string(),
                    }));
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
                    Ok(api::HostRequestLoginResponse::AlreadyConnected)
                } else {
                    Ok(api::HostRequestLoginResponse::Rejected)
                }
            }
            SsoPairingOutcome::Success => {
                login_owner.finish(Ok(()));
                Ok(api::HostRequestLoginResponse::Success)
            }
        }
    }

    /// Revoke the paired session and notify its peer.
    #[instrument(skip_all, fields(runtime.method = "account.disconnect"))]
    async fn disconnect(&self) {
        self.cancel_login();
        let session = self.session_state.current();
        self.clear_disconnected_session(true, None).await;
        if let Some(session) = session {
            let weak_self = self.weak_self.clone();
            (self.spawner)(Box::pin(async move {
                if let Some(host) = weak_self.upgrade() {
                    let _ = channel::submit_disconnected_message(&host, &session).await;
                }
            }));
        }
    }

    async fn primary_username(&self) -> Option<String> {
        self.refresh_current_session_identity()
            .await?
            .primary_username()
            .map(str::to_string)
    }
}

fn login_error_reason(err: &CallError<HostRequestLoginError>) -> String {
    match err {
        CallError::Domain(api::HostRequestLoginError::Unknown { reason })
        | CallError::HostFailure { reason } => reason.clone(),
        CallError::Unsupported => "login unsupported".to_string(),
        CallError::Denied => "login denied".to_string(),
        CallError::MalformedFrame { reason } => reason.clone(),
        CallError::Cancelled => "login cancelled".to_string(),
    }
}
