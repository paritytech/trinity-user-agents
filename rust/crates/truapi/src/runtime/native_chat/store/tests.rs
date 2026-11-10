// SPDX-License-Identifier: AGPL-3.0-only
//! Deterministic durability and authenticated restore regressions; no sleeps.

use std::{
	collections::BTreeMap,
	sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};

use futures::executor::block_on;
use parity_scale_codec::{Compact, Input};
use truapi::latest::GenericError;
use zeroize::{Zeroize, ZeroizeOnDrop};

use super::*;

#[derive(Clone, Encode, Zeroize, ZeroizeOnDrop)]
struct Secret([u8; 32]);

impl Decode for Secret {
	fn decode<I: Input>(input: &mut I) -> Result<Self, parity_scale_codec::Error> {
		let mut bytes = Zeroizing::new([0; 32]);
		input.read(&mut *bytes)?;
		Ok(Self(*bytes))
	}
}

#[derive(Clone, Encode, Decode)]
struct State {
	device: Secret,
	roster: Vec<[u8; 32]>,
	outbox: BTreeMap<u64, Vec<u8>>,
	processed: Vec<u64>,
}

type PublicSnapshot = ([u8; 32], Vec<[u8; 32]>, BTreeMap<u64, Vec<u8>>, Vec<u64>);

fn initial() -> Result<State, ChatError> {
	Ok(State {
		device: Secret([71; 32]),
		roster: Vec::new(),
		outbox: BTreeMap::new(),
		processed: Vec::new(),
	})
}

fn snapshot(state: &State) -> PublicSnapshot {
	(
		sp_crypto_hashing::blake2_256(&state.device.0),
		state.roster.clone(),
		state.outbox.clone(),
		state.processed.clone(),
	)
}

fn slot() -> CoreStorageKey {
	CoreStorageKey::NativeChatDevice {
		root_public_key: [1; 32],
		genesis_hash: [2; 32],
		product_id: "chat.testnet".into(),
	}
}

#[derive(Default)]
struct DurableSlot {
	values: SyncMutex<BTreeMap<Vec<u8>, Vec<u8>>>,
	fail_before: AtomicBool,
	fail_after: AtomicBool,
	writes: AtomicUsize,
	pause_next: SyncMutex<Option<(oneshot::Sender<()>, oneshot::Receiver<()>)>>,
}

impl DurableSlot {
	fn pause(&self) -> (oneshot::Receiver<()>, oneshot::Sender<()>) {
		let (entered_tx, entered_rx) = oneshot::channel();
		let (resume_tx, resume_rx) = oneshot::channel();
		*self.pause_next.lock() = Some((entered_tx, resume_rx));
		(entered_rx, resume_tx)
	}

	fn bytes(&self) -> Vec<u8> {
		self.values.lock().get(&slot().encode()).unwrap().clone()
	}

	fn replace(&self, key: &CoreStorageKey, bytes: Vec<u8>) {
		self.values.lock().insert(key.encode(), bytes);
	}
}

#[async_trait::async_trait]
impl CoreStorage for DurableSlot {
	async fn read_core_storage(
		&self,
		key: CoreStorageKey,
	) -> Result<Option<Vec<u8>>, GenericError> {
		Ok(self.values.lock().get(&key.encode()).cloned())
	}

	async fn write_core_storage(
		&self,
		key: CoreStorageKey,
		bytes: Vec<u8>,
	) -> Result<(), GenericError> {
		self.writes.fetch_add(1, Ordering::SeqCst);
		let pause = self.pause_next.lock().take();
		if let Some((entered, resume)) = pause {
			let _ = entered.send(());
			resume.await.unwrap();
		}
		if self.fail_before.swap(false, Ordering::SeqCst) {
			return Err(GenericError {
				reason: "private pre-replacement platform diagnostic".into(),
			});
		}
		self.values.lock().insert(key.encode(), bytes);
		if self.fail_after.swap(false, Ordering::SeqCst) {
			return Err(GenericError { reason: "private post-rename platform diagnostic".into() });
		}
		Ok(())
	}

	async fn clear_core_storage(&self, key: CoreStorageKey) -> Result<(), GenericError> {
		self.values.lock().remove(&key.encode());
		Ok(())
	}
}

