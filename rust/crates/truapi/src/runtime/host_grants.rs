//! Retained host capabilities and their persistence barrier.

use super::{
	allowances::{self, AllowanceCacheKey, AllowanceResource, GrantScope},
	authority::{
		Allowance, AuthorityError, AutoSigningKey, BulletinAllowanceKey, GrantKeeping,
		StatementStoreAllowanceKey,
	},
	product_subtree,
};
use crate::{
	host_logic::session::{SessionInfo, SessionState},
	platform::{CoreStorage, CoreStorageKey, ProductId},
};
use parity_scale_codec::{Decode, Encode};
use schnorrkel::SecretKey;
use std::{
	collections::HashMap,
	sync::{Arc, Mutex, MutexGuard},
};
use tracing::warn;
use zeroize::Zeroize;

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
	key: &AutoSigningKey,
	expected_product_subtree_public_key: [u8; 32],
) -> Result<(), AuthorityError> {
	let secret_key =
		SecretKey::from_bytes(key.as_secret_bytes()).map_err(|_| AuthorityError::Unavailable {
			reason: "AutoSigning capability contains an invalid subtree secret".to_string(),
		})?;
	if secret_key.to_public().to_bytes() != expected_product_subtree_public_key {
		return Err(AuthorityError::Unavailable {
			reason: "AutoSigning capability does not match the authenticated product subtree"
				.to_string(),
		});
	}
	Ok(())
}

#[derive(Default)]
struct GrantState {
	revision: u64,
	pending_deletions: Vec<CoreStorageKey>,
}

impl GrantState {
	fn queue_deletion(&mut self, key: CoreStorageKey) {
		if !self.pending_deletions.contains(&key) {
			self.pending_deletions.push(key);
		}
	}
}

/// Grants one host keeps for its products, and the revision that fences them.
///
/// Holds Statement Store and Bulletin allowance keys, AutoSigning keys and
/// product subtrees in memory. Under [`GrantKeeping::Paired`] it also stores
/// them in [`CoreStorage`] and queues the deletions a revoked session still
/// owes. Grant work reads the revision when it starts and is rejected with
/// [`AuthorityError::Disconnected`] once a reset, lock or session change has
/// advanced it.
pub struct HostGrantStore {
	storage: Arc<dyn CoreStorage>,
	keeping: GrantKeeping,
	state: Mutex<GrantState>,
	persistence: futures::lock::Mutex<()>,
	caches: Mutex<GrantCaches>,
	session_state: Arc<SessionState>,
}

#[derive(Default)]
struct GrantCaches {
	statement_store_allowances:
		HashMap<AllowanceCacheKey, (Option<u32>, StatementStoreAllowanceKey)>,
	bulletin_allowances: HashMap<AllowanceCacheKey, BulletinAllowanceKey>,
	product_subtrees: HashMap<(GrantScope, ProductId), [u8; 32]>,
	auto_signing_keys: HashMap<AutoSigningCacheKey, AutoSigningKey>,
}

impl GrantCaches {
	fn forget_product(&mut self, product_id: &ProductId) {
		self.statement_store_allowances.retain(|key, _| !key.is_for_product(product_id));
		self.bulletin_allowances.retain(|key, _| !key.is_for_product(product_id));
		self.product_subtrees.retain(|(_, owner), _| owner != product_id);
		self.auto_signing_keys.retain(|(_, owner), _| owner != product_id);
	}

	/// Drop the allowance keys cached for `session`, or for every session.
	fn forget_allowances(&mut self, session: Option<&SessionInfo>) {
		let Some(session) = session else {
			self.statement_store_allowances.clear();
			self.bulletin_allowances.clear();
			return;
		};
		let scope = GrantScope::from_session(session);
		self.statement_store_allowances.retain(|key, _| !key.is_for_session(scope));
		self.bulletin_allowances.retain(|key, _| !key.is_for_session(scope));
	}

	/// Drop the product subtrees cached for `session`, or for every session.
	fn forget_subtrees(&mut self, session: Option<&SessionInfo>) {
		let Some(session) = session else {
			self.product_subtrees.clear();
			return;
		};
		let scope = GrantScope::from_session(session);
		self.product_subtrees.retain(|(key, _), _| *key != scope);
	}

