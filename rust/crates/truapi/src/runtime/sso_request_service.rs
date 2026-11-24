//! Paired session lifecycle and typed outbound SSO transport.

mod channel;
mod pairing;
#[cfg(test)]
mod tests;

use super::{
	HostSession,
	auth_state::AuthStateMachine,
	authority::{AuthoritySession, authority_session},
	connected_session_ui_info, device_enc_public_key,
	host_grants::GrantBarrier,
	identity::{LookupOutcome, lookup_session_identity, resolve_session_identity_with_chain},
	services::{DotnsReads, RuntimeServices},
	sso_remote::{SSO_PEER_DISCONNECT_REASON, SessionDisconnects, SsoSessionKey},
	statement_store_rpc::StatementStoreRpc,
	vrf,
};
use crate::{
	Spawner,
	chain_runtime::ChainRuntime,
	host_logic::{
		session::{SessionInfo, SessionState, encode_persisted_session},
		session_store::SessionStoreChangeNotifier,
	},
	platform::{
		CoreStorageKey, PairingHostConfig, Platform, ProductContext, SessionUiInfo, StackRun,
	},
	session_usernames::SessionUsernames,
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
	latest::{GenericError, HostRequestLoginError, HostRequestLoginResponse},
};

/// `session` without its usernames, so they are looked up on the chains the
/// stack runs now.
fn without_usernames(session: SessionInfo) -> SessionInfo {
	SessionInfo { lite_username: None, full_username: None, ..session }
}

/// What [`CoreStorageKey::SessionUsernamesRun`] says about the usernames in a
/// session blob.
#[derive(Clone, Copy, PartialEq)]
enum BlobUsernames {
	/// They were read for this blob in the run the stack's chains are in.
	Current,
	/// Nothing is recorded, as for a session stored before the core recorded
	/// runs.
	Unrecorded,
	/// The record names another run or another blob, or cannot be read.
	Stale,
}

impl BlobUsernames {
	/// The session to install from a blob whose usernames are `self`, while
	/// `selected` is selected, and whether its usernames are to be looked up
	/// once it is installed.
	///
	/// Nothing waits on dotNS: usernames the core vouches for are installed as
	/// stored, and looked up only when there are none. Unrecorded ones, which
	/// an install stored before runs were recorded or a host keeps itself, are
	/// kept until a lookup answers. Stale ones are dropped. When `selected` is
	/// the same session, it keeps its own usernames instead of either, so
	/// installing a blob again does not undo a lookup.
	fn session_to_install(
		self,
		session: SessionInfo,
		selected: Option<SessionInfo>,
	) -> (SessionInfo, bool) {
		match (self, selected) {
			(Self::Current, _) => {
				let look_up = !session.has_username();
				(session, look_up)
			},
			(_, Some(selected)) if selected.is_same_session(&session) => (selected, true),
			(Self::Unrecorded, _) => (session, true),
			(Self::Stale, _) => (without_usernames(session), true),
		}
	}
}

/// The [`CoreStorageKey::SessionUsernamesRun`] value for `blob` in `run`: the
/// run and a hash of the blob, so the record vouches for that blob only.
fn usernames_record(run: StackRun, blob: &[u8]) -> Vec<u8> {
	run.stamp(&sp_crypto_hashing::blake2_256(blob))
}

/// Keeps grants for the paired session and hears when that session changes.
#[async_trait::async_trait]
pub trait PairedSessionOwner: Send + Sync {
	/// Hold back the owner's durable writes across a session change.
	async fn write_barrier(&self) -> GrantBarrier;

	/// `previous` is no longer selected: `revoked` when it was cleared, not replaced.
	///
	/// Runs while session selection is locked, so it must not call back into the service.
	fn session_ended(&self, previous: Option<&SessionInfo>, revoked: bool);

	/// Drop grants loaded before a pending revocation; whether one is pending.
	fn begin_cleanup(&self, barrier: &GrantBarrier) -> bool;

	/// Finish pending durable revocation; failures stay pending.
	async fn finish_cleanup(&self, barrier: &GrantBarrier) -> Result<(), String>;

