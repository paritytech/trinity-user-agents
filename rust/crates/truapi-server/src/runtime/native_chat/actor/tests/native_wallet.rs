// SPDX-License-Identifier: AGPL-3.0-only
//! Exercise injected native custody through the real Chat transport and registry.

use super::*;
use crate::test_support::wait_until;
use std::sync::atomic::AtomicUsize;
use truapi::v01::{
    HostPaymentTopUpError as TopUpError, HostPaymentTopUpRequest, PaymentTopUpSource,
};
use truapi_platform::{
    CoinageWalletHost, NativeCoinageFailure as Failure, NativeCoinageMemo,
    NativeCoinageOperation as Operation, NativeCoinagePaymentIntent, NativeCoinageRequest,
    NativeCoinageResponse as Response, NativeCoinageScope, NativeCoinageTopUpOutcome as Outcome,
};

// Scripts own only the native service side. Rust storage remains guarded, and
// encrypted transport acceptance/restart runs against the actual Host stores.
struct NativeService {
    call: Box<
        dyn Fn(&NativeCoinageRequest) -> Result<Response, truapi::v01::GenericError> + Send + Sync,
    >,
}

impl NativeService {
    fn new(
        call: impl Fn(&NativeCoinageRequest) -> Result<Response, truapi::v01::GenericError>
        + Send
        + Sync
        + 'static,
    ) -> Arc<Self> {
        Arc::new(Self {
            call: Box::new(call),
        })
    }

    fn platform(self: &Arc<Self>) -> Arc<StubPlatform> {
        Arc::new(StubPlatform {
            guard_main_purse_storage: true,
            chain_connect_error: Some("native custody must not connect through the Rust engine"),
            ..Default::default()
        })
    }
}

#[truapi_platform::async_trait]
impl CoinageWalletHost for NativeService {
    async fn native_coinage(
        &self,
        request: NativeCoinageRequest,
    ) -> Result<Response, truapi::v01::GenericError> {
        (self.call)(&Zeroizing::new(request))
    }
}

fn assert_native_isolation(platform: &StubPlatform) {
    assert_eq!(
        platform.main_purse_storage_accesses.load(Ordering::SeqCst),
        0
    );
    assert!(platform.chain_connects.lock().unwrap().is_empty());
    assert!(
        platform
            .main_purse_chat_payment_reviews
            .lock()
            .unwrap()
            .is_empty()
    );
}

fn native_memo() -> NativeCoinageMemo {
    NativeCoinageMemo {
        secret_keys: memo()
            .entries
            .iter()
            .map(|entry| entry.0.to_vec())
            .collect(),
        total_value_raw: "250".into(),
    }
}

fn card(
    intent: &NativeCoinagePaymentIntent,
    timestamp: u64,
    state: HostNativeChatPaymentState,
) -> HostNativeChatPayment {
    HostNativeChatPayment {
        operation_id: intent.operation_id,
        request_id: intent.request_id.clone(),
        message_id: format!("payment-{}", hex::encode(intent.operation_id)),
        timestamp,
        peer_identity: intent.peer_identity,
        direction: truapi::latest::HostNativeChatPaymentDirection::Outgoing,
        amount_cents: intent.amount_cents,
        state,
    }
}