async fn open(storage: Arc<DurableSlot>) -> Arc<ChatStateStore<State>> {
	ChatStateStore::open_storage(
		storage,
		slot(),
		&[3; 32],
		crate::test_support::test_spawner(),
		initial,
	)
	.await
	.unwrap()
}

fn change(state: &mut State) -> Result<(), ChatError> {
	state.roster.push([9; 32]);
	state.outbox.insert(12, b"committed outbox text".to_vec());
	state.processed.push(13);
	Ok(())
}

#[test]
fn restart_authenticates_same_device_and_atomic_chat_state() {
	block_on(async {
		let storage = Arc::new(DurableSlot::default());
		let store = open(storage.clone()).await;
		let device = store.read(|state| snapshot(state).0).await.unwrap();
		store.update(change).await.unwrap();
		let committed = store.read(snapshot).await.unwrap();
		assert_eq!(committed.0, device);
		assert_eq!(committed.1, vec![[9; 32]]);
		assert_eq!(committed.2.get(&12).unwrap(), b"committed outbox text");
		assert_eq!(committed.3, vec![13]);
		drop(store);
		let restored = ChatStateStore::<State>::open_storage(
			storage,
			slot(),
			&[3; 32],
			crate::test_support::test_spawner(),
			|| panic!("an existing device must never be reinitialized"),
		)
		.await
		.unwrap();
		assert_eq!(restored.read(snapshot).await.unwrap(), committed);
	});
}

// The deployed Epoca SDK used the same v3 plaintext/envelope but byte 14/15
// in the complete SCALE key used by both HKDF and AEAD.
fn epoca_snapshot(key: &CoreStorageKey, state: &State) -> Vec<u8> {
	let mut encoded_key = key.encode();
	encoded_key[0] = match key {
		CoreStorageKey::NativeChatDevice { .. } => 14,
		CoreStorageKey::NativeChatFileChunk { .. } => 15,
		_ => panic!("not a deployed Chat slot"),
	};
	let mut aad = b"truapi/host/native-chat/snapshot\0HCHS\x03\x00".to_vec();
	aad.extend_from_slice(&encoded_key);
	let mut key = Zeroizing::new([0; 32]);
	Hkdf::<Sha256>::new(Some(b"truapi/host/native-chat/snapshot-key/v1"), &[3; 32])
		.expand(&aad, &mut *key)
		.unwrap();
	let nonce = [7; 24];
	let mut ciphertext = Zeroizing::new(state.encode());
	XChaCha20Poly1305::new((&*key).into())
		.encrypt_in_place(XNonce::from_slice(&nonce), &aad, &mut *ciphertext)
		.unwrap();
	let mut envelope = b"EPCH\x01HCHS\x03\x00".to_vec();
	envelope.extend_from_slice(&nonce);
	envelope.extend_from_slice(&ciphertext);
	envelope
}

#[test]
fn epoca_migration_preserves_device_and_chunk_state_then_reopens_canonical_only() {
	block_on(async {
		for key in [
			slot(),
			CoreStorageKey::NativeChatFileChunk {
				root_public_key: [1; 32],
				genesis_hash: [2; 32],
				product_id: "chat.testnet".into(),
				attachment_id: [8; 32],
				chunk_index: 9,
			},
		] {
			let storage = Arc::new(DurableSlot::default());
			let mut state = initial().unwrap();
			change(&mut state).unwrap();
			storage.replace(&key, epoca_snapshot(&key, &state));
			let migrated = ChatStateStore::<State>::open_storage(
				storage.clone(),
				key.clone(),
				&[3; 32],
				crate::test_support::test_spawner(),
				|| panic!("migration must not mint a replacement device"),
			)
			.await
			.unwrap();
			assert_eq!(migrated.read(snapshot).await.unwrap(), snapshot(&state));
			drop(migrated);
			let canonical = storage.values.lock().get(&key.encode()).unwrap().clone();
			let restored: State =
				SnapshotCipher::new(&key, &[3; 32]).unwrap().decrypt(&canonical).unwrap();
			assert_eq!(snapshot(&restored), snapshot(&state));
			let reopened = ChatStateStore::<State>::open_storage(
				storage.clone(),
				key,
				&[3; 32],
				crate::test_support::test_spawner(),
				|| panic!("canonical state must survive restart"),
			)
			.await
			.unwrap();
			assert_eq!(reopened.read(snapshot).await.unwrap(), snapshot(&state));
		}
	});
}

