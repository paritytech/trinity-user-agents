// SPDX-License-Identifier: AGPL-3.0-only
//! Host-owned Coinage persistence implementing the Brevity Coinage contracts.
//!
//! The repository model and transaction semantics originate in
//! `brevity-dozer/core/crates/brevity-coinage` (AGPL-3.0); this adapter retains
//! those terms. No purse state belongs to a product's storage namespace.
//!
//! One live owner per backing platform and wallet/network is enforced in process.
//! CoreStorage must provide atomic, durable replacement of the whole value.
//! Mutations run as owned tasks so an RPC disconnect cannot cancel a durable
//! write before in-memory publication. The supplied executor must outlive these
//! tasks. Cross-process writers still require an external ownership protocol.

use std::collections::{BTreeMap, HashMap};
use std::sync::{
    Arc, LazyLock, Weak,
    atomic::{AtomicBool, Ordering},
};

use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{AeadInPlace, KeyInit},
};
use futures::lock::Mutex;
use truapi_coinage::{
    claim_plan::ClaimPlan,
    model::{Coin, Voucher},
    wal::TransferWalEntry,
};
use truapi_platform::{CoreStorage, CoreStorageKey, Platform};
use zeroize::Zeroizing;

use crate::subscription::Spawner;

mod codec;
mod repositories;
#[cfg(test)]
mod tests;

const MAGIC: &[u8; 4] = b"HCPS";
// Version 3 binds all persisted indices/WAL records to the current iOS
// MAIN_PURSE/page-0 derivations. Version 2 used //pps; never reinterpret its
// reservations or counters under another key tree, even for the same wallet.
const VERSION: u16 = 3;
const HEADER_BYTES: usize = 4 + 2 + 24;
const TAG_BYTES: usize = 16;
const MAX_SNAPSHOT_BYTES: usize = 32 * 1024 * 1024;
const MAX_ASSETS: usize = 65_536;
const MAX_RECORDS: usize = 4_096;
const MAX_ITEMS: usize = 4_096;
const MAX_ID_BYTES: usize = 1_024;
const MAX_OPERATION_BYTES: usize = 1024 * 1024;

/// Payload-free persistence errors; never contain plaintext or upstream logs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, derive_more::Display, derive_more::Error)]
pub enum StoreError {
    /// The Host could not read its durable slot.
    #[display("main purse storage read failed")]
    Read,
    /// The Host could not atomically replace its durable slot.
    #[display("main purse storage write failed")]
    Write,
    /// The snapshot belongs to a newer or unrecognized format.
    #[display("unsupported main purse snapshot version")]
    Version,
    /// The envelope or snapshot is malformed or internally inconsistent.
    #[display("invalid main purse snapshot")]
    Corrupt,
    /// Authentication failed, including a wrong wallet, network, or key.
    #[display("main purse snapshot authentication failed")]
    Authentication,
    /// OS cryptographic randomness was unavailable.
    #[display("main purse nonce generation failed")]
    Randomness,
    /// An input or snapshot exceeds the bounded persistence format.
    #[display("main purse storage bound exceeded")]
    Capacity,
    /// An expected asset, journal, or plan does not exist.
    #[display("main purse record does not exist")]
    Missing,
    /// A live slot owner, reservation, immutable identity, or index conflicts.
    #[display("main purse state conflict")]
    Conflict,
    /// A monotonic derivation counter cannot be advanced further.
    #[display("main purse derivation index exhausted")]
    Exhausted,
    /// The Host executor terminated a durable operation before completion.
    #[display("main purse persistence task stopped")]
    ExecutorStopped,
}

#[derive(Clone, Default)]
struct Snapshot {
    // Outer None is unbound; Some(None) is the legacy single-asset runtime.
    asset_instance: Option<Option<u32>>,
    coin_index: Option<u32>,
    voucher_index: Option<u32>,
    coins: BTreeMap<u32, Coin>,
    vouchers: BTreeMap<u32, Voucher>,
    wal: BTreeMap<String, TransferWalEntry>,
    plans: BTreeMap<[u8; 32], ClaimPlan>,
    operations: BTreeMap<[u8; 32], Zeroizing<Vec<u8>>>,
}

type SlotKey = (usize, [u8; 32], [u8; 32]);

struct Slot {
    storage: Weak<dyn CoreStorage>,
    owner: Weak<SlotOwner>,
    // Nonsecret uncertainty survives the last store and every owned operation.
    poisoned: Arc<AtomicBool>,
}

