// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use crate::{
    runtime::{authority::AuthoritySession, services::RuntimeServices},
    test_support::{StubPlatform, test_spawner},
};
use futures::executor::block_on;
use std::sync::atomic::{AtomicBool, Ordering};
use truapi_coinage::{CoinageIndexStore, IndexKind};

pub(super) fn context(platform: Arc<StubPlatform>) -> NativeChatContext {
    NativeChatContext {
        services: RuntimeServices::new(
            platform,
            truapi_platform::HostInfo {
                name: "Payment test".into(),
                icon: None,
                version: None,
                platform: latest::HostPlatform::Unknown,
            },
            [2; 32],
            [3; 32],
            [4; 32],
            test_spawner(),
        ),
        session: AuthoritySession {
            public_key: [1; 32],
            identity_account_id: Some([5; 32]),
            lite_username: None,
            full_username: None,
            validation_id: vec![1],
        },
        entropy: Zeroizing::new(vec![0x44; 16]),
        session_valid: Arc::new(|| true),
        network_suffix: "test".into(),
        genesis_hash: [2; 32],
        coinage_instance_id: None,
    }
}

fn intent() -> PaymentIntent {
    PaymentIntent {
        product_id: "chat.dot".into(),
        peer_identity: [7; 32],
        recipient_username: Some("alice.dot".into()),
        request_id: "request-1".into(),
        amount_cents: 25,
    }
}

struct NoTransport;
#[async_trait]
impl PaymentTransport for NoTransport {
    async fn accept(&self, _: &HostNativeChatPayment, _: TransferMemo) -> Result<(), ()> {
        panic!("rejected or conflicting operation must not reach transport")
    }
}

#[test]
fn cached_wallet_rejects_asset_retargeting_before_review_or_chain_effects() {
    block_on(async {
        let platform = Arc::new(StubPlatform::default());
        let mut context = context(platform);
        context.coinage_instance_id = Some(0);
        let wallet = WalletCoinage::open(&context).await.unwrap();
        context.coinage_instance_id = Some(1);
        assert_eq!(
            wallet.send(&context, intent(), Arc::new(NoTransport)).await,
            Err(Error::OperationConflict)
        );
        drop(wallet);
        assert!(matches!(
            WalletCoinage::open(&context).await,
            Err(Error::OperationConflict)
        ));
    });
}

#[test]
fn durable_denial_and_conflicting_body_never_reach_chain_or_transport() {
    block_on(async {
        let platform = Arc::new(StubPlatform {
            chain_connect_error: Some("offline"),
            ..Default::default()
        });
        let context = context(platform.clone());
        let wallet = WalletCoinage::open(&context).await.unwrap();
        assert_eq!(
            wallet.reconcile(&context).await,
            Ok(()),
            "ordinary Chat must work without a Coinage RPC"
        );
        let intent = intent();
        let id = operation_id([1; 32], [2; 32], &intent.product_id, &intent.request_id);
        let mut operation = new_outgoing(id, [2; 32], intent.clone());
        operation.phase = Phase::Denied;
        operation.card.state = State::Failed {
            reason: Failure::Cancelled,
        };
        wallet.save(&operation).await.unwrap();
        drop(wallet);
        let restarted = WalletCoinage::open(&context).await.unwrap();
        assert_eq!(
            restarted
                .send(&context, intent.clone(), Arc::new(NoTransport))
                .await,
            Err(Error::UserRejected)
        );
        let mut changed = intent.clone();
        changed.amount_cents += 1;
        assert_eq!(
            restarted
                .send(&context, changed, Arc::new(NoTransport))
                .await,
            Err(Error::OperationConflict)
        );
        assert_eq!(
            restarted.reconcile(&context).await,
            Ok(()),
            "denied payments cannot require chain recovery"
        );
        assert_eq!(
            restarted
                .store
                .current_index(IndexKind::Coin)
                .await
                .unwrap(),
            None
        );
        assert!(
            WalStore::load_all(restarted.store.as_ref())
                .await
                .unwrap()
                .is_empty()
        );
        assert!(platform.chain_connects.lock().unwrap().is_empty());
        assert!(
            platform
                .main_purse_chat_payment_reviews
                .lock()
                .unwrap()
                .is_empty()
        );
    });
}