#[test]
fn epoca_migration_requires_authenticated_explicit_envelope_and_durable_replacement() {
	block_on(async {
		let state = initial().unwrap();
		let legacy = epoca_snapshot(&slot(), &state);
		let mut tampered = legacy.clone();
		*tampered.last_mut().unwrap() ^= 1;
		let wrong_owner = epoca_snapshot(
			&CoreStorageKey::NativeChatDevice {
				root_public_key: [9; 32],
				genesis_hash: [2; 32],
				product_id: "chat.testnet".into(),
			},
			&state,
		);
		for bytes in [legacy[5..].to_vec(), tampered, wrong_owner, b"EPCH\x01".to_vec()] {
			let storage = Arc::new(DurableSlot::default());
			storage.replace(&slot(), bytes.clone());
			assert!(matches!(
				ChatStateStore::<State>::open_storage(
					storage.clone(),
					slot(),
					&[3; 32],
					crate::test_support::test_spawner(),
					|| panic!("invalid migration is not an absent slot"),
				)
				.await,
				Err(ChatError::StorageUnavailable)
			));
			assert_eq!(storage.bytes(), bytes);
			assert_eq!(storage.writes.load(Ordering::SeqCst), 0);
		}
		for fail_after in [false, true] {
			let storage = Arc::new(DurableSlot::default());
			storage.replace(&slot(), legacy.clone());
			if fail_after {
				storage.fail_after.store(true, Ordering::SeqCst);
			} else {
				storage.fail_before.store(true, Ordering::SeqCst);
			}
			assert!(matches!(
				ChatStateStore::<State>::open_storage(
					storage.clone(),
					slot(),
					&[3; 32],
					crate::test_support::test_spawner(),
					|| panic!("failed migration must retain the old device"),
				)
				.await,
				Err(ChatError::StorageUnavailable)
			));
			let recovered = ChatStateStore::<State>::open_storage(
				storage.clone(),
				slot(),
				&[3; 32],
				crate::test_support::test_spawner(),
				|| panic!("retry must authenticate retained state"),
			)
			.await
			.unwrap();
			assert_eq!(recovered.read(snapshot).await.unwrap(), snapshot(&state));
		}
	});
}

#[test]
fn canceled_update_still_commits_and_publishes_before_reads() {
	block_on(async {
		let storage = Arc::new(DurableSlot::default());
		let store = open(storage.clone()).await;
		let (entered, resume) = storage.pause();
		let mut caller = Box::pin(store.update(change));
		assert!(futures::poll!(&mut caller).is_pending());
		entered.await.unwrap();
		let mut reader = Box::pin(store.read(snapshot));
		assert!(futures::poll!(&mut reader).is_pending());
		drop(caller);
		resume.send(()).unwrap();
		let committed = reader.await.unwrap();
		assert_eq!(committed.1, vec![[9; 32]]);
		assert_eq!(committed.2.get(&12).unwrap(), b"committed outbox text");
		assert_eq!(committed.3, vec![13]);
		drop(store);
		assert_eq!(open(storage).await.read(snapshot).await.unwrap(), committed);
	});
}