static SLOTS: LazyLock<parking_lot::Mutex<HashMap<SlotKey, Slot>>> =
    LazyLock::new(|| parking_lot::Mutex::new(HashMap::new()));

struct SlotOwner {
    key: SlotKey,
    poisoned: Arc<AtomicBool>,
}

impl SlotOwner {
    fn acquire(
        storage: &Arc<dyn CoreStorage>,
        root: [u8; 32],
        genesis: [u8; 32],
    ) -> Result<Arc<Self>, StoreError> {
        let key = (Arc::as_ptr(storage) as *const () as usize, root, genesis);
        let mut slots = SLOTS.lock();
        let slot = slots.entry(key).or_insert_with(|| Slot {
            storage: Arc::downgrade(storage),
            owner: Weak::new(),
            poisoned: Arc::new(AtomicBool::new(false)),
        });
        if slot.storage.strong_count() == 0 {
            slot.storage = Arc::downgrade(storage);
            slot.poisoned = Arc::new(AtomicBool::new(false));
        }
        if slot.owner.strong_count() != 0 {
            return Err(StoreError::Conflict);
        }
        let owner = Arc::new(Self {
            key,
            poisoned: slot.poisoned.clone(),
        });
        slot.owner = Arc::downgrade(&owner);
        Ok(owner)
    }
}

impl Drop for SlotOwner {
    fn drop(&mut self) {
        let mut slots = SLOTS.lock();
        if slots.get(&self.key).is_some_and(|slot| {
            std::ptr::eq(slot.owner.as_ptr(), self) && !self.poisoned.load(Ordering::Acquire)
        }) {
            slots.remove(&self.key);
        }
    }
}

/// Singleton wallet/network repositories sharing one durable transaction gate.
///
/// The key and operation buffers are zeroized on drop. Coinage model records
/// contain derivation indices and public evidence, not coin/voucher secret keys.
/// Do not expose this type or its auxiliary storage through product APIs.
#[derive(Clone)]
pub struct HostCoinageStore {
    storage: Arc<dyn CoreStorage>,
    storage_key: CoreStorageKey,
    encryption_key: Arc<Zeroizing<[u8; 32]>>,
    associated_data: Arc<Vec<u8>>,
    state: Arc<Mutex<Snapshot>>,
    owner: Arc<SlotOwner>,
    spawner: Spawner,
}

impl HostCoinageStore {
    /// Claim and authenticate this wallet/network. A live owner conflicts; an
    /// absent slot is empty only if no prior write left its durability uncertain.
    /// Recovery callers require an existing snapshot so absence cannot activate a purse.
    pub async fn open(
        platform: Arc<dyn Platform>,
        root_public_key: [u8; 32],
        genesis_hash: [u8; 32],
        encryption_key: Zeroizing<[u8; 32]>,
        spawner: Spawner,
        require_existing: bool,
    ) -> Result<Arc<Self>, StoreError> {
        Self::open_storage(
            platform,
            root_public_key,
            genesis_hash,
            encryption_key,
            spawner,
            require_existing,
        )
        .await
    }

    async fn open_storage(
        storage: Arc<dyn CoreStorage>,
        root_public_key: [u8; 32],
        genesis_hash: [u8; 32],
        encryption_key: Zeroizing<[u8; 32]>,
        spawner: Spawner,
        require_existing: bool,
    ) -> Result<Arc<Self>, StoreError> {
        let storage_key = CoreStorageKey::MainPurseCoinage {
            root_public_key,
            genesis_hash,
        };
        // Reserve before the first await. Clones in owned tasks retain this
        // token even after their caller, registry, or session has been dropped.
        let owner = SlotOwner::acquire(&storage, root_public_key, genesis_hash)?;
        let mut associated_data = b"truapi/host/main-purse-coinage\0".to_vec();
        associated_data.extend_from_slice(MAGIC);
        associated_data.extend_from_slice(&VERSION.to_le_bytes());
        associated_data.extend_from_slice(&root_public_key);
        associated_data.extend_from_slice(&genesis_hash);
        let stored = storage
            .read_core_storage(storage_key.clone())
            .await
            .map_err(|_| StoreError::Read)?;
        let store = Self {
            storage,
            storage_key,
            encryption_key: Arc::new(encryption_key),
            associated_data: Arc::new(associated_data),
            state: Arc::new(Mutex::new(Snapshot::default())),
            owner,
            spawner,
        };
        match stored {
            Some(stored) => *store.state.lock().await = store.decrypt(&stored)?,
            None if require_existing || store.owner.poisoned.load(Ordering::Acquire) => {
                return Err(StoreError::Missing);
            }
            None => {}
        }
        // An authenticated read alone does not repair a failed durable write.
        if store.owner.poisoned.load(Ordering::Acquire) {
            store.reauthenticate().await?;
        }
        Ok(Arc::new(store))
    }