	/// Reconcile kept grants with `next` before it is published.
	async fn prepare_session(
		&self,
		barrier: &GrantBarrier,
		previous: Option<&SessionInfo>,
		next: &SessionInfo,
	);
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
struct Selection {
	login_generation: u64,
	session_epoch: u64,
	external_session_active: bool,
	pending_auth_deletion: bool,
}

impl Selection {
	fn advance_epoch(&mut self) -> u64 {
		self.session_epoch = self.session_epoch.checked_add(1).expect("session epoch exhausted");
		self.session_epoch
	}
}

#[derive(derive_more::Display)]
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

#[derive(Default)]
struct SessionStoreSync {
	// Storage clears can notify this subscription; avoid repeating a failing clear.
	cleared_after_read_error: bool,
}

impl SessionStoreSync {
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
	/// dotNS reads shared with the rest of this host, so username lookups that
	/// overlap read once.
	dotns_reads: DotnsReads,
	session_state: Arc<SessionState>,
	session_store_changes: Arc<SessionStoreChangeNotifier>,
	auth_state: AuthStateMachine,
	statement_store: StatementStoreRpc,
	session_disconnects: Arc<SessionDisconnects>,
	newest_request: Mutex<Option<String>>,
	disconnect_monitor: Mutex<Option<SsoDisconnectMonitor>>,
	login_in_flight: Mutex<Option<LoginInFlight>>,
	selection: Mutex<Selection>,
	owner: Arc<dyn PairedSessionOwner>,
	session_store_activation: futures::lock::Mutex<()>,
	#[cfg(test)]
	external_session_activation_pause: Mutex<Option<(oneshot::Sender<()>, oneshot::Receiver<()>)>>,
	#[cfg(test)]
	session_store_change_ticks: AtomicUsize,
	weak_self: Weak<SsoRequestService>,
	spawner: Spawner,
}

impl SsoRequestService {
	/// Whether the usernames in `blob` were read in the run the stack's chains
	/// are in.
	///
	/// The core records the run together with a hash of the blob it stored,
	/// so a blob the host keeps elsewhere, which may be a copy from before a
	/// wipe, is current only when it is that same blob. A record that cannot
	/// be read is stale: looking the usernames up again costs a lookup, and
	/// keeping stale ones would name accounts on chains that are gone.
	async fn blob_usernames(&self, blob: &[u8]) -> BlobUsernames {
		match self.platform.read_core_storage(CoreStorageKey::SessionUsernamesRun).await {
			Ok(None) => BlobUsernames::Unrecorded,
			Ok(Some(record)) if record == usernames_record(self.host_config.stack.run(), blob) => {
				BlobUsernames::Current
			},
			Ok(Some(_)) | Err(_) => BlobUsernames::Stale,
		}
	}

	/// Records that the usernames in `blob`, the blob in the `AuthSession`
	/// slot, were read in the run the stack's chains are in. A failed write
	/// leaves the earlier record, so they are looked up again on the next
	/// start.
	async fn record_usernames(&self, blob: &[u8]) {
		let record = usernames_record(self.host_config.stack.run(), blob);
		let written = self
			.platform
			.write_core_storage(CoreStorageKey::SessionUsernamesRun, record)
			.await;
		if let Err(error) = written {
			warn!(reason = %error.reason, "recording the run of the session's usernames failed");
		}
	}

	/// Writes `blob` to the `AuthSession` slot. Its usernames were read in
	/// the run the stack's chains are in, which is recorded once it is
	/// stored.
	async fn store_session(&self, blob: Vec<u8>) -> Result<(), GenericError> {
		self.platform
			.write_core_storage(CoreStorageKey::AuthSession, blob.clone())
			.await?;
		self.record_usernames(&blob).await;
		Ok(())
	}

	/// Looks up the usernames of `installed`, a session installed without
	/// waiting for them, and puts what it finds in the session while it is
	/// still selected.
	///
	/// `stored` is the `AuthSession` blob `installed` came from. While the
	/// slot still holds it, the slot is rewritten with the usernames found
	/// and their run is recorded. A failed lookup changes nothing, so the
	/// next start looks again and a stored username is never lost to a
	/// device that was offline.
	fn look_up_usernames_in_background(&self, installed: SessionInfo, stored: Option<Vec<u8>>) {
		let service = self.weak_self.clone();
		self.spawner.spawn(async move {
			if let Some(service) = service.upgrade() {
				service.look_up_usernames(installed, stored).await;
			}
		});
	}