#[test]
fn native_handoff_repairs_lost_commit_after_actor_restart_without_spending_again() {
    block_on(async {
        #[derive(Default)]
        struct Custody {
            intent: Option<NativeCoinagePaymentIntent>,
            allocations: usize,
            accepted: bool,
            delivered: bool,
            lost_commit: bool,
            reconciliations: usize,
        }
        let custody = Arc::new(Mutex::new(Custody {
            lost_commit: true,
            ..Default::default()
        }));
        let native = custody.clone();
        let timestamp = current_unix_secs() * 1000;
        let service = NativeService::new(move |request| {
            let mut native = native.lock();
            let view = |native: &Custody| {
                card(
                    native.intent.as_ref().unwrap(),
                    timestamp,
                    if native.delivered {
                        HostNativeChatPaymentState::Delivered
                    } else if native.accepted {
                        HostNativeChatPaymentState::Delivering
                    } else {
                        HostNativeChatPaymentState::Preparing
                    },
                )
            };
            Ok(match &request.operation {
                Operation::Denomination => Response::Denomination {
                    cents_unit_raw: "10".into(),
                },
                Operation::PreparePayment { intent } => {
                    if let Some(old) = &native.intent {
                        if old != intent {
                            return Ok(Response::Failed {
                                reason: Failure::OperationConflict,
                            });
                        }
                    } else {
                        native.allocations += 1;
                        native.intent = Some(intent.clone());
                    }
                    Response::Prepared {
                        payment: view(&native),
                        memo: Some(native_memo()),
                    }
                }
                Operation::CommitHandoff { operation_id, .. } => {
                    assert_eq!(*operation_id, native.intent.as_ref().unwrap().operation_id);
                    if std::mem::take(&mut native.lost_commit) {
                        return Err(truapi::v01::GenericError {
                            reason: "private native diagnostic must not escape".into(),
                        });
                    }
                    native.accepted = true;
                    Response::Done
                }
                Operation::PendingHandoffs {
                    accepted_operations,
                    ..
                } => {
                    let id = native.intent.as_ref().unwrap().operation_id;
                    assert_eq!(accepted_operations, &[id]);
                    native.accepted = true;
                    Response::Payments {
                        payments: if native.delivered {
                            vec![]
                        } else {
                            vec![view(&native)]
                        },
                    }
                }
                Operation::ReadHandoff { operation_id, .. } => {
                    assert_eq!(*operation_id, native.intent.as_ref().unwrap().operation_id);
                    assert!(native.accepted);
                    Response::Prepared {
                        payment: view(&native),
                        memo: Some(native_memo()),
                    }
                }
                Operation::NoteDelivery { operation_id, .. } => {
                    assert_eq!(*operation_id, native.intent.as_ref().unwrap().operation_id);
                    native.delivered = true;
                    Response::Done
                }
                Operation::Views { .. } => Response::Payments {
                    payments: vec![view(&native)],
                },
                Operation::Reconcile => {
                    native.reconciliations += 1;
                    Response::Done
                }
                _ => panic!("outgoing recovery cannot claim incoming coins"),
            })
        });
        let platform = service.platform();
        let fixture = Fixture::with_native_wallet(platform.clone(), Some(service.clone()));
        let actor = fixture.actor().await;
        let identity = IdentityFixture::new();
        let peer = DeviceFixture::new(1);
        seed_peer(&actor, &identity, &[&peer]).await;
        let registry = NativeChatRegistry::default();
        let wallet = registry.wallet(&fixture.context).await.unwrap();
        let intent = payment_intent(&identity);
        assert_eq!(
            wallet
                .send(
                    &fixture.context,
                    intent.clone(),
                    transport(&actor, &fixture, &identity)
                )
                .await,
            Err(Error::StorageUnavailable)
        );
        let id = custody.lock().intent.as_ref().unwrap().operation_id;
        assert!(
            actor
                .store
                .read(|state| state.accepted_payments.contains(&id))
                .await
                .unwrap()
        );
        assert!(!custody.lock().accepted);
        assert_native_isolation(&platform);
        drop(wallet);
        drop(registry);
        drop(actor);
        fixture.tasks.stop();

        let restarted = Fixture::with_native_wallet(platform.clone(), Some(service.clone()));
        let actor = restarted.actor().await;
        let registry = NativeChatRegistry::default();
        actor
            .reconcile(&restarted.context, &registry)
            .await
            .unwrap();
        assert!(custody.lock().accepted);
        let wallet = registry.wallet(&restarted.context).await.unwrap();
        wallet
            .send(
                &restarted.context,
                intent.clone(),
                transport(&actor, &restarted, &identity),
            )
            .await
            .unwrap();
        assert_eq!(custody.lock().allocations, 1);
        let mut conflicting = intent;
        conflicting.amount_cents += 1;
        assert_eq!(
            wallet
                .send(
                    &restarted.context,
                    conflicting,
                    transport(&actor, &restarted, &identity)
                )
                .await,
            Err(Error::OperationConflict)
        );
        let packet = outgoing(&actor, OutgoingKind::Payment(id)).await;
        // Even the peer's attempt to rewrap the outgoing ciphertext cannot turn
        // this response into an export of Host-generated bearer keys.
        assert_eq!(
            actor
                .open_statement(&restarted.context, &registry, packet.statement.clone())
                .await,
            Err(Error::InvalidStatement)
        );
        actor
            .open_statement(
                &restarted.context,
                &registry,
                acknowledgment(&actor, &identity, &peer, &packet.request_id, false),
            )
            .await
            .unwrap();
        assert_eq!(
            wallet.views(&restarted.context, PRODUCT).await.unwrap()[0].state,
            HostNativeChatPaymentState::Delivered
        );
        wallet.recover(&restarted.context).await.unwrap();
        assert_eq!(
            wallet.views(&restarted.context, PRODUCT).await.unwrap()[0].state,
            HostNativeChatPaymentState::Delivered
        );
        assert_eq!(custody.lock().allocations, 1);
        assert!(custody.lock().reconciliations > 0);
        assert_native_isolation(&platform);
    });
}