    fn decrypt(&self, stored: &[u8]) -> Result<Snapshot, StoreError> {
        if stored.len() < HEADER_BYTES + TAG_BYTES || &stored[..4] != MAGIC {
            return Err(StoreError::Corrupt);
        }
        if u16::from_le_bytes([stored[4], stored[5]]) != VERSION {
            return Err(StoreError::Version);
        }
        if stored.len() > HEADER_BYTES + MAX_SNAPSHOT_BYTES + TAG_BYTES {
            return Err(StoreError::Capacity);
        }
        let mut plaintext = Zeroizing::new(stored[HEADER_BYTES..].to_vec());
        XChaCha20Poly1305::new((&**self.encryption_key).into())
            .decrypt_in_place(
                XNonce::from_slice(&stored[6..HEADER_BYTES]),
                &self.associated_data,
                &mut *plaintext,
            )
            .map_err(|_| StoreError::Authentication)?;
        codec::decode(&plaintext)
    }

    fn encrypt(&self, state: &Snapshot) -> Result<Vec<u8>, StoreError> {
        let mut plaintext = codec::encode(state, TAG_BYTES)?;
        let mut nonce = [0u8; 24];
        getrandom::getrandom(&mut nonce).map_err(|_| StoreError::Randomness)?;
        XChaCha20Poly1305::new((&**self.encryption_key).into())
            .encrypt_in_place(
                XNonce::from_slice(&nonce),
                &self.associated_data,
                &mut *plaintext,
            )
            .map_err(|_| StoreError::Capacity)?;
        let mut envelope = Vec::with_capacity(HEADER_BYTES + plaintext.len());
        envelope.extend_from_slice(MAGIC);
        envelope.extend_from_slice(&VERSION.to_le_bytes());
        envelope.extend_from_slice(&nonce);
        envelope.extend_from_slice(&plaintext);
        Ok(envelope)
    }

    async fn mutate<T, F>(&self, change: F) -> Result<T, StoreError>
    where
        T: Send + 'static,
        F: FnOnce(&mut Snapshot) -> Result<T, StoreError> + Send + 'static,
    {
        let store = self.clone();
        let (result_tx, result_rx) = futures::channel::oneshot::channel();
        (self.spawner)(Box::pin(async move {
            let result = async {
                let mut published = store.checked_state().await?;
                let mut candidate = published.clone();
                let result = change(&mut candidate)?;
                codec::validate(&candidate)?;
                let encrypted = store.encrypt(&candidate)?;
                // A replacement can become visible before fsync reports an
                // error. Never overwrite it from our stale published state.
                store.owner.poisoned.store(true, Ordering::Release);
                if store
                    .storage
                    .write_core_storage(store.storage_key.clone(), encrypted)
                    .await
                    .is_err()
                {
                    // Keep the slot poisoned even if this was its last owner.
                    return Err(StoreError::Write);
                }
                // No await between successful durable replacement and publication.
                *published = candidate;
                store.owner.poisoned.store(false, Ordering::Release);
                Ok(result)
            }
            .await;
            // A canceled caller does not cancel the owned persistence task.
            drop(store);
            let _ = result_tx.send(result);
        }));
        result_rx.await.map_err(|_| StoreError::ExecutorStopped)?
    }

    /// Persist the trusted asset selection before inventory, claims or approvals.
    /// Changing configuration cannot reinterpret this wallet's durable operations.
    pub async fn bind_asset_instance(&self, instance: Option<u32>) -> Result<(), StoreError> {
        if let Some(bound) = self.checked_state().await?.asset_instance {
            return if bound == instance {
                Ok(())
            } else {
                Err(StoreError::Conflict)
            };
        }
        self.mutate(move |state| {
            match state.asset_instance {
                Some(bound) if bound != instance => return Err(StoreError::Conflict),
                Some(_) => {}
                None => state.asset_instance = Some(instance),
            }
            Ok(())
        })
        .await
    }