#[test]
fn operation_identity_rejects_cross_product_peer_network_and_amount_rebinding() {
    let original = intent();
    let id = operation_id([1; 32], [2; 32], &original.product_id, &original.request_id);
    let operation = new_outgoing(id, [2; 32], original.clone());
    for changed in [
        PaymentIntent {
            peer_identity: [8; 32],
            ..original.clone()
        },
        PaymentIntent {
            amount_cents: 26,
            ..original.clone()
        },
        PaymentIntent {
            product_id: "other.dot".into(),
            ..original.clone()
        },
    ] {
        assert!(!operation.matches(&changed, [2; 32]));
    }
    assert!(!operation.matches(&original, [3; 32]));
    assert!(operation.matches(&original, [2; 32]));
    let renamed = PaymentIntent {
        recipient_username: Some("renamed.dot".into()),
        ..original
    };
    assert!(operation.matches(&renamed, [2; 32]));
}

struct LogoutOnReview {
    valid: Arc<AtomicBool>,
    expected: MainPurseChatPaymentReview,
}
#[async_trait]
impl truapi_platform::UserConfirmation for LogoutOnReview {
    async fn confirm_user_action(
        &self,
        review: UserConfirmationReview,
    ) -> Result<bool, latest::GenericError> {
        assert_eq!(
            review,
            UserConfirmationReview::MainPurseChatPayment(self.expected.clone())
        );
        self.valid.store(false, Ordering::SeqCst);
        Ok(true)
    }
}

#[test]
fn exact_review_is_never_reused_across_logout_and_denial_is_not_approval() {
    block_on(async {
        let mut operation = new_outgoing([9; 32], [2; 32], intent());
        operation.max_debit_cents = 28;
        let valid = Arc::new(AtomicBool::new(true));
        let platform = LogoutOnReview {
            valid: valid.clone(),
            expected: MainPurseChatPaymentReview {
                calling_product_id: "chat.dot".into(),
                recipient_identity: [7; 32],
                recipient_username: Some("alice.dot".into()),
                amount_cents: 25,
                max_debit_cents: 28,
                genesis_hash: [2; 32],
                coinage_instance_id: None,
                operation_id: [9; 32],
            },
        };
        let live = move || valid.load(Ordering::SeqCst);
        assert_eq!(
            review_operation(&platform, &live, &operation, None).await,
            Err(Error::NotConnected)
        );
        let denied = StubPlatform::default();
        assert_eq!(
            review_operation(&denied, &|| true, &operation, None).await,
            Ok(false)
        );
    });
}

pub(super) fn source(seed: u8) -> MemoEntry {
    MemoEntry(
        schnorrkel::MiniSecretKey::from_bytes(&[seed; 32])
            .unwrap()
            .expand(schnorrkel::ExpansionMode::Ed25519)
            .to_bytes(),
    )
}

#[test]
fn delivery_ack_never_claims_settlement_or_overwrites_partial_clearing() {
    block_on(async {
        let context = context(Arc::new(StubPlatform {
            chain_connect_error: Some("offline"),
            ..Default::default()
        }));
        let wallet = WalletCoinage::open(&context).await.unwrap();
        let mut operation = new_outgoing([9; 32], [2; 32], intent());
        operation.phase = Phase::Accepted;
        operation.card.state = State::PartiallyCleared { cleared_cents: 10 };
        wallet.save(&operation).await.unwrap();
        assert_eq!(
            wallet.note_delivery(&context, "other.dot", [9; 32]).await,
            Err(Error::OperationNotFound)
        );
        wallet
            .note_delivery(&context, "chat.dot", [9; 32])
            .await
            .unwrap();
        assert_eq!(
            wallet.views("chat.dot").await.unwrap()[0].state,
            State::PartiallyCleared { cleared_cents: 10 }
        );
        assert!(wallet.views("other.dot").await.unwrap().is_empty());
        operation.card.state = State::Delivering;
        wallet.save(&operation).await.unwrap();
        wallet
            .note_delivery(&context, "chat.dot", [9; 32])
            .await
            .unwrap();
        drop(wallet);
        let restored = WalletCoinage::open(&context).await.unwrap();
        assert_eq!(
            restored.views("chat.dot").await.unwrap()[0].state,
            State::Delivered
        );
    });
}