#[test]
fn locked_native_wallet_never_falls_back_and_ordinary_chat_stays_available() {
    block_on(async {
        let service = NativeService::new(|_| {
            Ok(Response::Failed {
                reason: Failure::Unavailable,
            })
        });
        let platform = service.platform();
        let fixture = Fixture::with_native_wallet(platform.clone(), Some(service.clone()));
        set_product_grants(
            platform.as_ref(),
            PRODUCT,
            truapi_platform::PermissionAuthorizationStatus::NotDetermined,
        )
        .await;
        let registry = NativeChatRegistry::default();
        let wallet = registry.wallet(&fixture.context).await.unwrap();
        assert_eq!(
            wallet.views(&fixture.context, PRODUCT).await,
            Err(Error::StorageUnavailable)
        );
        assert_eq!(
            registry.denomination(&fixture.context).await,
            Err(Error::StorageUnavailable)
        );
        assert_eq!(
            wallet.recover(&fixture.context).await,
            Err(Error::StorageUnavailable)
        );
        let actor = registry.chat(&fixture.context, PRODUCT).await.unwrap();
        let identity = IdentityFixture::new();
        let peer = DeviceFixture::new(1);
        seed_peer(&actor, &identity, &[&peer]).await;
        assert_eq!(
            wallet
                .send(
                    &fixture.context,
                    payment_intent(&identity),
                    transport(&actor, &fixture, &identity),
                )
                .await,
            Err(Error::StorageUnavailable)
        );
        registry
            .execute(
                fixture.context.clone(),
                PRODUCT.into(),
                HostProductDeviceChatRequest::Initialize,
            )
            .await
            .unwrap();
        let text = wire::encode_rich_text_message(
            "ordinary",
            fixture.timestamp,
            Some("still works"),
            None,
        )
        .unwrap();
        let response = registry
            .execute(
                fixture.context.clone(),
                PRODUCT.into(),
                HostProductDeviceChatRequest::Open {
                    statement: request(&actor, &identity, &peer, "ordinary", &[text.clone()]),
                },
            )
            .await
            .unwrap();
        assert_eq!(
            response.opened[0].plaintext,
            wire::encode_transport_request_plaintext("ordinary", &[text]).unwrap()
        );
        let mut rebound = fixture.context.clone();
        rebound.coinage_instance_id = Some(1);
        assert!(matches!(
            registry.wallet(&rebound).await,
            Err(Error::OperationConflict)
        ));
        assert_native_isolation(&platform);
    });
}

#[test]
fn absent_native_dependency_uses_builtin_rust_wallet() {
    block_on(async {
        let fixture = Fixture::new();
        let registry = NativeChatRegistry::default();
        let actor = fixture.actor().await;
        let identity = IdentityFixture::new();
        let intent = payment_intent(&identity);
        let wallet = registry.wallet(&fixture.context).await.unwrap();
        assert_eq!(
            wallet
                .send(
                    &fixture.context,
                    intent,
                    transport(&actor, &fixture, &identity),
                )
                .await,
            Err(Error::NetworkUnavailable)
        );
        assert_eq!(
            *fixture.platform.chain_connects.lock().unwrap(),
            vec![fixture.context.genesis_hash]
        );
    });
}