	fn clear(&mut self) {
		*self = Self::default();
	}
}

/// Holds the grant revision steady while a check and its cache update run.
///
/// It is a std mutex guard, so it must be dropped before any `.await`.
pub struct HostGrantGuard<'a> {
	store: &'a HostGrantStore,
	state: MutexGuard<'a, GrantState>,
}

/// Exclusive access to the grants a host stores.
///
/// Grant writes, revocation and session replacement each hold it, so a
/// revocation never interleaves with a write it is meant to undo.
pub struct HostGrantPersistence<'a> {
	store: &'a HostGrantStore,
	_guard: futures::lock::MutexGuard<'a, ()>,
}

impl HostGrantStore {
	/// Bind retained capabilities to the host's storage, kept as `keeping` says.
	///
	/// Grants are only kept for the session `session_state` selects.
	pub fn new(
		storage: Arc<dyn CoreStorage>,
		keeping: GrantKeeping,
		session_state: Arc<SessionState>,
	) -> Self {
		Self {
			storage,
			keeping,
			state: Mutex::new(GrantState::default()),
			persistence: futures::lock::Mutex::new(()),
			caches: Mutex::new(GrantCaches::default()),
			session_state,
		}
	}

	/// Lock the grant revision for a synchronous check or update.
	pub fn lifecycle(&self) -> HostGrantGuard<'_> {
		HostGrantGuard {
			store: self,
			state: self.state.lock().expect("host grant state mutex poisoned"),
		}
	}

	/// Wait for exclusive access to the stored grants.
	///
	/// It is an async lock and may be held across `.await`, but never wait for
	/// it while holding [`Self::lifecycle`].
	pub async fn persistence(&self) -> HostGrantPersistence<'_> {
		HostGrantPersistence { store: self, _guard: self.persistence.lock().await }
	}

	/// Stop grant work for `previous`, revoking its durable grants when it was cleared.
	pub fn session_ended(&self, previous: Option<&SessionInfo>, revoked: bool) {
		let mut lifecycle = self.lifecycle();
		if revoked {
			lifecycle.revoke_session(previous);
		} else {
			lifecycle.advance();
		}
		drop(lifecycle);
		let mut caches = self.caches();
		caches.forget_allowances(previous);
		caches.forget_subtrees(previous);
	}

	fn session_secret_allocation_is_current(
		&self,
		session: &SessionInfo,
		grant_revision: u64,
	) -> bool {
		self.lifecycle().is_current(session, grant_revision)
	}

	fn cache_auto_signing_key_if_current(
		&self,
		session: &SessionInfo,
		grant_revision: u64,
		cache_key: AutoSigningCacheKey,
		key: AutoSigningKey,
	) -> bool {
		let lifecycle = self.lifecycle();
		if !lifecycle.is_current(session, grant_revision) {
			return false;
		}
		self.caches().auto_signing_keys.insert(cache_key, key);
		true
	}

	/// Load a retained public subtree from memory or durable storage.
	pub async fn known_product_subtree(
		&self,
		session: &SessionInfo,
		product_id: &ProductId,
	) -> Option<[u8; 32]> {
		let grant_revision = self.lifecycle().revision();
		let cache_key = (GrantScope::from_session(session), product_id.clone());
		if let Some(public_key) = self.caches().product_subtrees.get(&cache_key).copied() {
			return Some(public_key);
		}
		self.stored_product_subtree(session, grant_revision, product_id).await
	}

	/// Restore a public subtree only into the selected live session.
	async fn stored_product_subtree(
		&self,
		session: &SessionInfo,
		grant_revision: u64,
		product_id: &ProductId,
	) -> Option<[u8; 32]> {
		if self.keeping == GrantKeeping::Wallet {
			return None;
		}
		let public_key = product_subtree::read_product_subtree(&*self.storage, session, product_id)
			.await
			.inspect_err(|error| warn!(reason = %error, "stored product subtree read failed"))
			.ok()
			.flatten()?;
		// A stored key still has to belong to the live pairing before it is
		// served, exactly as a freshly fetched one does.
		self.cache_product_subtree_if_current(session, grant_revision, product_id, public_key)
			.then_some(public_key)
	}

	/// Persist and memory-cache a freshly fetched product subtree public key.
	///
	/// Returns whether the key is cached for the live session. Persistence is an
	/// optimisation: a storage failure still serves the key and leaves the next
	/// launch to re-ask the wallet. Losing the pairing mid-write is not, so the
	/// storage entry is rolled back rather than left addressing a session that
	/// has gone. A wallet's own host only caches the key.
	pub async fn persist_product_subtree_if_current(
		&self,
		session: &SessionInfo,
		grant_revision: u64,
		product_id: &ProductId,
		public_key: [u8; 32],
	) -> bool {
		if self.keeping == GrantKeeping::Wallet {
			return self.cache_product_subtree_if_current(
				session,
				grant_revision,
				product_id,
				public_key,
			);
		}
		let _storage_guard = self.persistence().await;
		if !self.session_secret_allocation_is_current(session, grant_revision) {
			return false;
		}
		let persisted = match product_subtree::write_product_subtree(
			&*self.storage,
			session,
			product_id,
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
		if self.cache_product_subtree_if_current(session, grant_revision, product_id, public_key) {
			return true;
		}
		if persisted {
			let _ =
				product_subtree::remove_product_subtree(&*self.storage, session, product_id).await;
		}
		false
	}

	/// Cache a public subtree only for the selected live session.
	fn cache_product_subtree_if_current(
		&self,
		session: &SessionInfo,
		grant_revision: u64,
		product_id: &ProductId,
		public_key: [u8; 32],
	) -> bool {
		let lifecycle = self.lifecycle();
		if !lifecycle.is_current(session, grant_revision) {
			return false;
		}
		self.caches()
			.product_subtrees
			.insert((GrantScope::from_session(session), product_id.clone()), public_key);
		true
	}

	/// Store and cache an allowance key for `product_id` in `session`.
	///
	/// A paired host writes the key before caching it and removes it again if
	/// the cache update is refused. Fails with [`AuthorityError::Disconnected`]
	/// when `grant_revision` is stale or `session` is no longer selected.
	pub async fn retain_allowance(
		&self,
		session: &SessionInfo,
		grant_revision: u64,
		product_id: &ProductId,
		allowance: &Allowance,
	) -> Result<(), AuthorityError> {
		let resource = allowance.resource();
		let _storage = self.persistence().await;
		if !self.session_secret_allocation_is_current(session, grant_revision) {
			return Err(AuthorityError::Disconnected);
		}
		if self.keeping == GrantKeeping::Paired {
			allowances::write_allowance_key(
				&*self.storage,
				session,
				product_id,
				resource,
				allowance.as_secret_bytes().to_vec(),
			)
			.await?;
		}
		if let Err(error) = self.remember_allowance(session, grant_revision, product_id, allowance)
		{
			if self.keeping == GrantKeeping::Paired {
				let _ =
					allowances::remove_allowance_key(&*self.storage, session, product_id, resource)
						.await;
			}
			return Err(error);
		}
		Ok(())
	}

	fn remember_allowance(
		&self,
		session: &SessionInfo,
		grant_revision: u64,
		product_id: &ProductId,
		allowance: &Allowance,
	) -> Result<(), AuthorityError> {
		let lifecycle = self.lifecycle();
		if !lifecycle.is_current(session, grant_revision) {
			return Err(AuthorityError::Disconnected);
		}
		match allowance {
			Allowance::StatementStore { key, period } => {
				self.caches().statement_store_allowances.insert(
					AllowanceCacheKey::new(session, product_id, AllowanceResource::StatementStore),
					(*period, key.clone()),
				);
			},
			Allowance::Bulletin(key) => {
				self.caches().bulletin_allowances.insert(
					AllowanceCacheKey::new(session, product_id, AllowanceResource::Bulletin),
					key.clone(),
				);
			},
		}
		Ok(())
	}

	/// The allowance key kept for `product_id` in `session`, if any.
	///
	/// A paired host falls back to storage and caches what it finds. Fails with
	/// [`AuthorityError::Disconnected`] when `grant_revision` is stale or
	/// `session` is no longer selected.
	pub async fn cached_allowance(
		&self,
		session: &SessionInfo,
		grant_revision: u64,
		product_id: &ProductId,
		resource: AllowanceResource,
	) -> Result<Option<Allowance>, AuthorityError> {
		let cache_key = AllowanceCacheKey::new(session, product_id, resource);
		let _storage = self.persistence().await;
		if !self.session_secret_allocation_is_current(session, grant_revision) {
			return Err(AuthorityError::Disconnected);
		}
		let cached = match resource {
			AllowanceResource::StatementStore => self
				.caches()
				.statement_store_allowances
				.get(&cache_key)
				.cloned()
				.map(|(period, key)| Allowance::StatementStore { key, period }),
			AllowanceResource::Bulletin => self
				.caches()
				.bulletin_allowances
				.get(&cache_key)
				.cloned()
				.map(Allowance::Bulletin),
		};
		if cached.is_some() {
			return Ok(cached);
		}
		if self.keeping == GrantKeeping::Wallet {
			return Ok(None);
		}
		let allowance = {
			let Some(secret) =
				allowances::read_allowance_key(&*self.storage, session, product_id, resource)
					.await?
			else {
				return Ok(None);
			};
			match resource {
				AllowanceResource::StatementStore => Allowance::StatementStore {
					key: StatementStoreAllowanceKey::from_secret_bytes(secret)?,
					period: None,
				},
				AllowanceResource::Bulletin => {
					Allowance::Bulletin(BulletinAllowanceKey::from_secret_bytes(secret)?)
				},
			}
		};
		self.remember_allowance(session, grant_revision, product_id, &allowance)?;
		Ok(Some(allowance))
	}

	/// Forget a product's Statement Store key while it is still `public_key`.
	///
	/// A key the product was given again since is kept.
	pub fn forget_statement_store_allowance_key(
		&self,
		product_id: &ProductId,
		public_key: [u8; 32],
	) {
		self.caches().statement_store_allowances.retain(|cache_key, (_, key)| {
			!(cache_key.is_for_product(product_id) && key.public_key == public_key)
		});
	}

	/// Drop a product's Bulletin key from memory and storage before it is replaced.
	pub async fn evict_bulletin_allowance(
		&self,
		session: &SessionInfo,
		grant_revision: u64,
		product_id: &ProductId,
	) -> Result<(), AuthorityError> {
		let _storage = self.persistence().await;
		if !self.session_secret_allocation_is_current(session, grant_revision) {
			return Err(AuthorityError::Disconnected);
		}
		self.caches().bulletin_allowances.remove(&AllowanceCacheKey::new(
			session,
			product_id,
			AllowanceResource::Bulletin,
		));
		if self.keeping == GrantKeeping::Paired {
			allowances::remove_allowance_key(
				&*self.storage,
				session,
				product_id,
				AllowanceResource::Bulletin,
			)
			.await?;
		}
		if !self.session_secret_allocation_is_current(session, grant_revision) {
			return Err(AuthorityError::Disconnected);
		}
		Ok(())
	}

	/// Validate and retain delegated signing authority for the selected session.
	///
	/// A wallet's own host keeps it in memory for this activation; a paired host stores it.
	pub async fn remember_auto_signing_key(
		&self,
		session: &SessionInfo,
		grant_revision: u64,
		product_id: &ProductId,
		expected_product_subtree_public_key: [u8; 32],
		grant: AutoSigningKey,
	) -> Result<(), AuthorityError> {
		validate_auto_signing_key(&grant, expected_product_subtree_public_key)?;
		let owner = AutoSigningOwner::from_session(session);
		let cache_key = (owner.clone(), product_id.clone());
		if self.keeping == GrantKeeping::Wallet {
			let kept =
				self.cache_auto_signing_key_if_current(session, grant_revision, cache_key, grant);
			return if kept { Ok(()) } else { Err(AuthorityError::Disconnected) };
		}
		let persistence = self.persistence().await;
		if !self.session_secret_allocation_is_current(session, grant_revision) {
			return Err(AuthorityError::Disconnected);
		}
		persistence.clear_legacy_auto_signing_key(product_id).await?;
		let mut keys =
			match self.storage.read_core_storage(CoreStorageKey::AutoSigningKeys).await.map_err(
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
			secret: *grant.as_secret_bytes(),
			ring_vrf_domain_entropy: *grant.ring_vrf_domain_entropy(),
		});
		if !self.session_secret_allocation_is_current(session, grant_revision) {
			return Err(AuthorityError::Disconnected);
		}
		self.storage
			.write_core_storage(CoreStorageKey::AutoSigningKeys, keys.encode())
			.await
			.map_err(|err| AuthorityError::Unknown {
				reason: format!("failed to persist AutoSigning capability: {}", err.reason),
			})?;
		if !self.cache_auto_signing_key_if_current(session, grant_revision, cache_key, grant) {
			let _ = persistence.clear_auto_signing_product(product_id).await;
			return Err(AuthorityError::Disconnected);
		}
		Ok(())
	}

	/// The AutoSigning key kept for `product_id` under `session`'s owner, if any.
	///
	/// A paired host falls back to storage. Stored keys that cannot be decoded
	/// or validated, or that do not match the product subtree cached for
	/// `session`, are wiped and fail with [`AuthorityError::Unavailable`]. Keys
	/// stored for another owner are wiped and read as `None`. A legacy unscoped
	/// key is erased and, when no scoped key replaces it, fails with
	/// [`AuthorityError::Unavailable`].
	pub async fn auto_signing_key(
		&self,
		session: &SessionInfo,
		product_id: &ProductId,
	) -> Result<Option<AutoSigningKey>, AuthorityError> {
		let owner = AutoSigningOwner::from_session(session);
		let cache_key = (owner.clone(), product_id.clone());
		if let Some(key) = self.caches().auto_signing_keys.get(&cache_key).cloned() {
			return Ok(Some(key));
		}
		if self.keeping == GrantKeeping::Wallet {
			return Ok(None);
		}

		let persistence = self.persistence().await;
		if let Some(key) = self.caches().auto_signing_keys.get(&cache_key).cloned() {
			return Ok(Some(key));
		}
		let legacy_present = persistence.clear_legacy_auto_signing_key(product_id).await?;
		let Some(mut blob) =
			self.storage.read_core_storage(CoreStorageKey::AutoSigningKeys).await.map_err(
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
				let _ = persistence.clear_auto_signing_keys().await;
				return Err(err);
			},
		};
		if keys.iter().any(|persisted| persisted.owner != owner) {
			let _ = persistence.clear_auto_signing_keys().await;
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
		let current_expected_subtree = self
			.caches()
			.product_subtrees
			.get(&(GrantScope::from_session(session), product_id.clone()))
			.copied();
		if current_expected_subtree
			.is_some_and(|expected| expected != persisted.expected_product_subtree_public_key)
		{
			let _ = persistence.clear_auto_signing_keys().await;
			return Err(AuthorityError::Unavailable {
				reason: "AutoSigning capability is not for the current product subtree".to_string(),
			});
		}
		let key = AutoSigningKey::from_parts(persisted.secret, persisted.ring_vrf_domain_entropy);
		if let Err(err) =
			validate_auto_signing_key(&key, persisted.expected_product_subtree_public_key)
		{
			let _ = persistence.clear_auto_signing_keys().await;
			return Err(err);
		}
		self.caches().auto_signing_keys.insert(cache_key, key.clone());
		Ok(Some(key))
	}

	fn caches(&self) -> MutexGuard<'_, GrantCaches> {
		self.caches.lock().expect("grant cache mutex poisoned")
	}

	/// Cache sizes for lifecycle regression checks.
	#[cfg(test)]
	pub fn capability_cache_sizes_for_tests(&self) -> (usize, usize, usize, usize) {
		let caches = self.caches();
		(
			caches.statement_store_allowances.len(),
			caches.bulletin_allowances.len(),
			caches.product_subtrees.len(),
			caches.auto_signing_keys.len(),
		)
	}

	/// Seed an authenticated subtree for existing runtime fixtures.
	#[cfg(test)]
	pub fn cache_product_subtree_for_test(
		&self,
		session: &SessionInfo,
		product_id: &ProductId,
		public_key: [u8; 32],
	) {
		self.caches()
			.product_subtrees
			.insert((GrantScope::from_session(session), product_id.clone()), public_key);
	}
}

