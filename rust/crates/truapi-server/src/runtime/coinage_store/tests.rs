// SPDX-License-Identifier: AGPL-3.0-only
//! Observable durability, atomicity, cancellation, and authenticated restore tests.

use std::sync::atomic::{AtomicBool, Ordering};

use futures::{channel::oneshot, executor::block_on};
use parity_scale_codec::Encode;
use parking_lot::Mutex as SyncMutex;
use truapi::latest::GenericError;
use truapi_coinage::{
    claim_plan::{ClaimPlanStatus, ClaimPlanStore, CodableClaimPlanEntry},
    index_store::{CoinageIndexStore, IndexKind},
    model::{CoinState, VoucherLocalState, VoucherPrivacyLevel, VoucherRemoteState},
    repo::{CoinRepository, TransferStateCommitter, VoucherRepository},
    wal::{CheckpointBlock, WalCoinRef, WalOperation, WalPayload, WalStore},
};

use super::*;

#[derive(Default)]
struct DurableSlot {
    values: SyncMutex<BTreeMap<Vec<u8>, Vec<u8>>>,
    fail_next: AtomicBool,
    fail_after_replace: AtomicBool,
    pause_next: SyncMutex<Option<(oneshot::Sender<()>, oneshot::Receiver<()>)>>,
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
        value: Vec<u8>,
    ) -> Result<(), GenericError> {
        let pause = self.pause_next.lock().take();
        if let Some((entered, resume)) = pause {
            let _ = entered.send(());
            resume.await.unwrap();
        }
        if self.fail_next.swap(false, Ordering::SeqCst) {
            return Err(GenericError {
                reason: "injected atomic write failure".into(),
            });
        }
        self.values.lock().insert(key.encode(), value);
        if self.fail_after_replace.swap(false, Ordering::SeqCst) {
            return Err(GenericError {
                reason: "injected durability failure after replacement".into(),
            });
        }
        Ok(())
    }

    async fn clear_core_storage(&self, key: CoreStorageKey) -> Result<(), GenericError> {
        self.values.lock().remove(&key.encode());
        Ok(())
    }
}

async fn open(storage: Arc<DurableSlot>) -> Arc<HostCoinageStore> {
    try_open(storage).await.unwrap()
}

async fn try_open(storage: Arc<DurableSlot>) -> Result<Arc<HostCoinageStore>, StoreError> {
    HostCoinageStore::open_storage(
        storage,
        [1; 32],
        [2; 32],
        Zeroizing::new([3; 32]),
        crate::test_support::test_spawner(),
        false,
    )
    .await
}

#[test]
fn recovery_open_never_creates_a_missing_purse() {
    block_on(async {
        let storage = Arc::new(DurableSlot::default());
        assert!(matches!(
            HostCoinageStore::open_storage(
                storage.clone(),
                [1; 32],
                [2; 32],
                Zeroizing::new([3; 32]),
                crate::test_support::test_spawner(),
                true,
            )
            .await,
            Err(StoreError::Missing)
        ));
        assert!(storage.values.lock().is_empty());
        let store = open(storage.clone()).await;
        store.bind_asset_instance(Some(4)).await.unwrap();
        drop(store);
        let restored = HostCoinageStore::open_storage(
            storage,
            [1; 32],
            [2; 32],
            Zeroizing::new([3; 32]),
            crate::test_support::test_spawner(),
            true,
        )
        .await
        .unwrap();
        assert!(matches!(
            restored.bind_asset_instance(Some(5)).await,
            Err(StoreError::Conflict)
        ));
    });
}