fn accepted_outgoing(request_id: &str) -> Operation {
    let intent = PaymentIntent {
        request_id: request_id.into(),
        ..intent()
    };
    let id = operation_id([1; 32], [2; 32], &intent.product_id, &intent.request_id);
    let mut operation = new_outgoing(id, [2; 32], intent);
    let memo = TransferMemo {
        entries: vec![source(12), source(13), source(14)],
        total_value: 250,
    };
    let public = memo_public(&memo).unwrap();
    operation.phase = Phase::Accepted;
    operation.card.state = State::Delivering;
    operation.max_debit_cents = 25;
    operation.denominations = Some((10, 0, 8, 2));
    operation.memo_key = Some(memo.identifier());
    operation.source_fingerprint = Some(source_fingerprint(&public));
    operation.source_exponents = vec![Some(4), Some(3), Some(0)];
    operation.source_seen = vec![true; 3];
    operation.source_cleared = vec![false; 3];
    operation.source_public = public;
    operation.detection_anchor = Some([8; 32]);
    operation.memo = memo.scale_encoded();
    operation
}

// Actor integration fixtures seed custody and its post-transport crash window.
// Subsequent delivery, encrypted restore, and memo replay use the actual wallet API.
impl WalletCoinage {
    pub(in crate::runtime::native_chat) async fn seed_accepted_for_test(
        &self,
        context: &NativeChatContext,
        intent: PaymentIntent,
        timestamp: u64,
        memo: &TransferMemo,
    ) -> Result<HostNativeChatPayment, Error> {
        self.check(context)?;
        let id = operation_id(
            self.root_public_key,
            self.genesis_hash,
            &intent.product_id,
            &intent.request_id,
        );
        let mut operation = new_outgoing(id, self.genesis_hash, intent);
        let public = memo_public(memo)?;
        operation.phase = Phase::Accepted;
        operation.card.state = State::Delivering;
        operation.card.timestamp = timestamp;
        operation.max_debit_cents = operation.card.amount_cents;
        operation.denominations = Some((10, 0, 8, 2));
        operation.memo_key = Some(memo.identifier());
        operation.source_fingerprint = Some(source_fingerprint(&public));
        operation.source_exponents = vec![None; public.len()];
        operation.source_seen = vec![true; public.len()];
        operation.source_cleared = vec![false; public.len()];
        operation.source_public = public;
        operation.detection_anchor = Some([8; 32]);
        operation.memo = memo.scale_encoded();
        self.stored_memo(&operation)?;
        self.save(&operation).await?;
        Ok(operation.card.clone())
    }
}

#[derive(Default)]
struct ReplayTransport {
    attempts: parking_lot::Mutex<Vec<(HostNativeChatPayment, Zeroizing<Vec<u8>>)>>,
    reject: AtomicBool,
}

#[async_trait]
impl PaymentTransport for ReplayTransport {
    async fn accept(&self, card: &HostNativeChatPayment, memo: TransferMemo) -> Result<(), ()> {
        self.attempts
            .lock()
            .push((card.clone(), Zeroizing::new(memo.scale_encoded())));
        if self.reject.load(Ordering::SeqCst) {
            Err(())
        } else {
            Ok(())
        }
    }
}

async fn save_receipt(
    wallet: &WalletCoinage,
    operation: &Operation,
    kind: truapi_coinage::WalOperation,
) {
    WalStore::save(
        wallet.store.as_ref(),
        &truapi_coinage::TransferWalEntry {
            entry_id: truapi_coinage::wal::operation_entry_id(
                &hex::encode(operation.card.operation_id),
                "parent",
            ),
            operation: kind,
            payload: truapi_coinage::WalPayload::default(),
            checkpoint: truapi_coinage::CheckpointBlock::Pending,
            created_at_ms: 1,
        },
    )
    .await
    .unwrap();
}

