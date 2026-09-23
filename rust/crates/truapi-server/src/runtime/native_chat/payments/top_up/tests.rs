// SPDX-License-Identifier: AGPL-3.0-only
use super::super::tests::{context, source};
use super::*;
use crate::test_support::StubPlatform;
use futures::executor::block_on;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use truapi_coinage::{
    CoinAllocator, CoinRepository, CoinageIndexStore, DenominationBreakdownContext,
    ExternalCoinTransferBackend, ExternalCoinTransferRequest, ExternalSecretClaimService,
    IndexKind, OnChainCoin,
};

fn request(entries: &[MemoEntry], amount: u128) -> HostPaymentTopUpRequest {
    HostPaymentTopUpRequest {
        into: None,
        amount,
        source: PaymentTopUpSource::Coins {
            sr25519_secret_keys: entries.iter().map(|entry| entry.0).collect(),
        },
    }
}

fn offline_context() -> NativeChatContext {
    context(Arc::new(StubPlatform {
        chain_connect_error: Some("offline"),
        ..Default::default()
    }))
}

#[test]
fn top_up_rejects_duplicate_invalid_and_over_limit_sources_before_custody() {
    block_on(async {
        let context = offline_context();
        let wallet = WalletCoinage::open(&context).await.unwrap();
        let key = source(40);
        for entries in [
            vec![],
            vec![key.clone(), key.clone()],
            vec![MemoEntry([255; 64])],
            vec![key; MAX_SOURCES + 1],
        ] {
            assert_eq!(
                wallet
                    .top_up(&context, "chat.dot", request(&entries, 0))
                    .await,
                Err(TopUpError::InvalidSource)
            );
        }
        let mut unsupported = request(&[source(40)], 0);
        unsupported.into = Some(7);
        assert!(matches!(
            wallet.top_up(&context, "chat.dot", unsupported).await,
            Err(TopUpError::Unknown { .. })
        ));
        let unsupported = HostPaymentTopUpRequest {
            into: None,
            amount: 1,
            source: PaymentTopUpSource::PrivateKey {
                sr25519_secret_key: source(40).0,
            },
        };
        assert_eq!(
            wallet.top_up(&context, "chat.dot", unsupported).await,
            Err(TopUpError::InvalidSource)
        );
        assert!(wallet.store.list_operations().await.unwrap().is_empty());
        assert_eq!(
            wallet.store.current_index(IndexKind::Coin).await.unwrap(),
            None
        );
    });
}

#[test]
fn top_up_caller_drop_keeps_owned_custody_and_reordered_offline_retry() {
    block_on(async {
        let context = offline_context();
        let wallet = WalletCoinage::open(&context).await.unwrap();
        let mut caller_context = context.clone();
        let queued = Arc::new(parking_lot::Mutex::new(Vec::new()));
        let capture = queued.clone();
        caller_context.services = crate::runtime::services::RuntimeServices::new(
            context.services.platform.clone(),
            context.services.host_info.clone(),
            [2; 32],
            [3; 32],
            [4; 32],
            Arc::new(move |job| capture.lock().push(job)),
        );
        let entries = vec![source(41), source(42)];
        let mut call = Box::pin(wallet.top_up(&caller_context, "chat.dot", request(&entries, 0)));
        assert!(futures::poll!(call.as_mut()).is_pending());
        drop(call);
        let job = queued.lock().pop().unwrap();
        job.await;
        let operations = wallet.top_ups().await.unwrap();
        assert_eq!(operations.len(), 1);
        let original = operations[0].memo().unwrap().identifier();
        drop(operations);
        drop(wallet);
        let wallet = WalletCoinage::open(&context).await.unwrap();
        let reversed = vec![entries[1].clone(), entries[0].clone()];
        assert!(matches!(
            wallet
                .top_up(&context, "chat.dot", request(&reversed, 80))
                .await,
            Err(TopUpError::Unknown { .. })
        ));
        let operations = wallet.top_ups().await.unwrap();
        assert_eq!(operations.len(), 1);
        assert_eq!(operations[0].memo().unwrap().identifier(), original);
        assert_eq!(
            wallet.store.current_index(IndexKind::Coin).await.unwrap(),
            None
        );
        assert!(wallet.views("chat.dot").await.unwrap().is_empty());
        wallet.reconcile(&context).await.unwrap();
        assert!(wallet.reconcile_top_ups(&context).await.is_err());
    });
}

