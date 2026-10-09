//! Retained host capabilities and their persistence barrier.

mod native_allowances;

use native_allowances::NativeAllowanceDeletion;

use super::allowances::{self, AllowanceCacheKey, AllowanceResource, GrantScope};
use super::authority::{
    AccountGrant, AuthorityError, AutoSigningKey, BulletinAllowanceKey,
    StatementStoreAllowanceKey,
};
use super::product_subtree;
use crate::host_logic::session::{SessionInfo, SessionState};
use crate::platform::{CoreStorage, CoreStorageKey};
use futures::lock::OwnedMutexGuard;
use parity_scale_codec::{Decode, Encode};
use schnorrkel::SecretKey;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};
use tracing::warn;
use zeroize::{Zeroize, Zeroizing};

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
    product_id: String,
    #[zeroize(skip)]
    expected_product_subtree_public_key: [u8; 32],
    secret: [u8; 64],
    ring_vrf_domain_entropy: [u8; 32],
}

type AutoSigningCacheKey = (AutoSigningOwner, String);

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
    Ok(AutoSigningKey::from_parts(
        *secret,
        *ring_vrf_domain_entropy,
    ))
}

#[derive(Clone, PartialEq, Eq)]
enum PendingDeletion {
    Core(CoreStorageKey),
    NativeAllowance(NativeAllowanceDeletion),
}

#[derive(Default)]
struct GrantState {
    revision: u64,
    pending_deletions: Vec<PendingDeletion>,
}

impl GrantState {
    fn queue_deletion(&mut self, key: PendingDeletion) {
        if !self.pending_deletions.contains(&key) {
            self.pending_deletions.push(key);
        }
    }
}

/// Retained grants, host revision and unfinished durable revocation.
pub struct HostGrantStore {
    storage: Arc<dyn CoreStorage>,
    state: Mutex<GrantState>,
    persistence: Arc<futures::lock::Mutex<()>>,
    statement_store_allowances:
        Mutex<HashMap<AllowanceCacheKey, (Option<u32>, StatementStoreAllowanceKey)>>,
    bulletin_allowances: Mutex<HashMap<AllowanceCacheKey, BulletinAllowanceKey>>,
    product_subtrees: Mutex<HashMap<(GrantScope, String), [u8; 32]>>,
    auto_signing_keys: Mutex<HashMap<AutoSigningCacheKey, AutoSigningKey>>,
}

/// Holds the host revision through validation and synchronous key use.
pub struct HostGrantGuard<'a> {
    store: &'a HostGrantStore,
    state: MutexGuard<'a, GrantState>,
}

/// Orders durable grants and session writes against revocation.
pub struct HostGrantPersistence<'a> {
    store: &'a HostGrantStore,
    _barrier: Barrier<'a>,
}

/// Holds back grant writes while a session changes or its grants are revoked.
pub struct GrantBarrier {
    _guard: OwnedMutexGuard<()>,
}

enum Barrier<'a> {
    Owned { _barrier: GrantBarrier },
    Held { _barrier: &'a GrantBarrier },
}

impl HostGrantStore {
    /// Bind retained capabilities to the host's storage.
    pub fn new(storage: Arc<dyn CoreStorage>) -> Self {
        Self {
            storage,
            state: Mutex::new(GrantState::default()),
            persistence: Arc::new(futures::lock::Mutex::new(())),
            statement_store_allowances: Mutex::new(HashMap::new()),
            bulletin_allowances: Mutex::new(HashMap::new()),
            product_subtrees: Mutex::new(HashMap::new()),
            auto_signing_keys: Mutex::new(HashMap::new()),
        }
    }