impl HostGrantGuard<'_> {
	/// Revoke transient grants before replacing or locking the local wallet.
	pub fn clear_memory(&mut self) {
		self.advance();
		self.store.caches().clear();
	}

	/// Invalidate one product without touching another product's wallet permission.
	pub fn revoke_product(&mut self, product_id: &ProductId) {
		self.advance();
		self.store.caches().forget_product(product_id);
	}

	/// Revision selected by the held guard.
	pub fn revision(&self) -> u64 {
		self.state.revision
	}

	fn is_current(&self, session: &SessionInfo, revision: u64) -> bool {
		self.state.revision == revision &&
			GrantScope::from_session(session).matches(&self.store.session_state)
	}

	fn advance(&mut self) {
		self.state.revision = self.state.revision.checked_add(1).expect("grant revision exhausted");
	}

	/// Reject grant work that began before revocation.
	pub fn require_revision(&self, revision: u64) -> Result<(), AuthorityError> {
		if self.state.revision != revision {
			return Err(AuthorityError::Disconnected);
		}
		Ok(())
	}

	/// Queue the old session's durable grants before its caches are detached.
	///
	/// Product subtrees are queued from the in-memory set, so a product never
	/// opened since launch keeps its slot. Those address session ids that
	/// cannot recur, and clearing them belongs to the host, as [`CoreStorage`]
	/// states.
	fn revoke_session(&mut self, previous: Option<&SessionInfo>) {
		self.advance();
		self.state.queue_deletion(CoreStorageKey::AutoSigningKeys);
		if let Some(sso) = previous.and_then(|session| session.sso.as_ref()) {
			let session_id = allowances::session_storage_id(sso);
			self.state
				.queue_deletion(CoreStorageKey::AllowanceKeys { session_id: session_id.clone() });
			let session_key = GrantScope::from_session(previous.expect("paired session exists"));
			for (key, product_id) in self.store.caches().product_subtrees.keys() {
				if *key == session_key {
					self.state.queue_deletion(CoreStorageKey::ProductSubtree {
						session_id: session_id.clone(),
						product_id: product_id.clone(),
					});
				}
			}
		}
	}
}