// Emulates an uncertain submission at the second source. Its destination can
// appear later, but no successful response or destination evidence is invented.
struct AmbiguousChain {
    rows: parking_lot::Mutex<BTreeMap<[u8; 32], OnChainCoin>>,
    hidden: parking_lot::Mutex<Option<([u8; 32], OnChainCoin)>>,
    transfers: AtomicUsize,
    valid: Arc<AtomicBool>,
    invalidate: AtomicBool,
}

#[async_trait]
impl ExternalCoinTransferBackend for AmbiguousChain {
    async fn denomination_context(&self) -> Result<DenominationBreakdownContext, String> {
        Ok(DenominationBreakdownContext {
            asset_unit: 10,
            min_exponent: 0,
            max_exponent: 8,
            precision: 2,
        })
    }

    async fn fetch_coins(&self, public: &[[u8; 32]]) -> Result<Vec<Option<OnChainCoin>>, String> {
        let rows = self.rows.lock();
        Ok(public.iter().map(|key| rows.get(key).copied()).collect())
    }

    async fn submit_transfer(
        &self,
        request: ExternalCoinTransferRequest,
    ) -> Result<OnChainCoin, String> {
        if !self.valid.load(Ordering::SeqCst) {
            return Err("session expired".into());
        }
        let count = self.transfers.fetch_add(1, Ordering::SeqCst);
        let mut rows = self.rows.lock();
        let landed = rows
            .remove(&request.source_public)
            .ok_or("source missing")?;
        if self.invalidate.load(Ordering::SeqCst) {
            self.valid.store(false, Ordering::SeqCst);
        }
        if count == 1 {
            *self.hidden.lock() = Some((request.recipient, landed));
            return Err("submission outcome unknown".into());
        }
        rows.insert(request.recipient, landed);
        Ok(landed)
    }
}

fn claimer(
    context: &NativeChatContext,
    wallet: &WalletCoinage,
    chain: Arc<AmbiguousChain>,
) -> ExternalSecretClaimService {
    ExternalSecretClaimService::new(
        &context.entropy,
        Arc::new(CoinAllocator::new(wallet.store.clone())),
        wallet.store.clone(),
        wallet.store.clone(),
        chain,
    )
}