    /// Hold revision validation through a host operation.
    pub fn lifecycle(&self) -> HostGrantGuard<'_> {
        HostGrantGuard {
            store: self,
            state: self.state.lock().expect("host grant state mutex poisoned"),
        }
    }

    /// Serialize persistence and replacement publication.
    pub async fn persistence(&self) -> HostGrantPersistence<'_> {
        HostGrantPersistence {
            store: self,
            _barrier: Barrier::Owned {
                _barrier: self.barrier().await,
            },
        }
    }

    async fn allowance_persistence(
        &self,
        session_state: &SessionState,
        session: &SessionInfo,
        lifecycle_epoch: u64,
        product_id: &str,
    ) -> Result<HostGrantPersistence<'_>, AuthorityError> {
        let storage = self.persistence().await;
        if session.sso.is_none()
            && let Err(reason) = storage.drain_cleanup().await
            && self.lifecycle().state.pending_deletions.iter().any(|deletion| {
                matches!(deletion, PendingDeletion::NativeAllowance(scope) if scope.includes(session.public_key, product_id))
            })
        {
            return Err(AuthorityError::Unavailable { reason });
        }
        if !self.session_secret_allocation_is_current(session_state, session, lifecycle_epoch) {
            return Err(AuthorityError::Disconnected);
        }
        Ok(storage)
    }

    /// Hold back grant writes across another component's session change.
    pub async fn barrier(&self) -> GrantBarrier {
        GrantBarrier {
            _guard: self.persistence.clone().lock_owned().await,
        }
    }

    /// Persist under a barrier this store already handed out.
    pub fn persistence_under<'a>(&'a self, barrier: &'a GrantBarrier) -> HostGrantPersistence<'a> {
        HostGrantPersistence {
            store: self,
            _barrier: Barrier::Held { _barrier: barrier },
        }
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
        self.clear_statement_store_allowance_keys(previous);
        self.clear_bulletin_allowance_keys(previous);
        self.clear_product_subtrees(previous);
    }

    fn session_secret_allocation_is_current(
        &self,
        session_state: &SessionState,
        session: &SessionInfo,
        lifecycle_epoch: u64,
    ) -> bool {
        GrantScope::from_session(session).matches(session_state)
            && self.lifecycle().revision() == lifecycle_epoch
    }

    fn cache_auto_signing_key_if_current(
        &self,
        session_state: &SessionState,
        session: &SessionInfo,
        lifecycle_epoch: u64,
        cache_key: AutoSigningCacheKey,
        key: AutoSigningKey,
    ) -> bool {
        let lifecycle = self.lifecycle();
        if lifecycle.revision() != lifecycle_epoch {
            return false;
        }
        if !GrantScope::from_session(session).matches(session_state) {
            return false;
        }
        self.auto_signing_keys
            .lock()
            .expect("AutoSigning key cache mutex poisoned")
            .insert(cache_key, key);
        true
    }

    /// Load a retained public subtree from memory or durable storage.
    pub async fn known_product_subtree(
        &self,
        session_state: &SessionState,
        session: &SessionInfo,
        cache_key: (GrantScope, String),
    ) -> Option<[u8; 32]> {
        let lifecycle_epoch = self.lifecycle().revision();
        if let Some(public_key) = self
            .product_subtrees
            .lock()
            .expect("product subtree cache mutex poisoned")
            .get(&cache_key)
            .copied()
        {
            return Some(public_key);
        }
        self.stored_product_subtree(session_state, session, lifecycle_epoch, cache_key)
            .await
    }

    /// Restore a public subtree only into the selected live session.
    async fn stored_product_subtree(
        &self,
        session_state: &SessionState,
        session: &SessionInfo,
        lifecycle_epoch: u64,
        cache_key: (GrantScope, String),
    ) -> Option<[u8; 32]> {
        session.sso.as_ref()?;
        let public_key = match product_subtree::read_product_subtree(
            &*self.storage,
            session,
            &cache_key.1,
        )
        .await
        {
            Ok(public_key) => public_key?,
            Err(error) => {
                warn!(reason = %error, "stored product subtree read failed");
                return None;
            }
        };
        // A stored key still has to belong to the live pairing before it is
        // served, exactly as a freshly fetched one does.
        self.cache_product_subtree_if_current(
            session_state,
            session,
            lifecycle_epoch,
            cache_key,
            public_key,
        )
        .then_some(public_key)
    }

    /// Retain a public subtree, rolling back a stale persisted receipt.
    pub async fn persist_product_subtree_if_current(
        &self,
        session_state: &SessionState,
        session: &SessionInfo,
        lifecycle_epoch: u64,
        cache_key: (GrantScope, String),
        public_key: [u8; 32],
    ) -> bool {
        let product_id = cache_key.1.clone();
        if session.sso.is_none() {
            return self.cache_product_subtree_if_current(
                session_state,
                session,
                lifecycle_epoch,
                cache_key,
                public_key,
            );
        }
        let _storage_guard = self.persistence().await;
        if !self.session_secret_allocation_is_current(session_state, session, lifecycle_epoch) {
            return false;
        }
        let persisted = match product_subtree::write_product_subtree(
            &*self.storage,
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
            }
        };
        if self.cache_product_subtree_if_current(
            session_state,
            session,
            lifecycle_epoch,
            cache_key,
            public_key,
        ) {
            return true;
        }
        if persisted {
            let _ =
                product_subtree::remove_product_subtree(&*self.storage, session, &product_id).await;
        }
        false
    }

    /// Cache a public subtree only for the selected live session.
    fn cache_product_subtree_if_current(
        &self,
        session_state: &SessionState,
        session: &SessionInfo,
        lifecycle_epoch: u64,
        cache_key: (GrantScope, String),
        public_key: [u8; 32],
    ) -> bool {
        let lifecycle = self.lifecycle();
        if lifecycle.revision() != lifecycle_epoch {
            return false;
        }
        if !GrantScope::from_session(session).matches(session_state) {
            return false;
        }
        self.product_subtrees
            .lock()
            .expect("product subtree cache mutex poisoned")
            .insert(cache_key, public_key);
        true
    }

    /// Publish an allowance only after its durable write and owner checks succeed.
    pub async fn retain_allowance(
        &self,
        session_state: &SessionState,
        session: &SessionInfo,
        lifecycle_epoch: u64,
        product_id: &str,
        allowance: &AccountGrant,
    ) -> Result<(), AuthorityError> {
        let (resource, secret) = match allowance {
            AccountGrant::StatementStore { key, .. } => {
                (AllowanceResource::StatementStore, key.as_secret_bytes())
            }
            AccountGrant::Bulletin(key) => (AllowanceResource::Bulletin, key.as_secret_bytes()),
            _ => return Err(AuthorityError::Rejected),
        };
        let storage = self
            .allowance_persistence(session_state, session, lifecycle_epoch, product_id)
            .await?;
        if session.sso.is_none() {
            native_allowances::retain_native_allowance(
                &storage,
                session_state,
                session,
                lifecycle_epoch,
                product_id,
                allowance,
            )
            .await?;
        } else {
            allowances::write_allowance_key(
                &*self.storage,
                session,
                product_id,
                resource,
                secret.to_vec(),
            )
            .await?;
        }
        if let Err(error) = self.remember_allowance(
            session_state,
            session,
            lifecycle_epoch,
            product_id,
            allowance,
        ) {
            if session.sso.is_some() {
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
        session_state: &SessionState,
        session: &SessionInfo,
        lifecycle_epoch: u64,
        product_id: &str,
        allowance: &AccountGrant,
    ) -> Result<(), AuthorityError> {
        let lifecycle = self.lifecycle();
        if lifecycle.revision() != lifecycle_epoch
            || !GrantScope::from_session(session).matches(session_state)
        {
            return Err(AuthorityError::Disconnected);
        }
        match allowance {
            AccountGrant::StatementStore { key, period } => {
                self.statement_store_allowances
                    .lock()
                    .expect("statement-store allowance cache mutex poisoned")
                    .insert(
                        AllowanceCacheKey::new(
                            session,
                            product_id,
                            AllowanceResource::StatementStore,
                        ),
                        (*period, key.clone()),
                    );
            }
            AccountGrant::Bulletin(key) => {
                self.bulletin_allowances
                    .lock()
                    .expect("bulletin allowance cache mutex poisoned")
                    .insert(
                        AllowanceCacheKey::new(session, product_id, AllowanceResource::Bulletin),
                        key.clone(),
                    );
            }
            _ => return Err(AuthorityError::Rejected),
        }
        Ok(())
    }

    /// Load a retained resource grant only into its current host session.
    pub async fn cached_allowance(
        &self,
        session_state: &SessionState,
        session: &SessionInfo,
        lifecycle_epoch: u64,
        product_id: &str,
        resource: AllowanceResource,
    ) -> Result<Option<AccountGrant>, AuthorityError> {
        let cache_key = AllowanceCacheKey::new(session, product_id, resource);
        let storage = self
            .allowance_persistence(session_state, session, lifecycle_epoch, product_id)
            .await?;
        let cached = match resource {
            AllowanceResource::StatementStore => self
                .statement_store_allowances
                .lock()
                .expect("statement-store allowance cache mutex poisoned")
                .get(&cache_key)
                .cloned()
                .map(|(period, key)| AccountGrant::StatementStore { key, period }),
            AllowanceResource::Bulletin => self
                .bulletin_allowances
                .lock()
                .expect("bulletin allowance cache mutex poisoned")
                .get(&cache_key)
                .cloned()
                .map(AccountGrant::Bulletin),
        };
        if cached.is_some() {
            return Ok(cached);
        }
        let allowance = if session.sso.is_none() {
            let Some(allowance) = native_allowances::native_allowance(
                &storage,
                session.public_key,
                product_id,
                resource,
            )
            .await?
            else {
                return Ok(None);
            };
            allowance
        } else {
            let Some(secret) =
                allowances::read_allowance_key(&*self.storage, session, product_id, resource)
                    .await?
            else {
                return Ok(None);
            };
            match resource {
                AllowanceResource::StatementStore => AccountGrant::StatementStore {
                    key: StatementStoreAllowanceKey::from_secret_bytes(secret)?,
                    period: None,
                },
                AllowanceResource::Bulletin => {
                    AccountGrant::Bulletin(BulletinAllowanceKey::from_secret_bytes(secret)?)
                }
            }
        };
        self.remember_allowance(
            session_state,
            session,
            lifecycle_epoch,
            product_id,
            &allowance,
        )?;
        Ok(Some(allowance))
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
        let session_key = GrantScope::from_session(session);
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
        let session_key = GrantScope::from_session(session);
        allowances.retain(|key, _| !key.is_for_session(session_key));
    }

    /// Validate and retain delegated signing authority for the selected session.
    ///
    /// A wallet's own host keeps it in memory for this activation; a paired host stores it.
    pub async fn remember_auto_signing_key(
        &self,
        session_state: &SessionState,
        session: &SessionInfo,
        lifecycle_epoch: u64,
        product_id: &str,
        expected_product_subtree_public_key: [u8; 32],
        grant: AutoSigningKey,
    ) -> Result<(), AuthorityError> {
        let secret = *grant.as_secret_bytes();
        let ring_vrf_domain_entropy = *grant.ring_vrf_domain_entropy();
        let key = validate_auto_signing_key(
            secret,
            expected_product_subtree_public_key,
            ring_vrf_domain_entropy,
        )?;
        let owner = AutoSigningOwner::from_session(session);
        let cache_key = (owner.clone(), product_id.to_string());
        if session.sso.is_none() {
            let kept = self.cache_auto_signing_key_if_current(
                session_state,
                session,
                lifecycle_epoch,
                cache_key,
                key,
            );
            return if kept {
                Ok(())
            } else {
                Err(AuthorityError::Disconnected)
            };
        }
        let _storage_guard = self.persistence().await;
        if !self.session_secret_allocation_is_current(session_state, session, lifecycle_epoch) {
            return Err(AuthorityError::Disconnected);
        }
        _storage_guard
            .clear_legacy_auto_signing_key(product_id)
            .await?;
        let mut keys = match self
            .storage
            .read_core_storage(CoreStorageKey::AutoSigningKeys)
            .await
            .map_err(|err| AuthorityError::Unknown {
                reason: format!("failed to read AutoSigning capabilities: {}", err.reason),
            })? {
            Some(mut blob) => {
                let decoded = decode_auto_signing_keys(&blob).unwrap_or_default();
                blob.zeroize();
                decoded
            }
            None => Vec::new(),
        };
        keys.retain(|persisted| persisted.owner == owner && persisted.product_id != product_id);
        keys.push(PersistedAutoSigningKey {
            owner,
            product_id: product_id.to_string(),
            expected_product_subtree_public_key,
            secret,
            ring_vrf_domain_entropy,
        });
        if !self.session_secret_allocation_is_current(session_state, session, lifecycle_epoch) {
            return Err(AuthorityError::Disconnected);
        }
        self.storage
            .write_core_storage(CoreStorageKey::AutoSigningKeys, keys.encode())
            .await
            .map_err(|err| AuthorityError::Unknown {
                reason: format!("failed to persist AutoSigning capability: {}", err.reason),
            })?;
        if !self.cache_auto_signing_key_if_current(
            session_state,
            session,
            lifecycle_epoch,
            cache_key,
            key,
        ) {
            let _ = _storage_guard.clear_auto_signing_product(product_id).await;
            return Err(AuthorityError::Disconnected);
        }
        Ok(())
    }

    /// Load and validate retained signing authority for this owner and product.
    pub async fn auto_signing_key(
        &self,
        session: &SessionInfo,
        product_id: &str,
    ) -> Result<Option<AutoSigningKey>, AuthorityError> {
        let owner = AutoSigningOwner::from_session(session);
        let cache_key = (owner.clone(), product_id.to_string());
        if let Some(key) = self
            .auto_signing_keys
            .lock()
            .expect("AutoSigning key cache mutex poisoned")
            .get(&cache_key)
            .cloned()
        {
            return Ok(Some(key));
        }
        if session.sso.is_none() {
            return Ok(None);
        }

        let _storage_guard = self.persistence().await;
        if let Some(key) = self
            .auto_signing_keys
            .lock()
            .expect("AutoSigning key cache mutex poisoned")
            .get(&cache_key)
            .cloned()
        {
            return Ok(Some(key));
        }
        let legacy_present = _storage_guard
            .clear_legacy_auto_signing_key(product_id)
            .await?;
        let Some(mut blob) = self
            .storage
            .read_core_storage(CoreStorageKey::AutoSigningKeys)
            .await
            .map_err(|err| AuthorityError::Unknown {
                reason: format!("failed to read AutoSigning capabilities: {}", err.reason),
            })?
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
                let _ = _storage_guard.clear_auto_signing_keys().await;
                return Err(err);
            }
        };
        if keys.iter().any(|persisted| persisted.owner != owner) {
            let _ = _storage_guard.clear_auto_signing_keys().await;
            return Ok(None);
        }
        let Some(persisted) = keys
            .iter()
            .find(|persisted| persisted.product_id == product_id)
        else {
            return if legacy_present {
                Err(AuthorityError::Unavailable {
                    reason: "legacy unscoped AutoSigning capability was rejected".to_string(),
                })
            } else {
                Ok(None)
            };
        };
        let current_expected_subtree = session.sso.as_ref().and_then(|_| {
            self.product_subtrees
                .lock()
                .expect("product subtree cache mutex poisoned")
                .get(&(GrantScope::from_session(session), product_id.to_string()))
                .copied()
        });
        if current_expected_subtree
            .is_some_and(|expected| expected != persisted.expected_product_subtree_public_key)
        {
            let _ = _storage_guard.clear_auto_signing_keys().await;
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
                let _ = _storage_guard.clear_auto_signing_keys().await;
                return Err(err);
            }
        };
        self.auto_signing_keys
            .lock()
            .expect("AutoSigning key cache mutex poisoned")
            .insert(cache_key, key.clone());
        Ok(Some(key))
    }

    /// Evict public subtrees for one session, or all sessions.
    pub fn clear_product_subtrees(&self, session: Option<&SessionInfo>) {
        let mut subtrees = self
            .product_subtrees
            .lock()
            .expect("product subtree cache mutex poisoned");
        let Some(session) = session else {
            subtrees.clear();
            return;
        };
        let session_key = GrantScope::from_session(session);
        subtrees.retain(|(key, _), _| *key != session_key);
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

    /// Seed an authenticated subtree for existing runtime fixtures.
    #[cfg(test)]
    pub fn cache_product_subtree_for_test(
        &self,
        session: &SessionInfo,
        product_id: &str,
        public_key: [u8; 32],
    ) {
        self.product_subtrees
            .lock()
            .expect("product subtree cache mutex poisoned")
            .insert(
                (GrantScope::from_session(session), product_id.to_string()),
                public_key,
            );
    }
}