async fn reopen_after_owned_work(storage: Arc<DurableSlot>) -> Arc<HostCoinageStore> {
    let reopen = async {
        loop {
            match try_open(storage.clone()).await {
                Ok(store) => break store,
                Err(StoreError::Conflict) => {
                    futures_timer::Delay::new(std::time::Duration::from_millis(1)).await;
                }
                Err(error) => panic!("unexpected reopen failure: {error}"),
            }
        }
    };
    let timeout = futures_timer::Delay::new(std::time::Duration::from_secs(5));
    futures::pin_mut!(reopen, timeout);
    match futures::future::select(reopen, timeout).await {
        futures::future::Either::Left((store, _)) => store,
        futures::future::Either::Right(_) => {
            panic!("owned commit did not release storage ownership")
        }
    }
}

fn coin(index: u32) -> Coin {
    Coin {
        derivation_index: index,
        exponent: 2,
        age: Some(3),
        state: CoinState::Available,
    }
}

fn voucher(index: u32) -> Voucher {
    Voucher {
        derivation_index: index,
        exponent: -2,
        allocated_at_ms: 100,
        ready_at_ms: 200,
        local_state: VoucherLocalState::Available,
        remote_state: VoucherRemoteState::InRecycler { recycler_index: 7 },
        privacy: VoucherPrivacyLevel::Full,
    }
}

fn journal(id: &str) -> TransferWalEntry {
    TransferWalEntry {
        entry_id: id.into(),
        operation: WalOperation::SecretHandoff,
        payload: WalPayload {
            input_coins: vec![WalCoinRef {
                derivation_index: 0,
                exponent: 2,
            }],
            ..WalPayload::default()
        },
        checkpoint: CheckpointBlock::Pending,
        created_at_ms: 100,
    }
}

#[test]
fn asset_binding_survives_restart_without_retargeting_reserved_operations() {
    block_on(async {
        let storage = Arc::new(DurableSlot::default());
        let store = open(storage.clone()).await;
        store.bind_asset_instance(Some(0)).await.unwrap();
        store
            .write_operation([9; 32], b"pending private handoff".to_vec())
            .await
            .unwrap();
        drop(store);
        let restored = open(storage).await;
        assert_eq!(
            restored.bind_asset_instance(Some(1)).await,
            Err(StoreError::Conflict)
        );
        assert_eq!(
            restored.bind_asset_instance(None).await,
            Err(StoreError::Conflict)
        );
        restored.bind_asset_instance(Some(0)).await.unwrap();
        assert_eq!(
            restored.read_operation([9; 32]).await.unwrap(),
            Some(b"pending private handoff".to_vec())
        );
    });
}

#[test]
fn a_bound_legacy_asset_is_not_an_uninitialized_asset_selection() {
    block_on(async {
        let storage = Arc::new(DurableSlot::default());
        let store = open(storage.clone()).await;
        store.bind_asset_instance(None).await.unwrap();
        drop(store);
        let restored = open(storage).await;
        assert_eq!(
            restored.bind_asset_instance(Some(0)).await,
            Err(StoreError::Conflict)
        );
    });
}