#[test]
fn post_rename_failure_blocks_stale_state_until_cancel_safe_reauthentication() {
	block_on(async {
		let storage = Arc::new(DurableSlot::default());
		let store = open(storage.clone()).await;
		storage.fail_after.store(true, Ordering::SeqCst);
		assert_eq!(store.update(change).await, Err(ChatError::StorageUnavailable));
		let accepted = storage.bytes();
		let writes = storage.writes.load(Ordering::SeqCst);
		assert_eq!(
			store.read(|_| panic!("poisoned state cannot authorize a read")).await,
			Err::<(), _>(ChatError::StorageUnavailable)
		);
		assert_eq!(
			store.update(|_| panic!("poisoned state cannot authorize a mutation")).await,
			Err::<(), _>(ChatError::StorageUnavailable)
		);
		assert_eq!(storage.writes.load(Ordering::SeqCst), writes);
		assert_eq!(storage.bytes(), accepted);

		storage.values.lock().remove(&slot().encode());
		assert_eq!(store.reauthenticate().await, Err(ChatError::StorageUnavailable));
		assert_eq!(store.read(snapshot).await, Err(ChatError::StorageUnavailable));
		let mut tampered = accepted.clone();
		*tampered.last_mut().unwrap() ^= 1;
		storage.replace(&slot(), tampered);
		assert_eq!(store.reauthenticate().await, Err(ChatError::StorageUnavailable));
		storage.replace(&slot(), accepted);
		storage.fail_before.store(true, Ordering::SeqCst);
		assert_eq!(store.reauthenticate().await, Err(ChatError::StorageUnavailable));
		assert_eq!(store.read(snapshot).await, Err(ChatError::StorageUnavailable));

		let (entered, resume) = storage.pause();
		let mut caller = Box::pin(store.reauthenticate());
		assert!(futures::poll!(&mut caller).is_pending());
		entered.await.unwrap();
		drop(caller);
		resume.send(()).unwrap();
		let recovered = store.read(snapshot).await.unwrap();
		assert_eq!(recovered.1, vec![[9; 32]]);
		assert_eq!(recovered.2.get(&12).unwrap(), b"committed outbox text");
		assert_eq!(recovered.3, vec![13]);
		store
			.update(|state| {
				state.processed.push(14);
				Ok(())
			})
			.await
			.unwrap();
		drop(store);
		assert_eq!(
			open(storage).await.read(|state| state.processed.clone()).await.unwrap(),
			vec![13, 14]
		);
	});
}

#[test]
fn canceled_initialization_cannot_mint_a_competing_device() {
	block_on(async {
		let storage = Arc::new(DurableSlot::default());
		let calls = AtomicUsize::new(0);
		let (entered, resume) = storage.pause();
		let mut first = Box::pin(ChatStateStore::<State>::open_storage(
			storage.clone(),
			slot(),
			&[3; 32],
			crate::test_support::test_spawner(),
			|| {
				calls.fetch_add(1, Ordering::SeqCst);
				initial()
			},
		));
		assert!(futures::poll!(&mut first).is_pending());
		entered.await.unwrap();
		drop(first);
		let mut replacement = Box::pin(ChatStateStore::<State>::open_storage(
			storage.clone(),
			slot(),
			&[3; 32],
			crate::test_support::test_spawner(),
			|| panic!("in-flight initialization owns the device identity"),
		));
		assert!(futures::poll!(&mut replacement).is_pending());
		resume.send(()).unwrap();
		let store = replacement.await.unwrap();
		assert_eq!(store.read(snapshot).await.unwrap(), snapshot(&initial().unwrap()));
		assert_eq!(calls.load(Ordering::SeqCst), 1);
		assert_eq!(storage.writes.load(Ordering::SeqCst), 1);
	});
}

#[test]
fn concurrent_opens_and_owned_updates_keep_exclusive_slot_custody() {
	block_on(async {
		let storage = Arc::new(DurableSlot::default());
		let (entered, resume) = storage.pause();
		let mut first = Box::pin(open(storage.clone()));
		assert!(futures::poll!(&mut first).is_pending());
		entered.await.unwrap();
		let mut competing = Box::pin(ChatStateStore::<State>::open_storage(
			storage.clone(),
			slot(),
			&[3; 32],
			crate::test_support::test_spawner(),
			|| panic!("a live store already owns this slot"),
		));
		assert!(futures::poll!(&mut competing).is_pending());
		resume.send(()).unwrap();
		let store = first.await;
		assert!(matches!(competing.await, Err(ChatError::StorageUnavailable)));
		assert_eq!(storage.writes.load(Ordering::SeqCst), 1);

		let (entered, resume) = storage.pause();
		let mut caller = Box::pin(store.update(change));
		assert!(futures::poll!(&mut caller).is_pending());
		entered.await.unwrap();
		drop(caller);
		drop(store);
		let mut replacement = Box::pin(ChatStateStore::<State>::open_storage(
			storage.clone(),
			slot(),
			&[3; 32],
			crate::test_support::test_spawner(),
			|| panic!("an owned update must finish before the next store opens"),
		));
		assert!(futures::poll!(&mut replacement).is_pending());
		assert_eq!(storage.writes.load(Ordering::SeqCst), 2);
		resume.send(()).unwrap();
		let restored = replacement.await.unwrap();
		assert_eq!(restored.read(|state| state.processed.clone()).await.unwrap(), vec![13]);
		restored
			.update(|state| {
				state.processed.push(14);
				Ok(())
			})
			.await
			.unwrap();
		assert_eq!(restored.read(|state| state.processed.clone()).await.unwrap(), vec![13, 14]);
	});
}

