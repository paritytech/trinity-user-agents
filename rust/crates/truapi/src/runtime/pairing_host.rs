//! Pairing role: product calls reach a wallet on a paired phone.

mod remote_calls;

use std::{
	collections::HashMap,
	sync::{Arc, Mutex, Weak},
};
use truapi::latest::{
	HostAccountCreateProofRequest, HostAccountGetAliasRequest, HostAccountListRingVrfKeysRequest,
	HostAccountRegisterRingVrfKeyRequest, HostAccountRingVrfSignRequest,
};

use parity_scale_codec::{Decode, Encode};
use schnorrkel::SecretKey;
use zeroize::Zeroize;

use super::{
	HostSession,
	allowances::{self, AllowanceCacheKey, AllowanceResource},
	authority::{
		AuthorityError, AuthoritySession, AutoSigningGrant, AutoSigningKey, BulletinAllowanceKey,
		CreateTransactionAuthorityRequest, PaymentTopUpAuthorityError, ProductAuthority,
		ProductDeviceChatAuthorityError, ProductDeviceChatAuthorityRequest,
		SignPayloadAuthorityRequest, SignRawAuthorityRequest, StatementStoreAllowanceKey,
		authority_session, require_current_session,
	},
	product_subtree,
	services::RuntimeServices,
	sso_remote::{SsoSessionKey, sso_message_id},
	sso_request_service::{PairedSessionOwner, SsoRequestService},
};
use crate::{
	chain_runtime::ChainRuntime,
	host_internal::{
		extrinsic::build_local_transaction,
		sso_messages::{PaymentTopUpRequest, ProductRequest, RingVrfError},
		transaction::sign_extrinsic_payload,
	},
	host_logic::{
		entropy::derive_product_entropy_from_source,
		product_account::{
			SR25519_SIGNING_CONTEXT, derivation_index_bytes,
			derive_product_keypair_from_subtree_secret, derive_ring_vrf_entropy_from_domain,
		},
		raw_signing::raw_payload_bytes,
		session::{SessionInfo, SessionState},
	},
	runtime::vrf,
	subscription::Spawner,
};

use crate::platform::{
	CoreStorageKey, PairingHostConfig, Platform, ProductId, SignVrfReview, UserConfirmationReview,
};
use tracing::warn;
use truapi::{CallContext, latest};
use zeroize::Zeroizing;

use super::{
	ring_vrf_registry::{RingVrfRegistryStore, apply_ring_vrf_disclosure, validate_owner_listing},
	signing_host::ring_vrf::{
		ChainRingResolver, MemberCandidate, RingResolver, context_bytes, create_proof,
	},
};

#[derive(Clone, PartialEq, Eq, Hash, Encode, Decode)]
struct AutoSigningOwner {
	root_public_key: [u8; 32],
	authenticated_sso_identity: Option<[u8; 32]>,
}

impl AutoSigningOwner {
	fn from_session(session: &SessionInfo) -> Self {
		Self {
			root_public_key: session.public_key,
			authenticated_sso_identity: session.sso.as_ref().map(|sso| sso.identity_account_id),
		}
	}
}

#[derive(Encode, Decode, zeroize::ZeroizeOnDrop)]
struct PersistedAutoSigningKey {
	#[zeroize(skip)]
	owner: AutoSigningOwner,
	#[zeroize(skip)]
	product_id: ProductId,
	#[zeroize(skip)]
	expected_product_subtree_public_key: [u8; 32],
	secret: [u8; 64],
	ring_vrf_domain_entropy: [u8; 32],
}

type AutoSigningCacheKey = (AutoSigningOwner, ProductId);

fn decode_auto_signing_keys(blob: &[u8]) -> Result<Vec<PersistedAutoSigningKey>, AuthorityError> {
	let mut input = blob;
	let keys = Vec::<PersistedAutoSigningKey>::decode(&mut input).map_err(|_| {
		AuthorityError::Unavailable {
			reason: "persisted AutoSigning capabilities are invalid".to_string(),
		}
	})?;
	if !input.is_empty() {
		return Err(AuthorityError::Unavailable {
			reason: "persisted AutoSigning capabilities contain trailing bytes".to_string(),
		});
	}
	Ok(keys)
}

fn validate_auto_signing_key(
	secret: [u8; 64],
	expected_product_subtree_public_key: [u8; 32],
	ring_vrf_domain_entropy: [u8; 32],
) -> Result<AutoSigningKey, AuthorityError> {
	let secret = Zeroizing::new(secret);
	let ring_vrf_domain_entropy = Zeroizing::new(ring_vrf_domain_entropy);
	let secret_key =
		SecretKey::from_bytes(&secret[..]).map_err(|_| AuthorityError::Unavailable {
			reason: "AutoSigning capability contains an invalid subtree secret".to_string(),
		})?;
	if secret_key.to_public().to_bytes() != expected_product_subtree_public_key {
		return Err(AuthorityError::Unavailable {
			reason: "AutoSigning capability does not match the authenticated product subtree"
				.to_string(),
		});
	}
	Ok(AutoSigningKey::from_parts(*secret, *ring_vrf_domain_entropy))
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
	/// Shared chain runtime, used to assemble granted transactions locally.
	pub chain: ChainRuntime,
	/// The paired session, selected and served by [`SsoRequestService`].
	session: Arc<SsoRequestService>,
	/// Active inter-host session with a signing host.
	session_state: Arc<SessionState>,
	statement_store_allowances: Mutex<HashMap<AllowanceCacheKey, StatementStoreAllowanceKey>>,
	bulletin_allowances: Mutex<HashMap<AllowanceCacheKey, BulletinAllowanceKey>>,
	product_subtrees: Mutex<HashMap<(SsoSessionKey, ProductId), [u8; 32]>>,
	auto_signing_keys: Mutex<HashMap<AutoSigningCacheKey, AutoSigningKey>>,
	ring_resolver: Arc<dyn RingResolver>,
	ring_vrf_registry: Arc<RingVrfRegistryStore>,
	/// Orders session-secret cache/storage writes against teardown and activation.
	session_secret_storage: futures::lock::Mutex<()>,
	/// Keep preimage submissions in the core instead of sending them to the
	/// Bulletin chain. A test host whose wallet answers allowances in-page
	/// holds no on-chain authorization to submit with.
	#[cfg(feature = "test-host")]
	submit_preimages_locally: core::sync::atomic::AtomicBool,
	/// Self-reference captured by spawned registration mirrors.
	weak_self: Weak<PairingHost>,
	/// Task spawner for background work.
	pub spawner: Spawner,
}