impl HostGrantGuard<'_> {
    /// Revoke transient grants before replacing or locking the local wallet.
    pub fn clear_memory(&mut self) {
        self.advance();
        self.store
            .statement_store_allowances
            .lock()
            .expect("statement-store allowance cache mutex poisoned")
            .clear();
        self.store
            .bulletin_allowances
            .lock()
            .expect("bulletin allowance cache mutex poisoned")
            .clear();
        self.store
            .product_subtrees
            .lock()
            .expect("product subtree cache mutex poisoned")
            .clear();
        self.store
            .auto_signing_keys
            .lock()
            .expect("AutoSigning key cache mutex poisoned")
            .clear();
    }

    /// Invalidate one product without touching another product's wallet permission.
    pub fn revoke_product(&mut self, product_id: &str) {
        self.advance();
        self.store
            .statement_store_allowances
            .lock()
            .expect("statement-store allowance cache mutex poisoned")
            .retain(|key, _| !key.is_for_product(product_id));
        self.store
            .bulletin_allowances
            .lock()
            .expect("bulletin allowance cache mutex poisoned")
            .retain(|key, _| !key.is_for_product(product_id));
        self.store
            .auto_signing_keys
            .lock()
            .expect("AutoSigning key cache mutex poisoned")
            .retain(|(_, owner), _| owner != product_id);
        self.store
            .product_subtrees
            .lock()
            .expect("product subtree cache mutex poisoned")
            .retain(|(_, owner), _| owner != product_id);
    }

    /// Revoke this product across native owners, including while locked.
    pub fn revoke_native_product(&mut self, product_id: &str) {
        self.revoke_product(product_id);
        self.state.queue_deletion(PendingDeletion::NativeAllowance(
            NativeAllowanceDeletion::Product(product_id.to_string()),
        ));
    }

    /// Delete an owner's retained grants without revoking another wallet's cache.
    pub fn revoke_native_owner(&mut self, owner: [u8; 32]) {
        self.advance();
        self.store
            .statement_store_allowances
            .lock()
            .expect("statement-store allowance cache mutex poisoned")
            .retain(|key, _| !key.is_for_owner(owner));
        self.store
            .bulletin_allowances
            .lock()
            .expect("bulletin allowance cache mutex poisoned")
            .retain(|key, _| !key.is_for_owner(owner));
        self.state.queue_deletion(PendingDeletion::NativeAllowance(
            NativeAllowanceDeletion::Owner(owner),
        ));
    }

    /// Revision selected by the held guard.
    pub fn revision(&self) -> u64 {
        self.state.revision
    }

    /// Invalidate host operations without selecting a wallet session.
    pub fn advance(&mut self) -> u64 {
        self.state.revision = self
            .state
            .revision
            .checked_add(1)
            .expect("session lifecycle epoch exhausted");
        self.state.revision
    }

    /// Reject grant work that began before revocation.
    pub fn require_revision(&self, revision: u64) -> Result<(), AuthorityError> {
        if self.state.revision != revision {
            return Err(AuthorityError::Disconnected);
        }
        Ok(())
    }

    /// Queue the old session's durable grants before its caches are detached.
    pub fn revoke_session(&mut self, previous: Option<&SessionInfo>) {
        self.advance();
        self.state
            .queue_deletion(PendingDeletion::Core(CoreStorageKey::AutoSigningKeys));
        if let Some(sso) = previous.and_then(|session| session.sso.as_ref()) {
            let session_id = allowances::session_storage_id(sso);
            self.state
                .queue_deletion(PendingDeletion::Core(CoreStorageKey::AllowanceKeys {
                    session_id: session_id.clone(),
                }));
            let session_key = GrantScope::from_session(previous.expect("paired session exists"));
            for (key, product_id) in self
                .store
                .product_subtrees
                .lock()
                .expect("product subtree cache mutex poisoned")
                .keys()
            {
                if *key == session_key {
                    self.state.queue_deletion(PendingDeletion::Core(
                        CoreStorageKey::ProductSubtree {
                            session_id: session_id.clone(),
                            product_id: product_id.clone(),
                        },
                    ));
                }
            }
        }
    }
}