	async fn look_up_usernames(&self, installed: SessionInfo, stored: Option<Vec<u8>>) {
		let asset_hub = self.host_config.stack.chains().asset_hub;
		let outcome =
			lookup_session_identity(&self.dotns_reads, &self.chain, asset_hub, &installed).await;
		let (lite_username, full_username) = match outcome {
			LookupOutcome::Found(identity) => identity.into_usernames(),
			LookupOutcome::NoRecord => (None, None),
			LookupOutcome::Failed => return,
		};
		let named = SessionInfo { lite_username, full_username, ..installed.clone() };
		self.name_session(&installed, named, stored).await;
	}

	/// Puts `named`, which is `installed` with the usernames a lookup found,
	/// in its place while `installed` is still selected, and in the
	/// `AuthSession` slot while that still holds `stored`.
	///
	/// It waits for an activation in progress. An activation reads the slot
	/// twice and takes a change between the two reads for another writer's
	/// session, which ends the installed one together with its grants.
	async fn name_session(
		&self,
		installed: &SessionInfo,
		named: SessionInfo,
		stored: Option<Vec<u8>>,
	) {
		let _activation = self.session_store_activation.lock().await;
		let barrier = self.owner.write_barrier().await;
		self.drain_session_deletions(&barrier).await;
		if self.session_state.replace_session_if_current(installed, named.clone()) {
			if named != *installed {
				self.auth_state.connected(&Self::session_ui_info(&named));
			}
			if let Some(stored) = stored {
				self.store_named_session(&stored, &named).await;
			}
		}
		self.drain_session_deletions(&barrier).await;
	}

	/// Rewrites the `AuthSession` slot with `named` while it still holds
	/// `stored`, and records the run of its usernames.
	async fn store_named_session(&self, stored: &[u8], named: &SessionInfo) {
		let latest = self.platform.read_core_storage(CoreStorageKey::AuthSession).await;
		if !matches!(latest, Ok(Some(latest)) if latest == stored) {
			return;
		}
		let blob = encode_persisted_session(named);
		if blob == stored {
			self.record_usernames(&blob).await;
		} else if let Err(error) = self.store_session(blob).await {
			warn!(reason = %error.reason, "storing the session's usernames failed");
		}
	}