    async fn checked_state(&self) -> Result<futures::lock::MutexGuard<'_, Snapshot>, StoreError> {
        let state = self.state.lock().await;
        if self.owner.poisoned.load(Ordering::Acquire) {
            return Err(StoreError::Write);
        }
        Ok(state)
    }

    /// Recover an ambiguous replacement only by authenticating the actual
    /// durable slot under the same transaction gate.
    pub async fn reauthenticate(&self) -> Result<(), StoreError> {
        let store = self.clone();
        let (tx, rx) = futures::channel::oneshot::channel();
        (self.spawner)(Box::pin(async move {
            let result = async {
                let mut published = store.state.lock().await;
                if !store.owner.poisoned.load(Ordering::Acquire) {
                    return Ok(());
                }
                let observed = store
                    .storage
                    .read_core_storage(store.storage_key.clone())
                    .await
                    .map_err(|_| StoreError::Read)?;
                // Absence after an ambiguous write is never proof that its
                // derivation reservations or accepted custody did not happen.
                let bytes = observed.ok_or(StoreError::Missing)?;
                let durable = store.decrypt(&bytes)?;
                // Visibility after rename does not establish durability after
                // a failed directory fsync. Complete a successful replacement
                // before publishing and permitting another spend.
                store
                    .storage
                    .write_core_storage(store.storage_key.clone(), bytes)
                    .await
                    .map_err(|_| StoreError::Write)?;
                *published = durable;
                store.owner.poisoned.store(false, Ordering::Release);
                Ok(())
            }
            .await;
            drop(store);
            let _ = tx.send(result);
        }));
        rx.await.map_err(|_| StoreError::ExecutorStopped)?
    }

    /// Read Host-private approval/handoff bytes. The caller must zeroize its copy.
    pub async fn read_operation(&self, id: [u8; 32]) -> Result<Option<Vec<u8>>, StoreError> {
        Ok(self
            .checked_state()
            .await?
            .operations
            .get(&id)
            .map(|bytes| bytes.to_vec()))
    }

    /// Atomically persist an operation alongside all repositories, never product storage.
    pub fn write_operation(
        &self,
        id: [u8; 32],
        bytes: Vec<u8>,
    ) -> impl core::future::Future<Output = Result<(), StoreError>> + Send + '_ {
        // Wrap before constructing the future, including when it is never polled.
        let bytes = Zeroizing::new(bytes);
        async move {
            if bytes.len() > MAX_OPERATION_BYTES {
                return Err(StoreError::Capacity);
            }
            self.mutate(move |state| {
                state.operations.insert(id, bytes);
                Ok(())
            })
            .await
        }
    }

    /// Enumerate the encrypted snapshot's actual operation map, without a
    /// separately written catalog that could lose an accepted operation.
    pub async fn list_operations(&self) -> Result<Vec<([u8; 32], Zeroizing<Vec<u8>>)>, StoreError> {
        Ok(self
            .checked_state()
            .await?
            .operations
            .iter()
            .map(|(id, bytes)| (*id, bytes.clone()))
            .collect())
    }

    /// Commit one native recovery scan batch, its horizon, and monotonic
    /// derivation counters together. Existing reservations always win.
    #[allow(clippy::too_many_arguments)]
    pub async fn commit_inventory_batch(
        &self,
        progress_id: [u8; 32],
        expected: Option<Vec<u8>>,
        progress: Vec<u8>,
        coins: Vec<Coin>,
        vouchers: Vec<Voucher>,
        coin_index: Option<u32>,
        voucher_index: Option<u32>,
    ) -> Result<(), StoreError> {
        let expected = expected.map(Zeroizing::new);
        let progress = Zeroizing::new(progress);
        self.mutate(move |state| {
            if state.operations.get(&progress_id).map(|v| v.as_slice())
                != expected.as_ref().map(|v| v.as_slice())
            {
                return Err(StoreError::Conflict);
            }
            for coin in coins {
                state.coins.entry(coin.derivation_index).or_insert(coin);
            }
            for voucher in vouchers {
                state
                    .vouchers
                    .entry(voucher.derivation_index)
                    .or_insert(voucher);
            }
            if let Some(index) = coin_index {
                state.coin_index = Some(state.coin_index.map_or(index, |old| old.max(index)));
            }
            if let Some(index) = voucher_index {
                state.voucher_index = Some(state.voucher_index.map_or(index, |old| old.max(index)));
            }
            state.operations.insert(progress_id, progress);
            Ok(())
        })
        .await
    }
}