#[test]
fn restart_restores_atomic_spend_repositories_and_host_operations() {
    block_on(async {
        let storage = Arc::new(DurableSlot::default());
        let store = open(storage.clone()).await;
        CoinRepository::upsert(&*store, &coin(0)).await.unwrap();
        VoucherRepository::upsert(&*store, &voucher(4))
            .await
            .unwrap();
        store.reserve(&[0], &[4]).await.unwrap();
        WalStore::save(&*store, &journal("handoff")).await.unwrap();
        store
            .update_checkpoint(
                "handoff",
                CheckpointBlock::Known {
                    number: 42,
                    hash: [8; 32],
                },
            )
            .await
            .unwrap();
        ClaimPlanStore::save(
            &*store,
            &ClaimPlan {
                memo_key: [9; 32],
                message_id: Some("payment".into()),
                entries: vec![CodableClaimPlanEntry {
                    entry_index: 0,
                    exponent: 2,
                    derivation_index: 2,
                }],
                outgoing_public_keys: vec![[10; 32]],
                detection_anchor: Some([8; 32]),
                status: ClaimPlanStatus::Detected,
                claimed_amount: None,
                total_value: 400,
            },
        )
        .await
        .unwrap();
        store
            .write_operation([7; 32], b"host-private handoff material".to_vec())
            .await
            .unwrap();
        store
            .commit(&[0], &[4], &[coin(1)], &[coin(2)])
            .await
            .unwrap();
        drop(store);

        let restored = open(storage.clone()).await;
        let coins = CoinRepository::list(&*restored).await.unwrap();
        assert_eq!(
            coins
                .iter()
                .map(|coin| (coin.derivation_index, coin.state))
                .collect::<Vec<_>>(),
            vec![
                (0, CoinState::Spent),
                (1, CoinState::Available),
                (2, CoinState::Spent)
            ]
        );
        assert!(
            VoucherRepository::list(&*restored)
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            restored.current_index(IndexKind::Voucher).await.unwrap(),
            Some(4)
        );
        assert_eq!(restored.get_next_index(IndexKind::Coin).await.unwrap(), 3);
        assert_eq!(
            WalStore::load_all(&*restored).await.unwrap()[0].checkpoint,
            CheckpointBlock::Known {
                number: 42,
                hash: [8; 32]
            }
        );
        assert_eq!(
            restored
                .plan(&[9; 32])
                .await
                .unwrap()
                .unwrap()
                .outgoing_public_keys,
            vec![[10; 32]]
        );
        assert_eq!(
            restored.read_operation([7; 32]).await.unwrap(),
            Some(b"host-private handoff material".to_vec())
        );
        // No decrypted operation material is persisted in the Host slot.
        assert!(!storage.values.lock().values().any(|bytes| {
            bytes
                .windows(b"host-private handoff material".len())
                .any(|part| part == b"host-private handoff material")
        }));
    });
}

#[test]
fn failed_writes_and_invalid_cross_repository_changes_publish_nothing() {
    block_on(async {
        let storage = Arc::new(DurableSlot::default());
        let store = open(storage.clone()).await;
        CoinRepository::upsert(&*store, &coin(0)).await.unwrap();
        VoucherRepository::upsert(&*store, &voucher(0))
            .await
            .unwrap();
        assert!(store.reserve(&[0], &[99]).await.is_err());
        assert_eq!(
            CoinRepository::list(&*store).await.unwrap()[0].state,
            CoinState::Available
        );
        storage.fail_next.store(true, Ordering::SeqCst);
        assert!(store.reserve(&[0], &[0]).await.is_err());
        assert!(CoinRepository::list(&*store).await.is_err());
        assert!(VoucherRepository::list(&*store).await.is_err());
        drop(store);
        let store = open(storage.clone()).await;
        assert_eq!(
            VoucherRepository::list(&*store).await.unwrap()[0].local_state,
            VoucherLocalState::Available
        );
        store.reserve(&[0], &[0]).await.unwrap();
        storage.fail_next.store(true, Ordering::SeqCst);
        assert!(store.commit(&[0], &[0], &[coin(1)], &[]).await.is_err());
        assert!(CoinRepository::list(&*store).await.is_err());
        assert!(VoucherRepository::list(&*store).await.is_err());
        drop(store);
        let restored = open(storage).await;
        assert_eq!(
            CoinRepository::list(&*restored).await.unwrap()[0].state,
            CoinState::PendingTransfer
        );
        assert_eq!(
            VoucherRepository::list(&*restored).await.unwrap()[0].local_state,
            VoucherLocalState::PendingTransfer
        );
        restored.revert(&[0], &[0]).await.unwrap();
        assert_eq!(
            CoinRepository::list(&*restored).await.unwrap()[0].state,
            CoinState::Available
        );
        assert_eq!(
            VoucherRepository::list(&*restored).await.unwrap()[0].local_state,
            VoucherLocalState::Available
        );
    });
}