#[test]
fn unavailable_injected_wallet_stays_native_across_session_release() {
    block_on(async {
        let calls = Arc::new(AtomicUsize::new(0));
        let native_calls = calls.clone();
        let service = NativeService::new(move |_| {
            native_calls.fetch_add(1, Ordering::SeqCst);
            Err(truapi::v01::GenericError {
                reason: "native service temporarily unavailable".into(),
            })
        });
        let platform = service.platform();
        let fixture = Fixture::with_native_wallet(platform.clone(), Some(service));
        let registry = NativeChatRegistry::default();
        for _ in 0..2 {
            let wallet = registry
                .existing_wallet(&fixture.context, false)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(
                wallet.views(&fixture.context, PRODUCT).await,
                Err(Error::StorageUnavailable)
            );
            registry.release();
        }
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert_native_isolation(&platform);
    });
}

#[test]
fn unlock_recovery_reaches_native_owner_without_opening_guarded_rust_custody() {
    let reconciliations = Arc::new(AtomicUsize::new(0));
    let calls = reconciliations.clone();
    let service = NativeService::new(move |request| {
        assert!(matches!(request.operation, Operation::Reconcile));
        calls.fetch_add(1, Ordering::SeqCst);
        Ok(Response::Failed {
            reason: Failure::Unavailable,
        })
    });
    let platform = service.platform();
    let fixture = Fixture::with_native_wallet(platform.clone(), Some(service));
    let registry = NativeChatRegistry::default();
    registry.resume_wallet_recovery(fixture.context.clone());
    wait_until(
        || reconciliations.load(Ordering::SeqCst) > 0,
        "native unlock recovery was not called",
    );
    fixture.tasks.stop();
    assert_native_isolation(&platform);
}

fn top_up(keys: Vec<[u8; 64]>, amount: u128) -> HostPaymentTopUpRequest {
    HostPaymentTopUpRequest {
        into: None,
        amount,
        source: PaymentTopUpSource::Coins {
            sr25519_secret_keys: keys,
        },
    }
}

#[test]
fn native_zero_minimum_import_still_waits_for_finalized_credit() {
    block_on(async {
        let outcome = Arc::new(Mutex::new(Outcome::Pending));
        let native_outcome = outcome.clone();
        let service = NativeService::new(move |request| {
            let Operation::TopUp {
                minimum_amount_raw, ..
            } = &request.operation
            else {
                panic!("only incoming custody was requested");
            };
            assert_eq!(minimum_amount_raw, "0");
            Ok(Response::TopUp {
                outcome: native_outcome.lock().clone(),
            })
        });
        let platform = service.platform();
        let fixture = Fixture::with_native_wallet(platform.clone(), Some(service));
        let registry = NativeChatRegistry::default();
        let wallet = registry.wallet(&fixture.context).await.unwrap();
        let keys = vec![keypair(0x43).secret.to_bytes()];
        assert!(matches!(
            wallet
                .top_up(&fixture.context, PRODUCT, top_up(keys.clone(), 0))
                .await,
            Err(TopUpError::Unknown { .. })
        ));
        *outcome.lock() = Outcome::Cleared;
        wallet
            .top_up(&fixture.context, PRODUCT, top_up(keys, 0))
            .await
            .unwrap();
        *outcome.lock() = Outcome::NotClaimed;
        assert_eq!(
            wallet
                .top_up(
                    &fixture.context,
                    PRODUCT,
                    top_up(vec![keypair(0x44).secret.to_bytes()], 0)
                )
                .await,
            Err(TopUpError::InsufficientFunds)
        );
        assert_native_isolation(&platform);
    });
}