impl HostGrantPersistence<'_> {
    /// Evict keys loaded before revocation acquired persistence.
    pub fn begin_cleanup(&self) -> bool {
        if !self
            .store
            .lifecycle()
            .state
            .pending_deletions
            .iter()
            .any(|deletion| matches!(deletion, PendingDeletion::Core(_)))
        {
            return false;
        }
        self.store.clear_statement_store_allowance_keys(None);
        self.store.clear_bulletin_allowance_keys(None);
        self.store.clear_product_subtrees(None);
        self.store
            .auto_signing_keys
            .lock()
            .expect("AutoSigning key cache mutex poisoned")
            .clear();
        true
    }

    /// Attempt all queued deletions, retaining failures for a later drain.
    pub async fn drain_cleanup(&self) -> Result<(), String> {
        let mut attempted: Vec<PendingDeletion> = Vec::new();
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
            let result = match &key {
                PendingDeletion::Core(key) => {
                    self.store.storage.clear_core_storage(key.clone()).await
                }
                PendingDeletion::NativeAllowance(deletion) => {
                    native_allowances::delete_native_allowances(self, deletion)
                        .await
                        .map_err(|error| crate::latest::GenericError {
                            reason: error.to_string(),
                        })
                }
            };
            match result {
                Ok(()) => self
                    .store
                    .lifecycle()
                    .state
                    .pending_deletions
                    .retain(|pending| *pending != key),
                Err(error) if first_error.is_none() => first_error = Some(error.reason),
                Err(_) => {}
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
        } else if previous.is_none()
            && let Err(reason) = self.clear_auto_signing_keys_for_other_owner(session).await
        {
            warn!(%reason, "AutoSigning capability owner reconciliation failed");
        }
        if let Some(previous) = previous.filter(|previous| *previous != session) {
            self.store
                .clear_statement_store_allowance_keys(Some(previous));
            self.store.clear_bulletin_allowance_keys(Some(previous));
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
        product_id: &str,
    ) -> Result<(), String> {
        self.store
            .statement_store_allowances
            .lock()
            .expect("statement-store allowance cache mutex poisoned")
            .retain(|key, _| !key.is_for_product(product_id));
        self.store
            .bulletin_allowances
            .lock()
            .expect("bulletin allowance cache mutex poisoned")
            .retain(|key, _| !key.is_for_product(product_id));
        self.store
            .product_subtrees
            .lock()
            .expect("product subtree cache mutex poisoned")
            .retain(|(_, cached_product_id), _| cached_product_id != product_id);
        self.store
            .auto_signing_keys
            .lock()
            .expect("AutoSigning key cache mutex poisoned")
            .retain(|(_, cached_product_id), _| cached_product_id != product_id);

        let mut first_error = self.clear_auto_signing_product(product_id).await.err();
        if let Some(session) = session
            && session.sso.is_some()
            && let Err(error) =
                allowances::clear_product_allowance_keys(&*self.store.storage, session, product_id)
                    .await
            && first_error.is_none()
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
        self.store
            .auto_signing_keys
            .lock()
            .expect("AutoSigning key cache mutex poisoned")
            .clear();
        self.store
            .storage
            .clear_core_storage(CoreStorageKey::AutoSigningKeys)
            .await
            .map_err(|err| err.reason)
    }

    /// Erase signing authority for one product.
    async fn clear_auto_signing_product(&self, product_id: &str) -> Result<(), String> {
        let aggregate_result = match self
            .store
            .storage
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
                        .store
                        .storage
                        .clear_core_storage(CoreStorageKey::AutoSigningKeys)
                        .await
                        .map_err(|error| error.reason),
                    Ok(mut keys) => {
                        let before = keys.len();
                        keys.retain(|key| key.product_id != product_id);
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
                                .write_core_storage(CoreStorageKey::AutoSigningKeys, keys.encode())
                                .await
                                .map_err(|error| error.reason)
                        }
                    }
                }
            }
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
        self.store
            .auto_signing_keys
            .lock()
            .expect("AutoSigning key cache mutex poisoned")
            .clear();
        self.store
            .storage
            .clear_core_storage(CoreStorageKey::AutoSigningKeys)
            .await
            .map_err(|err| err.reason)
    }

    async fn clear_legacy_auto_signing_key(
        &self,
        product_id: &str,
    ) -> Result<bool, AuthorityError> {
        let storage_key = CoreStorageKey::AutoSigningKey {
            product_id: product_id.to_string(),
        };
        let legacy = self
            .store
            .storage
            .read_core_storage(storage_key.clone())
            .await
            .map_err(|err| AuthorityError::Unknown {
                reason: format!("failed to inspect legacy AutoSigning key: {}", err.reason),
            })?;
        let present = if let Some(mut secret) = legacy {
            secret.zeroize();
            true
        } else {
            false
        };
        if present {
            self.store
                .storage
                .clear_core_storage(storage_key)
                .await
                .map_err(|err| AuthorityError::Unknown {
                    reason: format!("failed to clear legacy AutoSigning key: {}", err.reason),
                })?;
        }
        Ok(present)
    }
}