#[test]
fn top_up_ambiguous_retry_preserves_plan_and_credit_across_restart() {
    block_on(async {
        let context = offline_context();
        let wallet = WalletCoinage::open(&context).await.unwrap();
        let memo = TransferMemo {
            entries: vec![source(43), source(44)],
            total_value: 60,
        };
        let public = memo_public(&memo).unwrap();
        let operation = CoinTopUp {
            product_id: "chat.dot".into(),
            source_public: public.clone(),
            denominations: Some((10, 0, 8, 2)),
            memo: memo.scale_encoded(),
        };
        wallet.save_top_up(&operation).await.unwrap();
        let chain = Arc::new(AmbiguousChain {
            rows: parking_lot::Mutex::new(BTreeMap::from([
                (
                    public[0],
                    OnChainCoin {
                        exponent: 1,
                        age: 0,
                    },
                ),
                (
                    public[1],
                    OnChainCoin {
                        exponent: 2,
                        age: 0,
                    },
                ),
            ])),
            hidden: parking_lot::Mutex::new(None),
            transfers: AtomicUsize::new(0),
            valid: Arc::new(AtomicBool::new(true)),
            invalidate: AtomicBool::new(false),
        });
        let engine = claimer(&context, &wallet, chain.clone());
        assert_eq!(
            wallet.claim_top_up(&context, &operation, &engine).await,
            Err(Error::NetworkUnavailable)
        );
        let plan = wallet
            .store
            .plan(&memo.identifier())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(plan.claimed_amount, Some(20));
        assert_eq!(plan.status, ClaimPlanStatus::Error);
        let index = wallet.store.current_index(IndexKind::Coin).await.unwrap();
        // The finalized first destination is subsequently spent: its persisted
        // prefix remains credited even though both its keys are now absent.
        chain.rows.lock().clear();
        drop(engine);
        drop(wallet);
        let wallet = WalletCoinage::open(&context).await.unwrap();
        let reversed = TransferMemo {
            entries: vec![memo.entries[1].clone(), memo.entries[0].clone()],
            total_value: 0,
        };
        let replay = wallet
            .admit_top_up(&context, "chat.dot", reversed, vec![public[1], public[0]])
            .await
            .unwrap();
        let engine = claimer(&context, &wallet, chain.clone());
        assert_eq!(
            wallet.claim_top_up(&context, &replay, &engine).await,
            Err(Error::NetworkUnavailable)
        );
        assert_eq!(chain.transfers.load(Ordering::SeqCst), 2);
        let (recipient, landed) = chain.hidden.lock().take().unwrap();
        chain.rows.lock().insert(recipient, landed);
        assert_eq!(
            wallet.claim_top_up(&context, &replay, &engine).await,
            Ok(60)
        );
        assert_eq!(chain.transfers.load(Ordering::SeqCst), 2);
        assert_eq!(
            wallet.store.current_index(IndexKind::Coin).await.unwrap(),
            index
        );
        assert_eq!(
            wallet
                .store
                .plan(&memo.identifier())
                .await
                .unwrap()
                .unwrap()
                .entries,
            plan.entries
        );
        assert_eq!(
            ClaimPlanStore::load_all(wallet.store.as_ref())
                .await
                .unwrap()
                .len(),
            1
        );
        let entries = vec![memo.entries[1].clone(), memo.entries[0].clone()];
        assert_eq!(
            wallet
                .top_up(&context, "chat.dot", request(&entries, 80))
                .await,
            Err(TopUpError::PartialPayment { credited: 60 })
        );
        assert_eq!(
            wallet
                .top_up(&context, "chat.dot", request(&entries, 60))
                .await,
            Ok(())
        );
        let mut all = request(&entries, 0);
        all.into = Some(truapi::v01::MAIN_PURSE);
        assert_eq!(wallet.top_up(&context, "chat.dot", all).await, Ok(()));
        let overlap = vec![entries[0].clone(), source(45)];
        assert_eq!(
            wallet
                .top_up(&context, "chat.dot", request(&overlap, 0))
                .await,
            Err(TopUpError::InvalidSource)
        );
        assert_eq!(
            wallet
                .top_up(&context, "other.dot", request(&entries, 0))
                .await,
            Err(TopUpError::InvalidSource)
        );
    });
}

#[test]
fn top_up_session_invalidation_preserves_finalized_prefix_without_next_effect() {
    block_on(async {
        let mut context = offline_context();
        let valid = Arc::new(AtomicBool::new(true));
        let validity = valid.clone();
        context.session_valid = Arc::new(move || validity.load(Ordering::SeqCst));
        let wallet = WalletCoinage::open(&context).await.unwrap();
        let memo = TransferMemo {
            entries: vec![source(46), source(47)],
            total_value: 60,
        };
        let public = memo_public(&memo).unwrap();
        let operation = CoinTopUp {
            product_id: "chat.dot".into(),
            source_public: public.clone(),
            denominations: Some((10, 0, 8, 2)),
            memo: memo.scale_encoded(),
        };
        wallet.save_top_up(&operation).await.unwrap();
        let chain = Arc::new(AmbiguousChain {
            rows: parking_lot::Mutex::new(BTreeMap::from([
                (
                    public[0],
                    OnChainCoin {
                        exponent: 1,
                        age: 0,
                    },
                ),
                (
                    public[1],
                    OnChainCoin {
                        exponent: 2,
                        age: 0,
                    },
                ),
            ])),
            hidden: parking_lot::Mutex::new(None),
            transfers: AtomicUsize::new(0),
            valid,
            invalidate: AtomicBool::new(true),
        });
        let engine = claimer(&context, &wallet, chain.clone());
        assert_eq!(
            wallet.claim_top_up(&context, &operation, &engine).await,
            Err(Error::NotConnected)
        );
        assert_eq!(chain.transfers.load(Ordering::SeqCst), 1);
        assert_eq!(
            wallet
                .store
                .plan(&memo.identifier())
                .await
                .unwrap()
                .unwrap()
                .claimed_amount,
            Some(20)
        );
        assert_eq!(
            CoinRepository::list(wallet.store.as_ref())
                .await
                .unwrap()
                .len(),
            1
        );
    });
}