#[test]
fn accepted_replay_survives_transport_failure_and_restart_without_new_payment_or_delivery() {
    block_on(async {
        let platform = Arc::new(StubPlatform {
            chain_connect_error: Some("offline"),
            ..Default::default()
        });
        let context = context(platform.clone());
        let wallet = WalletCoinage::open(&context).await.unwrap();
        let operation = accepted_outgoing("accepted-replay");
        wallet.save(&operation).await.unwrap();
        save_receipt(
            &wallet,
            &operation,
            truapi_coinage::WalOperation::TransferAccepted,
        )
        .await;
        let reserved = truapi_coinage::Coin {
            exponent: 5,
            derivation_index: 42,
            age: Some(0),
            state: truapi_coinage::CoinState::PendingTransfer,
        };
        truapi_coinage::CoinRepository::upsert(wallet.store.as_ref(), &reserved)
            .await
            .unwrap();
        let transport = Arc::new(ReplayTransport::default());
        transport.reject.store(true, Ordering::SeqCst);
        assert_eq!(
            wallet
                .redeliver(
                    &context,
                    "chat.dot",
                    operation.card.operation_id,
                    transport.clone()
                )
                .await,
            Err(Error::NetworkUnavailable)
        );
        assert_eq!(
            wallet
                .pending_handoffs(&context, "chat.dot", &[])
                .await
                .unwrap(),
            vec![operation.card.clone()]
        );
        assert_eq!(
            truapi_coinage::CoinRepository::list(wallet.store.as_ref())
                .await
                .unwrap(),
            vec![reserved.clone()]
        );
        drop(wallet);
        let restarted = WalletCoinage::open(&context).await.unwrap();
        transport.reject.store(false, Ordering::SeqCst);
        restarted
            .redeliver(
                &context,
                "chat.dot",
                operation.card.operation_id,
                transport.clone(),
            )
            .await
            .unwrap();
        // Durable custody by the replacement transport is not a peer ACK.
        assert_eq!(
            restarted
                .pending_handoffs(&context, "chat.dot", &[])
                .await
                .unwrap(),
            vec![operation.card.clone()]
        );
        assert_eq!(
            restarted.views("chat.dot").await.unwrap(),
            vec![operation.card.clone()]
        );
        assert_eq!(
            truapi_coinage::CoinRepository::list(restarted.store.as_ref())
                .await
                .unwrap(),
            vec![reserved]
        );
        let attempts = transport.attempts.lock();
        assert_eq!(attempts.len(), 2);
        for (card, bytes) in attempts.iter() {
            assert_eq!(card, &operation.card);
            assert!(
                bytes.as_slice() == operation.memo.as_slice(),
                "replay must retain the exact native memo"
            );
        }
        assert!(platform.chain_connects.lock().unwrap().is_empty());
        assert!(
            platform
                .main_purse_chat_payment_reviews
                .lock()
                .unwrap()
                .is_empty()
        );
    });
}

#[test]
fn completed_send_retry_repairs_transport_custody_offline_without_review() {
    block_on(async {
        let platform = Arc::new(StubPlatform {
            chain_connect_error: Some("offline"),
            ..Default::default()
        });
        let context = context(platform.clone());
        let wallet = WalletCoinage::open(&context).await.unwrap();
        // The core receipt survived, but the Host's post-acceptance write did not.
        let mut operation = accepted_outgoing("completed-retry");
        operation.phase = Phase::HandoffReady;
        wallet.save(&operation).await.unwrap();
        save_receipt(
            &wallet,
            &operation,
            truapi_coinage::WalOperation::TransferCompleted,
        )
        .await;
        assert!(
            wallet
                .pending_handoffs(&context, "chat.dot", &[])
                .await
                .unwrap()
                .is_empty()
        );
        let transport = Arc::new(ReplayTransport::default());
        let retry = PaymentIntent {
            request_id: operation.card.request_id.clone(),
            ..intent()
        };
        let card = wallet
            .send(&context, retry, transport.clone())
            .await
            .unwrap();
        assert_eq!(card, operation.card);
        {
            let attempts = transport.attempts.lock();
            assert_eq!(attempts.len(), 1);
            assert!(attempts[0].1.as_slice() == operation.memo.as_slice());
        }
        assert_eq!(
            wallet
                .pending_handoffs(&context, "chat.dot", &[])
                .await
                .unwrap(),
            vec![card]
        );
        assert!(platform.chain_connects.lock().unwrap().is_empty());
        assert!(
            platform
                .main_purse_chat_payment_reviews
                .lock()
                .unwrap()
                .is_empty()
        );
    });
}