impl HostGrantPersistence<'_> {
	/// Drop every cached grant while a revoked session still owes deletions.
	///
	/// Keys loaded before the revocation took this lock would otherwise outlive
	/// it in memory. Returns whether deletions are queued, which is when
	/// [`Self::drain_cleanup`] has work to do.
	pub fn begin_cleanup(&self) -> bool {
		if self.store.lifecycle().state.pending_deletions.is_empty() {
			return false;
		}
		self.store.caches().clear();
		true
	}

	/// Attempt all queued deletions, retaining failures for a later drain.
	pub async fn drain_cleanup(&self) -> Result<(), String> {
		let mut attempted: Vec<CoreStorageKey> = Vec::new();
		let mut first_error = None;
		loop {
			let next = {
				let lifecycle = self.store.lifecycle();
				lifecycle
					.state
					.pending_deletions
					.iter()
					.find(|key| !attempted.contains(key))
					.cloned()
			};
			let Some(key) = next else {
				break;
			};
			attempted.push(key.clone());
			match self.store.storage.clear_core_storage(key.clone()).await {
				Ok(()) => {
					self.store.lifecycle().state.pending_deletions.retain(|pending| *pending != key)
				},
				Err(error) if first_error.is_none() => first_error = Some(error.reason),
				Err(_) => {},
			}
		}
		match first_error {
			Some(error) => Err(error),
			None => Ok(()),
		}
	}

	/// Reconcile retained grants with the session about to be published.
	pub async fn prepare_session(&self, previous: Option<&SessionInfo>, session: &SessionInfo) {
		let identity_replaced = previous.is_some_and(|previous| {
			AutoSigningOwner::from_session(previous) != AutoSigningOwner::from_session(session)
		});
		if identity_replaced {
			if let Err(reason) = self.clear_auto_signing_keys().await {
				warn!(%reason, "AutoSigning capability clear failed during identity replacement");
			}
		} else if previous.is_none() &&
			let Err(reason) = self.clear_auto_signing_keys_for_other_owner(session).await
		{
			warn!(%reason, "AutoSigning capability owner reconciliation failed");
		}
		if let Some(previous) = previous.filter(|previous| *previous != session) {
			self.store.caches().forget_allowances(Some(previous));
			if let Err(reason) =
				allowances::clear_session_allowance_keys(&*self.store.storage, previous).await
			{
				warn!(%reason, "allowance capability clear failed during session replacement");
			}
		}
	}

	/// Remove one product's capabilities without changing session selection.
	pub async fn clear_product(
		&self,
		session: Option<&SessionInfo>,
		product_id: &ProductId,
	) -> Result<(), String> {
		self.store.caches().forget_product(product_id);
		let mut first_error = self.clear_auto_signing_product(product_id).await.err();
		if let Some(session) = session &&
			self.store.keeping == GrantKeeping::Paired &&
			let Err(error) = allowances::clear_product_allowance_keys(
				&*self.store.storage,
				session,
				product_id,
			)
			.await && first_error.is_none()
		{
			first_error = Some(error.to_string());
		}
		match first_error {
			Some(error) => Err(error),
			None => Ok(()),
		}
	}

	/// Erase all retained signing authority.
	pub async fn clear_auto_signing_keys(&self) -> Result<(), String> {
		self.store.caches().auto_signing_keys.clear();
		self.store
			.storage
			.clear_core_storage(CoreStorageKey::AutoSigningKeys)
			.await
			.map_err(|err| err.reason)
	}

	/// Erase signing authority for one product.
	async fn clear_auto_signing_product(&self, product_id: &ProductId) -> Result<(), String> {
		let aggregate_result =
			match self.store.storage.read_core_storage(CoreStorageKey::AutoSigningKeys).await {
				Err(error) => Err(error.reason),
				Ok(None) => Ok(()),
				Ok(Some(mut blob)) => {
					let decoded = decode_auto_signing_keys(&blob);
					blob.zeroize();
					match decoded {
						Err(_) => self
							.store
							.storage
							.clear_core_storage(CoreStorageKey::AutoSigningKeys)
							.await
							.map_err(|error| error.reason),
						Ok(mut keys) => {
							let before = keys.len();
							keys.retain(|key| &key.product_id != product_id);
							if keys.len() == before {
								Ok(())
							} else if keys.is_empty() {
								self.store
									.storage
									.clear_core_storage(CoreStorageKey::AutoSigningKeys)
									.await
									.map_err(|error| error.reason)
							} else {
								self.store
									.storage
									.write_core_storage(
										CoreStorageKey::AutoSigningKeys,
										keys.encode(),
									)
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

	/// Reject retained authority belonging to a different owner.
	async fn clear_auto_signing_keys_for_other_owner(
		&self,
		session: &SessionInfo,
	) -> Result<(), String> {
		let owner = AutoSigningOwner::from_session(session);
		let Some(mut blob) = self
			.store
			.storage
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
		self.store.caches().auto_signing_keys.clear();
		self.store
			.storage
			.clear_core_storage(CoreStorageKey::AutoSigningKeys)
			.await
			.map_err(|err| err.reason)
	}

	async fn clear_legacy_auto_signing_key(
		&self,
		product_id: &ProductId,
	) -> Result<bool, AuthorityError> {
		let storage_key = CoreStorageKey::AutoSigningKey { product_id: product_id.clone() };
		let legacy =
			self.store.storage.read_core_storage(storage_key.clone()).await.map_err(|err| {
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
			self.store.storage.clear_core_storage(storage_key).await.map_err(|err| {
				AuthorityError::Unknown {
					reason: format!("failed to clear legacy AutoSigning key: {}", err.reason),
				}
			})?;
		}
		Ok(present)
	}
}
