//! Retained host capabilities and their persistence barrier.

mod native_allowances;

use native_allowances::NativeAllowanceDeletion;

use super::allowances::{self, AllowanceCacheKey, AllowanceResource, GrantScope};
use super::authority::{
    AccountGrant, AuthorityError, AuthoritySession, AutoSigningKey, BulletinAllowanceKey,
    HostOperation, StatementStoreAllowanceKey,
};
use super::product_subtree;
use crate::host_logic::session::{SessionInfo, SessionState};
use crate::platform::SecretCoreStorageKey;
use crate::platform::{CoreStorage, CoreStorageKey, SecretCoreStorage};
use futures::lock::MutexGuard as AsyncMutexGuard;
use parity_scale_codec::{Decode, Encode};
use schnorrkel::SecretKey;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};
use tracing::warn;
use truapi::latest::GenericError;
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
    Secret(SecretCoreStorageKey),
    NativeAllowance(NativeAllowanceDeletion),
}

#[derive(Default)]
struct GrantState {
    revision: u64,
    pending_deletions: Vec<PendingDeletion>,
    wallet_authorizations: HashMap<String, super::WalletAuthorization>,
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
    secret_storage: Arc<dyn SecretCoreStorage>,
    state: Mutex<GrantState>,
    persistence: futures::lock::Mutex<()>,
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
    _guard: AsyncMutexGuard<'a, ()>,
}