#[test]
fn counters_are_monotonic_durable_and_unique_under_concurrency() {
    block_on(async {
        let storage = Arc::new(DurableSlot::default());
        let store = open(storage.clone()).await;
        let mut allocated =
            futures::future::join_all((0..32).map(|_| store.get_next_index(IndexKind::Coin)))
                .await
                .into_iter()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
        allocated.sort_unstable();
        assert_eq!(allocated, (0..32).collect::<Vec<_>>());
        assert!(store.set_index(IndexKind::Coin, 1).await.is_err());
        storage.fail_next.store(true, Ordering::SeqCst);
        assert!(store.get_next_index(IndexKind::Coin).await.is_err());
        assert!(store.current_index(IndexKind::Coin).await.is_err());
        drop(store);
        let restored = open(storage).await;
        assert_eq!(restored.get_next_index(IndexKind::Coin).await.unwrap(), 32);
        restored
            .set_index(IndexKind::Voucher, u32::MAX)
            .await
            .unwrap();
        assert!(restored.get_next_index(IndexKind::Voucher).await.is_err());
        assert_eq!(
            restored.current_index(IndexKind::Voucher).await.unwrap(),
            Some(u32::MAX)
        );
    });
}

#[test]
fn canceled_caller_cannot_interrupt_commit_and_publication() {
    block_on(async {
        let storage = Arc::new(DurableSlot::default());
        let store = open(storage.clone()).await;
        let (entered_tx, entered_rx) = oneshot::channel();
        let (resume_tx, resume_rx) = oneshot::channel();
        *storage.pause_next.lock() = Some((entered_tx, resume_rx));
        let mut caller = Box::pin(store.write_operation([9; 32], vec![4, 5, 6]));
        assert!(futures::poll!(&mut caller).is_pending());
        entered_rx.await.unwrap();
        drop(caller);
        resume_tx.send(()).unwrap();
        // Reads wait for the owned task, not for the disconnected caller.
        assert_eq!(
            store.read_operation([9; 32]).await.unwrap(),
            Some(vec![4, 5, 6])
        );
        drop(store);
        assert_eq!(
            reopen_after_owned_work(storage)
                .await
                .read_operation([9; 32])
                .await
                .unwrap(),
            Some(vec![4, 5, 6])
        );
    });
}

#[test]
fn journal_batch_and_checkpoint_failures_do_not_erase_recovery_evidence() {
    block_on(async {
        let storage = Arc::new(DurableSlot::default());
        let store = open(storage.clone()).await;
        WalStore::save(&*store, &journal("first")).await.unwrap();
        let mut conflicting = journal("first");
        conflicting.created_at_ms += 1;
        assert!(
            store
                .save_all(&[journal("second"), conflicting])
                .await
                .is_err()
        );
        assert_eq!(
            WalStore::load_all(&*store).await.unwrap(),
            vec![journal("first")]
        );
        assert!(
            store
                .update_checkpoint("absent", CheckpointBlock::Pending)
                .await
                .is_err()
        );
        storage.fail_next.store(true, Ordering::SeqCst);
        assert!(
            store
                .update_checkpoint(
                    "first",
                    CheckpointBlock::Known {
                        number: 10,
                        hash: [6; 32]
                    }
                )
                .await
                .is_err()
        );
        assert!(WalStore::load_all(&*store).await.is_err());
        drop(store);
        assert_eq!(
            WalStore::load_all(&*open(storage).await).await.unwrap(),
            vec![journal("first")]
        );
    });
}

