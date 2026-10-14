//! Paired session lifecycle and typed outbound SSO transport.

mod channel;
mod pairing;

use super::{
	HostSession,
	auth_state::AuthStateMachine,
	authority::{AuthoritySession, authority_session},
	connected_session_ui_info,
	identity::resolve_session_identity_with_chain,
	services::RuntimeServices,
	sso_remote::{SSO_PEER_DISCONNECT_REASON, SessionDisconnects, SsoSessionKey},
	statement_store_rpc::StatementStoreRpc,
	vrf,
};
use crate::{
	chain_runtime::ChainRuntime,
	host_logic::{
		session::{SessionInfo, SessionState, encode_persisted_session},
		session_store::SessionStoreChangeNotifier,
	},
	platform::{CoreStorageKey, PairingHostConfig, Platform, ProductContext},
	session_usernames::SessionUsernames,
	subscription::Spawner,
};
use channel::SsoDisconnectMonitor;
use futures::{StreamExt, channel::oneshot};
use pairing::{SsoPairingFlow, SsoPairingOutcome};
#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, Weak};
use tracing::{instrument, warn};
use truapi::{
	CallError, latest as api,
	latest::{HostRequestLoginError, HostRequestLoginResponse},
};

/// Keeps grants for the paired session and hears when that session changes.
#[async_trait::async_trait]
pub trait PairedSessionOwner: Send + Sync {
	/// `previous` is no longer selected: `revoked` when it was cleared, not replaced.
	///
	/// Runs while session selection is locked, so it must not call back into the service.
	fn session_ended(&self, previous: Option<&SessionInfo>, revoked: bool);

	/// Finish the durable revocation a cleared session left pending.
	async fn session_cleared(&self);

	/// Reconcile kept grants with `next` before it replaces `previous`.
	async fn prepare_session(&self, previous: Option<&SessionInfo>, next: &SessionInfo);
}

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
			self.host.finish_login_in_flight(Err("login request aborted".to_string()));
		}
	}
}

#[derive(Default)]
struct SessionLifecycle {
	epoch: u64,
	external_session_active: bool,
}

impl SessionLifecycle {
	fn advance(&mut self) -> u64 {
		self.epoch = self.epoch.checked_add(1).expect("session lifecycle epoch exhausted");
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
	#[display("stored auth session changed during activation")]
	Changed,
}

/// State carried across the reconciles of one session store sync task.
#[derive(Default)]
struct SessionStoreSync {
	// Storage clears can notify this subscription; clear at most once per read-error streak.
	cleared_after_read_error: bool,
}

impl SessionStoreSync {
	/// Re-read the persisted auth session and reconcile the in-memory one.
	async fn reconcile(&mut self, service: &SsoRequestService) {
		self.cleared_after_read_error = matches!(
			service.reconcile_stored_session(!self.cleared_after_read_error, true).await,
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
	/// `message_id` of the request the session's request channel carries,
	/// the only one a `Cancel` may name without replacing another request.
	newest_request: Mutex<Option<String>>,
	disconnect_monitor: Mutex<Option<SsoDisconnectMonitor>>,
	login_in_flight: Mutex<Option<LoginInFlight>>,
	login_generation: Mutex<u64>,
	session_store_activation: futures::lock::Mutex<()>,
	session_lifecycle: Mutex<SessionLifecycle>,
	owner: Weak<dyn PairedSessionOwner>,
	#[cfg(test)]
	external_session_activation_pause: Mutex<Option<(oneshot::Sender<()>, oneshot::Receiver<()>)>>,
	/// Change notifications the sync task has finished reconciling.
	#[cfg(test)]
	session_store_change_ticks: AtomicUsize,
	weak_self: Weak<SsoRequestService>,
	spawner: Spawner,
}

impl SsoRequestService {
	/// Select paired sessions for `owner`, which keeps their account material.
	pub fn new(
		services: Arc<RuntimeServices>,
		host_config: PairingHostConfig,
		owner: Weak<dyn PairedSessionOwner>,
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
			login_generation: Mutex::new(0),
			session_store_activation: futures::lock::Mutex::new(()),
			session_lifecycle: Mutex::new(SessionLifecycle::default()),
			owner,
			#[cfg(test)]
			external_session_activation_pause: Mutex::new(None),
			#[cfg(test)]
			session_store_change_ticks: AtomicUsize::new(0),
			weak_self: weak_self.clone(),
			spawner: services.spawner.clone(),
		})
	}