#[test]
fn dropped_poisoned_store_keeps_custody_until_durable_repair() {
	block_on(async {
		let storage = Arc::new(DurableSlot::default());
		let store = open(storage.clone()).await;
		storage.fail_after.store(true, Ordering::SeqCst);
		assert_eq!(store.update(change).await, Err(ChatError::StorageUnavailable));
		let accepted = storage.bytes();
		drop(store);
		storage.values.lock().remove(&slot().encode());
		assert!(matches!(
			ChatStateStore::<State>::open_storage(
				storage.clone(),
				slot(),
				&[3; 32],
				crate::test_support::test_spawner(),
				|| panic!("dropping the last poisoned store cannot authorize a new device"),
			)
			.await,
			Err(ChatError::StorageUnavailable)
		));
		assert_eq!(storage.writes.load(Ordering::SeqCst), 2);
		storage.replace(&slot(), accepted.clone());
		storage.fail_before.store(true, Ordering::SeqCst);
		assert!(matches!(
			ChatStateStore::<State>::open_storage(
				storage.clone(),
				slot(),
				&[3; 32],
				crate::test_support::test_spawner(),
				|| panic!("repair must authenticate the existing device"),
			)
			.await,
			Err(ChatError::StorageUnavailable)
		));
		storage.values.lock().remove(&slot().encode());
		assert!(matches!(
			ChatStateStore::<State>::open_storage(
				storage.clone(),
				slot(),
				&[3; 32],
				crate::test_support::test_spawner(),
				|| panic!("failed repair must retain poison without a live store"),
			)
			.await,
			Err(ChatError::StorageUnavailable)
		));
		storage.replace(&slot(), accepted.clone());
		let recovered = open(storage.clone()).await;
		assert_eq!(recovered.read(|state| state.processed.clone()).await.unwrap(), vec![13]);
		assert_eq!(storage.bytes(), accepted);
		assert_eq!(storage.writes.load(Ordering::SeqCst), 4);
	});
}

#[test]
fn same_slot_on_different_platforms_has_independent_custody() {
	block_on(async {
		let first = Arc::new(DurableSlot::default());
		let second = Arc::new(DurableSlot::default());
		let first_store = open(first.clone()).await;
		let second_store = open(second.clone()).await;
		first.fail_after.store(true, Ordering::SeqCst);
		assert_eq!(first_store.update(change).await, Err(ChatError::StorageUnavailable));
		assert_eq!(second_store.read(snapshot).await.unwrap(), snapshot(&initial().unwrap()));
		second_store.update(change).await.unwrap();
		assert_eq!(second_store.read(|state| state.processed.clone()).await.unwrap(), vec![13]);
	});
}

#[test]
fn completed_attachment_chunks_release_bookkeeping_but_preserve_storage() {
	block_on(async {
		let storage = Arc::new(DurableSlot::default());
		let storage_id = Arc::as_ptr(&storage) as usize;
		for chunk_index in 0..64 {
			let key = CoreStorageKey::NativeChatFileChunk {
				root_public_key: [1; 32],
				genesis_hash: [2; 32],
				product_id: "chat.testnet".into(),
				attachment_id: [7; 32],
				chunk_index,
			};
			let store = ChatStateStore::<State>::open_storage(
				storage.clone(),
				key.clone(),
				&[3; 32],
				crate::test_support::test_spawner(),
				|| {
					let mut state = initial()?;
					state.processed.push(u64::from(chunk_index));
					Ok(state)
				},
			)
			.await
			.unwrap();
			let released = Arc::downgrade(&store.gate.state);
			drop(store);
			assert!(released.upgrade().is_none());
			let restored = ChatStateStore::<State>::open_storage(
				storage.clone(),
				key,
				&[3; 32],
				crate::test_support::test_spawner(),
				|| panic!("reclaiming a clean gate must not delete durable chunks"),
			)
			.await
			.unwrap();
			assert_eq!(
				restored.read(|state| state.processed.clone()).await.unwrap(),
				vec![u64::from(chunk_index)]
			);
			drop(restored);
			// This is the resource bound under review: neither strong gates
			// nor dead weak/key entries may accumulate per completed chunk.
			assert!(SLOTS.lock().get(&storage_id).unwrap().slots.is_empty());
		}
		assert_eq!(storage.writes.load(Ordering::SeqCst), 64);
	});
}