#[test]
fn legacy_purse_snapshot_cannot_rebind_reserved_indices_to_current_keys() {
    block_on(async {
        let storage = Arc::new(DurableSlot::default());
        let slot = CoreStorageKey::MainPurseCoinage {
            root_public_key: [1; 32],
            genesis_hash: [2; 32],
        };
        let legacy = Snapshot {
            coin_index: Some(7),
            coins: BTreeMap::from([(7, coin(7))]),
            ..Default::default()
        };
        let mut plaintext = codec::encode(&legacy, TAG_BYTES).unwrap();
        let mut aad = b"truapi/host/main-purse-coinage\0".to_vec();
        aad.extend_from_slice(b"HCPS");
        aad.extend_from_slice(&2u16.to_le_bytes());
        aad.extend_from_slice(&[1; 32]);
        aad.extend_from_slice(&[2; 32]);
        let nonce = [9; 24];
        XChaCha20Poly1305::new((&[3; 32]).into())
            .encrypt_in_place(XNonce::from_slice(&nonce), &aad, &mut *plaintext)
            .unwrap();
        let mut encoded = b"HCPS".to_vec();
        encoded.extend_from_slice(&2u16.to_le_bytes());
        encoded.extend_from_slice(&nonce);
        encoded.extend_from_slice(&plaintext);
        storage.values.lock().insert(slot.encode(), encoded.clone());
        let restored = HostCoinageStore::open_storage(
            storage.clone(),
            [1; 32],
            [2; 32],
            Zeroizing::new([3; 32]),
            crate::test_support::test_spawner(),
            false,
        )
        .await;
        assert!(matches!(restored, Err(StoreError::Version)));
        assert_eq!(storage.values.lock().get(&slot.encode()), Some(&encoded));
    });
}

#[test]
fn authentication_binds_wallet_network_version_and_rejects_corruption() {
    block_on(async {
        let storage = Arc::new(DurableSlot::default());
        let store = open(storage.clone()).await;
        store.write_operation([4; 32], vec![42; 32]).await.unwrap();
        let first = storage
            .values
            .lock()
            .get(&store.storage_key.encode())
            .unwrap()
            .clone();
        store.write_operation([4; 32], vec![42; 32]).await.unwrap();
        let second = storage
            .values
            .lock()
            .get(&store.storage_key.encode())
            .unwrap()
            .clone();
        assert_ne!(&first[6..HEADER_BYTES], &second[6..HEADER_BYTES]);
        drop(store);
        let mut tampered = first.clone();
        *tampered.last_mut().unwrap() ^= 1;
        let mut unknown = first.clone();
        unknown[4..6].copy_from_slice(&(VERSION + 1).to_le_bytes());
        for (root, genesis, key, bytes, error) in [
            (
                [8; 32],
                [2; 32],
                [3; 32],
                first.clone(),
                StoreError::Authentication,
            ),
            (
                [1; 32],
                [8; 32],
                [3; 32],
                first.clone(),
                StoreError::Authentication,
            ),
            ([1; 32], [2; 32], [8; 32], first, StoreError::Authentication),
            (
                [1; 32],
                [2; 32],
                [3; 32],
                tampered,
                StoreError::Authentication,
            ),
            ([1; 32], [2; 32], [3; 32], unknown, StoreError::Version),
            ([1; 32], [2; 32], [3; 32], Vec::new(), StoreError::Corrupt),
        ] {
            storage.values.lock().insert(
                CoreStorageKey::MainPurseCoinage {
                    root_public_key: root,
                    genesis_hash: genesis,
                }
                .encode(),
                bytes,
            );
            assert!(matches!(
                HostCoinageStore::open_storage(
                    storage.clone(),
                    root,
                    genesis,
                    Zeroizing::new(key),
                    crate::test_support::test_spawner(),
                    false,
                )
                .await,
                Err(observed) if observed == error
            ));
        }
    });
}