#[test]
fn replay_rejects_other_product_wallet_network_and_expired_session() {
    block_on(async {
        let context = context(Arc::new(StubPlatform::default()));
        let wallet = WalletCoinage::open(&context).await.unwrap();
        let operation = accepted_outgoing("scoped-replay");
        wallet.save(&operation).await.unwrap();
        assert!(
            wallet
                .pending_handoffs(&context, "other.dot", &[])
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            wallet
                .redeliver(
                    &context,
                    "other.dot",
                    operation.card.operation_id,
                    Arc::new(NoTransport)
                )
                .await,
            Err(Error::OperationNotFound)
        );
        let mut other_wallet = context.clone();
        other_wallet.session.public_key = [9; 32];
        let mut other_network = context.clone();
        other_network.genesis_hash = [9; 32];
        let mut expired = context.clone();
        expired.session_valid = Arc::new(|| false);
        for invalid in [other_wallet, other_network, expired] {
            assert_eq!(
                wallet.pending_handoffs(&invalid, "chat.dot", &[]).await,
                Err(Error::NotConnected)
            );
            assert_eq!(
                wallet
                    .redeliver(
                        &invalid,
                        "chat.dot",
                        operation.card.operation_id,
                        Arc::new(NoTransport)
                    )
                    .await,
                Err(Error::NotConnected)
            );
        }
        let valid = Arc::new(AtomicBool::new(true));
        let mut queued_context = context.clone();
        let validity = valid.clone();
        queued_context.session_valid = Arc::new(move || validity.load(Ordering::SeqCst));
        let gate = wallet.gate.lock().await;
        let replay = wallet.redeliver(
            &queued_context,
            "chat.dot",
            operation.card.operation_id,
            Arc::new(NoTransport),
        );
        futures::pin_mut!(replay);
        assert!(futures::poll!(replay.as_mut()).is_pending());
        valid.store(false, Ordering::SeqCst);
        drop(gate);
        assert_eq!(replay.await, Err(Error::NotConnected));
        assert_eq!(
            wallet
                .pending_handoffs(&context, "chat.dot", &[])
                .await
                .unwrap(),
            vec![operation.card.clone()]
        );
    });
}

#[test]
fn replay_excludes_unaccepted_rejected_cleared_failed_and_acknowledged_secrets() {
    block_on(async {
        let context = context(Arc::new(StubPlatform::default()));
        let wallet = WalletCoinage::open(&context).await.unwrap();
        for (index, phase) in [
            Phase::Review,
            Phase::Approved,
            Phase::HandoffReady,
            Phase::Denied,
            Phase::Incoming,
        ]
        .into_iter()
        .enumerate()
        {
            let mut operation = accepted_outgoing(&format!("unaccepted-{index}"));
            operation.phase = phase;
            if phase == Phase::Incoming {
                operation.card.direction = Direction::Incoming;
            }
            wallet.save(&operation).await.unwrap();
            let error = if phase == Phase::Incoming {
                Error::OperationNotFound
            } else {
                Error::OperationConflict
            };
            assert_eq!(
                wallet
                    .redeliver(
                        &context,
                        "chat.dot",
                        operation.card.operation_id,
                        Arc::new(NoTransport)
                    )
                    .await,
                Err(error)
            );
        }
        for (index, state) in [
            State::Delivered,
            State::Cleared,
            State::Failed {
                reason: Failure::Cancelled,
            },
        ]
        .into_iter()
        .enumerate()
        {
            let mut operation = accepted_outgoing(&format!("terminal-{index}"));
            operation.card.state = state;
            wallet.save(&operation).await.unwrap();
            assert_eq!(
                wallet
                    .redeliver(
                        &context,
                        "chat.dot",
                        operation.card.operation_id,
                        Arc::new(NoTransport)
                    )
                    .await,
                Err(Error::OperationConflict)
            );
        }
        // Rejection evidence wins even over a stale Accepted card.
        let rejected = accepted_outgoing("rejected");
        wallet.save(&rejected).await.unwrap();
        save_receipt(
            &wallet,
            &rejected,
            truapi_coinage::WalOperation::TransferRejected,
        )
        .await;
        assert_eq!(
            wallet
                .redeliver(
                    &context,
                    "chat.dot",
                    rejected.card.operation_id,
                    Arc::new(NoTransport)
                )
                .await,
            Err(Error::OperationConflict)
        );
        let acknowledged = accepted_outgoing("acknowledged");
        wallet.save(&acknowledged).await.unwrap();
        wallet
            .note_delivery(&context, "chat.dot", acknowledged.card.operation_id)
            .await
            .unwrap();
        assert_eq!(
            wallet
                .redeliver(
                    &context,
                    "chat.dot",
                    acknowledged.card.operation_id,
                    Arc::new(NoTransport)
                )
                .await,
            Err(Error::OperationConflict)
        );
        assert!(
            wallet
                .pending_handoffs(&context, "chat.dot", &[])
                .await
                .unwrap()
                .is_empty()
        );
        // Partial clearing is not terminal: replay retains the complete memo,
        // including spent entries, instead of constructing a different payment.
        let mut partial = accepted_outgoing("partial");
        partial.card.state = State::PartiallyCleared { cleared_cents: 16 };
        partial.source_cleared[0] = true;
        wallet.save(&partial).await.unwrap();
        assert_eq!(
            wallet
                .pending_handoffs(&context, "chat.dot", &[])
                .await
                .unwrap(),
            vec![partial.card.clone()]
        );
        let transport = Arc::new(ReplayTransport::default());
        wallet
            .redeliver(
                &context,
                "chat.dot",
                partial.card.operation_id,
                transport.clone(),
            )
            .await
            .unwrap();
        assert!(transport.attempts.lock()[0].1.as_slice() == partial.memo.as_slice());
    });
}

