//! Pairing role: product calls reach a wallet on a paired phone.

#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};
use std::{
	collections::HashMap,
	sync::{Arc, Mutex, Weak},
};
use truapi::latest::{
	HostAccountCreateProofRequest, HostAccountGetAliasRequest, HostAccountListRingVrfKeysRequest,
	HostAccountRegisterRingVrfKeyRequest, HostAccountRingVrfSignRequest,
};

use futures::channel::oneshot;
use parity_scale_codec::{Decode, Encode};
use schnorrkel::SecretKey;
use sso_channel::SsoDisconnectMonitor;
use zeroize::Zeroize;

use super::{
	allowances::{self, AllowanceCacheKey, AllowanceResource},
	auth_state::AuthStateMachine,
	authority::{
		AuthorityError, AuthoritySession, AutoSigningGrant, AutoSigningKey, BulletinAllowanceKey,
		CreateTransactionAuthorityRequest, ProductAuthority, SignPayloadAuthorityRequest,
		SignRawAuthorityRequest, StatementStoreAllowanceKey, authority_session,
		require_current_session,
	},
	connected_session_ui_info,
	identity::resolve_session_identity_with_chain,
	product_subtree,
	services::RuntimeServices,
	sso_pairing::{SsoPairingFlow, SsoPairingOutcome},
	sso_remote::{SSO_PEER_DISCONNECT_REASON, SessionDisconnects, SsoSessionKey, sso_message_id},
	statement_store_rpc::StatementStoreRpc,
};
use crate::{
	chain_runtime::ChainRuntime,
	host_internal::{
		extrinsic::build_local_transaction,
		sso_messages::{ProductRequest, RingVrfError},
		transaction::sign_extrinsic_payload,
	},
	host_logic::{
		entropy::derive_product_entropy_from_source,
		product_account::{
			SR25519_SIGNING_CONTEXT, derivation_index_bytes,
			derive_product_keypair_from_subtree_secret, derive_ring_vrf_entropy_from_domain,
		},
		raw_signing::raw_payload_bytes,
		session::{SessionInfo, SessionState, encode_persisted_session},
		session_store::SessionStoreChangeNotifier,
	},
	runtime::vrf,
	session_usernames::SessionUsernames,
	subscription::Spawner,
};

use crate::platform::{
	CoreStorageKey, PairingHostConfig, Platform, ProductContext, SignVrfReview,
	UserConfirmationReview, normalize_product_identifier,
};
use futures::StreamExt;
use tracing::{instrument, warn};
use truapi::{
	CallContext, CallError, latest,
	versioned::account::{HostRequestLoginError, HostRequestLoginResponse},
};
use zeroize::Zeroizing;

use super::{
	ring_vrf_registry::{RingVrfRegistryStore, validate_owner_listing},
	signing_host::ring_vrf::{
		ChainRingResolver, MemberCandidate, RingResolver, create_proof, development_context_bytes,
	},
};

#[derive(Default)]
struct SessionLifecycle {
	epoch: u64,
	external_session_active: bool,
}