#[test]
fn authenticated_but_inconsistent_and_unbounded_snapshots_fail_closed() {
    block_on(async {
        let storage = Arc::new(DurableSlot::default());
        let store = open(storage.clone()).await;
        let mut inconsistent = Snapshot::default();
        inconsistent.coins.insert(0, coin(0)); // Missing high-water mark would reissue key zero.
        let encrypted = store.encrypt(&inconsistent).unwrap();
        storage
            .values
            .lock()
            .insert(store.storage_key.encode(), encrypted);
        drop(store);
        assert!(matches!(
            HostCoinageStore::open_storage(
                storage,
                [1; 32],
                [2; 32],
                Zeroizing::new([3; 32]),
                crate::test_support::test_spawner(),
                false,
            )
            .await,
            Err(StoreError::Corrupt)
        ));

        let mut excessive_rows = vec![0, 0, 0]; // Unbound asset and absent counters.
        ((MAX_ASSETS + 1) as u32).encode_to(&mut excessive_rows);
        assert!(matches!(
            codec::decode(&excessive_rows),
            Err(StoreError::Capacity)
        ));
        let mut duplicate = Vec::new();
        None::<Option<u32>>.encode_to(&mut duplicate);
        Some(0u32).encode_to(&mut duplicate);
        None::<u32>.encode_to(&mut duplicate);
        2u32.encode_to(&mut duplicate);
        for _ in 0..2 {
            0u32.encode_to(&mut duplicate);
            2i16.encode_to(&mut duplicate);
            Some(3i16).encode_to(&mut duplicate);
            CoinState::Available.as_raw().encode_to(&mut duplicate);
        }
        assert!(matches!(
            codec::decode(&duplicate),
            Err(StoreError::Corrupt)
        ));
        let mut trailing = codec::encode(&Snapshot::default(), 1).unwrap();
        trailing.push(0);
        assert!(matches!(codec::decode(&trailing), Err(StoreError::Corrupt)));
    });
}

#[test]
fn post_replacement_failure_blocks_stale_reads_and_writes_until_recovery() {
    block_on(async {
        let storage = Arc::new(DurableSlot::default());
        let store = open(storage.clone()).await;
        CoinRepository::upsert(&*store, &coin(0)).await.unwrap();
        VoucherRepository::upsert(&*store, &voucher(0))
            .await
            .unwrap();
        WalStore::save(&*store, &journal("guard")).await.unwrap();
        store.write_operation([7; 32], vec![1, 2, 3]).await.unwrap();
        store.reserve(&[0], &[0]).await.unwrap();
        storage.fail_after_replace.store(true, Ordering::SeqCst);
        assert!(store.commit(&[0], &[0], &[coin(1)], &[]).await.is_err());

        // The old published view cannot authorize reads or overwrite the new
        // snapshot while durability is unresolved, including "absent" queries.
        assert!(CoinRepository::list(&*store).await.is_err());
        assert!(VoucherRepository::list(&*store).await.is_err());
        assert!(WalStore::load_all(&*store).await.is_err());
        assert!(ClaimPlanStore::load_all(&*store).await.is_err());
        assert!(store.plan(&[9; 32]).await.is_err());
        assert!(store.current_index(IndexKind::Coin).await.is_err());
        assert_eq!(store.read_operation([7; 32]).await, Err(StoreError::Write));
        assert_eq!(store.read_operation([8; 32]).await, Err(StoreError::Write));
        assert!(store.list_operations().await.is_err());
        assert_eq!(
            store.write_operation([8; 32], vec![9]).await,
            Err(StoreError::Write)
        );
        assert!(store.get_next_index(IndexKind::Coin).await.is_err());
        assert!(store.revert(&[0], &[0]).await.is_err());

        // An unauthenticated replacement must not clear the failure latch.
        let replaced = storage
            .values
            .lock()
            .get(&store.storage_key.encode())
            .unwrap()
            .clone();
        let mut corrupted = replaced.clone();
        *corrupted.last_mut().unwrap() ^= 1;
        storage
            .values
            .lock()
            .insert(store.storage_key.encode(), corrupted);
        assert_eq!(
            store.reauthenticate().await,
            Err(StoreError::Authentication)
        );
        assert_eq!(store.read_operation([7; 32]).await, Err(StoreError::Write));
        storage
            .values
            .lock()
            .insert(store.storage_key.encode(), replaced);
        storage.fail_next.store(true, Ordering::SeqCst);
        assert_eq!(store.reauthenticate().await, Err(StoreError::Write));
        assert!(store.current_index(IndexKind::Coin).await.is_err());

        // Reauthentication must finish its durability repair even when its
        // caller disappears while the replacement is being written.
        let (entered_tx, entered_rx) = oneshot::channel();
        let (resume_tx, resume_rx) = oneshot::channel();
        *storage.pause_next.lock() = Some((entered_tx, resume_rx));
        let mut recovery = Box::pin(store.reauthenticate());
        assert!(futures::poll!(&mut recovery).is_pending());
        entered_rx.await.unwrap();
        drop(recovery);
        resume_tx.send(()).unwrap();

        let coins = CoinRepository::list(&*store).await.unwrap();
        assert_eq!(
            coins
                .iter()
                .map(|coin| (coin.derivation_index, coin.state))
                .collect::<Vec<_>>(),
            vec![(0, CoinState::Spent), (1, CoinState::Available)]
        );
        assert!(VoucherRepository::list(&*store).await.unwrap().is_empty());
        assert_eq!(
            store.read_operation([7; 32]).await.unwrap(),
            Some(vec![1, 2, 3])
        );
        assert_eq!(store.read_operation([8; 32]).await.unwrap(), None);
        assert_eq!(store.get_next_index(IndexKind::Coin).await.unwrap(), 2);
        drop(store);
        let restored = reopen_after_owned_work(storage).await;
        assert_eq!(CoinRepository::list(&*restored).await.unwrap(), coins);
        assert_eq!(
            restored.current_index(IndexKind::Coin).await.unwrap(),
            Some(2)
        );
        assert_eq!(
            restored.read_operation([7; 32]).await.unwrap(),
            Some(vec![1, 2, 3])
        );
    });
}