impl HostGrantStore {
    /// Bind retained capabilities to the host's storage.
    pub fn new(storage: Arc<dyn CoreStorage>, secret_storage: Arc<dyn SecretCoreStorage>) -> Self {
        Self {
            storage,
            secret_storage,
            state: Mutex::new(GrantState::default()),
            persistence: futures::lock::Mutex::new(()),
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
            _guard: self.persistence.lock().await,
        }
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

    /// Persist and memory-cache a freshly allocated statement-store allowance
    /// key.
    pub async fn cache_statement_store_allowance_key(
        &self,
        session_state: &SessionState,
        session: &SessionInfo,
        lifecycle_epoch: u64,
        product_id: &str,
        allowance: StatementStoreAllowanceKey,
        period: Option<u32>,
    ) -> Result<StatementStoreAllowanceKey, AuthorityError> {
        let storage = self.persistence().await;
        if session.sso.is_none() {
            storage.begin_cleanup();
            storage
                .drain_cleanup()
                .await
                .map_err(|reason| AuthorityError::Unavailable { reason })?;
        }
        if !self.session_secret_allocation_is_current(session_state, session, lifecycle_epoch) {
            return Err(AuthorityError::Disconnected);
        }
        if session.sso.is_none() {
            storage
                .retain_native_allowance(
                    session_state,
                    session,
                    lifecycle_epoch,
                    product_id,
                    &AccountGrant::StatementStore {
                        key: allowance.clone(),
                        period,
                    },
                )
                .await?;
            self.remember_statement_store_allowance_key(
                session_state,
                session,
                lifecycle_epoch,
                product_id,
                allowance.clone(),
                period,
            )?;
            return Ok(allowance);
        }
        allowances::write_allowance_key(
            &*self.secret_storage,
            session,
            product_id,
            AllowanceResource::StatementStore,
            allowance.secret.to_vec(),
        )
        .await?;
        if let Err(error) = self.remember_statement_store_allowance_key(
            session_state,
            session,
            lifecycle_epoch,
            product_id,
            allowance.clone(),
            period,
        ) {
            let _ = allowances::remove_allowance_key(
                &*self.secret_storage,
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
        session_state: &SessionState,
        session: &SessionInfo,
        lifecycle_epoch: u64,
        product_id: &str,
        allowance: StatementStoreAllowanceKey,
        period: Option<u32>,
    ) -> Result<(), AuthorityError> {
        let cache_key =
            AllowanceCacheKey::new(session, product_id, AllowanceResource::StatementStore);
        let lifecycle = self.lifecycle();
        if lifecycle.revision() != lifecycle_epoch
            || !GrantScope::from_session(session).matches(session_state)
        {
            return Err(AuthorityError::Disconnected);
        }
        self.statement_store_allowances
            .lock()
            .expect("statement-store allowance cache mutex poisoned")
            .insert(cache_key, (period, allowance));
        Ok(())
    }

    /// Cached statement-store allowance key for the product, falling back to
    /// persisted storage.
    pub async fn cached_statement_store_allowance_key(
        &self,
        session_state: &SessionState,
        session: &SessionInfo,
        lifecycle_epoch: u64,
        product_id: &str,
    ) -> Result<Option<(Option<u32>, StatementStoreAllowanceKey)>, AuthorityError> {
        let cache_key =
            AllowanceCacheKey::new(session, product_id, AllowanceResource::StatementStore);
        let storage = self.persistence().await;
        if session.sso.is_none() {
            storage.begin_cleanup();
            storage
                .drain_cleanup()
                .await
                .map_err(|reason| AuthorityError::Unavailable { reason })?;
        }
        if !self.session_secret_allocation_is_current(session_state, session, lifecycle_epoch) {
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
        if session.sso.is_none() {
            let allowance = storage
                .native_allowance(
                    session.public_key,
                    product_id,
                    AllowanceResource::StatementStore,
                )
                .await?;
            let Some(AccountGrant::StatementStore { key, period }) = allowance else {
                return Ok(None);
            };
            self.remember_statement_store_allowance_key(
                session_state,
                session,
                lifecycle_epoch,
                product_id,
                key.clone(),
                period,
            )?;
            return Ok(Some((period, key)));
        }
        let Some(secret) = allowances::read_allowance_key(
            &*self.secret_storage,
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
            session_state,
            session,
            lifecycle_epoch,
            product_id,
            allowance.clone(),
            None,
        )?;
        Ok(Some((None, allowance)))
    }

    /// Persist and memory-cache a freshly allocated Bulletin allowance key.
    pub async fn cache_bulletin_allowance_key(
        &self,
        session_state: &SessionState,
        session: &SessionInfo,
        lifecycle_epoch: u64,
        product_id: &str,
        allowance: BulletinAllowanceKey,
    ) -> Result<BulletinAllowanceKey, AuthorityError> {
        let storage = self.persistence().await;
        if session.sso.is_none() {
            storage.begin_cleanup();
            storage
                .drain_cleanup()
                .await
                .map_err(|reason| AuthorityError::Unavailable { reason })?;
        }
        if !self.session_secret_allocation_is_current(session_state, session, lifecycle_epoch) {
            return Err(AuthorityError::Disconnected);
        }
        if session.sso.is_none() {
            storage
                .retain_native_allowance(
                    session_state,
                    session,
                    lifecycle_epoch,
                    product_id,
                    &AccountGrant::Bulletin(allowance.clone()),
                )
                .await?;
            self.remember_bulletin_allowance_key(
                session_state,
                session,
                lifecycle_epoch,
                product_id,
                allowance.clone(),
            )?;
            return Ok(allowance);
        }
        allowances::write_allowance_key(
            &*self.secret_storage,
            session,
            product_id,
            AllowanceResource::Bulletin,
            allowance.as_secret_bytes().to_vec(),
        )
        .await?;
        if let Err(error) = self.remember_bulletin_allowance_key(
            session_state,
            session,
            lifecycle_epoch,
            product_id,
            allowance.clone(),
        ) {
            let _ = allowances::remove_allowance_key(
                &*self.secret_storage,
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
        session_state: &SessionState,
        session: &SessionInfo,
        lifecycle_epoch: u64,
        product_id: &str,
        allowance: BulletinAllowanceKey,
    ) -> Result<(), AuthorityError> {
        let cache_key = AllowanceCacheKey::new(session, product_id, AllowanceResource::Bulletin);
        let lifecycle = self.lifecycle();
        if lifecycle.revision() != lifecycle_epoch
            || !GrantScope::from_session(session).matches(session_state)
        {
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
        session_state: &SessionState,
        session: &SessionInfo,
        lifecycle_epoch: u64,
        product_id: &str,
    ) -> Result<Option<BulletinAllowanceKey>, AuthorityError> {
        let cache_key = AllowanceCacheKey::new(session, product_id, AllowanceResource::Bulletin);
        let storage = self.persistence().await;
        if session.sso.is_none() {
            storage.begin_cleanup();
            storage
                .drain_cleanup()
                .await
                .map_err(|reason| AuthorityError::Unavailable { reason })?;
        }
        if !self.session_secret_allocation_is_current(session_state, session, lifecycle_epoch) {
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
        if session.sso.is_none() {
            let allowance = storage
                .native_allowance(session.public_key, product_id, AllowanceResource::Bulletin)
                .await?;
            let Some(AccountGrant::Bulletin(key)) = allowance else {
                return Ok(None);
            };
            self.remember_bulletin_allowance_key(
                session_state,
                session,
                lifecycle_epoch,
                product_id,
                key.clone(),
            )?;
            return Ok(Some(key));
        }
        let Some(secret) = allowances::read_allowance_key(
            &*self.secret_storage,
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
            session_state,
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
        session_state: &SessionState,
        session: &SessionInfo,
        lifecycle_epoch: u64,
        product_id: &str,
    ) -> Result<(), AuthorityError> {
        let cache_key = AllowanceCacheKey::new(session, product_id, AllowanceResource::Bulletin);
        if session.sso.is_none() {
            {
                let mut lifecycle = self.lifecycle();
                if lifecycle.revision() != lifecycle_epoch
                    || !GrantScope::from_session(session).matches(session_state)
                {
                    return Err(AuthorityError::Disconnected);
                }
                self.bulletin_allowances
                    .lock()
                    .expect("bulletin allowance cache mutex poisoned")
                    .remove(&cache_key);
                lifecycle
                    .state
                    .queue_deletion(PendingDeletion::NativeAllowance(
                        NativeAllowanceDeletion::Bulletin {
                            owner: session.public_key,
                            product_id: product_id.to_string(),
                        },
                    ));
            }
            let storage = self.persistence().await;
            storage.begin_cleanup();
            storage
                .drain_cleanup()
                .await
                .map_err(|reason| AuthorityError::Unavailable { reason })?;
            if !self.session_secret_allocation_is_current(session_state, session, lifecycle_epoch) {
                return Err(AuthorityError::Disconnected);
            }
            return Ok(());
        }
        let _storage_guard = self.persistence().await;
        if !self.session_secret_allocation_is_current(session_state, session, lifecycle_epoch) {
            return Err(AuthorityError::Disconnected);
        }
        self.bulletin_allowances
            .lock()
            .expect("bulletin allowance cache mutex poisoned")
            .remove(&cache_key);

        allowances::remove_allowance_key(
            &*self.secret_storage,
            session,
            product_id,
            AllowanceResource::Bulletin,
        )
        .await?;
        if !self.session_secret_allocation_is_current(session_state, session, lifecycle_epoch) {
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
        let _storage_guard = self.persistence().await;
        if !self.session_secret_allocation_is_current(session_state, session, lifecycle_epoch) {
            return Err(AuthorityError::Disconnected);
        }
        let mut keys = match self
            .secret_storage
            .read_secret_core_storage(SecretCoreStorageKey::AutoSigningKeys)
            .await
            .map_err(|err| AuthorityError::Unknown {
                reason: format!("failed to read AutoSigning capabilities: {}", err.reason),
            })? {
            Some(mut blob) => {
                let decoded = decode_auto_signing_keys(&blob);
                blob.zeroize();
                decoded?
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
        self.secret_storage
            .write_secret_core_storage(SecretCoreStorageKey::AutoSigningKeys, keys.encode())
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
        if session.sso.is_none() {
            return Ok(None);
        }

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
        let Some(mut blob) = self
            .secret_storage
            .read_secret_core_storage(SecretCoreStorageKey::AutoSigningKeys)
            .await
            .map_err(|err| AuthorityError::Unknown {
                reason: format!("failed to read AutoSigning capabilities: {}", err.reason),
            })?
        else {
            return Ok(None);
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
                let _ = self
                    .secret_storage
                    .clear_secret_core_storage(SecretCoreStorageKey::AutoSigningKeys)
                    .await;
                return Err(err);
            }
        };
        if keys.iter().any(|persisted| persisted.owner != owner) {
            self.auto_signing_keys
                .lock()
                .expect("AutoSigning key cache mutex poisoned")
                .clear();
            let _ = self
                .secret_storage
                .clear_secret_core_storage(SecretCoreStorageKey::AutoSigningKeys)
                .await;
            return Ok(None);
        }
        let Some(persisted) = keys
            .iter()
            .find(|persisted| persisted.product_id == product_id)
        else {
            return Ok(None);
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
            self.auto_signing_keys
                .lock()
                .expect("AutoSigning key cache mutex poisoned")
                .clear();
            let _ = self
                .secret_storage
                .clear_secret_core_storage(SecretCoreStorageKey::AutoSigningKeys)
                .await;
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
                let _ = self
                    .secret_storage
                    .clear_secret_core_storage(SecretCoreStorageKey::AutoSigningKeys)
                    .await;
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
        self.state.wallet_authorizations.clear();
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

    /// Retain a receipt issued to this runtime's canonical wallet activation.
    pub fn retain_wallet_authorization(
        &mut self,
        session_state: &Arc<SessionState>,
        operation: &HostOperation,
        product_id: &str,
        authorization: super::WalletAuthorization,
    ) -> Result<(), AuthorityError> {
        self.require(operation)?;
        if !authorization.issuer.ptr_eq(&Arc::downgrade(session_state))
            || authorization.validation_id != operation.session.validation_id
            || authorization.product_id != product_id
        {
            return Err(AuthorityError::Rejected);
        }
        self.state
            .wallet_authorizations
            .insert(product_id.to_string(), authorization);
        Ok(())
    }

    /// Permission retained for the calling product under the held revision.
    pub fn wallet_authorization(
        &self,
        operation: &HostOperation,
        product_id: &str,
    ) -> Result<Option<super::WalletAuthorization>, AuthorityError> {
        self.require(operation)?;
        Ok(self.state.wallet_authorizations.get(product_id).cloned())
    }

    /// Invalidate one product without touching another product's wallet permission.
    pub fn revoke_product(&mut self, product_id: &str) {
        self.advance();
        self.state.wallet_authorizations.remove(product_id);
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

    /// Forget exactly the dated native grant observed before submission.
    pub fn forget_statement_store_allowance(
        &mut self,
        session: &SessionInfo,
        product_id: &str,
        public_key: [u8; 32],
        period: u32,
    ) {
        let scope = GrantScope::from_session(session);
        self.store
            .statement_store_allowances
            .lock()
            .expect("statement-store allowance cache mutex poisoned")
            .retain(|owner, (cached_period, key)| {
                !owner.is_for_session(scope)
                    || !owner.is_for_product(product_id)
                    || *cached_period != Some(period)
                    || key.public_key != public_key
            });
        self.state.queue_deletion(PendingDeletion::NativeAllowance(
            NativeAllowanceDeletion::StatementStore {
                owner: session.public_key,
                product_id: product_id.to_string(),
                public_key,
                period,
            },
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

    /// Bind a selected wallet session to this host's revision.
    pub fn capture(&self, session: AuthoritySession) -> HostOperation {
        HostOperation::new(session, self.state.revision)
    }

    /// Reject work selected before host grants were invalidated.
    pub fn require(&self, operation: &HostOperation) -> Result<(), AuthorityError> {
        operation.require_revision(self.state.revision)
    }

    /// Preserve cleanup intent across failed or dropped session writes.
    pub fn queue_auth_deletion(&mut self) {
        self.state
            .queue_deletion(PendingDeletion::Secret(SecretCoreStorageKey::AuthSession));
    }

    /// Consume the selected write's cleanup intent at commit.
    pub fn forget_auth_deletion(&mut self) {
        self.state
            .pending_deletions
            .retain(|key| *key != PendingDeletion::Secret(SecretCoreStorageKey::AuthSession));
    }

    /// Queue the old session's durable grants before its caches are detached.
    pub fn revoke_session(&mut self, previous: Option<&SessionInfo>, clear_auth: bool) {
        self.advance();
        if clear_auth {
            self.queue_auth_deletion();
        }
        self.state.queue_deletion(PendingDeletion::Secret(
            SecretCoreStorageKey::AutoSigningKeys,
        ));
        if let Some(sso) = previous.and_then(|session| session.sso.as_ref()) {
            let session_id = allowances::session_storage_id(sso);
            self.state.queue_deletion(PendingDeletion::Secret(
                SecretCoreStorageKey::AllowanceKeys {
                    session_id: session_id.clone(),
                },
            ));
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
    /// Read the auth snapshot while replacement and deletion are excluded.
    pub async fn read_auth_session(&self) -> Result<Option<Vec<u8>>, GenericError> {
        self.store
            .secret_storage
            .read_secret_core_storage(SecretCoreStorageKey::AuthSession)
            .await
    }

    /// Persist the auth snapshot within the caller's selected commit.
    pub async fn write_auth_session(&self, blob: Vec<u8>) -> Result<(), GenericError> {
        self.store
            .secret_storage
            .write_secret_core_storage(SecretCoreStorageKey::AuthSession, blob)
            .await
    }

    /// Evict keys loaded before revocation acquired persistence.
    pub fn begin_cleanup(&self) -> bool {
        if self.store.lifecycle().state.pending_deletions.is_empty() {
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
        let mut cleared = Vec::new();
        let mut first_error = None;
        loop {
            let next = {
                let mut lifecycle = self.store.lifecycle();
                // Guarded writes cannot intervene, so repeated revocation shares a completed deletion.
                lifecycle
                    .state
                    .pending_deletions
                    .retain(|key| !cleared.contains(key));
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
                PendingDeletion::NativeAllowance(deletion) => self
                    .delete_native_allowances(deletion)
                    .await
                    .map_err(|error| GenericError {
                        reason: error.to_string(),
                    }),
                PendingDeletion::Core(key) => {
                    self.store.storage.clear_core_storage(key.clone()).await
                }
                PendingDeletion::Secret(key) => {
                    self.store
                        .secret_storage
                        .clear_secret_core_storage(key.clone())
                        .await
                }
            };
            match result {
                Ok(()) => cleared.push(key),
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
                allowances::clear_session_allowance_keys(&*self.store.secret_storage, previous)
                    .await
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
            && let Err(error) = allowances::clear_product_allowance_keys(
                &*self.store.secret_storage,
                session,
                product_id,
            )
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
            .secret_storage
            .clear_secret_core_storage(SecretCoreStorageKey::AutoSigningKeys)
            .await
            .map_err(|err| err.reason)
    }

    /// Erase signing authority for one product.
    async fn clear_auto_signing_product(&self, product_id: &str) -> Result<(), String> {
        match self
            .store
            .secret_storage
            .read_secret_core_storage(SecretCoreStorageKey::AutoSigningKeys)
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
                        .secret_storage
                        .clear_secret_core_storage(SecretCoreStorageKey::AutoSigningKeys)
                        .await
                        .map_err(|error| error.reason),
                    Ok(mut keys) => {
                        let before = keys.len();
                        keys.retain(|key| key.product_id != product_id);
                        if keys.len() == before {
                            Ok(())
                        } else if keys.is_empty() {
                            self.store
                                .secret_storage
                                .clear_secret_core_storage(SecretCoreStorageKey::AutoSigningKeys)
                                .await
                                .map_err(|error| error.reason)
                        } else {
                            self.store
                                .secret_storage
                                .write_secret_core_storage(
                                    SecretCoreStorageKey::AutoSigningKeys,
                                    keys.encode(),
                                )
                                .await
                                .map_err(|error| error.reason)
                        }
                    }
                }
            }
        }
    }

    /// Reject retained authority belonging to a different owner.
    async fn clear_auto_signing_keys_for_other_owner(
        &self,
        session: &SessionInfo,
    ) -> Result<(), String> {
        let owner = AutoSigningOwner::from_session(session);
        let Some(mut blob) = self
            .store
            .secret_storage
            .read_secret_core_storage(SecretCoreStorageKey::AutoSigningKeys)
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
            .secret_storage
            .clear_secret_core_storage(SecretCoreStorageKey::AutoSigningKeys)
            .await
            .map_err(|err| err.reason)
    }
}