#[test]
fn top_up_reuses_legacy_incoming_custody_through_finality() {
    block_on(async {
        let context = offline_context();
        let wallet = WalletCoinage::open(&context).await.unwrap();
        let memo = TransferMemo {
            entries: vec![source(48), source(49)],
            total_value: 60,
        };
        let public = memo_public(&memo).unwrap();
        let mut native = new_outgoing(
            [98; 32],
            context.genesis_hash,
            PaymentIntent {
                product_id: "chat.dot".into(),
                peer_identity: [7; 32],
                recipient_username: None,
                request_id: "native".into(),
                amount_cents: 6,
            },
        );
        native.phase = Phase::Incoming;
        native.card.direction = Direction::Incoming;
        native.card.state = State::Claiming;
        native.denominations = Some((10, 0, 8, 2));
        native.source_public = public.clone();
        native.source_exponents = vec![None; 2];
        native.source_seen = vec![false; 2];
        native.source_cleared = vec![false; 2];
        native.memo_key = Some(memo.identifier());
        native.source_fingerprint = Some(source_fingerprint(&public));
        native.memo = memo.scale_encoded();
        wallet.save(&native).await.unwrap();
        let incoming = TransferMemo {
            entries: vec![memo.entries[1].clone(), memo.entries[0].clone()],
            total_value: 0,
        };
        let operation = wallet
            .admit_top_up(&context, "chat.dot", incoming, vec![public[1], public[0]])
            .await
            .unwrap();
        let chain = Arc::new(AmbiguousChain {
            rows: parking_lot::Mutex::new(BTreeMap::from([
                (
                    public[0],
                    OnChainCoin {
                        exponent: 1,
                        age: 0,
                    },
                ),
                (
                    public[1],
                    OnChainCoin {
                        exponent: 2,
                        age: 0,
                    },
                ),
            ])),
            hidden: parking_lot::Mutex::new(None),
            transfers: AtomicUsize::new(0),
            valid: Arc::new(AtomicBool::new(true)),
            invalidate: AtomicBool::new(false),
        });
        let engine = claimer(&context, &wallet, chain.clone());
        assert!(
            wallet
                .claim_top_up(&context, &operation, &engine)
                .await
                .is_err()
        );
        let (recipient, landed) = chain.hidden.lock().take().unwrap();
        chain.rows.lock().insert(recipient, landed);
        assert_eq!(
            wallet.claim_top_up(&context, &operation, &engine).await,
            Ok(60)
        );
        assert_eq!(
            wallet
                .top_up(&context, "chat.dot", request(&memo.entries, 0))
                .await,
            Ok(())
        );
        assert_eq!(chain.transfers.load(Ordering::SeqCst), 2);
        assert_eq!(
            ClaimPlanStore::load_all(wallet.store.as_ref())
                .await
                .unwrap()
                .len(),
            1
        );
    });
}

#[test]
fn wallet_recovery_without_existing_purse_does_not_activate_allocator_or_chat() {
    block_on(async {
        let platform = Arc::new(StubPlatform {
            remote_permission_denied: true,
            chain_connect_error: Some("unlock must not connect"),
            ..Default::default()
        });
        let mut context = context(platform.clone());
        let queued = Arc::new(parking_lot::Mutex::new(Vec::new()));
        let capture = queued.clone();
        context.services = crate::runtime::services::RuntimeServices::new(
            platform.clone(),
            context.services.host_info.clone(),
            [2; 32],
            [3; 32],
            [4; 32],
            Arc::new(move |job| capture.lock().push(job)),
        );
        let registry = crate::runtime::native_chat::NativeChatRegistry::default();

        // A wallet unlock alone has neither product metadata nor Chat grants.
        // Drive the actual worker, rather than calling the purse directly.
        registry.resume_wallet_recovery(context);
        let mut worker = queued
            .lock()
            .pop()
            .expect("wallet unlock schedules recovery");
        assert!(
            futures::poll!(worker.as_mut()).is_ready(),
            "an absent purse must stop recovery without opening an allocator or retrying"
        );
        assert!(
            platform.local_storage.lock().unwrap().is_empty(),
            "wallet unlock must not create purse, device, product, or permission storage"
        );
        assert!(platform.chain_connects.lock().unwrap().is_empty());
        assert!(platform.sent_rpc.lock().unwrap().is_empty());
        assert!(
            platform
                .remote_permission_requests
                .lock()
                .unwrap()
                .is_empty()
        );
        assert!(platform.chat_authority_reviews.lock().is_empty());
        assert!(queued.lock().is_empty());
        registry.release();
    });
}