#[test]
fn ambiguous_initialization_requires_authenticated_bytes_never_absence() {
	block_on(async {
		let absent = Arc::new(DurableSlot::default());
		absent.fail_before.store(true, Ordering::SeqCst);
		assert!(matches!(
			ChatStateStore::<State>::open_storage(
				absent.clone(),
				slot(),
				&[3; 32],
				crate::test_support::test_spawner(),
				initial,
			)
			.await,
			Err(ChatError::StorageUnavailable)
		));
		assert!(matches!(
			ChatStateStore::<State>::open_storage(
				absent.clone(),
				slot(),
				&[3; 32],
				crate::test_support::test_spawner(),
				|| panic!("absence cannot disprove an earlier initialization attempt"),
			)
			.await,
			Err(ChatError::StorageUnavailable)
		));
		assert_eq!(absent.writes.load(Ordering::SeqCst), 1);

		let accepted = Arc::new(DurableSlot::default());
		accepted.fail_after.store(true, Ordering::SeqCst);
		assert!(matches!(
			ChatStateStore::<State>::open_storage(
				accepted.clone(),
				slot(),
				&[3; 32],
				crate::test_support::test_spawner(),
				initial,
			)
			.await,
			Err(ChatError::StorageUnavailable)
		));
		let bytes = accepted.bytes();
		let recovered = ChatStateStore::<State>::open_storage(
			accepted.clone(),
			slot(),
			&[3; 32],
			crate::test_support::test_spawner(),
			|| panic!("authenticated initialization already exists"),
		)
		.await
		.unwrap();
		assert_eq!(recovered.read(snapshot).await.unwrap(), snapshot(&initial().unwrap()));
		assert_eq!(accepted.bytes(), bytes);
		assert_eq!(accepted.writes.load(Ordering::SeqCst), 2);
	});
}

#[test]
fn wrong_owner_network_product_entropy_and_tampering_fail_closed() {
	block_on(async {
		let storage = Arc::new(DurableSlot::default());
		drop(open(storage.clone()).await);
		let bytes = storage.bytes();
		for key in [
			CoreStorageKey::NativeChatDevice {
				root_public_key: [4; 32],
				genesis_hash: [2; 32],
				product_id: "chat.testnet".into(),
			},
			CoreStorageKey::NativeChatDevice {
				root_public_key: [1; 32],
				genesis_hash: [4; 32],
				product_id: "chat.testnet".into(),
			},
			CoreStorageKey::NativeChatDevice {
				root_public_key: [1; 32],
				genesis_hash: [2; 32],
				product_id: "other.testnet".into(),
			},
		] {
			storage.replace(&key, bytes.clone());
			assert!(matches!(
				ChatStateStore::<State>::open_storage(
					storage.clone(),
					key,
					&[3; 32],
					crate::test_support::test_spawner(),
					|| panic!("wrong ownership is not an absent slot"),
				)
				.await,
				Err(ChatError::StorageUnavailable)
			));
		}
		assert!(matches!(
			ChatStateStore::<State>::open_storage(
				storage.clone(),
				slot(),
				&[4; 32],
				crate::test_support::test_spawner(),
				|| panic!("wrong entropy is not an absent slot"),
			)
			.await,
			Err(ChatError::StorageUnavailable)
		));
		for offset in [0, 4, 6, HEADER_BYTES, bytes.len() - 1] {
			let mut tampered = bytes.clone();
			tampered[offset] ^= 1;
			storage.replace(&slot(), tampered);
			assert!(matches!(
				ChatStateStore::<State>::open_storage(
					storage.clone(),
					slot(),
					&[3; 32],
					crate::test_support::test_spawner(),
					|| panic!("tampering is not an absent slot"),
				)
				.await,
				Err(ChatError::StorageUnavailable)
			));
		}
	});
}