#[test]
fn native_incoming_minimum_and_source_identity_survive_partial_and_ambiguous_retries() {
    block_on(async {
        type Receipt = (NativeCoinageScope, String, [u8; 32], String);
        let receipts = Arc::new(Mutex::new(Vec::<Receipt>::new()));
        let outcome = Arc::new(Mutex::new(Outcome::Pending));
        let received = receipts.clone();
        let result = outcome.clone();
        let service = NativeService::new(move |request| {
            let Operation::TopUp {
                product_id,
                operation_id,
                minimum_amount_raw,
                secret_keys,
            } = &request.operation
            else {
                if matches!(request.operation, Operation::Reconcile) {
                    return Ok(Response::Done);
                }
                panic!("incoming claim cannot prepare an outgoing payment");
            };
            assert_eq!(secret_keys.len(), 2);
            let mut received = received.lock();
            if received
                .iter()
                .any(|(_, _, id, minimum)| id == operation_id && minimum != minimum_amount_raw)
            {
                return Ok(Response::Failed {
                    reason: Failure::OperationConflict,
                });
            }
            received.push((
                request.scope.clone(),
                product_id.clone(),
                *operation_id,
                minimum_amount_raw.clone(),
            ));
            Ok(Response::TopUp {
                outcome: result.lock().clone(),
            })
        });
        let platform = service.platform();
        let fixture = Fixture::with_native_wallet(platform.clone(), Some(service));
        let registry = NativeChatRegistry::default();
        let wallet = registry.wallet(&fixture.context).await.unwrap();
        let keys = vec![
            keypair(0x41).secret.to_bytes(),
            keypair(0x42).secret.to_bytes(),
        ];
        assert!(matches!(
            wallet
                .top_up(&fixture.context, PRODUCT, top_up(keys.clone(), 50))
                .await,
            Err(TopUpError::Unknown { .. })
        ));
        *outcome.lock() = Outcome::Partial {
            credited_amount_raw: "30".into(),
        };
        assert_eq!(
            wallet
                .top_up(
                    &fixture.context,
                    PRODUCT,
                    top_up(vec![keys[1], keys[0]], 50)
                )
                .await,
            Err(TopUpError::PartialPayment { credited: 30 })
        );
        let id = receipts.lock()[0].2;
        assert_eq!(receipts.lock()[1].2, id);
        assert_eq!(
            wallet
                .top_up(&fixture.context, PRODUCT, top_up(keys.clone(), 51))
                .await,
            Err(TopUpError::InvalidSource)
        );
        drop(wallet);
        drop(registry);
        let restarted = NativeChatRegistry::default();
        let wallet = restarted.wallet(&fixture.context).await.unwrap();
        *outcome.lock() = Outcome::Cleared;
        wallet
            .top_up(&fixture.context, PRODUCT, top_up(keys.clone(), 50))
            .await
            .unwrap();
        assert_eq!(receipts.lock().last().unwrap().2, id);
        wallet
            .top_up(&fixture.context, "other.dot", top_up(keys.clone(), 50))
            .await
            .unwrap();
        assert_ne!(receipts.lock().last().unwrap().2, id);
        let mut other_wallet = fixture.context.clone();
        other_wallet.session.public_key = [9; 32];
        restarted
            .wallet(&other_wallet)
            .await
            .unwrap()
            .top_up(&other_wallet, PRODUCT, top_up(keys.clone(), 50))
            .await
            .unwrap();
        assert_ne!(receipts.lock().last().unwrap().2, id);
        for invalid in [vec![], vec![keys[0], keys[0]], vec![[255; 64]]] {
            assert_eq!(
                wallet
                    .top_up(&fixture.context, PRODUCT, top_up(invalid, 50))
                    .await,
                Err(TopUpError::InvalidSource)
            );
        }
        for invalid in [
            "030",
            "50",
            "51",
            "0",
            "-1",
            "340282366920938463463374607431768211456",
        ] {
            *outcome.lock() = Outcome::Partial {
                credited_amount_raw: invalid.into(),
            };
            assert!(matches!(
                wallet
                    .top_up(&fixture.context, PRODUCT, top_up(keys.clone(), 50))
                    .await,
                Err(TopUpError::Unknown { .. })
            ));
        }
        *outcome.lock() = Outcome::NotClaimed;
        assert_eq!(
            wallet
                .top_up(&fixture.context, PRODUCT, top_up(keys, 50))
                .await,
            Err(TopUpError::InsufficientFunds)
        );
        assert_native_isolation(&platform);
    });
}