	/// Signal that the persisted auth session may have changed; the sync task
	/// re-reads it.
	pub fn notify_session_store_changed(&self) {
		self.advance_session_lifecycle();
		self.session_store_changes.notify();
	}

	fn advance_session_lifecycle(&self) -> u64 {
		let mut lifecycle =
			self.session_lifecycle.lock().expect("session lifecycle mutex poisoned");
		lifecycle.advance()
	}

	fn is_session_lifecycle_current(&self, epoch: u64) -> bool {
		self.session_lifecycle.lock().expect("session lifecycle mutex poisoned").epoch == epoch
	}

	/// Test hook for [`Self::start_session_store_sync`].
	#[cfg(test)]
	pub fn start_session_store_sync_for_tests(self: Arc<Self>, spawner: Spawner) {
		self.start_session_store_sync(spawner);
	}

	/// `message_id` of the request the session's request channel carries.
	#[cfg(test)]
	pub fn newest_request_for_tests(&self) -> Option<String> {
		self.newest_request.lock().expect("newest request mutex poisoned").clone()
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

	/// Selected remote account identity.
	pub fn current_session(&self) -> Option<AuthoritySession> {
		self.session_state.current().as_ref().map(authority_session)
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
		let resolved = resolve_session_identity_with_chain(
			&self.chain,
			self.host_config.asset_hub_chain_genesis_hash,
			session,
		)
		.await;
		#[cfg(test)]
		self.wait_at_external_session_activation_pause().await;
		self.set_connected_session_if_current(resolved, activation_epoch, true).await;
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
		if preserve_external_session &&
			self.session_lifecycle
				.lock()
				.expect("session lifecycle mutex poisoned")
				.external_session_active
		{
			return Ok(());
		}
		let blob = match self.platform.read_core_storage(CoreStorageKey::AuthSession).await {
			Ok(Some(blob)) => blob,
			Ok(None) => {
				self.clear_disconnected_session(false).await;
				return Err(StoredSessionActivationError::Missing);
			},
			Err(error) => {
				self.clear_disconnected_session(false).await;
				if clear_after_read_error {
					let _ = self.platform.clear_core_storage(CoreStorageKey::AuthSession).await;
				}
				return Err(StoredSessionActivationError::Read(error.reason));
			},
		};
		let session = match crate::host_logic::session::decode_persisted_session(&blob) {
			Ok(session) => session,
			Err(error) => {
				self.clear_disconnected_session(true).await;
				return Err(StoredSessionActivationError::Invalid(error));
			},
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
		let latest = match self.platform.read_core_storage(CoreStorageKey::AuthSession).await {
			Ok(latest) => latest,
			Err(error) => {
				self.clear_disconnected_session(false).await;
				if clear_after_read_error {
					let _ = self.platform.clear_core_storage(CoreStorageKey::AuthSession).await;
				}
				return Err(StoredSessionActivationError::Read(error.reason));
			},
		};
		if latest.as_deref() != Some(blob.as_slice()) {
			self.clear_disconnected_session(false).await;
			return Err(StoredSessionActivationError::Changed);
		}

		let resolved_blob = encode_persisted_session(&resolved);
		if resolved_blob != blob {
			let _ = self
				.platform
				.write_core_storage(CoreStorageKey::AuthSession, resolved_blob)
				.await;
		}
		self.set_connected_session(resolved).await;
		Ok(())
	}

	/// Reconcile at boot and after storage notifications, reporting auth state.
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
				pairing_host.session_store_change_ticks.fetch_add(1, Ordering::SeqCst);
			}
		}));
	}

	/// Discard pairing bootstrap material so the next login generates a new
	/// device keypair and topic.
	pub async fn forget_pairing_identity(&self) -> Result<(), String> {
		self.platform
			.clear_core_storage(CoreStorageKey::PairingDeviceIdentity)
			.await
			.map_err(|error| {
				format!("session disconnected, but pairing identity reset failed: {}", error.reason)
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

	/// Clear the canonical local session and all session capabilities without
	/// sending a peer-disconnect statement. Reports the resulting auth state to
	/// the host, including when there was no session to clear.
	pub async fn reset_session_state(&self) {
		self.cancel_login();
		self.clear_disconnected_session(true).await;
		self.auth_state.announce_current();
	}

	/// Invalidate in-flight login attempts and emit the cancelled auth state.
	#[instrument(skip_all, fields(runtime.method = "account.cancel_login"))]
	pub fn cancel_login(&self) {
		self.invalidate_login_attempts();
		self.auth_state.login_cancelled();
	}

	fn begin_login_attempt(&self) -> u64 {
		let mut generation = self.login_generation.lock().expect("login generation mutex poisoned");
		*generation = generation.wrapping_add(1);
		*generation
	}

	fn invalidate_login_attempts(&self) {
		let mut generation = self.login_generation.lock().expect("login generation mutex poisoned");
		*generation = generation.wrapping_add(1);
	}

	fn is_current_login_attempt(&self, generation: u64) -> bool {
		*self.login_generation.lock().expect("login generation mutex poisoned") == generation
	}

	fn login_waiter(&self) -> Option<oneshot::Receiver<Result<(), String>>> {
		let mut in_flight = self.login_in_flight.lock().expect("login in-flight mutex poisoned");
		if let Some(in_flight) = in_flight.as_mut() {
			let (tx, rx) = oneshot::channel();
			in_flight.waiters.push(tx);
			Some(rx)
		} else {
			*in_flight = Some(LoginInFlight { waiters: Vec::new() });
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
	async fn clear_disconnected_session(&self, clear_auth_session: bool) {
		let previous = {
			let mut lifecycle =
				self.session_lifecycle.lock().expect("session lifecycle mutex poisoned");
			lifecycle.advance();
			let previous = self.session_state.current();
			if let Some(owner) = self.owner.upgrade() {
				owner.session_ended(previous.as_ref(), true);
			}
			self.session_state.clear_session();
			previous
		};
		self.stop_session_channel(previous.as_ref());
		if clear_auth_session {
			let _ = self.platform.clear_core_storage(CoreStorageKey::AuthSession).await;
		}
		if let Some(owner) = self.owner.upgrade() {
			owner.session_cleared().await;
		}
		self.auth_state.store_disconnected();
	}

	async fn set_connected_session(&self, session: SessionInfo) {
		let activation_epoch = self.advance_session_lifecycle();
		self.set_connected_session_if_current(session, activation_epoch, false).await;
	}

	async fn set_connected_session_if_current(
		&self,
		session: SessionInfo,
		activation_epoch: u64,
		external_session: bool,
	) -> bool {
		if !self.is_session_lifecycle_current(activation_epoch) {
			return false;
		}
		let previous = self.session_state.current();
		if let Some(owner) = self.owner.upgrade() {
			owner.prepare_session(previous.as_ref(), &session).await;
		}
		let previous = {
			let mut lifecycle =
				self.session_lifecycle.lock().expect("session lifecycle mutex poisoned");
			if lifecycle.epoch != activation_epoch {
				return false;
			}
			let previous = self.session_state.current();
			if previous.as_ref() != Some(&session) &&
				let Some(owner) = self.owner.upgrade()
			{
				owner.session_ended(previous.as_ref(), false);
			}
			self.session_state.set_session(session.clone());
			lifecycle.external_session_active = external_session;
			previous
		};
		if previous.as_ref() != Some(&session) {
			self.stop_session_channel(previous.as_ref());
		}
		self.start_disconnect_monitor(&session);
		vrf::prefetch(&self.spawner);
		self.auth_state.connected(&connected_session_ui_info(&session));
		true
	}

	/// Install a selected session through the real lifecycle.
	#[cfg(test)]
	pub async fn set_connected_session_for_tests(&self, session: SessionInfo) {
		self.set_connected_session(session).await;
	}

	/// Single funnel for peer-initiated disconnects. Every detection source
	/// (monitor task, request-path error) must route here: it wakes in-flight
	/// waiters for `key`, then clears the session when `key` is still current,
	/// so stale notifications for replaced sessions only wake their own
	/// waiters.
	async fn handle_signing_host_disconnected(&self, key: SsoSessionKey) {
		self.session_disconnects.notify_key(key, SSO_PEER_DISCONNECT_REASON);
		if !self.current_sso_session_matches(key) {
			return;
		}

		self.clear_disconnected_session(true).await;
	}

	fn current_sso_session_matches(&self, key: SsoSessionKey) -> bool {
		key.matches(&self.session_state)
	}

	async fn refresh_current_session_identity(&self) -> Option<AuthoritySession> {
		let current = self.session_state.current()?;
		if current.has_username() || self.host_config.asset_hub_chain_genesis_hash == [0; 32] {
			return Some(authority_session(&current));
		}

		let resolved = resolve_session_identity_with_chain(
			&self.chain,
			self.host_config.asset_hub_chain_genesis_hash,
			current.clone(),
		)
		.await;
		if resolved == current {
			return self.current_session();
		}

		if !self.session_state.replace_session_if_current(&current, resolved.clone()) {
			return self.current_session();
		}
		self.auth_state.connected(&connected_session_ui_info(&resolved));

		if let Err(err) = self
			.platform
			.write_core_storage(CoreStorageKey::AuthSession, encode_persisted_session(&resolved))
			.await
		{
			warn!(reason = %err.reason, "refreshed session identity persist failed");
		}

		match self.session_state.current() {
			Some(live) if live != resolved => {
				if let Err(err) = self
					.platform
					.write_core_storage(
						CoreStorageKey::AuthSession,
						encode_persisted_session(&live),
					)
					.await
				{
					warn!(reason = %err.reason, "live session identity persist repair failed");
				}
				Some(authority_session(&live))
			},
			None => {
				if let Err(err) =
					self.platform.clear_core_storage(CoreStorageKey::AuthSession).await
				{
					warn!(reason = %err.reason, "cleared session identity persist repair failed");
				}
				None
			},
			_ => Some(authority_session(&resolved)),
		}
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
			self.auth_state.connected(&connected_session_ui_info(&session));
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
				},
				Ok(Err(reason)) => {
					return Err(CallError::Domain(api::HostRequestLoginError::Unknown { reason }));
				},
				Err(_) => {
					return Err(CallError::Domain(api::HostRequestLoginError::Unknown {
						reason: "login waiter dropped".to_string(),
					}));
				},
			}
		}

		let mut login_owner = LoginInFlightOwner::new(self);
		let login_generation = self.begin_login_attempt();
		let outcome = match SsoPairingFlow::new(self).request_session().await {
			Ok(outcome) => outcome,
			Err(err) => {
				login_owner.finish(Err(login_error_reason(&err)));
				return Err(err);
			},
		};
		match outcome {
			SsoPairingOutcome::Cancelled => {
				login_owner.finish(Ok(()));
				if self.session_state.current().is_some() {
					Ok(api::HostRequestLoginResponse::AlreadyConnected)
				} else {
					Ok(api::HostRequestLoginResponse::Rejected)
				}
			},
			SsoPairingOutcome::Success(session) => {
				if !self.is_current_login_attempt(login_generation) {
					let _ = self.platform.clear_core_storage(CoreStorageKey::AuthSession).await;
					login_owner.finish(Ok(()));
					return Ok(api::HostRequestLoginResponse::Rejected);
				}
				self.set_connected_session(*session).await;
				login_owner.finish(Ok(()));
				Ok(api::HostRequestLoginResponse::Success)
			},
		}
	}

	/// Revoke the paired session and notify its peer.
	#[instrument(skip_all, fields(runtime.method = "account.disconnect"))]
	async fn disconnect(&self) {
		self.cancel_login();
		let session = self.session_state.current();
		self.clear_disconnected_session(true).await;
		if let Some(session) = session {
			let weak_self = self.weak_self.clone();
			(self.spawner)(Box::pin(async move {
				if let Some(host) = weak_self.upgrade() {
					let _ = host.submit_disconnected_message(&session).await;
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
		CallError::Domain(api::HostRequestLoginError::Unknown { reason }) |
		CallError::HostFailure { reason } => reason.clone(),
		CallError::Unsupported => "login unsupported".to_string(),
		CallError::Denied => "login denied".to_string(),
		CallError::MalformedFrame { reason } => reason.clone(),
		CallError::Cancelled => "login cancelled".to_string(),
	}
}