#[test]
fn live_owner_and_abandoned_owned_write_exclude_a_second_allocator() {
    block_on(async {
        let storage = Arc::new(DurableSlot::default());
        let store = open(storage.clone()).await;
        assert_eq!(store.get_next_index(IndexKind::Coin).await.unwrap(), 0);
        assert!(matches!(
            try_open(storage.clone()).await,
            Err(StoreError::Conflict)
        ));

        let (entered_tx, entered_rx) = oneshot::channel();
        let (resume_tx, resume_rx) = oneshot::channel();
        *storage.pause_next.lock() = Some((entered_tx, resume_rx));
        let mut allocate = Box::pin(store.get_next_index(IndexKind::Coin));
        assert!(futures::poll!(&mut allocate).is_pending());
        entered_rx.await.unwrap();
        drop(allocate);
        drop(store);
        assert!(matches!(
            try_open(storage.clone()).await,
            Err(StoreError::Conflict)
        ));
        resume_tx.send(()).unwrap();
        let restored = reopen_after_owned_work(storage).await;
        assert_eq!(restored.get_next_index(IndexKind::Coin).await.unwrap(), 2);
    });
}

#[test]
fn uncertain_owner_release_requires_durable_repair_and_never_initializes_absence() {
    block_on(async {
        let storage = Arc::new(DurableSlot::default());
        let store = open(storage.clone()).await;
        storage.fail_after_replace.store(true, Ordering::SeqCst);
        assert!(store.get_next_index(IndexKind::Coin).await.is_err());
        let key = store.storage_key.encode();
        let observed = storage.values.lock().get(&key).unwrap().clone();
        drop(store);

        storage.fail_next.store(true, Ordering::SeqCst);
        assert!(matches!(
            try_open(storage.clone()).await,
            Err(StoreError::Write)
        ));
        storage.values.lock().remove(&key);
        assert!(matches!(
            try_open(storage.clone()).await,
            Err(StoreError::Missing)
        ));
        assert!(matches!(
            try_open(storage.clone()).await,
            Err(StoreError::Missing)
        ));
        assert!(!storage.values.lock().contains_key(&key));
        storage.values.lock().insert(key, observed);
        let restored = open(storage).await;
        assert_eq!(restored.get_next_index(IndexKind::Coin).await.unwrap(), 1);
    });
}