// Construct authenticated malicious plaintext, so decoder tests cannot pass
// merely because AEAD authentication rejected an invalid envelope first.
fn seal(plaintext: &[u8]) -> Vec<u8> {
	let cipher = SnapshotCipher::new(&slot(), &[3; 32]).unwrap();
	let nonce = [5; 24];
	let mut ciphertext = Zeroizing::new(plaintext.to_vec());
	XChaCha20Poly1305::new((&*cipher.key).into())
		.encrypt_in_place(XNonce::from_slice(&nonce), &cipher.aad, &mut *ciphertext)
		.unwrap();
	let mut envelope = MAGIC.to_vec();
	envelope.extend_from_slice(&VERSION.to_le_bytes());
	envelope.extend_from_slice(&nonce);
	envelope.extend_from_slice(&ciphertext);
	envelope
}

#[derive(Clone, Encode, Decode)]
enum Nested {
	End,
	More(Box<Nested>),
}

#[test]
fn malformed_oversized_and_authenticated_decode_bombs_fail_closed() {
	block_on(async {
		let storage = Arc::new(DurableSlot::default());
		let encoded = codec::encode(&initial().unwrap()).unwrap();
		let mut trailing = encoded.clone();
		trailing.push(0);
		for bytes in [
			Vec::new(),
			vec![0; HEADER_BYTES + TAG_BYTES - 1],
			vec![0; HEADER_BYTES + MAX_SNAPSHOT_BYTES + TAG_BYTES + 1],
			seal(&encoded[..encoded.len() - 1]),
			seal(&trailing),
		] {
			storage.replace(&slot(), bytes);
			assert!(matches!(
				ChatStateStore::<State>::open_storage(
					storage.clone(),
					slot(),
					&[3; 32],
					crate::test_support::test_spawner(),
					|| panic!("malformed storage must not initialize a device"),
				)
				.await,
				Err(ChatError::StorageUnavailable)
			));
		}
		let mut nested = Nested::End;
		for _ in 0..=MAX_DECODE_DEPTH {
			nested = Nested::More(Box::new(nested));
		}
		storage.replace(&slot(), seal(&nested.encode()));
		assert!(matches!(
			ChatStateStore::<Nested>::open_storage(
				storage.clone(),
				slot(),
				&[3; 32],
				crate::test_support::test_spawner(),
				|| panic!("depth overflow must not initialize state"),
			)
			.await,
			Err(ChatError::StorageUnavailable)
		));

		// SCALE maps account for their declared node allocation before decode.
		// This valid small wire payload would otherwise decode to one entry;
		// repeated keys do not evade the allocation budget of the input.
		let mut oversized_map = Compact(6_000_000u32).encode();
		oversized_map.resize(oversized_map.len() + 12_000_000, 0);
		storage.replace(&slot(), seal(&oversized_map));
		assert!(matches!(
			ChatStateStore::<BTreeMap<u8, u8>>::open_storage(
				storage,
				slot(),
				&[3; 32],
				crate::test_support::test_spawner(),
				|| panic!("allocation overflow must not initialize state"),
			)
			.await,
			Err(ChatError::StorageUnavailable)
		));
	});
}

#[test]
fn rejected_or_unrestorable_mutations_do_not_replace_committed_state() {
	block_on(async {
		let storage = Arc::new(DurableSlot::default());
		let store = open(storage.clone()).await;
		let committed = store.read(snapshot).await.unwrap();
		let bytes = storage.bytes();
		assert_eq!(
			store
				.update(|state| {
					change(state)?;
					Err::<(), _>(ChatError::OperationConflict)
				})
				.await,
			Err(ChatError::OperationConflict)
		);
		assert_eq!(store.read(snapshot).await.unwrap(), committed);
		assert_eq!(storage.bytes(), bytes);
		assert_eq!(
			store
				.update(|state| {
					state.outbox.insert(1, vec![0; MAX_SNAPSHOT_BYTES]);
					Ok(())
				})
				.await,
			Err(ChatError::StorageUnavailable)
		);
		assert_eq!(store.read(snapshot).await.unwrap(), committed);
		assert_eq!(storage.bytes(), bytes);
		assert_eq!(storage.writes.load(Ordering::SeqCst), 1);
	});
}