impl SessionLifecycle {
	fn advance_preserving_source(&mut self) -> u64 {
		let external_session_active = self.external_session_active;
		let epoch = self.advance();
		self.external_session_active = external_session_active;
		epoch
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
	login_generation: Mutex<u64>,
	statement_store_allowances: Mutex<HashMap<AllowanceCacheKey, StatementStoreAllowanceKey>>,
	bulletin_allowances: Mutex<HashMap<AllowanceCacheKey, BulletinAllowanceKey>>,
	product_subtrees: Mutex<HashMap<(SsoSessionKey, String), [u8; 32]>>,
	auto_signing_keys: Mutex<HashMap<AutoSigningCacheKey, AutoSigningKey>>,
	ring_resolver: Arc<dyn RingResolver>,
	ring_vrf_registry: Arc<RingVrfRegistryStore>,
	/// Orders session-secret cache/storage writes against teardown and activation.
	session_secret_storage: futures::lock::Mutex<()>,
	session_store_activation: futures::lock::Mutex<()>,
	session_lifecycle: Mutex<SessionLifecycle>,
	/// Keep preimage submissions in the core instead of sending them to the
	/// Bulletin chain. A test host whose wallet answers allowances in-page
	/// holds no on-chain authorization to submit with.
	#[cfg(feature = "test-host")]
	submit_preimages_locally: core::sync::atomic::AtomicBool,
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
			login_generation: Mutex::new(0),
			statement_store_allowances: Mutex::new(HashMap::new()),
			bulletin_allowances: Mutex::new(HashMap::new()),
			product_subtrees: Mutex::new(HashMap::new()),
			auto_signing_keys: Mutex::new(HashMap::new()),
			ring_resolver: ChainRingResolver::new(services.chain.clone()),
			ring_vrf_registry: RingVrfRegistryStore::new(services.platform.clone()),
			session_secret_storage: futures::lock::Mutex::new(()),
			session_store_activation: futures::lock::Mutex::new(()),
			session_lifecycle: Mutex::new(SessionLifecycle::default()),
			#[cfg(feature = "test-host")]
			submit_preimages_locally: core::sync::atomic::AtomicBool::new(false),
			#[cfg(test)]
			external_session_activation_pause: Mutex::new(None),
			#[cfg(test)]
			session_store_change_ticks: AtomicUsize::new(0),
			weak_self: weak_self.clone(),
			spawner: services.spawner.clone(),
		})
	}

	pub fn current_session_lifecycle_epoch(&self) -> u64 {
		self.session_lifecycle.lock().expect("session lifecycle mutex poisoned").epoch
	}

	pub fn is_session_lifecycle_current(&self, epoch: u64) -> bool {
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

	/// Providers registered for this ring under the active account.
	pub async fn ring_vrf_providers(
		&self,
		ring: &latest::RingLocation,
	) -> Result<Vec<latest::ProductAccountId>, RingVrfError> {
		let session = self
			.session_state
			.current()
			.ok_or(RingVrfError::Unknown { reason: "no active session".to_string() })?;
		self.ring_vrf_registry.providers(session.public_key, ring).await
	}

	/// Provider selected for this ring under the active account.
	pub async fn selected_ring_vrf_provider(
		&self,
		ring: &latest::RingLocation,
	) -> Result<Option<latest::ProductAccountId>, RingVrfError> {
		let session = self
			.session_state
			.current()
			.ok_or(RingVrfError::Unknown { reason: "no active session".to_string() })?;
		self.ring_vrf_registry.selected_provider(session.public_key, ring).await
	}

	/// Start the disconnect monitor when a session is already active.
	#[cfg(test)]
	pub fn start_remote_monitor_for_current_session(&self) {
		if let Some(session) = self.session_state.current() {
			self.start_disconnect_monitor(&session);
		}
	}

	/// Disconnect and discard pairing bootstrap material so the next login
	/// generates a new device keypair and topic.
	pub async fn logout_and_reset_pairing(&self) -> Result<(), String> {
		self.disconnect().await;
		self.clear_auto_signing_keys().await.map_err(|reason| {
			format!("session disconnected, but AutoSigning reset failed: {reason}")
		})?;
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
		let identity_replaced = previous.as_ref().is_some_and(|previous| {
			AutoSigningOwner::from_session(previous) != AutoSigningOwner::from_session(&session)
		});
		if identity_replaced {
			if let Err(reason) = self.clear_auto_signing_keys().await {
				warn!(%reason, "AutoSigning capability clear failed during identity replacement");
			}
		} else if previous.is_none() {
			if let Err(reason) = self.clear_auto_signing_keys_for_other_owner(&session).await {
				warn!(%reason, "AutoSigning capability owner reconciliation failed");
			}
		} else {
			let _storage_guard = self.session_secret_storage.lock().await;
		}
		if let Some(previous) = previous.as_ref().filter(|previous| *previous != &session) {
			let _storage_guard = self.session_secret_storage.lock().await;
			self.clear_statement_store_allowance_keys(Some(previous));
			self.clear_bulletin_allowance_keys(Some(previous));
			if let Err(reason) =
				allowances::clear_session_allowance_keys(&*self.platform, previous).await
			{
				warn!(%reason, "allowance capability clear failed during session replacement");
			}
		}
		let previous = {
			let mut lifecycle =
				self.session_lifecycle.lock().expect("session lifecycle mutex poisoned");
			if lifecycle.epoch != activation_epoch {
				return false;
			}
			let previous = self.session_state.current();
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

	#[cfg(test)]
	pub async fn has_auto_signing_key_for_tests(
		&self,
		session: &SessionInfo,
		product_id: &str,
	) -> Result<bool, AuthorityError> {
		self.auto_signing_key(session, product_id).await.map(|key| key.is_some())
	}

	#[cfg(test)]
	pub async fn remember_auto_signing_key_for_tests(
		&self,
		session: &SessionInfo,
		lifecycle_epoch: u64,
		product_id: &str,
		expected_product_subtree_public_key: [u8; 32],
		secret: [u8; 64],
		ring_vrf_domain_entropy: [u8; 32],
	) -> Result<(), AuthorityError> {
		self.remember_auto_signing_key(
			session,
			lifecycle_epoch,
			product_id,
			expected_product_subtree_public_key,
			secret,
			ring_vrf_domain_entropy,
		)
		.await
	}

	/// Register fixture material in the actual durable registry.
	#[cfg(test)]
	pub async fn register_ring_vrf_key_for_tests(
		&self,
		session: &SessionInfo,
		handle: latest::ProductAccountId,
		ring: latest::RingLocation,
		public_key: [u8; 32],
	) -> Result<(), RingVrfError> {
		self.ring_vrf_registry
			.register(session.public_key, handle, ring, public_key)
			.await
	}

	/// Cache sizes for lifecycle regression checks.
	#[cfg(test)]
	pub fn capability_cache_sizes_for_tests(&self) -> (usize, usize, usize, usize) {
		(
			self.statement_store_allowances
				.lock()
				.expect("statement-store allowance cache mutex poisoned")
				.len(),
			self.bulletin_allowances
				.lock()
				.expect("bulletin allowance cache mutex poisoned")
				.len(),
			self.product_subtrees
				.lock()
				.expect("product subtree cache mutex poisoned")
				.len(),
			self.auto_signing_keys
				.lock()
				.expect("AutoSigning key cache mutex poisoned")
				.len(),
		)
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
		self.known_product_subtree(&session, cache_key).await.is_none()
	}

	fn current_private_session(
		&self,
		session: &AuthoritySession,
	) -> Result<SessionInfo, AuthorityError> {
		require_current_session(&self.session_state, session)
	}

	/// Persist and memory-cache a freshly allocated statement-store allowance
	/// key.
	pub async fn cache_statement_store_allowance_key(
		&self,
		session: &SessionInfo,
		lifecycle_epoch: u64,
		product_id: &str,
		slot_account_key: Vec<u8>,
	) -> Result<StatementStoreAllowanceKey, AuthorityError> {
		let allowance = StatementStoreAllowanceKey::from_secret_bytes(slot_account_key)?;
		let _storage_guard = self.session_secret_storage.lock().await;
		if !self.session_secret_allocation_is_current(session, lifecycle_epoch) {
			return Err(AuthorityError::Disconnected);
		}
		allowances::write_allowance_key(
			&*self.platform,
			session,
			product_id,
			AllowanceResource::StatementStore,
			allowance.secret.to_vec(),
		)
		.await?;
		if let Err(error) = self.remember_statement_store_allowance_key(
			session,
			lifecycle_epoch,
			product_id,
			allowance.clone(),
		) {
			let _ = allowances::remove_allowance_key(
				&*self.platform,
				session,
				product_id,
				AllowanceResource::StatementStore,
			)
			.await;
			return Err(error);
		}
		Ok(allowance)
	}

	fn remember_statement_store_allowance_key(
		&self,
		session: &SessionInfo,
		lifecycle_epoch: u64,
		product_id: &str,
		allowance: StatementStoreAllowanceKey,
	) -> Result<(), AuthorityError> {
		let cache_key =
			AllowanceCacheKey::new(session, product_id, AllowanceResource::StatementStore)?;
		let lifecycle = self.session_lifecycle.lock().expect("session lifecycle mutex poisoned");
		if lifecycle.epoch != lifecycle_epoch ||
			!session.sso.as_ref().is_some_and(|sso| {
				self.current_sso_session_matches(SsoSessionKey::from_session(sso))
			}) {
			return Err(AuthorityError::Disconnected);
		}
		self.statement_store_allowances
			.lock()
			.expect("statement-store allowance cache mutex poisoned")
			.insert(cache_key, allowance);
		Ok(())
	}

	/// Cached statement-store allowance key for the product, falling back to
	/// persisted storage.
	pub async fn cached_statement_store_allowance_key(
		&self,
		session: &SessionInfo,
		lifecycle_epoch: u64,
		product_id: &str,
	) -> Result<Option<StatementStoreAllowanceKey>, AuthorityError> {
		let cache_key =
			AllowanceCacheKey::new(session, product_id, AllowanceResource::StatementStore)?;
		let _storage_guard = self.session_secret_storage.lock().await;
		if !self.session_secret_allocation_is_current(session, lifecycle_epoch) {
			return Err(AuthorityError::Disconnected);
		}
		if let Some(allowance) = self
			.statement_store_allowances
			.lock()
			.expect("statement-store allowance cache mutex poisoned")
			.get(&cache_key)
			.cloned()
		{
			return Ok(Some(allowance));
		}
		let Some(secret) = allowances::read_allowance_key(
			&*self.platform,
			session,
			product_id,
			AllowanceResource::StatementStore,
		)
		.await?
		else {
			return Ok(None);
		};
		let allowance = StatementStoreAllowanceKey::from_secret_bytes(secret)?;
		self.remember_statement_store_allowance_key(
			session,
			lifecycle_epoch,
			product_id,
			allowance.clone(),
		)?;
		Ok(Some(allowance))
	}

	/// Persist and memory-cache a freshly allocated Bulletin allowance key.
	pub async fn cache_bulletin_allowance_key(
		&self,
		session: &SessionInfo,
		lifecycle_epoch: u64,
		product_id: &str,
		slot_account_key: Vec<u8>,
	) -> Result<BulletinAllowanceKey, AuthorityError> {
		let allowance = BulletinAllowanceKey::from_secret_bytes(slot_account_key)?;
		let _storage_guard = self.session_secret_storage.lock().await;
		if !self.session_secret_allocation_is_current(session, lifecycle_epoch) {
			return Err(AuthorityError::Disconnected);
		}
		allowances::write_allowance_key(
			&*self.platform,
			session,
			product_id,
			AllowanceResource::Bulletin,
			allowance.as_secret_bytes().to_vec(),
		)
		.await?;
		if let Err(error) = self.remember_bulletin_allowance_key(
			session,
			lifecycle_epoch,
			product_id,
			allowance.clone(),
		) {
			let _ = allowances::remove_allowance_key(
				&*self.platform,
				session,
				product_id,
				AllowanceResource::Bulletin,
			)
			.await;
			return Err(error);
		}
		Ok(allowance)
	}

	fn remember_bulletin_allowance_key(
		&self,
		session: &SessionInfo,
		lifecycle_epoch: u64,
		product_id: &str,
		allowance: BulletinAllowanceKey,
	) -> Result<(), AuthorityError> {
		let cache_key = AllowanceCacheKey::new(session, product_id, AllowanceResource::Bulletin)?;
		let lifecycle = self.session_lifecycle.lock().expect("session lifecycle mutex poisoned");
		if lifecycle.epoch != lifecycle_epoch ||
			!session.sso.as_ref().is_some_and(|sso| {
				self.current_sso_session_matches(SsoSessionKey::from_session(sso))
			}) {
			return Err(AuthorityError::Disconnected);
		}
		self.bulletin_allowances
			.lock()
			.expect("bulletin allowance cache mutex poisoned")
			.insert(cache_key, allowance);
		Ok(())
	}

	/// Cached Bulletin allowance key for the product, falling back to
	/// persisted storage.
	pub async fn cached_bulletin_allowance_key(
		&self,
		session: &SessionInfo,
		lifecycle_epoch: u64,
		product_id: &str,
	) -> Result<Option<BulletinAllowanceKey>, AuthorityError> {
		let cache_key = AllowanceCacheKey::new(session, product_id, AllowanceResource::Bulletin)?;
		let _storage_guard = self.session_secret_storage.lock().await;
		if !self.session_secret_allocation_is_current(session, lifecycle_epoch) {
			return Err(AuthorityError::Disconnected);
		}
		if let Some(allowance) = self
			.bulletin_allowances
			.lock()
			.expect("bulletin allowance cache mutex poisoned")
			.get(&cache_key)
			.cloned()
		{
			return Ok(Some(allowance));
		}
		let Some(secret) = allowances::read_allowance_key(
			&*self.platform,
			session,
			product_id,
			AllowanceResource::Bulletin,
		)
		.await?
		else {
			return Ok(None);
		};
		let allowance = BulletinAllowanceKey::from_secret_bytes(secret)?;
		self.remember_bulletin_allowance_key(
			session,
			lifecycle_epoch,
			product_id,
			allowance.clone(),
		)?;
		Ok(Some(allowance))
	}

	/// Drop the cached and persisted Bulletin allowance key for one product.
	pub async fn evict_bulletin_allowance_key(
		&self,
		session: &SessionInfo,
		lifecycle_epoch: u64,
		product_id: &str,
	) -> Result<(), AuthorityError> {
		let cache_key = AllowanceCacheKey::new(session, product_id, AllowanceResource::Bulletin)?;
		let _storage_guard = self.session_secret_storage.lock().await;
		if !self.session_secret_allocation_is_current(session, lifecycle_epoch) {
			return Err(AuthorityError::Disconnected);
		}
		self.bulletin_allowances
			.lock()
			.expect("bulletin allowance cache mutex poisoned")
			.remove(&cache_key);
		allowances::remove_allowance_key(
			&*self.platform,
			session,
			product_id,
			AllowanceResource::Bulletin,
		)
		.await?;
		if !self.session_secret_allocation_is_current(session, lifecycle_epoch) {
			return Err(AuthorityError::Disconnected);
		}
		Ok(())
	}

	async fn clear_auto_signing_keys_under_storage_guard(&self) -> Result<(), String> {
		self.auto_signing_keys
			.lock()
			.expect("AutoSigning key cache mutex poisoned")
			.clear();
		self.platform
			.clear_core_storage(CoreStorageKey::AutoSigningKeys)
			.await
			.map_err(|err| err.reason)
	}

	async fn clear_auto_signing_product_under_storage_guard(
		&self,
		product_id: &str,
	) -> Result<(), String> {
		let aggregate_result = match self
			.platform
			.read_core_storage(CoreStorageKey::AutoSigningKeys)
			.await
		{
			Err(error) => Err(error.reason),
			Ok(None) => Ok(()),
			Ok(Some(mut blob)) => {
				let decoded = decode_auto_signing_keys(&blob);
				blob.zeroize();
				match decoded {
					Err(_) => self
						.platform
						.clear_core_storage(CoreStorageKey::AutoSigningKeys)
						.await
						.map_err(|error| error.reason),
					Ok(mut keys) => {
						let before = keys.len();
						keys.retain(|key| key.product_id != product_id);
						if keys.len() == before {
							Ok(())
						} else if keys.is_empty() {
							self.platform
								.clear_core_storage(CoreStorageKey::AutoSigningKeys)
								.await
								.map_err(|error| error.reason)
						} else {
							self.platform
								.write_core_storage(CoreStorageKey::AutoSigningKeys, keys.encode())
								.await
								.map_err(|error| error.reason)
						}
					},
				}
			},
		};
		let legacy_result = self
			.clear_legacy_auto_signing_key(product_id)
			.await
			.map_err(|error| error.to_string());
		aggregate_result.and(legacy_result.map(|_| ()))
	}

	/// Drop the persisted subtree slots this run knows about for `session`.
	///
	/// Scoped to the in-memory set, so a product never opened since launch
	/// keeps its slot. Those address session ids that cannot recur, and
	/// clearing them belongs to the host, as `CoreStorage` states.
	async fn clear_stored_product_subtrees(&self, session: &SessionInfo) {
		let Some(sso) = session.sso.as_ref() else {
			return;
		};
		let session_key = SsoSessionKey::from_session(sso);
		let product_ids: Vec<String> = self
			.product_subtrees
			.lock()
			.expect("product subtree cache mutex poisoned")
			.keys()
			.filter(|(key, _)| *key == session_key)
			.map(|(_, product_id)| product_id.clone())
			.collect();
		for product_id in product_ids {
			if let Err(reason) =
				product_subtree::remove_product_subtree(&*self.platform, session, &product_id).await
			{
				warn!(%reason, %product_id, "product subtree clear failed during disconnect");
			}
		}
	}

	async fn local_ring_vrf_entropy_for_ring(
		&self,
		session: &SessionInfo,
		handle: &latest::ProductAccountId,
		ring: &latest::RingLocation,
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
			if let Err(error) = host.remote_register_ring_vrf_key(&cx, &session, request).await {
				warn!(?error, "ring-VRF registration mirror failed");
			}
		}));
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
	/// product's subtree, not the caller's. Serving one locally would sign with a key the caller
	/// was never granted and skip the confirmation the signing host raises for a
	/// relayed request, so the binding is checked here rather than only at the
	/// gate.
	async fn local_product_signing_key(
		&self,
		session: &SessionInfo,
		calling_product_id: Option<&str>,
		account: &latest::ProductAccountId,
	) -> Result<Option<schnorrkel::Keypair>, AuthorityError> {
		if calling_product_id != Some(account.dot_ns_identifier.as_str()) {
			return Ok(None);
		}
		let Some(key) = self.auto_signing_key(session, &account.dot_ns_identifier).await? else {
			return Ok(None);
		};
		derive_product_keypair_from_subtree_secret(
			*key.as_secret_bytes(),
			derivation_index_bytes(&account.derivation_index),
		)
		.map(Some)
		.map_err(|err| AuthorityError::Unknown { reason: err.to_string() })
	}

	/// Whether an AutoSigning capability lets this host serve `account`
	/// locally for `calling_product_id`.
	///
	/// A capability this host cannot trust is an error, not a fall-through:
	/// the lookup erases the slot as it rejects it, and prompting afterwards
	/// would ask the user to approve a signature the host just refused to make.
	async fn auto_signing_status(
		&self,
		session: &AuthoritySession,
		calling_product_id: &str,
		account: &latest::ProductAccountId,
	) -> Result<AutoSigningGrant, AuthorityError> {
		let session = self.current_private_session(session)?;
		Ok(
			match self
				.local_product_signing_key(&session, Some(calling_product_id), account)
				.await?
			{
				Some(_) => AutoSigningGrant::Active,
				None => AutoSigningGrant::Absent,
			},
		)
	}

	async fn allocate_resources(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		product_id: String,
		request: latest::HostRequestResourceAllocationRequest,
	) -> Result<latest::HostRequestResourceAllocationResponse, AuthorityError> {
		let session = self.current_private_session(session)?;
		self.remote_allocate_resources(cx, &session, product_id, request).await
	}

	async fn statement_store_allowance_key(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		product_id: String,
	) -> Result<StatementStoreAllowanceKey, AuthorityError> {
		let session = self.current_private_session(session)?;
		self.remote_statement_store_allowance_key(cx, &session, product_id).await
	}

	async fn bulletin_allowance_key(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		product_id: String,
	) -> Result<BulletinAllowanceKey, AuthorityError> {
		let session = self.current_private_session(session)?;
		self.remote_bulletin_allowance_key(cx, &session, product_id).await
	}

	async fn refresh_bulletin_allowance_key(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		product_id: String,
	) -> Result<BulletinAllowanceKey, AuthorityError> {
		let session = self.current_private_session(session)?;
		self.remote_refresh_bulletin_allowance_key(cx, &session, product_id).await
	}

	/// Contact-handle key from the wallet-provided root entropy source.
	///
	/// Unlike `derive_entropy` this does not require a live SSO channel: it
	/// needs the key material the session already carries, not a round trip to
	/// the wallet.
	fn contacts_handle_key(&self, session: &AuthoritySession) -> Result<[u8; 32], AuthorityError> {
		let session = self.current_private_session(session)?;
		let root_entropy_source = session.root_entropy_source.ok_or_else(|| {
			AuthorityError::Unavailable { reason: "Session secret missing".to_string() }
		})?;
		Ok(crate::runtime::contacts::handle_key_from_root_source(&root_entropy_source))
	}
}