#[test]
fn uncertain_first_write_cannot_be_reset_by_dropping_its_owner() {
    block_on(async {
        let storage = Arc::new(DurableSlot::default());
        let store = open(storage.clone()).await;
        storage.fail_next.store(true, Ordering::SeqCst);
        assert!(store.get_next_index(IndexKind::Coin).await.is_err());
        assert_eq!(store.reauthenticate().await, Err(StoreError::Missing));
        drop(store);
        assert!(matches!(
            try_open(storage.clone()).await,
            Err(StoreError::Missing)
        ));
        assert!(storage.values.lock().is_empty());
    });
}

#[test]
fn scan_batch_atomically_advances_progress_without_releasing_reservations() {
    block_on(async {
        let storage = Arc::new(DurableSlot::default());
        let store = open(storage.clone()).await;
        CoinRepository::upsert(&*store, &coin(5)).await.unwrap();
        store.reserve(&[5], &[]).await.unwrap();
        store
            .commit_inventory_batch(
                [4; 32],
                None,
                vec![1],
                vec![coin(5), coin(6)],
                Vec::new(),
                Some(6),
                None,
            )
            .await
            .unwrap();
        // A stale scan cannot import a row or move the allocation counter.
        assert_eq!(
            store
                .commit_inventory_batch(
                    [4; 32],
                    None,
                    vec![2],
                    vec![coin(9)],
                    Vec::new(),
                    Some(9),
                    None
                )
                .await,
            Err(StoreError::Conflict)
        );
        drop(store);
        let restored = open(storage).await;
        assert_eq!(
            restored.read_operation([4; 32]).await.unwrap(),
            Some(vec![1])
        );
        assert_eq!(
            CoinRepository::list(&*restored)
                .await
                .unwrap()
                .iter()
                .map(|coin| (coin.derivation_index, coin.state))
                .collect::<Vec<_>>(),
            vec![(5, CoinState::PendingTransfer), (6, CoinState::Available)]
        );
        assert_eq!(restored.get_next_index(IndexKind::Coin).await.unwrap(), 7);
    });
}

#[test]
fn operation_receipt_progress_preserves_identity_and_survives_restart() {
    block_on(async {
        let storage = Arc::new(DurableSlot::default());
        let store = open(storage.clone()).await;
        let mut receipt = TransferWalEntry {
            entry_id: truapi_coinage::wal::operation_entry_id("payment", "parent"),
            operation: WalOperation::TransferPrepared,
            payload: WalPayload {
                output_coins: vec![WalCoinRef {
                    derivation_index: 5,
                    exponent: 2,
                }],
                ..WalPayload::default()
            },
            checkpoint: CheckpointBlock::Pending,
            created_at_ms: 10,
        };
        WalStore::save(&*store, &receipt).await.unwrap();
        receipt.operation = WalOperation::TransferAccepted;
        WalStore::save(&*store, &receipt).await.unwrap();
        let mut rebound = receipt.clone();
        rebound.payload.output_coins[0].derivation_index = 6;
        assert!(WalStore::save(&*store, &rebound).await.is_err());
        receipt.operation = WalOperation::TransferCompleted;
        WalStore::save(&*store, &receipt).await.unwrap();
        drop(store);
        let restored = open(storage).await;
        assert_eq!(
            restored.load_operation("payment").await.unwrap(),
            vec![receipt.clone()]
        );
        receipt.operation = WalOperation::TransferPrepared;
        assert!(WalStore::save(&*restored, &receipt).await.is_err());
    });
}