#[test]
fn invalid_native_memo_never_reaches_ciphertext_custody_or_commit() {
    block_on(async {
        let timestamp = current_unix_secs() * 1000;
        let service = NativeService::new(move |request| {
            Ok(match &request.operation {
                Operation::PreparePayment { intent } => Response::Prepared {
                    payment: card(intent, timestamp, HostNativeChatPaymentState::Preparing),
                    memo: Some(NativeCoinageMemo {
                        secret_keys: vec![vec![0; 63]],
                        total_value_raw: "250".into(),
                    }),
                },
                Operation::Denomination => Response::Denomination {
                    cents_unit_raw: "10".into(),
                },
                _ => panic!("malformed memo must not be committed"),
            })
        });
        let platform = service.platform();
        let fixture = Fixture::with_native_wallet(platform.clone(), Some(service));
        let actor = fixture.actor().await;
        let identity = IdentityFixture::new();
        let peer = DeviceFixture::new(1);
        seed_peer(&actor, &identity, &[&peer]).await;
        let registry = NativeChatRegistry::default();
        let wallet = registry.wallet(&fixture.context).await.unwrap();
        assert_eq!(
            wallet
                .send(
                    &fixture.context,
                    payment_intent(&identity),
                    transport(&actor, &fixture, &identity)
                )
                .await,
            Err(Error::StorageUnavailable)
        );
        assert!(
            actor
                .store
                .read(|state| state.accepted_payments.is_empty() && state.outbox.is_empty())
                .await
                .unwrap()
        );
        assert_native_isolation(&platform);
    });
}

#[test]
fn cancelled_incoming_caller_does_not_cancel_owned_native_custody() {
    block_on(async {
        let receipts = Arc::new(Mutex::new(Vec::new()));
        let recorded = receipts.clone();
        let service = NativeService::new(move |request| {
            let Operation::TopUp { operation_id, .. } = &request.operation else {
                panic!("only incoming custody was requested");
            };
            recorded.lock().push(*operation_id);
            Ok(Response::TopUp {
                outcome: Outcome::Cleared,
            })
        });
        let platform = service.platform();
        let fixture = Fixture::with_native_wallet(platform.clone(), Some(service.clone()));
        let registry = NativeChatRegistry::default();
        let wallet = registry.wallet(&fixture.context).await.unwrap();
        let mut caller_context = fixture.context.clone();
        let queued = Arc::new(Mutex::new(Vec::new()));
        let queue = queued.clone();
        caller_context.services = RuntimeServices::with_chat_platform(
            platform.clone(),
            fixture.context.services.host_info.clone(),
            [2; 32],
            [3; 32],
            [4; 32],
            Arc::new(move |job| queue.lock().push(job)),
            None,
            Some(service),
        );
        let keys = vec![
            keypair(0x51).secret.to_bytes(),
            keypair(0x52).secret.to_bytes(),
        ];
        let mut call = Box::pin(wallet.top_up(&caller_context, PRODUCT, top_up(keys.clone(), 50)));
        assert!(futures::poll!(call.as_mut()).is_pending());
        drop(call);
        let job = queued.lock().pop().unwrap();
        job.await;
        assert_eq!(receipts.lock().len(), 1);
        wallet
            .top_up(
                &fixture.context,
                PRODUCT,
                top_up(vec![keys[1], keys[0]], 50),
            )
            .await
            .unwrap();
        let receipts = receipts.lock();
        assert_eq!(receipts[0], receipts[1]);
        assert_native_isolation(&platform);
    });
}

#[test]
fn native_preparation_losing_session_cannot_handoff_or_commit() {
    block_on(async {
        let valid = Arc::new(AtomicBool::new(true));
        let current = valid.clone();
        let timestamp = current_unix_secs() * 1000;
        let service = NativeService::new(move |request| {
            let Operation::PreparePayment { intent } = &request.operation else {
                panic!("revoked session must not perform the next native effect");
            };
            current.store(false, Ordering::SeqCst);
            Ok(Response::Prepared {
                payment: card(intent, timestamp, HostNativeChatPaymentState::Preparing),
                memo: Some(native_memo()),
            })
        });
        let platform = service.platform();
        let mut fixture = Fixture::with_native_wallet(platform.clone(), Some(service));
        fixture.context.session_valid = Arc::new(move || valid.load(Ordering::SeqCst));
        let actor = fixture.actor().await;
        let identity = IdentityFixture::new();
        let peer = DeviceFixture::new(1);
        seed_peer(&actor, &identity, &[&peer]).await;
        let registry = NativeChatRegistry::default();
        let wallet = registry.wallet(&fixture.context).await.unwrap();
        assert_eq!(
            wallet
                .send(
                    &fixture.context,
                    payment_intent(&identity),
                    transport(&actor, &fixture, &identity)
                )
                .await,
            Err(Error::NotConnected)
        );
        assert!(
            actor
                .store
                .read(|state| state.accepted_payments.is_empty() && state.outbox.is_empty())
                .await
                .unwrap()
        );
        assert_native_isolation(&platform);
    });
}