#[async_trait::async_trait]
impl ProductAuthority for PairingHost {
	#[cfg(feature = "test-host")]
	fn submits_preimages_locally(&self) -> bool {
		self.submit_preimages_locally.load(core::sync::atomic::Ordering::Relaxed)
	}

	#[cfg(test)]
	fn cache_product_subtree_for_test(
		&self,
		session: &SessionInfo,
		product_id: &str,
		public_key: [u8; 32],
	) {
		let sso = session.sso.as_ref().expect("test session must contain SSO");
		self.product_subtrees
			.lock()
			.expect("product subtree cache mutex poisoned")
			.insert((SsoSessionKey::from_session(sso), product_id.to_string()), public_key);
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

	async fn auto_signing_status(
		&self,
		session: &AuthoritySession,
		calling_product_id: &str,
		account: &latest::ProductAccountId,
	) -> Result<AutoSigningGrant, AuthorityError> {
		PairingHost::auto_signing_status(self, session, calling_product_id, account).await
	}

	async fn sign_raw(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		calling_product_id: Option<&str>,
		request: SignRawAuthorityRequest,
		watermarked: bool,
	) -> Result<latest::HostSignPayloadResponse, AuthorityError> {
		PairingHost::sign_raw(self, cx, session, calling_product_id, request, watermarked).await
	}

	async fn create_transaction(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		calling_product_id: Option<&str>,
		request: CreateTransactionAuthorityRequest,
	) -> Result<latest::HostCreateTransactionResponse, AuthorityError> {
		PairingHost::create_transaction(self, cx, session, calling_product_id, request).await
	}

	async fn list_ring_vrf_keys(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		request: ProductRequest<HostAccountListRingVrfKeysRequest>,
	) -> Result<Vec<latest::RegisteredRingVrfKey>, RingVrfError> {
		PairingHost::list_ring_vrf_keys(self, cx, session, request).await
	}

	async fn allocate_resources(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		product_id: String,
		request: latest::HostRequestResourceAllocationRequest,
	) -> Result<latest::HostRequestResourceAllocationResponse, AuthorityError> {
		PairingHost::allocate_resources(self, cx, session, product_id, request).await
	}

	async fn statement_store_allowance_key(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		product_id: String,
	) -> Result<StatementStoreAllowanceKey, AuthorityError> {
		PairingHost::statement_store_allowance_key(self, cx, session, product_id).await
	}

	async fn bulletin_allowance_key(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		product_id: String,
	) -> Result<BulletinAllowanceKey, AuthorityError> {
		PairingHost::bulletin_allowance_key(self, cx, session, product_id).await
	}

	async fn refresh_bulletin_allowance_key(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		product_id: String,
	) -> Result<BulletinAllowanceKey, AuthorityError> {
		PairingHost::refresh_bulletin_allowance_key(self, cx, session, product_id).await
	}

	async fn sign_statement_store_product_payload(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		calling_product_id: Option<&str>,
		account: latest::ProductAccountId,
		payload: Vec<u8>,
	) -> Result<[u8; 64], AuthorityError> {
		PairingHost::sign_statement_store_product_payload(
			self,
			cx,
			session,
			calling_product_id,
			account,
			payload,
		)
		.await
	}

	fn contacts_handle_key(&self, session: &AuthoritySession) -> Result<[u8; 32], AuthorityError> {
		PairingHost::contacts_handle_key(self, session)
	}
}