	/// Select paired sessions in `session_state` for `owner`, which keeps their grants.
	pub fn new(
		services: Arc<RuntimeServices>,
		host_config: PairingHostConfig,
		session_state: Arc<SessionState>,
		owner: Arc<dyn PairedSessionOwner>,
	) -> Arc<Self> {
		let platform = services.platform.clone();
		let auth_state = AuthStateMachine::new(platform.clone());
		Arc::new_cyclic(|weak_self| Self {
			platform,
			host_config,
			chain: services.chain.clone(),
			dotns_reads: services.dotns_reads.clone(),
			session_state,
			session_store_changes: SessionStoreChangeNotifier::new(),
			auth_state,
			statement_store: services.statement_store.clone(),
			session_disconnects: Arc::new(SessionDisconnects::default()),
			newest_request: Mutex::new(None),
			disconnect_monitor: Mutex::new(None),
			login_in_flight: Mutex::new(None),
			selection: Mutex::new(Selection::default()),
			owner,
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
		let mut selection = self.selection.lock().expect("session selection mutex poisoned");
		selection.external_session_active = false;
		selection.advance_epoch()
	}

	fn session_epoch(&self) -> u64 {
		self.selection.lock().expect("session selection mutex poisoned").session_epoch
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

	/// This device's X25519 encryption secret: the key its pairing proposal
	/// advertised, which the wallet registered. `None` before pairing.
	pub fn device_encryption_secret(&self) -> Option<[u8; 32]> {
		pairing_encryption_secret(&self.session_state.current()?)
	}

	/// `AuthState::Connected` projection of a paired `session`, naming this
	/// device by the key its pairing proposal advertised.
	pub fn session_ui_info(session: &SessionInfo) -> SessionUiInfo {
		connected_session_ui_info(
			session,
			pairing_encryption_secret(session).map(device_enc_public_key),
		)
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
		let (session, look_up) = self
			.blob_usernames(blob)
			.await
			.session_to_install(session, self.session_state.current());
		self.invalidate_login_attempts();
		let activation_epoch = self.advance_session_lifecycle();
		#[cfg(test)]
		self.wait_at_external_session_activation_pause().await;
		let installed = self
			.set_connected_session_if_current(session.clone(), activation_epoch, true)
			.await;
		if installed && look_up {
			self.look_up_usernames_in_background(session, None);
		}
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
			self.selection
				.lock()
				.expect("session selection mutex poisoned")
				.external_session_active
		{
			return Ok(());
		}
		let (activation_epoch, read) = {
			let barrier = self.owner.write_barrier().await;
			self.drain_session_deletions(&barrier).await;
			let epoch = self.advance_session_lifecycle();
			let read = self.platform.read_core_storage(CoreStorageKey::AuthSession).await;
			(epoch, read)
		};
		let blob = match read {
			Ok(Some(blob)) => blob,
			Ok(None) => {
				self.clear_disconnected_session(false, Some(activation_epoch)).await;
				return Err(StoredSessionActivationError::Missing);
			},
			Err(error) => {
				self.clear_disconnected_session(clear_after_read_error, Some(activation_epoch))
					.await;
				return Err(StoredSessionActivationError::Read(error.reason));
			},
		};
		let session = match crate::host_logic::session::decode_persisted_session(&blob) {
			Ok(session) => session,
			Err(error) => {
				self.clear_disconnected_session(true, Some(activation_epoch)).await;
				return Err(StoredSessionActivationError::Invalid(error));
			},
		};
		let usernames = self.blob_usernames(&blob).await;
		let (resolved, look_up) =
			usernames.session_to_install(session, self.session_state.current());
		let barrier = self.owner.write_barrier().await;
		self.drain_session_deletions(&barrier).await;
		if self.session_epoch() != activation_epoch {
			return Err(StoredSessionActivationError::Changed);
		}
		let latest = self.platform.read_core_storage(CoreStorageKey::AuthSession).await;
		let error = match latest {
			Ok(latest) if latest.as_deref() == Some(blob.as_slice()) => None,
			Ok(_) => Some((false, StoredSessionActivationError::Changed)),
			Err(error) => {
				Some((clear_after_read_error, StoredSessionActivationError::Read(error.reason)))
			},
		};
		if let Some((clear_auth, error)) = error {
			if self.begin_session_clear(clear_auth, Some(activation_epoch), None) {
				self.drain_session_deletions(&barrier).await;
			}
			return Err(error);
		}
		self.prepare_session_installation(&barrier, &resolved).await;
		if self.session_epoch() != activation_epoch {
			return Err(StoredSessionActivationError::Changed);
		}
		let resolved_blob = encode_persisted_session(&resolved);
		if !self.install_session_if_current(resolved.clone(), activation_epoch, false, None) {
			return Err(StoredSessionActivationError::Changed);
		}
		// Only a blob the core vouches for is rewritten now, as when an older
		// layout is stored again in the current one; any other is rewritten
		// once its usernames are read.
		let rewritten = usernames == BlobUsernames::Current &&
			resolved_blob != blob &&
			self.session_epoch() == activation_epoch &&
			self.store_session(resolved_blob.clone()).await.is_ok();
		if look_up {
			let in_slot = if rewritten { resolved_blob } else { blob };
			self.look_up_usernames_in_background(resolved, Some(in_slot));
		}
		self.drain_session_deletions(&barrier).await;
		Ok(())
	}

	/// Reconcile at boot and after storage notifications, reporting auth state.
	#[instrument(skip_all, fields(runtime.method = "session_store.sync"))]
	pub fn start_session_store_sync(self: Arc<Self>, spawner: Spawner) {
		let service = Arc::downgrade(&self);
		drop(self);
		spawner.spawn(async move {
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
				service.session_store_change_ticks.fetch_add(1, Ordering::SeqCst);
			}
		});
	}

	async fn commit_login_session(
		&self,
		session: &SessionInfo,
		generation: u64,
	) -> Result<bool, truapi::latest::GenericError> {
		let barrier = self.owner.write_barrier().await;
		if !self.is_current_login_attempt(generation) {
			return Ok(false);
		}
		self.prepare_session_installation(&barrier, session).await;
		let mut epoch = {
			let mut selection = self.selection.lock().expect("session selection mutex poisoned");
			if selection.login_generation != generation {
				return Ok(false);
			}
			selection.pending_auth_deletion = true;
			selection.external_session_active = false;
			selection.advance_epoch()
		};
		let blob = encode_persisted_session(session);
		self.platform
			.write_core_storage(CoreStorageKey::AuthSession, blob.clone())
			.await?;
		while self.is_current_login_attempt(generation) {
			let latest_epoch = self.session_epoch();
			if latest_epoch != epoch {
				let latest = self.platform.read_core_storage(CoreStorageKey::AuthSession).await?;
				let mut selection =
					self.selection.lock().expect("session selection mutex poisoned");
				if selection.login_generation != generation {
					break;
				}
				if selection.session_epoch != latest_epoch {
					continue;
				}
				if latest.as_deref() != Some(blob.as_slice()) {
					selection.pending_auth_deletion = false;
					return Ok(false);
				}
				epoch = latest_epoch;
			}
			if self.install_session_if_current(session.clone(), epoch, false, Some(generation)) {
				self.record_usernames(&blob).await;
				return Ok(true);
			}
		}
		self.drain_session_deletions(&barrier).await;
		Ok(false)
	}

	async fn discard_login_session(&self) {
		let barrier = self.owner.write_barrier().await;
		self.drain_session_deletions(&barrier).await;
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
		let mut selection = self.selection.lock().expect("session selection mutex poisoned");
		selection.login_generation = selection.login_generation.wrapping_add(1);
		selection.login_generation
	}

	fn invalidate_login_attempts(&self) {
		let mut selection = self.selection.lock().expect("session selection mutex poisoned");
		selection.login_generation = selection.login_generation.wrapping_add(1);
	}

	fn is_current_login_attempt(&self, generation: u64) -> bool {
		self.selection
			.lock()
			.expect("session selection mutex poisoned")
			.login_generation ==
			generation
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

	fn begin_session_clear(
		&self,
		clear_auth_session: bool,
		expected_epoch: Option<u64>,
		expected_peer: Option<SsoSessionKey>,
	) -> bool {
		let mut selection = self.selection.lock().expect("session selection mutex poisoned");
		if expected_epoch.is_some_and(|epoch| selection.session_epoch != epoch) ||
			expected_peer.is_some_and(|key| !self.current_sso_session_matches(key))
		{
			return false;
		}
		let previous = self.session_state.current();
		selection.advance_epoch();
		self.owner.session_ended(previous.as_ref(), true);
		selection.pending_auth_deletion |= clear_auth_session;
		selection.external_session_active = false;
		self.session_state.clear_session();
		let monitor = channel::detach_session_channel(self);
		drop(selection);
		channel::stop_session_channel(self, previous.as_ref(), monitor);
		true
	}

	/// Retry the deletions cleared sessions still owe; failed ones stay queued.
	async fn drain_session_deletions(&self, barrier: &GrantBarrier) {
		let pending_auth_deletion = self
			.selection
			.lock()
			.expect("session selection mutex poisoned")
			.pending_auth_deletion;
		let pending_grants = self.owner.begin_cleanup(barrier);
		if !pending_auth_deletion && !pending_grants {
			return;
		}
		if self.session_state.current().is_none() {
			self.auth_state.store_disconnected();
		}
		let auth_result = if pending_auth_deletion {
			self.platform
				.clear_core_storage(CoreStorageKey::AuthSession)
				.await
				.map(|()| {
					self.selection
						.lock()
						.expect("session selection mutex poisoned")
						.pending_auth_deletion = false
				})
				.map_err(|error| error.reason)
		} else {
			Ok(())
		};
		let grants_result = self.owner.finish_cleanup(barrier).await;
		if let Err(reason) = auth_result.and(grants_result) {
			warn!(%reason, "session cleanup remains pending");
		}
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
		let barrier = self.owner.write_barrier().await;
		self.drain_session_deletions(&barrier).await;
	}

	async fn prepare_session_installation(&self, barrier: &GrantBarrier, session: &SessionInfo) {
		self.drain_session_deletions(barrier).await;
		self.owner
			.prepare_session(barrier, self.session_state.current().as_ref(), session)
			.await;
	}

	async fn set_connected_session_if_current(
		&self,
		session: SessionInfo,
		activation_epoch: u64,
		external_session: bool,
	) -> bool {
		let barrier = self.owner.write_barrier().await;
		if self.session_epoch() != activation_epoch {
			return false;
		}
		self.prepare_session_installation(&barrier, &session).await;
		self.install_session_if_current(session, activation_epoch, external_session, None)
	}

	fn install_session_if_current(
		&self,
		session: SessionInfo,
		activation_epoch: u64,
		external_session: bool,
		expected_login_generation: Option<u64>,
	) -> bool {
		let detached = {
			let mut selection = self.selection.lock().expect("session selection mutex poisoned");
			if expected_login_generation
				.is_some_and(|expected| expected != selection.login_generation)
			{
				return false;
			}
			if selection.session_epoch != activation_epoch {
				return false;
			}
			if expected_login_generation.is_none() {
				selection.login_generation = selection.login_generation.wrapping_add(1);
			}
			let previous = self.session_state.current();
			let replaced =
				!previous.as_ref().is_some_and(|previous| previous.is_same_session(&session));
			let detached = replaced.then(|| {
				self.owner.session_ended(previous.as_ref(), false);
				let monitor = channel::detach_session_channel(self);
				(previous, monitor)
			});
			selection.pending_auth_deletion = false;
			self.session_state.set_session(session.clone());
			selection.external_session_active = external_session;
			detached
		};
		if let Some((previous, monitor)) = detached {
			channel::stop_session_channel(self, previous.as_ref(), monitor);
		}
		channel::start_disconnect_monitor(self, &session);
		vrf::prefetch(&self.spawner);
		self.auth_state.connected(&Self::session_ui_info(&session));
		true
	}

	/// Install a selected session through the real lifecycle.
	#[cfg(test)]
	pub async fn set_connected_session_for_tests(&self, session: SessionInfo) {
		let epoch = self.advance_session_lifecycle();
		self.set_connected_session_if_current(session, epoch, false).await;
	}

	// Stale peer notifications wake only their own waiters.
	async fn handle_signing_host_disconnected(&self, key: SsoSessionKey) {
		self.session_disconnects.notify_key(key, SSO_PEER_DISCONNECT_REASON);
		if !self.begin_session_clear(true, None, Some(key)) {
			return;
		}
		let barrier = self.owner.write_barrier().await;
		self.drain_session_deletions(&barrier).await;
	}

	/// Whether the selected session's SSO channel is `key`.
	fn current_sso_session_matches(&self, key: SsoSessionKey) -> bool {
		key.matches(&self.session_state)
	}

	/// Resolve missing identity information for the selected session.
	async fn refresh_current_session_identity(&self) -> Option<AuthoritySession> {
		let (current, epoch) = {
			let selection = self.selection.lock().expect("session selection mutex poisoned");
			(self.session_state.current()?, selection.session_epoch)
		};
		if current.has_username() {
			return Some(authority_session(&current));
		}

		let resolved = resolve_session_identity_with_chain(
			&self.dotns_reads,
			&self.chain,
			self.host_config.stack.chains().asset_hub,
			current.clone(),
		)
		.await;
		if resolved == current {
			return self.current_session();
		}

		let barrier = self.owner.write_barrier().await;
		self.drain_session_deletions(&barrier).await;
		{
			let selection = self.selection.lock().expect("session selection mutex poisoned");
			if selection.session_epoch != epoch ||
				!self.session_state.replace_session_if_current(&current, resolved.clone())
			{
				return self.current_session();
			}
		}
		self.auth_state.connected(&Self::session_ui_info(&resolved));
		if let Err(err) = self.store_session(encode_persisted_session(&resolved)).await {
			warn!(reason = %err.reason, "refreshed session identity persist failed");
		}
		self.drain_session_deletions(&barrier).await;
		self.current_session()
	}
}

#[async_trait::async_trait]
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
			self.auth_state.connected(&Self::session_ui_info(&session));
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
		let outcome = match SsoPairingFlow::new(self, login_generation).request_session().await {
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
			SsoPairingOutcome::Success => {
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
		self.clear_disconnected_session(true, None).await;
		if let Some(session) = session {
			let weak_self = self.weak_self.clone();
			self.spawner.spawn(async move {
				if let Some(host) = weak_self.upgrade() {
					let _ = channel::submit_disconnected_message(&host, &session).await;
				}
			});
		}
	}

	async fn primary_username(&self) -> Option<String> {
		self.refresh_current_session_identity()
			.await?
			.primary_username()
			.map(str::to_string)
	}
}

/// The secret a paired session's pairing proposal advertised for this device.
fn pairing_encryption_secret(session: &SessionInfo) -> Option<[u8; 32]> {
	Some(session.sso.as_ref()?.enc_secret)
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