#[test]
fn replay_never_exports_a_memo_with_corrupt_immutable_bindings_or_shape() {
    block_on(async {
        let context = context(Arc::new(StubPlatform::default()));
        let wallet = WalletCoinage::open(&context).await.unwrap();
        for mutation in 0..7 {
            let mut operation = accepted_outgoing("corrupt");
            match mutation {
                0 => operation.card.amount_cents += 1,
                1 => operation.source_fingerprint = Some([0; 32]),
                2 => operation.memo_key = Some([0; 32]),
                3 => operation.source_public.reverse(),
                4 => operation.memo.push(0),
                5 => operation.card.message_id = "different-message".into(),
                6 => {
                    let memo = TransferMemo {
                        entries: vec![source(12), source(12)],
                        total_value: 250,
                    };
                    operation.memo_key = Some(memo.identifier());
                    operation.memo = memo.scale_encoded();
                }
                _ => unreachable!(),
            }
            wallet.save(&operation).await.unwrap();
            assert_eq!(
                wallet.pending_handoffs(&context, "chat.dot", &[]).await,
                Err(Error::StorageUnavailable)
            );
            assert_eq!(
                wallet
                    .redeliver(
                        &context,
                        "chat.dot",
                        operation.card.operation_id,
                        Arc::new(NoTransport)
                    )
                    .await,
                Err(Error::StorageUnavailable)
            );
        }
    });
}

#[test]
fn actor_custody_cannot_override_missing_or_rejected_wallet_recovery_evidence() {
    block_on(async {
        for (receipt, expected) in [
            (None, Error::StorageUnavailable),
            (
                Some(truapi_coinage::WalOperation::TransferRejected),
                Error::OperationConflict,
            ),
        ] {
            let platform = Arc::new(StubPlatform::default());
            let context = context(platform.clone());
            let wallet = WalletCoinage::open(&context).await.unwrap();
            let mut operation = accepted_outgoing("conflicting-custody");
            operation.phase = Phase::HandoffReady;
            wallet.save(&operation).await.unwrap();
            if let Some(receipt) = receipt {
                save_receipt(&wallet, &operation, receipt).await;
            }
            let accepted = [operation.card.operation_id];
            assert_eq!(
                wallet
                    .pending_handoffs(&context, "chat.dot", &accepted)
                    .await,
                Err(expected)
            );
            let transport = Arc::new(ReplayTransport::default());
            assert_eq!(
                wallet
                    .redeliver(
                        &context,
                        "chat.dot",
                        operation.card.operation_id,
                        transport.clone(),
                    )
                    .await,
                Err(Error::OperationConflict)
            );
            assert!(transport.attempts.lock().is_empty());
            assert!(platform.chain_connects.lock().unwrap().is_empty());
            assert!(
                platform
                    .main_purse_chat_payment_reviews
                    .lock()
                    .unwrap()
                    .is_empty()
            );
        }
    });
}