impl PairingHost {
	/// Keep preimage submissions in the core instead of the Bulletin chain.
	///
	/// For test hosts only, with the `test-host` feature enabled.
	#[cfg(feature = "test-host")]
	pub fn set_submit_preimages_locally(&self, local: bool) {
		self.submit_preimages_locally
			.store(local, core::sync::atomic::Ordering::Relaxed);
	}

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
		Arc::new_cyclic(|weak_self: &Weak<PairingHost>| {
			let owner: Weak<dyn PairedSessionOwner> = weak_self.clone();
			let session = SsoRequestService::new(services.clone(), host_config, owner);
			Self {
				services: services.clone(),
				platform: services.platform.clone(),
				chain: services.chain.clone(),
				session_state: session.session_state(),
				session,
				statement_store_allowances: Mutex::new(HashMap::new()),
				bulletin_allowances: Mutex::new(HashMap::new()),
				product_subtrees: Mutex::new(HashMap::new()),
				auto_signing_keys: Mutex::new(HashMap::new()),
				ring_resolver: ChainRingResolver::new(services.chain.clone()),
				ring_vrf_registry: RingVrfRegistryStore::new(services.platform.clone()),
				session_secret_storage: futures::lock::Mutex::new(()),
				#[cfg(feature = "test-host")]
				submit_preimages_locally: core::sync::atomic::AtomicBool::new(false),
				weak_self: weak_self.clone(),
				spawner: services.spawner.clone(),
			}
		})
	}

	/// The paired session: login, replacement and disconnection.
	pub fn session(&self) -> &Arc<SsoRequestService> {
		&self.session
	}

	fn current_session(&self) -> Option<AuthoritySession> {
		self.session_state.current().as_ref().map(authority_session)
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

	/// Persist the provider choice under the active account.
	pub async fn select_ring_vrf_provider(
		&self,
		ring: latest::RingLocation,
		handle: latest::ProductAccountId,
	) -> Result<(), RingVrfError> {
		let session = self
			.session_state
			.current()
			.ok_or(RingVrfError::Unknown { reason: "no active session".to_string() })?;
		self.ring_vrf_registry.select_provider(session.public_key, ring, handle).await
	}

	/// Disconnect and discard pairing bootstrap material so the next login
	/// generates a new device keypair and topic.
	pub async fn logout_and_reset_pairing(&self) -> Result<(), String> {
		self.session.disconnect().await;
		self.clear_auto_signing_keys().await.map_err(|reason| {
			format!("session disconnected, but AutoSigning reset failed: {reason}")
		})?;
		self.session.forget_pairing_identity().await
	}

	/// Clear all capability material owned by one product while preserving the
	/// active session and unrelated products.
	pub async fn clear_product_state(&self, product_id: &ProductId) -> Result<(), String> {
		let session = self.session.advance_session_lifecycle_preserving_source();
		let _storage_guard = self.session_secret_storage.lock().await;

		self.statement_store_allowances
			.lock()
			.expect("statement-store allowance cache mutex poisoned")
			.retain(|key, _| !key.is_for_product(product_id));
		self.bulletin_allowances
			.lock()
			.expect("bulletin allowance cache mutex poisoned")
			.retain(|key, _| !key.is_for_product(product_id));
		self.product_subtrees
			.lock()
			.expect("product subtree cache mutex poisoned")
			.retain(|(_, cached_product_id), _| cached_product_id != product_id);
		self.auto_signing_keys
			.lock()
			.expect("AutoSigning key cache mutex poisoned")
			.retain(|(_, cached_product_id), _| cached_product_id != product_id);

		let mut first_error =
			self.clear_auto_signing_product_under_storage_guard(product_id).await.err();
		if let Some(session) = session.as_ref() &&
			let Err(error) =
				allowances::clear_product_allowance_keys(&*self.platform, session, product_id)
					.await && first_error.is_none()
		{
			first_error = Some(error.to_string());
		}
		match first_error {
			Some(error) => Err(error),
			None => Ok(()),
		}
	}

	#[cfg(test)]
	pub async fn has_auto_signing_key_for_tests(
		&self,
		session: &SessionInfo,
		product_id: &str,
	) -> Result<bool, AuthorityError> {
		self.auto_signing_key(session, &crate::platform::ProductId::for_test(product_id))
			.await
			.map(|key| key.is_some())
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
			&crate::platform::ProductId::for_test(product_id),
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

	fn current_sso_session_matches(&self, key: SsoSessionKey) -> bool {
		key.matches(&self.session_state)
	}

	fn session_secret_allocation_is_current(
		&self,
		session: &SessionInfo,
		lifecycle_epoch: u64,
	) -> bool {
		session.sso.as_ref().is_some_and(|sso| {
			self.current_sso_session_matches(SsoSessionKey::from_session(sso)) &&
				self.session.is_session_lifecycle_current(lifecycle_epoch)
		})
	}

	fn cache_auto_signing_key_if_current(
		&self,
		session: &SessionInfo,
		lifecycle_epoch: u64,
		cache_key: AutoSigningCacheKey,
		key: AutoSigningKey,
	) -> bool {
		self.session
			.while_session_current(session, lifecycle_epoch, || {
				self.auto_signing_keys
					.lock()
					.expect("AutoSigning key cache mutex poisoned")
					.insert(cache_key, key);
			})
			.is_some()
	}

	/// Whether resolving `product_id`'s subtree would reach the Account Holder,
	/// i.e. neither the memory cache nor the persisted slot already holds it.
	///
	/// A stale or missing session resolves as `false` so a broken state falls
	/// through to the resolution's own error rather than a spurious prompt.
	pub async fn subtree_reaches_account_holder(
		&self,
		session: &AuthoritySession,
		product_id: &ProductId,
	) -> bool {
		let Ok(session) = self.current_private_session(session) else {
			return false;
		};
		let Some(sso) = session.sso.as_ref() else {
			return false;
		};
		let cache_key = (SsoSessionKey::from_session(sso), product_id.clone());
		self.known_product_subtree(&session, cache_key).await.is_none()
	}

	/// Load a retained public subtree without contacting the account holder.
	pub async fn known_product_subtree(
		&self,
		session: &SessionInfo,
		cache_key: (SsoSessionKey, ProductId),
	) -> Option<[u8; 32]> {
		let lifecycle_epoch = self.session.current_session_lifecycle_epoch();
		if let Some(public_key) = self
			.product_subtrees
			.lock()
			.expect("product subtree cache mutex poisoned")
			.get(&cache_key)
			.copied()
		{
			return Some(public_key);
		}
		self.stored_product_subtree(session, lifecycle_epoch, cache_key).await
	}

	/// Restore a public subtree only into the selected live session.
	pub async fn stored_product_subtree(
		&self,
		session: &SessionInfo,
		lifecycle_epoch: u64,
		cache_key: (SsoSessionKey, ProductId),
	) -> Option<[u8; 32]> {
		let public_key =
			product_subtree::read_product_subtree(&*self.platform, session, &cache_key.1)
				.await
				.inspect_err(|error| warn!(reason = %error, "stored product subtree read failed"))
				.ok()
				.flatten()?;
		// A stored key still has to belong to the live pairing before it is
		// served, exactly as a freshly fetched one does.
		self.cache_product_subtree_if_current(session, lifecycle_epoch, cache_key, public_key)
			.then_some(public_key)
	}

	/// Persist and memory-cache a freshly fetched product subtree public key.
	///
	/// Persistence is an optimisation: a storage failure still serves the key
	/// and leaves the next launch to re-ask the wallet, which is what happens
	/// today. Losing the pairing mid-write is not, so the storage entry is
	/// rolled back rather than left addressing a session that has gone.
	pub async fn persist_product_subtree_if_current(
		&self,
		session: &SessionInfo,
		lifecycle_epoch: u64,
		cache_key: (SsoSessionKey, ProductId),
		public_key: [u8; 32],
	) -> bool {
		let product_id = cache_key.1.clone();
		let _storage_guard = self.session_secret_storage.lock().await;
		if !self.session_secret_allocation_is_current(session, lifecycle_epoch) {
			return false;
		}
		let persisted = match product_subtree::write_product_subtree(
			&*self.platform,
			session,
			&product_id,
			public_key,
		)
		.await
		{
			Ok(()) => true,
			Err(error) => {
				warn!(reason = %error, "product subtree persist failed");
				false
			},
		};
		if self.cache_product_subtree_if_current(session, lifecycle_epoch, cache_key, public_key) {
			return true;
		}
		if persisted {
			let _ = product_subtree::remove_product_subtree(&*self.platform, session, &product_id)
				.await;
		}
		false
	}

	/// Cache a public subtree only for the selected live session.
	pub fn cache_product_subtree_if_current(
		&self,
		session: &SessionInfo,
		lifecycle_epoch: u64,
		cache_key: (SsoSessionKey, ProductId),
		public_key: [u8; 32],
	) -> bool {
		self.session
			.while_session_current(session, lifecycle_epoch, || {
				self.product_subtrees
					.lock()
					.expect("product subtree cache mutex poisoned")
					.insert(cache_key, public_key);
			})
			.is_some()
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
		product_id: &ProductId,
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
		product_id: &ProductId,
		allowance: StatementStoreAllowanceKey,
	) -> Result<(), AuthorityError> {
		let cache_key =
			AllowanceCacheKey::new(session, product_id, AllowanceResource::StatementStore)?;
		self.session
			.while_session_current(session, lifecycle_epoch, || {
				self.statement_store_allowances
					.lock()
					.expect("statement-store allowance cache mutex poisoned")
					.insert(cache_key, allowance);
			})
			.ok_or(AuthorityError::Disconnected)
	}

	/// Cached statement-store allowance key for the product, falling back to
	/// persisted storage.
	pub async fn cached_statement_store_allowance_key(
		&self,
		session: &SessionInfo,
		lifecycle_epoch: u64,
		product_id: &ProductId,
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
		product_id: &ProductId,
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
		product_id: &ProductId,
		allowance: BulletinAllowanceKey,
	) -> Result<(), AuthorityError> {
		let cache_key = AllowanceCacheKey::new(session, product_id, AllowanceResource::Bulletin)?;
		self.session
			.while_session_current(session, lifecycle_epoch, || {
				self.bulletin_allowances
					.lock()
					.expect("bulletin allowance cache mutex poisoned")
					.insert(cache_key, allowance);
			})
			.ok_or(AuthorityError::Disconnected)
	}

	/// Cached Bulletin allowance key for the product, falling back to
	/// persisted storage.
	pub async fn cached_bulletin_allowance_key(
		&self,
		session: &SessionInfo,
		lifecycle_epoch: u64,
		product_id: &ProductId,
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
		product_id: &ProductId,
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

	/// Drop memory-cached statement-store allowance keys, scoped to `session`
	/// when given, otherwise all.
	pub fn clear_statement_store_allowance_keys(&self, session: Option<&SessionInfo>) {
		let mut allowances = self
			.statement_store_allowances
			.lock()
			.expect("statement-store allowance cache mutex poisoned");
		let Some(session) = session else {
			allowances.clear();
			return;
		};
		let Some(sso) = session.sso.as_ref() else {
			return;
		};
		let session_key = SsoSessionKey::from_session(sso);
		allowances.retain(|key, _| !key.is_for_session(session_key));
	}

	/// Drop memory-cached Bulletin allowance keys, scoped to `session` when
	/// given, otherwise all.
	pub fn clear_bulletin_allowance_keys(&self, session: Option<&SessionInfo>) {
		let mut allowances = self
			.bulletin_allowances
			.lock()
			.expect("bulletin allowance cache mutex poisoned");
		let Some(session) = session else {
			allowances.clear();
			return;
		};
		let Some(sso) = session.sso.as_ref() else {
			return;
		};
		let session_key = SsoSessionKey::from_session(sso);
		allowances.retain(|key, _| !key.is_for_session(session_key));
	}

	async fn clear_auto_signing_keys(&self) -> Result<(), String> {
		let _storage_guard = self.session_secret_storage.lock().await;
		self.clear_auto_signing_keys_under_storage_guard().await
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
		product_id: &ProductId,
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
						keys.retain(|key| &key.product_id != product_id);
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

	async fn clear_auto_signing_keys_for_other_owner(
		&self,
		session: &SessionInfo,
	) -> Result<(), String> {
		let owner = AutoSigningOwner::from_session(session);
		let _storage_guard = self.session_secret_storage.lock().await;
		let Some(mut blob) = self
			.platform
			.read_core_storage(CoreStorageKey::AutoSigningKeys)
			.await
			.map_err(|err| err.reason)?
		else {
			return Ok(());
		};
		let decoded = decode_auto_signing_keys(&blob);
		blob.zeroize();
		let should_clear = decoded
			.as_ref()
			.map(|keys| keys.iter().any(|key| key.owner != owner))
			.unwrap_or(true);
		if !should_clear {
			return Ok(());
		}
		self.auto_signing_keys
			.lock()
			.expect("AutoSigning key cache mutex poisoned")
			.clear();
		self.platform
			.clear_core_storage(CoreStorageKey::AutoSigningKeys)
			.await
			.map_err(|err| err.reason)
	}

	async fn clear_legacy_auto_signing_key(
		&self,
		product_id: &ProductId,
	) -> Result<bool, AuthorityError> {
		let storage_key = CoreStorageKey::AutoSigningKey { product_id: product_id.to_string() };
		let legacy = self.platform.read_core_storage(storage_key.clone()).await.map_err(|err| {
			AuthorityError::Unknown {
				reason: format!("failed to inspect legacy AutoSigning key: {}", err.reason),
			}
		})?;
		let present = if let Some(mut secret) = legacy {
			secret.zeroize();
			true
		} else {
			false
		};
		if present {
			self.platform.clear_core_storage(storage_key).await.map_err(|err| {
				AuthorityError::Unknown {
					reason: format!("failed to clear legacy AutoSigning key: {}", err.reason),
				}
			})?;
		}
		Ok(present)
	}

	async fn remember_auto_signing_key(
		&self,
		session: &SessionInfo,
		lifecycle_epoch: u64,
		product_id: &ProductId,
		expected_product_subtree_public_key: [u8; 32],
		secret: [u8; 64],
		ring_vrf_domain_entropy: [u8; 32],
	) -> Result<(), AuthorityError> {
		let key = validate_auto_signing_key(
			secret,
			expected_product_subtree_public_key,
			ring_vrf_domain_entropy,
		)?;
		let owner = AutoSigningOwner::from_session(session);
		let cache_key = (owner.clone(), product_id.clone());
		let _storage_guard = self.session_secret_storage.lock().await;
		if !self.session_secret_allocation_is_current(session, lifecycle_epoch) {
			return Err(AuthorityError::Disconnected);
		}
		self.clear_legacy_auto_signing_key(product_id).await?;
		let mut keys =
			match self.platform.read_core_storage(CoreStorageKey::AutoSigningKeys).await.map_err(
				|err| AuthorityError::Unknown {
					reason: format!("failed to read AutoSigning capabilities: {}", err.reason),
				},
			)? {
				Some(mut blob) => {
					let decoded = decode_auto_signing_keys(&blob).unwrap_or_default();
					blob.zeroize();
					decoded
				},
				None => Vec::new(),
			};
		keys.retain(|persisted| persisted.owner == owner && &persisted.product_id != product_id);
		keys.push(PersistedAutoSigningKey {
			owner,
			product_id: product_id.clone(),
			expected_product_subtree_public_key,
			secret,
			ring_vrf_domain_entropy,
		});
		if !self.session_secret_allocation_is_current(session, lifecycle_epoch) {
			return Err(AuthorityError::Disconnected);
		}
		self.platform
			.write_core_storage(CoreStorageKey::AutoSigningKeys, keys.encode())
			.await
			.map_err(|err| AuthorityError::Unknown {
				reason: format!("failed to persist AutoSigning capability: {}", err.reason),
			})?;
		if !self.cache_auto_signing_key_if_current(session, lifecycle_epoch, cache_key, key) {
			let _ = self.clear_auto_signing_product_under_storage_guard(product_id).await;
			return Err(AuthorityError::Disconnected);
		}
		Ok(())
	}

	async fn auto_signing_key(
		&self,
		session: &SessionInfo,
		product_id: &ProductId,
	) -> Result<Option<AutoSigningKey>, AuthorityError> {
		let owner = AutoSigningOwner::from_session(session);
		let cache_key = (owner.clone(), product_id.clone());
		if let Some(key) = self
			.auto_signing_keys
			.lock()
			.expect("AutoSigning key cache mutex poisoned")
			.get(&cache_key)
			.cloned()
		{
			return Ok(Some(key));
		}

		let _storage_guard = self.session_secret_storage.lock().await;
		if let Some(key) = self
			.auto_signing_keys
			.lock()
			.expect("AutoSigning key cache mutex poisoned")
			.get(&cache_key)
			.cloned()
		{
			return Ok(Some(key));
		}
		let legacy_present = self.clear_legacy_auto_signing_key(product_id).await?;
		let Some(mut blob) =
			self.platform.read_core_storage(CoreStorageKey::AutoSigningKeys).await.map_err(
				|err| AuthorityError::Unknown {
					reason: format!("failed to read AutoSigning capabilities: {}", err.reason),
				},
			)?
		else {
			return if legacy_present {
				Err(AuthorityError::Unavailable {
					reason: "legacy unscoped AutoSigning capability was rejected".to_string(),
				})
			} else {
				Ok(None)
			};
		};
		let decoded = decode_auto_signing_keys(&blob);
		blob.zeroize();
		let keys = match decoded {
			Ok(keys) => keys,
			Err(err) => {
				self.auto_signing_keys
					.lock()
					.expect("AutoSigning key cache mutex poisoned")
					.clear();
				let _ = self.platform.clear_core_storage(CoreStorageKey::AutoSigningKeys).await;
				return Err(err);
			},
		};
		if keys.iter().any(|persisted| persisted.owner != owner) {
			self.auto_signing_keys
				.lock()
				.expect("AutoSigning key cache mutex poisoned")
				.clear();
			let _ = self.platform.clear_core_storage(CoreStorageKey::AutoSigningKeys).await;
			return Ok(None);
		}
		let Some(persisted) = keys.iter().find(|persisted| &persisted.product_id == product_id)
		else {
			return if legacy_present {
				Err(AuthorityError::Unavailable {
					reason: "legacy unscoped AutoSigning capability was rejected".to_string(),
				})
			} else {
				Ok(None)
			};
		};
		let current_expected_subtree = session.sso.as_ref().and_then(|sso| {
			self.product_subtrees
				.lock()
				.expect("product subtree cache mutex poisoned")
				.get(&(SsoSessionKey::from_session(sso), product_id.clone()))
				.copied()
		});
		if current_expected_subtree
			.is_some_and(|expected| expected != persisted.expected_product_subtree_public_key)
		{
			self.auto_signing_keys
				.lock()
				.expect("AutoSigning key cache mutex poisoned")
				.clear();
			let _ = self.platform.clear_core_storage(CoreStorageKey::AutoSigningKeys).await;
			return Err(AuthorityError::Unavailable {
				reason: "AutoSigning capability is not for the current product subtree".to_string(),
			});
		}
		let key = match validate_auto_signing_key(
			persisted.secret,
			persisted.expected_product_subtree_public_key,
			persisted.ring_vrf_domain_entropy,
		) {
			Ok(key) => key,
			Err(err) => {
				self.auto_signing_keys
					.lock()
					.expect("AutoSigning key cache mutex poisoned")
					.clear();
				let _ = self.platform.clear_core_storage(CoreStorageKey::AutoSigningKeys).await;
				return Err(err);
			},
		};
		self.auto_signing_keys
			.lock()
			.expect("AutoSigning key cache mutex poisoned")
			.insert(cache_key, key.clone());
		Ok(Some(key))
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
		let product_ids: Vec<ProductId> = self
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

	fn clear_product_subtrees(&self, session: Option<&SessionInfo>) {
		let mut subtrees =
			self.product_subtrees.lock().expect("product subtree cache mutex poisoned");
		let Some(session) = session else {
			subtrees.clear();
			return;
		};
		let Some(sso) = session.sso.as_ref() else {
			return;
		};
		let session_key = SsoSessionKey::from_session(sso);
		subtrees.retain(|(key, _), _| *key != session_key);
	}

	/// Whether `calling_product_id` may act on `handle`'s ring-VRF key.
	///
	/// Delegates to [`crate::runtime::ring_vrf_key_access_granted`], which
	/// resolves the owner's manifest here rather than trusting the request: on
	/// this role the request can have arrived over the pairing wire.
	async fn require_ring_vrf_key_access(
		&self,
		calling_product_id: ProductId,
		handle: &latest::ProductAccountId,
	) -> Result<
		(latest::ProductAccountId, crate::runtime::product_manifest::AuthorizedAccess),
		RingVrfError,
	> {
		let access = crate::runtime::product_manifest::ring_vrf_key_access_granted(
			&self.services,
			self.platform.as_ref(),
			calling_product_id,
			handle,
		)
		.await?;
		Ok((
			latest::ProductAccountId {
				dot_ns_identifier: access.owner.clone(),
				derivation_index: handle.derivation_index.clone(),
			},
			access,
		))
	}

	async fn local_ring_vrf_entropy(
		&self,
		session: &SessionInfo,
		handle: &latest::ProductAccountId,
	) -> Result<Option<Zeroizing<[u8; 32]>>, RingVrfError> {
		let Some(auto_signing) = self
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

	async fn product_subtree_public_key(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		product_id: &ProductId,
	) -> Result<[u8; 32], AuthorityError> {
		let session = self.current_private_session(session)?;
		self.remote_product_subtree_public_key(cx, &session, product_id).await
	}

	/// The keypair that can serve `account` locally under an AutoSigning
	/// capability, when this host holds one.
	///
	/// The single source of truth for local serviceability: the grant
	/// predicate and every local fast path call it, so a request can never be
	/// told "no prompt" and then relayed to a signing host that will prompt
	/// for it.
	///
	/// A caller may name an account belonging to another product.
	/// `may_use_product_account` admits that for a `localhost` caller, and
	/// for one the owner granted `context`, and the grant covers the owning
	/// product's subtree, not the caller's. Serving one locally would sign with a key the caller
	/// was never granted and skip the confirmation the signing host raises for a
	/// relayed request, so the binding is checked here rather than only at the
	/// gate.
	async fn local_product_signing_key(
		&self,
		session: &SessionInfo,
		calling_product_id: Option<&ProductId>,
		account: &latest::ProductAccountId,
	) -> Result<Option<schnorrkel::Keypair>, AuthorityError> {
		let Some(calling_product_id) = calling_product_id else {
			return Ok(None);
		};
		if *calling_product_id != account.dot_ns_identifier {
			return Ok(None);
		}
		let Some(key) = self.auto_signing_key(session, calling_product_id).await? else {
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
		calling_product_id: &ProductId,
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

	async fn sign_vrf(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		calling_product_id: &ProductId,
		request: latest::HostAccountSignVrfRequest,
	) -> Result<latest::VrfSignature, AuthorityError> {
		let session = self.current_private_session(session)?;
		if *calling_product_id == request.account.dot_ns_identifier &&
			let Some(auto_signing_key) =
				self.auto_signing_key(&session, calling_product_id).await?
		{
			let keypair = derive_product_keypair_from_subtree_secret(
				*auto_signing_key.as_secret_bytes(),
				derivation_index_bytes(&request.account.derivation_index),
			)
			.map_err(|err| AuthorityError::Unknown { reason: err.to_string() })?;
			let (pre_output, proof) = crate::dynamic_vrf::sign_dynamic_vrf(
				&keypair,
				&request.transcript_label,
				request.items.iter().map(|item| (item.label.as_slice(), item.value.as_slice())),
			);
			return Ok(latest::VrfSignature { pre_output, proof });
		}
		if !super::authority::is_blessed_owner(
			calling_product_id,
			&request.account.dot_ns_identifier,
		) {
			let confirmed = super::until_cancelled(
				cx,
				self.platform
					.confirm_user_action(UserConfirmationReview::SignVrf(SignVrfReview {
						calling_product_id: calling_product_id.clone(),
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
		self.remote_sign_vrf(cx, &session, calling_product_id, request).await
	}

	async fn sign_payload(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		calling_product_id: Option<&ProductId>,
		request: SignPayloadAuthorityRequest,
	) -> Result<latest::HostSignPayloadResponse, AuthorityError> {
		let session = self.current_private_session(session)?;
		if let SignPayloadAuthorityRequest::Product(payload) = &request &&
			let Some(keypair) = self
				.local_product_signing_key(&session, calling_product_id, &payload.account)
				.await?
		{
			return Ok(sign_extrinsic_payload(&keypair, payload.payload.clone())?);
		}
		self.remote_sign_payload(cx, &session, request).await
	}

	async fn sign_raw(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		calling_product_id: Option<&ProductId>,
		request: SignRawAuthorityRequest,
		watermarked: bool,
	) -> Result<latest::HostSignPayloadResponse, AuthorityError> {
		let session = self.current_private_session(session)?;
		// The unwatermarked API is never grant-covered, so a local signature
		// here would skip a prompt the gate deliberately raised.
		if watermarked &&
			let SignRawAuthorityRequest::Product(payload) = &request &&
			let Some(keypair) = self
				.local_product_signing_key(&session, calling_product_id, &payload.account)
				.await?
		{
			let message = raw_payload_bytes(payload.payload.clone(), watermarked)?;
			let signature = keypair
				.secret
				.sign_simple(SR25519_SIGNING_CONTEXT, &message, &keypair.public)
				.to_bytes();
			return Ok(latest::HostSignPayloadResponse {
				signature: signature.to_vec(),
				signed_transaction: None,
			});
		}
		self.remote_sign_raw(cx, &session, request, watermarked).await
	}

	async fn create_transaction(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		calling_product_id: Option<&ProductId>,
		request: CreateTransactionAuthorityRequest,
	) -> Result<latest::HostCreateTransactionResponse, AuthorityError> {
		let session = self.current_private_session(session)?;
		if let CreateTransactionAuthorityRequest::Product(payload) = &request &&
			let Some(keypair) = self
				.local_product_signing_key(&session, calling_product_id, &payload.signer)
				.await?
		{
			// A payload this host cannot assemble is an error, not a
			// fall-through to the relay: the gate already told the caller no
			// prompt was coming, and relaying would raise one on the signing
			// host after a chain timeout.
			//
			// Assembling needs runtime metadata, which a granted product
			// moves from the signing host to this one. A pairing host is
			// assumed to reach any genesis a product it has granted signs
			// against; where it cannot, the call fails rather than producing
			// the prompt-and-signature an ungranted product would have got.
			//
			// The failure is not prompt. An unreachable genesis leaves the
			// metadata read waiting on the chain, so the caller can sit on the
			// authority request timeout before it sees anything — the cost of
			// assembling locally is paid before the grant can be found wanting.
			return Ok(build_local_transaction(
				&self.chain,
				&keypair,
				payload.genesis_hash,
				&payload.call_data,
				&payload.extensions,
				payload.tx_ext_version,
			)
			.await?);
		}
		self.remote_create_transaction(cx, &session, request).await
	}

	async fn account_alias(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		request: ProductRequest<HostAccountGetAliasRequest>,
	) -> Result<latest::ContextualAlias, RingVrfError> {
		let private_session = self.current_private_session(session)?;
		if request.calling_product_id == request.payload.key_handle.dot_ns_identifier &&
			let Some(entropy) = self
				.local_ring_vrf_entropy_for_ring(
					&private_session,
					&request.payload.key_handle,
					&request.payload.ring_location,
				)
				.await?
		{
			self.ring_resolver.validate(&request.payload.ring_location).await?;
			let vrf = vrf::load().await?;
			self.current_private_session(session)?;
			let context = context_bytes(&request.payload.context);
			let alias = vrf.alias(&entropy, &context)?;
			return Ok(latest::ContextualAlias { context, alias: alias.to_vec() });
		}
		self.remote_account_alias(cx, &private_session, request).await
	}

	async fn create_proof(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		request: ProductRequest<HostAccountCreateProofRequest>,
	) -> Result<latest::HostAccountCreateProofResponse, RingVrfError> {
		let (key_handle, access) = self
			.require_ring_vrf_key_access(
				request.calling_product_id.clone(),
				&request.payload.key_handle,
			)
			.await?;
		// A grant cannot expose the owner's alias in a third product's context unless
		// that product's manifest grants the caller `context`.
		crate::runtime::product_manifest::require_context_access(
			&self.services,
			&access,
			&request.payload.context,
		)
		.await?;
		let private_session = self.current_private_session(session)?;
		if let Some(entropy) = self
			.local_ring_vrf_entropy_for_ring(
				&private_session,
				&key_handle,
				&request.payload.ring_location,
			)
			.await?
		{
			let vrf = vrf::load().await?;
			let member = vrf.member(&entropy)?;
			let resolved = self
				.ring_resolver
				.resolve(&request.payload.ring_location, &[MemberCandidate { member }])
				.await?;
			self.current_private_session(session)?;
			let context = context_bytes(&request.payload.context);
			let (proof, alias) =
				create_proof(&vrf, &entropy, &resolved, &context, &request.payload.message)?;
			return Ok(latest::HostAccountCreateProofResponse {
				proof,
				contextual_alias: latest::ContextualAlias { context, alias: alias.to_vec() },
				ring_index: resolved.ring_index,
				ring_revision: resolved.ring_revision,
			});
		}
		self.remote_create_proof(cx, &private_session, request).await
	}

	async fn register_ring_vrf_key(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		request: ProductRequest<HostAccountRegisterRingVrfKeyRequest>,
	) -> Result<[u8; 32], RingVrfError> {
		let private_session = self.current_private_session(session)?;
		let handle = latest::ProductAccountId {
			dot_ns_identifier: request.calling_product_id.clone(),
			derivation_index: request.payload.index.clone(),
		};
		if let Some(auto_signing) = self
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
		let public_key =
			self.remote_register_ring_vrf_key(cx, &private_session, request.clone()).await?;
		self.ring_vrf_registry
			.register(private_session.public_key, handle, request.payload.ring, public_key)
			.await?;
		self.current_private_session(session)?;
		Ok(public_key)
	}

	async fn list_ring_vrf_keys(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		request: ProductRequest<HostAccountListRingVrfKeysRequest>,
	) -> Result<Vec<latest::RegisteredRingVrfKey>, RingVrfError> {
		let private_session = self.current_private_session(session)?;
		let owner = request
			.payload
			.owner
			.parse::<ProductId>()
			.map_err(|error| RingVrfError::Unknown { reason: error.to_string() })?;
		if request.calling_product_id == owner.as_str() &&
			let Some(mut entries) = self
				.ring_vrf_registry
				.complete_owner_entries(private_session.public_key, &owner)
				.await?
		{
			self.current_private_session(session)?;
			apply_ring_vrf_disclosure(&mut entries, request.payload.disclosure);
			return Ok(entries);
		}
		let requested_disclosure = request.payload.disclosure;
		let mut remote_request = request;
		if remote_request.calling_product_id == owner.as_str() {
			remote_request.payload.disclosure = latest::RingVrfKeyDisclosure::PublicKey;
		}
		let mut entries =
			self.remote_list_ring_vrf_keys(cx, &private_session, remote_request).await?;
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
		cx: &CallContext,
		session: &AuthoritySession,
		request: ProductRequest<HostAccountRingVrfSignRequest>,
	) -> Result<Vec<u8>, RingVrfError> {
		let (key_handle, _access) = self
			.require_ring_vrf_key_access(
				request.calling_product_id.clone(),
				&request.payload.key_handle,
			)
			.await?;
		let private_session = self.current_private_session(session)?;
		if let Some(entropy) = self.local_ring_vrf_entropy(&private_session, &key_handle).await? {
			let vrf = vrf::load().await?;
			self.current_private_session(session)?;
			return vrf.sign(&entropy, &request.payload.message);
		}
		self.remote_ring_vrf_sign(cx, &private_session, request).await
	}

	async fn allocate_resources(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		product_id: &ProductId,
		request: latest::HostRequestResourceAllocationRequest,
	) -> Result<latest::HostRequestResourceAllocationResponse, AuthorityError> {
		let session = self.current_private_session(session)?;
		self.remote_allocate_resources(cx, &session, product_id, request).await
	}

	async fn statement_store_allowance_key(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		product_id: &ProductId,
	) -> Result<StatementStoreAllowanceKey, AuthorityError> {
		let session = self.current_private_session(session)?;
		self.remote_statement_store_allowance_key(cx, &session, product_id).await
	}

	async fn bulletin_allowance_key(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		product_id: &ProductId,
	) -> Result<BulletinAllowanceKey, AuthorityError> {
		let session = self.current_private_session(session)?;
		self.remote_bulletin_allowance_key(cx, &session, product_id).await
	}

	async fn refresh_bulletin_allowance_key(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		product_id: &ProductId,
	) -> Result<BulletinAllowanceKey, AuthorityError> {
		let session = self.current_private_session(session)?;
		self.remote_refresh_bulletin_allowance_key(cx, &session, product_id).await
	}

	async fn sign_statement_store_product_payload(
		&self,
		_cx: &CallContext,
		session: &AuthoritySession,
		calling_product_id: Option<&ProductId>,
		account: latest::ProductAccountId,
		payload: Vec<u8>,
	) -> Result<[u8; 64], AuthorityError> {
		let session = self.current_private_session(session)?;
		// The SSO raw-signing protocol cannot carry an exact, unwatermarked
		// payload, so the capability's own key is the only way this role signs
		// one. Statement proofs raise no confirmation on either role, so this
		// unlocks the operation rather than waiving a prompt.
		let keypair = self
			.local_product_signing_key(&session, calling_product_id, &account)
			.await?
			.ok_or_else(|| AuthorityError::Unavailable {
				reason: "pairing host: exact statement proof signing needs an AutoSigning \
                         capability; the current SSO raw-signing protocol cannot carry it"
					.to_string(),
			})?;
		Ok(keypair
			.secret
			.sign_simple(SR25519_SIGNING_CONTEXT, &payload, &keypair.public)
			.to_bytes())
	}

	fn derive_entropy(
		&self,
		session: &AuthoritySession,
		product_id: &ProductId,
		context: &[u8],
	) -> Result<[u8; 32], AuthorityError> {
		let session = self.current_private_session(session)?;
		if session.sso.is_none() {
			return Err(AuthorityError::Disconnected);
		}
		let root_entropy_source = session.root_entropy_source.ok_or_else(|| {
			AuthorityError::Unavailable { reason: "Session secret missing".to_string() }
		})?;
		derive_product_entropy_from_source(&root_entropy_source, product_id, context)
			.map_err(|err| AuthorityError::Unknown { reason: err.to_string() })
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
impl PairedSessionOwner for PairingHost {
	fn session_ended(&self, previous: Option<&SessionInfo>) {
		self.clear_statement_store_allowance_keys(previous);
		self.clear_bulletin_allowance_keys(previous);
		self.clear_product_subtrees(previous);
	}

	async fn session_cleared(&self, previous: Option<&SessionInfo>) {
		let _storage_guard = self.session_secret_storage.lock().await;
		if let Some(session) = previous {
			self.clear_statement_store_allowance_keys(Some(session));
			self.clear_bulletin_allowance_keys(Some(session));
			if let Err(reason) =
				allowances::clear_session_allowance_keys(&*self.platform, session).await
			{
				warn!(%reason, "allowance capability clear failed during disconnect");
			}
			self.clear_stored_product_subtrees(session).await;
		}
		self.clear_product_subtrees(previous);
		if let Err(reason) = self.clear_auto_signing_keys_under_storage_guard().await {
			warn!(%reason, "AutoSigning capability clear failed during disconnect");
		}
	}

	async fn prepare_session(&self, previous: Option<&SessionInfo>, next: &SessionInfo) {
		let identity_replaced = previous.is_some_and(|previous| {
			AutoSigningOwner::from_session(previous) != AutoSigningOwner::from_session(next)
		});
		if identity_replaced {
			if let Err(reason) = self.clear_auto_signing_keys().await {
				warn!(%reason, "AutoSigning capability clear failed during identity replacement");
			}
		} else if previous.is_none() {
			if let Err(reason) = self.clear_auto_signing_keys_for_other_owner(next).await {
				warn!(%reason, "AutoSigning capability owner reconciliation failed");
			}
		} else {
			let _storage_guard = self.session_secret_storage.lock().await;
		}
		if let Some(previous) = previous.filter(|previous| *previous != next) {
			let _storage_guard = self.session_secret_storage.lock().await;
			self.clear_statement_store_allowance_keys(Some(previous));
			self.clear_bulletin_allowance_keys(Some(previous));
			if let Err(reason) =
				allowances::clear_session_allowance_keys(&*self.platform, previous).await
			{
				warn!(%reason, "allowance capability clear failed during session replacement");
			}
		}
	}

	async fn session_reset(&self) {
		let _storage_guard = self.session_secret_storage.lock().await;
		self.clear_statement_store_allowance_keys(None);
		self.clear_bulletin_allowance_keys(None);
		self.clear_product_subtrees(None);
	}
}

#[async_trait::async_trait]
impl ProductAuthority for PairingHost {
	fn current_session(&self) -> Option<AuthoritySession> {
		PairingHost::current_session(self)
	}

	fn session_is_current(
		&self,
		session: &AuthoritySession,
		_product_id: Option<&ProductId>,
	) -> bool {
		// Names may refresh during consent; the authenticated identity and
		// pairing token, rather than display metadata, define the authority.
		self.current_private_session(session).is_ok_and(|current| {
			current.public_key == session.public_key &&
				current.identity_account_id == session.identity_account_id
		})
	}

	fn session_state(&self) -> Arc<SessionState> {
		self.session_state.clone()
	}

	#[cfg(feature = "test-host")]
	fn submits_preimages_locally(&self) -> bool {
		self.submit_preimages_locally.load(core::sync::atomic::Ordering::Relaxed)
	}

	#[cfg(test)]
	fn cache_product_subtree_for_test(
		&self,
		session: &SessionInfo,
		product_id: &ProductId,
		public_key: [u8; 32],
	) {
		let sso = session.sso.as_ref().expect("test session must contain SSO");
		self.product_subtrees
			.lock()
			.expect("product subtree cache mutex poisoned")
			.insert((SsoSessionKey::from_session(sso), product_id.clone()), public_key);
	}

	async fn product_subtree_public_key(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		product_id: &ProductId,
	) -> Result<[u8; 32], AuthorityError> {
		PairingHost::product_subtree_public_key(self, cx, session, product_id).await
	}

	async fn subtree_resolution_reaches_account_holder(
		&self,
		session: &AuthoritySession,
		product_id: &ProductId,
	) -> bool {
		PairingHost::subtree_reaches_account_holder(self, session, product_id).await
	}

	async fn auto_signing_status(
		&self,
		session: &AuthoritySession,
		calling_product_id: &ProductId,
		account: &latest::ProductAccountId,
	) -> Result<AutoSigningGrant, AuthorityError> {
		PairingHost::auto_signing_status(self, session, calling_product_id, account).await
	}

	async fn refresh_session_identity(&self) -> Result<Option<AuthoritySession>, String> {
		self.session.refresh_current_session_identity().await
	}

	async fn sign_vrf(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		calling_product_id: &ProductId,
		request: latest::HostAccountSignVrfRequest,
	) -> Result<latest::VrfSignature, AuthorityError> {
		PairingHost::sign_vrf(self, cx, session, calling_product_id, request).await
	}

	async fn sign_payload(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		calling_product_id: Option<&ProductId>,
		request: SignPayloadAuthorityRequest,
	) -> Result<latest::HostSignPayloadResponse, AuthorityError> {
		PairingHost::sign_payload(self, cx, session, calling_product_id, request).await
	}

	async fn sign_raw(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		calling_product_id: Option<&ProductId>,
		request: SignRawAuthorityRequest,
		watermarked: bool,
	) -> Result<latest::HostSignPayloadResponse, AuthorityError> {
		PairingHost::sign_raw(self, cx, session, calling_product_id, request, watermarked).await
	}

	async fn create_transaction(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		calling_product_id: Option<&ProductId>,
		request: CreateTransactionAuthorityRequest,
	) -> Result<latest::HostCreateTransactionResponse, AuthorityError> {
		PairingHost::create_transaction(self, cx, session, calling_product_id, request).await
	}

	async fn account_alias(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		request: ProductRequest<HostAccountGetAliasRequest>,
	) -> Result<latest::ContextualAlias, RingVrfError> {
		PairingHost::account_alias(self, cx, session, request).await
	}

	async fn create_proof(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		request: ProductRequest<HostAccountCreateProofRequest>,
	) -> Result<latest::HostAccountCreateProofResponse, RingVrfError> {
		PairingHost::create_proof(self, cx, session, request).await
	}

	async fn register_ring_vrf_key(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		request: ProductRequest<HostAccountRegisterRingVrfKeyRequest>,
	) -> Result<[u8; 32], RingVrfError> {
		PairingHost::register_ring_vrf_key(self, cx, session, request).await
	}

	async fn list_ring_vrf_keys(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		request: ProductRequest<HostAccountListRingVrfKeysRequest>,
	) -> Result<Vec<latest::RegisteredRingVrfKey>, RingVrfError> {
		PairingHost::list_ring_vrf_keys(self, cx, session, request).await
	}

	async fn ring_vrf_sign(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		request: ProductRequest<HostAccountRingVrfSignRequest>,
	) -> Result<Vec<u8>, RingVrfError> {
		PairingHost::ring_vrf_sign(self, cx, session, request).await
	}

	async fn allocate_resources(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		product_id: &ProductId,
		request: latest::HostRequestResourceAllocationRequest,
	) -> Result<latest::HostRequestResourceAllocationResponse, AuthorityError> {
		PairingHost::allocate_resources(self, cx, session, product_id, request).await
	}

	async fn statement_store_allowance_key(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		product_id: &ProductId,
	) -> Result<StatementStoreAllowanceKey, AuthorityError> {
		PairingHost::statement_store_allowance_key(self, cx, session, product_id).await
	}

	async fn bulletin_allowance_key(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		product_id: &ProductId,
	) -> Result<BulletinAllowanceKey, AuthorityError> {
		PairingHost::bulletin_allowance_key(self, cx, session, product_id).await
	}

	async fn refresh_bulletin_allowance_key(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		product_id: &ProductId,
	) -> Result<BulletinAllowanceKey, AuthorityError> {
		PairingHost::refresh_bulletin_allowance_key(self, cx, session, product_id).await
	}

	async fn product_device_chat(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		request: ProductDeviceChatAuthorityRequest,
	) -> Result<latest::HostProductDeviceChatResponse, ProductDeviceChatAuthorityError> {
		let private_session = self
			.current_private_session(session)
			.map_err(|_| ProductDeviceChatAuthorityError::Disconnected)?;
		self.remote_product_device_chat(cx, &private_session, request).await
	}

	async fn payment_top_up(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		request: PaymentTopUpRequest,
	) -> Result<(), PaymentTopUpAuthorityError> {
		let current = require_current_session(&self.session_state, session)?;
		self.remote_payment_top_up(cx, &current, request).await
	}

	async fn sign_statement_store_product_payload(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		calling_product_id: Option<&ProductId>,
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

	fn derive_entropy(
		&self,
		session: &AuthoritySession,
		product_id: &ProductId,
		context: &[u8],
	) -> Result<[u8; 32], AuthorityError> {
		PairingHost::derive_entropy(self, session, product_id, context)
	}

	fn contacts_handle_key(&self, session: &AuthoritySession) -> Result<[u8; 32], AuthorityError> {
		PairingHost::contacts_handle_key(self, session)
	}
}
