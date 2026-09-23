// SPDX-License-Identifier: AGPL-3.0-only
//! Host-private, wallet/network/product-bound Chat snapshots.
//!
//! One live store owns each typed slot, including across session replacement.
//! CoreStorage must atomically and durably replace complete values; the Host
//! executor must outlive owned persistence tasks. Another process must not write
//! the same slot concurrently.

use std::{
    collections::HashMap,
    sync::{
        Arc, LazyLock, Weak,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{AeadInPlace, KeyInit},
};
use futures::{channel::oneshot, lock::Mutex};
use hkdf::Hkdf;
use parity_scale_codec::{Decode, Encode};
use parking_lot::Mutex as SyncMutex;
use sha2::Sha256;
use truapi::latest::HostProductDeviceChatError as ChatError;
use truapi_platform::{CoreStorage, CoreStorageKey};
use zeroize::Zeroizing;

use super::NativeChatContext;
use crate::subscription::Spawner;

mod codec;
#[cfg(test)]
mod tests;

const MAGIC: &[u8; 4] = b"HCHS";
const VERSION: u16 = 3;
const HEADER_BYTES: usize = 4 + 2 + 24;
const TAG_BYTES: usize = 16;
// A retained legacy snapshot and a bounded (16 MiB) authenticated HOP handoff
// can coexist during the explicit custody transfer. Neither bound is unbounded.
const MAX_SNAPSHOT_BYTES: usize = 32 * 1024 * 1024;
const MAX_DECODE_BYTES: usize = 64 * 1024 * 1024;
const MAX_DECODE_DEPTH: u32 = 64;

// Clean slots exist only while an open/store holds a lease. Uncertain slots
// retain strong, nonsecret poison independently of those leases until durable
// repair or the backing platform's death. Weak gates alone would lose custody
// after a failed first write whose value is not visible on the next read.
struct SlotState {
    owner: Arc<Mutex<Weak<()>>>,
    uncertain: AtomicBool,
    // Updated under SLOTS. Arc counts include other destructors' not-yet-dropped
    // fields, so inspecting them can miss reclamation when leases drop together.
    leases: AtomicUsize,
}

struct PlatformSlots {
    storage: Weak<dyn CoreStorage>,
    slots: HashMap<Arc<Vec<u8>>, Arc<SlotState>>,
}

static SLOTS: LazyLock<SyncMutex<HashMap<usize, PlatformSlots>>> =
    LazyLock::new(|| SyncMutex::new(HashMap::new()));

struct SlotGate {
    storage_id: usize,
    key: Arc<Vec<u8>>,
    state: Arc<SlotState>,
}

impl Drop for SlotGate {
    fn drop(&mut self) {
        let mut platforms = SLOTS.lock();
        let last_lease = self.state.leases.fetch_sub(1, Ordering::Relaxed) == 1;
        let Some(platform) = platforms.get_mut(&self.storage_id) else {
            return;
        };
        // Acquiring another lease also holds SLOTS, so this check and removal
        // cannot race a new open. Owned writes retain their store's lease.
        if last_lease
            && !self.state.uncertain.load(Ordering::Acquire)
            && platform
                .slots
                .get(&self.key)
                .is_some_and(|state| Arc::ptr_eq(state, &self.state))
        {
            platform.slots.remove(&self.key);
        }
    }
}

fn slot_gate(storage: &Arc<dyn CoreStorage>, key: &CoreStorageKey) -> SlotGate {
    let storage_id = Arc::as_ptr(storage) as *const () as usize;
    let mut platforms = SLOTS.lock();
    let same_platform = platforms.get(&storage_id).is_some_and(|platform| {
        platform
            .storage
            .upgrade()
            .is_some_and(|existing| Arc::ptr_eq(&existing, storage))
    });
    if !same_platform {
        // Sweep only when attaching a new platform, not for each file chunk.
        // The Weak check, not a possibly reused address, proves its identity.
        platforms.retain(|_, platform| platform.storage.strong_count() != 0);
        platforms.insert(
            storage_id,
            PlatformSlots {
                storage: Arc::downgrade(storage),
                slots: HashMap::new(),
            },
        );
    }
    let platform = platforms.get_mut(&storage_id).expect("registered platform");
    let encoded = key.encode();
    let (key, state) = if let Some((key, state)) = platform.slots.get_key_value(&encoded) {
        state.leases.fetch_add(1, Ordering::Relaxed);
        (key.clone(), state.clone())
    } else {
        let key = Arc::new(encoded);
        let state = Arc::new(SlotState {
            owner: Arc::new(Mutex::new(Weak::new())),
            uncertain: AtomicBool::new(false),
            leases: AtomicUsize::new(1),
        });
        platform.slots.insert(key.clone(), state.clone());
        (key, state)
    };
    SlotGate {
        storage_id,
        key,
        state,
    }
}

struct SnapshotCipher {
    key: Zeroizing<[u8; 32]>,
    aad: Vec<u8>,
}

impl SnapshotCipher {
    fn new(storage_key: &CoreStorageKey, entropy: &[u8]) -> Result<Self, ChatError> {
        if entropy.is_empty() {
            return Err(ChatError::NotConnected);
        }
        let mut aad = b"truapi/host/native-chat/snapshot\0".to_vec();
        aad.extend_from_slice(MAGIC);
        aad.extend_from_slice(&VERSION.to_le_bytes());
        storage_key.encode_to(&mut aad);
        let mut key = Zeroizing::new([0; 32]);
        Hkdf::<Sha256>::new(Some(b"truapi/host/native-chat/snapshot-key/v1"), entropy)
            .expand(&aad, &mut *key)
            .map_err(|_| ChatError::StorageUnavailable)?;
        Ok(Self { key, aad })
    }

    fn decrypt<T: Decode>(&self, stored: &[u8]) -> Result<T, ChatError> {
        if stored.len() < HEADER_BYTES + TAG_BYTES
            || stored.len() > HEADER_BYTES + MAX_SNAPSHOT_BYTES + TAG_BYTES
            || &stored[..4] != MAGIC
            || u16::from_le_bytes([stored[4], stored[5]]) != VERSION
        {
            return Err(ChatError::StorageUnavailable);
        }
        let mut plaintext = Zeroizing::new(stored[HEADER_BYTES..].to_vec());
        XChaCha20Poly1305::new((&*self.key).into())
            .decrypt_in_place(
                XNonce::from_slice(&stored[6..HEADER_BYTES]),
                &self.aad,
                &mut *plaintext,
            )
            .map_err(|_| ChatError::StorageUnavailable)?;
        codec::decode(&plaintext)
    }

    fn encrypt<T: Encode + Decode>(&self, state: &T) -> Result<Vec<u8>, ChatError> {
        let mut plaintext = codec::encode(state)?;
        // A generic state can be small on the wire yet exceed restore's depth
        // or allocation limits. Refuse to commit a snapshot we cannot reopen.
        drop(codec::decode::<T>(&plaintext)?);
        let mut nonce = [0u8; 24];
        getrandom::getrandom(&mut nonce).map_err(|_| ChatError::StorageUnavailable)?;
        XChaCha20Poly1305::new((&*self.key).into())
            .encrypt_in_place(XNonce::from_slice(&nonce), &self.aad, &mut *plaintext)
            .map_err(|_| ChatError::StorageUnavailable)?;
        let mut envelope = Vec::with_capacity(HEADER_BYTES + plaintext.len());
        envelope.extend_from_slice(MAGIC);
        envelope.extend_from_slice(&VERSION.to_le_bytes());
        envelope.extend_from_slice(&nonce);
        envelope.extend_from_slice(&plaintext);
        Ok(envelope)
    }
}

/// Generic transactional state; never expose this or its closure results to a
/// product without the actor's typed public projection.
///
/// `T` must zeroize its own secrets, including partial decode failure paths.
/// Custom Decode implementations must honor SCALE Input's allocation/depth
/// hooks; derived Decode and SCALE's ordinary collections already do so.
/// There is deliberately no Debug implementation for the store or its state.
pub(super) struct ChatStateStore<T> {
    storage: Arc<dyn CoreStorage>,
    storage_key: CoreStorageKey,
    cipher: SnapshotCipher,
    owner: Arc<()>,
    gate: SlotGate,
    // Only access while holding gate. The short sync lock makes T: Send enough,
    // without requiring T: Sync or holding a sync guard over storage I/O.
    published: SyncMutex<T>,
    spawner: Spawner,
}

impl<T: Encode + Decode + Clone + Send + 'static> ChatStateStore<T> {
    /// Restore authenticated state, or initialize only a genuinely absent slot.
    /// Never fall back to a new device after corruption or an ambiguous write.
    pub(super) async fn open(
        context: &NativeChatContext,
        product_id: &str,
        initial: impl FnOnce() -> Result<T, ChatError>,
    ) -> Result<Arc<Self>, ChatError> {
        let key = CoreStorageKey::NativeChatDevice {
            root_public_key: context.session.public_key,
            genesis_hash: context.genesis_hash,
            product_id: product_id.to_owned(),
        };
        Self::open_storage(
            context.services.platform.clone(),
            key,
            &context.entropy,
            context.services.spawner.clone(),
            initial,
        )
        .await
    }

    /// One immutable, bounded private file chunk in a distinct authenticated slot.
    pub(super) async fn open_file_chunk(
        context: &NativeChatContext,
        product_id: &str,
        attachment_id: [u8; 32],
        chunk_index: u32,
        initial: impl FnOnce() -> Result<T, ChatError>,
    ) -> Result<Arc<Self>, ChatError> {
        context.require_current()?;
        Self::open_storage(
            context.services.platform.clone(),
            CoreStorageKey::NativeChatFileChunk {
                root_public_key: context.session.public_key,
                genesis_hash: context.genesis_hash,
                product_id: product_id.to_owned(),
                attachment_id,
                chunk_index,
            },
            &context.entropy,
            context.services.spawner.clone(),
            initial,
        )
        .await
    }

    async fn open_storage(
        storage: Arc<dyn CoreStorage>,
        storage_key: CoreStorageKey,
        entropy: &[u8],
        spawner: Spawner,
        initial: impl FnOnce() -> Result<T, ChatError>,
    ) -> Result<Arc<Self>, ChatError> {
        let cipher = SnapshotCipher::new(&storage_key, entropy)?;
        let gate = slot_gate(&storage, &storage_key);
        let mut owner = gate.state.owner.clone().lock_owned().await;
        if owner.strong_count() != 0 {
            // Serializing writes is not enough: a second store's cached state
            // could overwrite the first store's subsequent durable updates.
            return Err(ChatError::StorageUnavailable);
        }
        let stored = storage
            .read_core_storage(storage_key.clone())
            .await
            .map_err(|_| ChatError::StorageUnavailable)?;
        let (state, replacement) = match stored {
            Some(bytes) => {
                let state = cipher.decrypt(&bytes)?;
                // A post-rename failure did not prove directory durability.
                // Authenticate, then successfully rewrite the same envelope.
                let replacement = if gate.state.uncertain.load(Ordering::Acquire) {
                    Some(bytes)
                } else {
                    None
                };
                (state, replacement)
            }
            None => {
                if gate.state.uncertain.load(Ordering::Acquire) {
                    // Absence is not proof the prior operation never happened.
                    return Err(ChatError::StorageUnavailable);
                }
                let state = initial()?;
                let bytes = cipher.encrypt(&state)?;
                (state, Some(bytes))
            }
        };
        let store = Arc::new(Self {
            storage,
            storage_key,
            cipher,
            owner: Arc::new(()),
            gate,
            published: SyncMutex::new(state),
            spawner: spawner.clone(),
        });
        *owner = Arc::downgrade(&store.owner);
        let Some(bytes) = replacement else {
            return Ok(store);
        };
        // Set before dispatch: even an executor dropping an unpolled task must
        // not let a later open silently replace the attempted device identity.
        store.gate.state.uncertain.store(true, Ordering::Release);
        let (tx, rx) = oneshot::channel();
        (spawner)(Box::pin(async move {
            let result = store
                .storage
                .write_core_storage(store.storage_key.clone(), bytes)
                .await
                .map_err(|_| ChatError::StorageUnavailable);
            if result.is_ok() {
                store.gate.state.uncertain.store(false, Ordering::Release);
            }
            let _ = tx.send(result.map(|()| store));
            // Release an undeliverable store before unlocking: a waiting open
            // must not see custody from a canceled initialization's result.
            drop(owner);
        }));
        rx.await.map_err(|_| ChatError::StorageUnavailable)?
    }

    pub(super) async fn read<R>(&self, read: impl FnOnce(&T) -> R) -> Result<R, ChatError> {
        let _owner = self.gate.state.owner.lock().await;
        if self.gate.state.uncertain.load(Ordering::Acquire) {
            return Err(ChatError::StorageUnavailable);
        }
        Ok(read(&self.published.lock()))
    }

    /// Stage, commit durably, then publish, independent of the caller's lifetime.
    pub(super) async fn update<R: Send + 'static>(
        self: &Arc<Self>,
        mutation: impl FnOnce(&mut T) -> Result<R, ChatError> + Send + 'static,
    ) -> Result<R, ChatError> {
        let store = self.clone();
        let (tx, rx) = oneshot::channel();
        (self.spawner)(Box::pin(async move {
            let owner = store.gate.state.owner.clone().lock_owned().await;
            let result = async {
                if store.gate.state.uncertain.load(Ordering::Acquire) {
                    return Err(ChatError::StorageUnavailable);
                }
                let mut candidate = store.published.lock().clone();
                let output = mutation(&mut candidate)?;
                let encrypted = store.cipher.encrypt(&candidate)?;
                store.gate.state.uncertain.store(true, Ordering::Release);
                store
                    .storage
                    .write_core_storage(store.storage_key.clone(), encrypted)
                    .await
                    .map_err(|_| ChatError::StorageUnavailable)?;
                // No await between success and publication. Any write error
                // leaves the shared latch set, blocking stale reads and writes.
                *store.published.lock() = candidate;
                store.gate.state.uncertain.store(false, Ordering::Release);
                Ok(output)
            }
            .await;
            drop(store);
            let _ = tx.send(result);
            drop(owner);
        }));
        rx.await.map_err(|_| ChatError::StorageUnavailable)?
    }

    /// Recover only by authenticating the actual slot and completing a durable
    /// rewrite. Missing, malformed, or unreadable storage never clears poison.
    pub(super) async fn reauthenticate(self: &Arc<Self>) -> Result<(), ChatError> {
        let store = self.clone();
        let (tx, rx) = oneshot::channel();
        (self.spawner)(Box::pin(async move {
            let owner = store.gate.state.owner.clone().lock_owned().await;
            let result = async {
                if !store.gate.state.uncertain.load(Ordering::Acquire) {
                    return Ok(());
                }
                let bytes = store
                    .storage
                    .read_core_storage(store.storage_key.clone())
                    .await
                    .map_err(|_| ChatError::StorageUnavailable)?
                    .ok_or(ChatError::StorageUnavailable)?;
                let durable = store.cipher.decrypt(&bytes)?;
                store
                    .storage
                    .write_core_storage(store.storage_key.clone(), bytes)
                    .await
                    .map_err(|_| ChatError::StorageUnavailable)?;
                *store.published.lock() = durable;
                store.gate.state.uncertain.store(false, Ordering::Release);
                Ok(())
            }
            .await;
            drop(store);
            let _ = tx.send(result);
            drop(owner);
        }));
        rx.await.map_err(|_| ChatError::StorageUnavailable)?
    }
}
