// SPDX-License-Identifier: AGPL-3.0-only
//! Native signed/encrypted packets against the real actor and encrypted stores.
#![cfg(not(target_arch = "wasm32"))]

mod hop_history;

use super::*;
use crate::{
    host_logic::statement_store::decode_verified_statement_data,
    runtime::{authority::AuthoritySession, services::RuntimeServices},
    subscription::Spawner,
    test_support::{StubPlatform, core_storage_test_key},
};
use futures::{
    executor::block_on,
    future::{AbortHandle, Abortable},
};
use parking_lot::Mutex;
use truapi_coinage::{MemoEntry, TransferMemo};
use truapi_platform::CoreStorageKey;

const PRODUCT: &str = "chat.dot";

// Every owned persistence future runs to completion before its awaited call
// returns. Delivery loops belong to this session and are aborted and joined at
// logout/drop, including on assertion failure; no five-second timer leaks into
// another test and no test depends on the delivery thread winning a race.
struct SessionTasks {
    live: Arc<AtomicBool>,
    tasks: Arc<Mutex<Vec<(AbortHandle, std::thread::JoinHandle<()>)>>>,
}

impl SessionTasks {
    fn new() -> Self {
        Self {
            live: Arc::new(AtomicBool::new(true)),
            tasks: Default::default(),
        }
    }

    fn spawner(&self) -> Spawner {
        let tasks = self.tasks.clone();
        Arc::new(move |future| {
            let (abort, registration) = AbortHandle::new_pair();
            let thread = std::thread::spawn(move || {
                let _ = block_on(Abortable::new(future, registration));
            });
            tasks.lock().push((abort, thread));
        })
    }

    fn stop(&self) {
        self.live.store(false, Ordering::Release);
        // Aborting a task can race with it spawning its last storage operation.
        // Join the current wave and drain again, without holding the list lock.
        loop {
            let tasks = std::mem::take(&mut *self.tasks.lock());
            if tasks.is_empty() {
                break;
            }
            for (abort, _) in &tasks {
                abort.abort();
            }
            for (_, thread) in tasks {
                thread.join().unwrap();
            }
        }
    }
}

impl Drop for SessionTasks {
    fn drop(&mut self) {
        self.stop();
    }
}

struct Fixture {
    context: NativeChatContext,
    platform: Arc<StubPlatform>,
    tasks: SessionTasks,
    timestamp: u64,
}

impl Fixture {
    fn new() -> Self {
        Self::on_platform(Arc::new(StubPlatform {
            chain_connect_error: Some("actor fixture has no RPC"),
            ..Default::default()
        }))
    }

    fn on_platform(platform: Arc<StubPlatform>) -> Self {
        let tasks = SessionTasks::new();
        let live = tasks.live.clone();
        let context = NativeChatContext {
            services: RuntimeServices::new(
                platform.clone(),
                truapi_platform::HostInfo {
                    name: "Native actor test".into(),
                    icon: None,
                    version: None,
                    platform: HostPlatform::Unknown,
                },
                [2; 32],
                [3; 32],
                [4; 32],
                tasks.spawner(),
            ),
            session: AuthoritySession {
                public_key: [1; 32],
                identity_account_id: Some([5; 32]),
                lite_username: None,
                full_username: None,
                validation_id: vec![1],
            },
            entropy: Zeroizing::new(vec![0x44; 16]),
            session_valid: Arc::new(move || live.load(Ordering::Acquire)),
            network_suffix: "test".into(),
            genesis_hash: [2; 32],
            coinage_instance_id: None,
        };
        Self {
            context,
            platform,
            tasks,
            timestamp: current_unix_secs() * 1000,
        }
    }

    async fn actor(&self) -> Arc<NativeChatActor> {
        NativeChatActor::open(&self.context, PRODUCT).await.unwrap()
    }
}

struct IdentityFixture {
    account: [u8; 32],
    secret: [u8; 32],
}

impl IdentityFixture {
    fn new() -> Self {
        Self {
            account: keypair(0x70).public.to_bytes(),
            secret: [0x71; 32],
        }
    }
    fn public_key(&self) -> [u8; 32] {
        wire::x25519_public_key(&self.secret)
    }
}

struct DeviceFixture {
    signer: Keypair,
    secret: [u8; 32],
}

impl DeviceFixture {
    fn new(seed: u8) -> Self {
        Self {
            signer: keypair(seed),
            secret: [seed + 64; 32],
        }
    }
    fn account(&self) -> [u8; 32] {
        self.signer.public.to_bytes()
    }
    fn public_key(&self) -> [u8; 32] {
        wire::x25519_public_key(&self.secret)
    }
}

fn keypair(seed: u8) -> Keypair {
    schnorrkel::MiniSecretKey::from_bytes(&[seed; 32])
        .unwrap()
        .expand_to_keypair(schnorrkel::ExpansionMode::Ed25519)
}

async fn seed_peer(
    actor: &NativeChatActor,
    identity: &IdentityFixture,
    devices: &[&DeviceFixture],
) {
    // Initial authenticated discovery and a completed secure-device handshake
    // are seeded. Later mutations and ACKs enter through signature/route checks.
    let peer = Peer {
        identity: identity.account,
        root_key: identity.public_key(),
        username: Some("peer.dot".into()),
        devices: devices
            .iter()
            .map(|device| DeviceRecord {
                account: device.account(),
                key: Some(device.public_key()),
                active: true,
                timestamp: 0,
                message_id: "authenticated-fixture".into(),
            })
            .collect(),
        invitation: None,
        invitation_timestamp: None,
        invitation_text: None,
        established: true,
        revocation_request: None,
        revocation_acked: true,
        revocation_acks: devices.iter().map(|device| device.account()).collect(),
        revision: 1,
    };
    actor
        .store
        .update(move |state| {
            state.peers.push(peer);
            Ok(())
        })
        .await
        .unwrap();
}

#[test]
fn full_account_refreshes_committed_ciphertext_without_starving_other_peers() {
    block_on(async {
        let floor = (current_unix_secs() + LIFETIME + 60) << 32;
        let platform = Arc::new(StubPlatform {
            rpc_method_responses: vec![
                (
                    "statement_submit",
                    serde_json::json!({
                        "status":"rejected", "reason":"accountFull", "min_expiry":floor
                    })
                    .to_string(),
                );
                2
            ],
            ..Default::default()
        });
        let fixture = Fixture::on_platform(platform.clone());
        let actor = fixture.actor().await;
        let identities = [
            IdentityFixture::new(),
            IdentityFixture {
                account: keypair(0x72).public.to_bytes(),
                secret: [0x73; 32],
            },
        ];
        let device = DeviceFixture::new(1);
        for identity in &identities {
            seed_peer(&actor, identity, &[&device]).await;
        }
        let sender = actor.clone();
        let peers = identities.map(|identity| identity.account);
        let timestamp = fixture.timestamp;
        actor
            .store
            .update(move |state| {
                for (index, identity) in peers.into_iter().enumerate() {
                    let peer = state.peer(&identity)?.clone();
                    let request_id = format!("retained-{index}");
                    let messages =
                        vec![wire::encode_contact_added_message(&request_id, timestamp).unwrap()];
                    let statement = sender.multi_statement(
                        state,
                        &peer,
                        &peer.active_devices(),
                        &request_id,
                        &messages,
                    )?;
                    state.queue(Outgoing {
                        peer: identity,
                        request_id,
                        digest: hash(&messages.encode()),
                        kind: OutgoingKind::Ordinary,
                        roster_revision: peer.revision,
                        statement,
                        last_attempt: 0,
                    })?;
                }
                Ok(())
            })
            .await
            .unwrap();
        let original = actor
            .store
            .read(|state| state.outbox.clone())
            .await
            .unwrap();
        assert_eq!(
            actor.flush(&fixture.context).await,
            Err(Error::NetworkUnavailable)
        );
        let submissions: Vec<SignedStatement> = platform
            .sent_rpc
            .lock()
            .unwrap()
            .iter()
            .map(|request| serde_json::from_str::<serde_json::Value>(request).unwrap())
            .filter(|request| request["method"] == "statement_submit")
            .map(|request| {
                let bytes = hex::decode(
                    request["params"][0]
                        .as_str()
                        .unwrap()
                        .trim_start_matches("0x"),
                )
                .unwrap();
                let verified = decode_verified_statement_data(&bytes, None).unwrap();
                assert_eq!(verified.signer, actor.public.account_id);
                decode_signed_statement(&bytes).unwrap()
            })
            .collect();
        // A persistent rejection is bounded, but cannot prevent the second
        // peer's statement from reaching the transport.
        assert_eq!(submissions.len(), 4);
        for (pair, before) in submissions.chunks_exact(2).zip(&original) {
            assert_eq!(pair[0].expiry, before.statement.expiry);
            assert!(pair[1].expiry.unwrap() > floor);
            assert_eq!(pair[1].data, before.statement.data);
            assert_eq!(pair[1].topics, before.statement.topics);
            assert_eq!(pair[1].channel, before.statement.channel);
        }
        drop(actor);
        let reopened = fixture.actor().await;
        let retained = reopened
            .store
            .read(|state| state.outbox.clone())
            .await
            .unwrap();
        assert_eq!(retained.len(), 2);
        for (entry, submitted) in retained.iter().zip([&submissions[1], &submissions[3]]) {
            assert_eq!(entry.statement, *submitted);
        }
        assert!(
            reopened
                .public_view(&fixture.context, vec![])
                .await
                .unwrap()
                .acknowledgments
                .is_empty()
        );
    });
}

async fn seed_outgoing_invitation(
    actor: &NativeChatActor,
    identity: &IdentityFixture,
    devices: &[&DeviceFixture],
    timestamp: u64,
) {
    seed_peer(actor, identity, devices).await;
    let identity = identity.account;
    actor
        .store
        .update(move |state| {
            let peer = state.peer_mut(&identity)?;
            peer.established = false;
            peer.invitation = Some("pending-invitation".into());
            peer.invitation_timestamp = Some(timestamp);
            Ok(())
        })
        .await
        .unwrap();
}

fn signed_packet(
    sender: &DeviceFixture,
    topic: [u8; 32],
    response: bool,
    data: Vec<u8>,
) -> SignedStatement {
    let channel = if response {
        wire::chat_identity_response_topic(&topic)
    } else {
        wire::chat_identity_request_topic(&topic)
    }
    .unwrap();
    let fields = statement_fields_from_v01(Statement {
        proof: None,
        decryption_key: None,
        expiry: Some((current_unix_secs() + LIFETIME) << 32),
        channel: Some(channel),
        topics: vec![topic],
        data: Some(data),
    })
    .unwrap();
    let signed =
        sign_statement_fields(sender.signer.secret.to_bytes(), sender.account(), fields).unwrap();
    decode_signed_statement(&signed.encode()).unwrap()
}

fn resign_with_expiry(
    sender: &DeviceFixture,
    statement: SignedStatement,
    expiry: u64,
) -> SignedStatement {
    let fields = statement_fields_from_v01(Statement {
        proof: None,
        decryption_key: None,
        expiry: Some(expiry),
        channel: statement.channel,
        topics: statement.topics,
        data: statement.data,
    })
    .unwrap();
    let signed =
        sign_statement_fields(sender.signer.secret.to_bytes(), sender.account(), fields).unwrap();
    decode_signed_statement(&signed.encode()).unwrap()
}

fn route(shared: &[u8; 32], sender: &[u8; 32], recipient: &[u8; 32]) -> [u8; 32] {
    wire::chat_identity_session_id(shared, sender, None, recipient, None).unwrap()
}

// Native IncomingMessageChannel publishes ACKs on sessionId.own, just like
// requests: the sender's outgoing route, not the original requester's route.
// Peer-side ciphertext uses the independent native codec, not actor sealing.
fn native_packet(
    actor: &NativeChatActor,
    identity: &IdentityFixture,
    sender: &DeviceFixture,
    plaintext: &[u8],
    response: bool,
    root_route: bool,
) -> SignedStatement {
    let (shared, topic) = if root_route {
        let shared =
            wire::x25519_shared_secret(&identity.secret, &actor.public.identity_chat_public_key)
                .unwrap();
        let topic = route(
            &shared,
            &identity.account,
            &actor.public.identity_account_id,
        );
        (shared, topic)
    } else {
        let shared =
            wire::x25519_shared_secret(&sender.secret, &actor.public.identity_chat_public_key)
                .unwrap();
        let topic = route(
            &shared,
            &sender.account(),
            &actor.public.identity_account_id,
        );
        (shared, topic)
    };
    let inner = if root_route {
        plaintext.to_vec()
    } else {
        let one_shot = Zeroizing::new(hash(plaintext));
        let encrypted =
            wire::encrypt_multi_device_payload_with_nonce(&one_shot, &plaintext[1..], [0x21; 12])
                .unwrap();
        let devices_info = vec![wire::V2RequestDeviceInfo {
            statement_account_id: actor.public.account_id,
            encrypted_key: wire::wrap_multi_device_key_with_nonce(
                &sender.secret,
                &actor.public.chat_public_key,
                &one_shot,
                [0x22; 12],
            )
            .unwrap(),
        }];
        if response {
            wire::encode_transport_multi_response_plaintext(&wire::V2MultiDeviceResponse {
                encrypted_response: encrypted,
                devices_info,
            })
            .unwrap()
        } else {
            wire::encode_transport_multi_request_plaintext(&wire::V2MultiDeviceRequest {
                encrypted_request: encrypted,
                devices_info,
            })
            .unwrap()
        }
    };
    let nonce: [u8; 12] = hash(&inner)[..12].try_into().unwrap();
    let key = Zeroizing::new(wire::hkdf_sha256_32(&shared).unwrap());
    let ciphertext = wire::encrypt_multi_device_payload_with_nonce(&key, &inner, nonce).unwrap();
    signed_packet(sender, topic, response, ciphertext)
}

fn request(
    actor: &NativeChatActor,
    identity: &IdentityFixture,
    sender: &DeviceFixture,
    id: &str,
    messages: &[Vec<u8>],
) -> SignedStatement {
    native_packet(
        actor,
        identity,
        sender,
        &wire::encode_transport_request_plaintext(id, messages).unwrap(),
        false,
        false,
    )
}

fn acknowledgment(
    actor: &NativeChatActor,
    identity: &IdentityFixture,
    sender: &DeviceFixture,
    id: &str,
    root: bool,
) -> SignedStatement {
    native_packet(
        actor,
        identity,
        sender,
        &wire::encode_transport_response_plaintext(id, 0).unwrap(),
        true,
        root,
    )
}

fn open_output(
    actor: &NativeChatActor,
    identity: &IdentityFixture,
    statement: &SignedStatement,
    response: bool,
    root: bool,
) -> wire::V2StatementTransportData {
    let verified = decode_verified_statement_data(
        &signed_statement_to_scale(statement.clone()).unwrap(),
        None,
    )
    .unwrap();
    assert_eq!(verified.signer, actor.public.account_id);
    let shared = if root {
        wire::x25519_shared_secret(&identity.secret, &actor.public.identity_chat_public_key)
            .unwrap()
    } else {
        wire::x25519_shared_secret(&identity.secret, &actor.public.chat_public_key).unwrap()
    };
    let topic = if root {
        route(
            &shared,
            &actor.public.identity_account_id,
            &identity.account,
        )
    } else {
        route(&shared, &actor.public.account_id, &identity.account)
    };
    assert!(statement.topics.contains(&topic));
    assert_eq!(
        statement.channel,
        Some(
            if response {
                wire::chat_identity_response_topic(&topic)
            } else {
                wire::chat_identity_request_topic(&topic)
            }
            .unwrap()
        )
    );
    wire::decode_transport(&verified.data, &wire::hkdf_sha256_32(&shared).unwrap()).unwrap()
}

fn open_body(
    actor: &NativeChatActor,
    recipient: &DeviceFixture,
    encrypted: &[u8],
    devices: &[wire::V2RequestDeviceInfo],
) -> Zeroizing<Vec<u8>> {
    let own = devices
        .iter()
        .find(|device| device.statement_account_id == recipient.account())
        .unwrap();
    let key = Zeroizing::new(
        wire::unwrap_multi_device_key(
            &recipient.secret,
            &actor.public.chat_public_key,
            &own.encrypted_key,
        )
        .unwrap(),
    );
    Zeroizing::new(wire::decrypt_multi_device_payload(&key, encrypted).unwrap())
}

async fn outgoing(actor: &NativeChatActor, kind: OutgoingKind) -> Outgoing {
    actor
        .store
        .read(move |state| {
            state
                .outbox
                .iter()
                .find(|entry| entry.kind == kind)
                .cloned()
        })
        .await
        .unwrap()
        .expect("committed packet must remain available offline")
}

fn memo() -> TransferMemo {
    TransferMemo {
        entries: vec![
            MemoEntry(keypair(0x31).secret.to_bytes()),
            MemoEntry(keypair(0x32).secret.to_bytes()),
        ],
        total_value: 250,
    }
}

fn payment_intent(identity: &IdentityFixture) -> PaymentIntent {
    PaymentIntent {
        product_id: PRODUCT.into(),
        peer_identity: identity.account,
        recipient_username: Some("peer.dot".into()),
        request_id: "one-shot-approved-payment".into(),
        amount_cents: 25,
    }
}

fn transport(
    actor: &Arc<NativeChatActor>,
    fixture: &Fixture,
    identity: &IdentityFixture,
) -> Arc<dyn PaymentTransport> {
    Arc::new(ChatPaymentTransport {
        actor: actor.clone(),
        context: fixture.context.clone(),
        peer_identity: identity.account,
    })
}

#[test]
fn root_identity_acceptance_ack_removes_only_the_acknowledged_acceptance() {
    block_on(async {
        let fixture = Fixture::new();
        let actor = fixture.actor().await;
        let identity = IdentityFixture::new();
        let peer = DeviceFixture::new(1);
        let invitation = Invitation {
            id: [0x11; 32],
            peer: identity.account,
            root_key: identity.public_key(),
            username: Some("peer.dot".into()),
            device_account: peer.account(),
            device_key: peer.public_key(),
            message_id: "native-invitation".into(),
            timestamp: fixture.timestamp,
            text: "hello from native".into(),
        };
        actor
            .store
            .update(move |state| {
                state.invitations.push(invitation);
                Ok(())
            })
            .await
            .unwrap();
        // No RPC exists, but acceptance must have committed before submission.
        assert_eq!(
            actor.accept(&fixture.context, [0x11; 32]).await,
            Err(Error::NetworkUnavailable)
        );
        let accepted = outgoing(&actor, OutgoingKind::Acceptance).await;
        let wire::V2StatementTransportData::Request {
            request_id,
            messages,
        } = open_output(&actor, &identity, &accepted.statement, false, true)
        else {
            panic!("native acceptance must use the root identity request")
        };
        assert_eq!(messages.len(), 1);
        assert_eq!(
            wire::decode_message(&messages[0]).unwrap().content,
            wire::V2ChatMessageContent::MultiChatAccepted {
                request_id: "native-invitation".into(),
                device: wire::V2PeerDevice {
                    statement_account_id: actor.public.account_id,
                    encryption_public_key: actor.public.chat_public_key,
                },
            }
        );
        let registry = NativeChatRegistry::default();
        let ack = acknowledgment(&actor, &identity, &peer, &request_id, true);
        actor
            .receive(&fixture.context, &registry, ack.clone())
            .await
            .unwrap();
        actor
            .receive(&fixture.context, &registry, ack)
            .await
            .unwrap();
        assert!(
            actor
                .store
                .read(|state| state
                    .outbox
                    .iter()
                    .all(|entry| entry.kind != OutgoingKind::Acceptance))
                .await
                .unwrap()
        );
        let view = actor.public_view(&fixture.context, vec![]).await.unwrap();
        assert_eq!(
            view.acknowledgments,
            vec![HostNativeChatAcknowledgment {
                peer_identity: identity.account,
                request_id,
                response_code: 0,
            }]
        );
        assert!(view.invitations.is_empty());
        assert!(
            !view.peers[0].ready_for_payments,
            "acceptance ACK cannot stand in for legacy-device revocation ACK"
        );
        let revoked = outgoing(&actor, OutgoingKind::Revocation).await;
        assert_ne!(revoked.request_id, accepted.request_id);
    });
}

#[test]
fn payment_ack_survives_wallet_failure_and_actor_store_reopen() {
    block_on(async {
        let fixture = Fixture::new();
        let actor = fixture.actor().await;
        let identity = IdentityFixture::new();
        let peer = DeviceFixture::new(1);
        seed_peer(&actor, &identity, &[&peer]).await;
        let registry = NativeChatRegistry::default();
        let wallet = registry.wallet(&fixture.context).await.unwrap();
        let card = wallet
            .seed_accepted_for_test(
                &fixture.context,
                payment_intent(&identity),
                fixture.timestamp,
                &memo(),
            )
            .await
            .unwrap();
        transport(&actor, &fixture, &identity)
            .accept(&card, memo())
            .await
            .unwrap();
        assert_eq!(wallet.views(PRODUCT).await.unwrap(), vec![card.clone()]);
        let packet = outgoing(&actor, OutgoingKind::Payment(card.operation_id)).await;

        // Reopen wallet custody after restart so authenticated storage is read,
        // rather than mutating disk underneath a still-valid in-memory cache.
        drop(wallet);
        drop(registry);
        let registry = NativeChatRegistry::default();
        let slot = core_storage_test_key(CoreStorageKey::MainPurseCoinage {
            root_public_key: fixture.context.session.public_key,
            genesis_hash: fixture.context.genesis_hash,
        });
        let durable_wallet = fixture
            .platform
            .local_storage
            .lock()
            .unwrap()
            .insert(slot.clone(), vec![0xff])
            .unwrap();
        assert_eq!(
            actor
                .receive(
                    &fixture.context,
                    &registry,
                    acknowledgment(&actor, &identity, &peer, &packet.request_id, false)
                )
                .await,
            Err(Error::StorageUnavailable)
        );
        let view = actor.public_view(&fixture.context, vec![]).await.unwrap();
        assert_eq!(
            view.acknowledgments,
            vec![HostNativeChatAcknowledgment {
                peer_identity: identity.account,
                request_id: packet.request_id,
                response_code: 0,
            }]
        );
        assert!(
            actor
                .store
                .read(|state| state
                    .outbox
                    .iter()
                    .all(|entry| !matches!(entry.kind, OutgoingKind::Payment(_))))
                .await
                .unwrap()
        );
        fixture.tasks.stop();
        drop(registry);
        drop(actor);
        fixture
            .platform
            .local_storage
            .lock()
            .unwrap()
            .insert(slot, durable_wallet);

        let restarted = Fixture::on_platform(fixture.platform.clone());
        let actor = restarted.actor().await;
        let registry = NativeChatRegistry::default();
        let wallet = registry.wallet(&restarted.context).await.unwrap();
        assert_eq!(wallet.views(PRODUCT).await.unwrap(), vec![card.clone()]);
        // No packet is re-received. Reconcile must repair delivery before its
        // independent finalized-chain observation encounters the offline RPC.
        assert_eq!(
            actor.reconcile(&restarted.context, &registry).await,
            Err(Error::NetworkUnavailable)
        );
        let delivered = HostNativeChatPayment {
            state: HostNativeChatPaymentState::Delivered,
            ..card
        };
        assert_eq!(
            actor
                .public_view(&restarted.context, wallet.views(PRODUCT).await.unwrap())
                .await
                .unwrap()
                .payments,
            vec![delivered.clone()]
        );
        actor
            .replay_payment_acknowledgments(&restarted.context, &registry)
            .await
            .unwrap();
        assert_eq!(wallet.views(PRODUCT).await.unwrap(), vec![delivered]);
        assert!(
            restarted
                .platform
                .main_purse_chat_payment_reviews
                .lock()
                .unwrap()
                .is_empty()
        );
    });
}

#[test]
fn accepted_payment_rewraps_exact_memo_only_for_new_authenticated_roster() {
    block_on(async {
        let fixture = Fixture::new();
        let actor = fixture.actor().await;
        let identity = IdentityFixture::new();
        let old = DeviceFixture::new(1);
        let new = DeviceFixture::new(2);
        let offline = DeviceFixture::new(3);
        seed_peer(&actor, &identity, &[&old]).await;
        let registry = NativeChatRegistry::default();
        let added = wire::encode_device_added_message(
            "add-offline-device",
            fixture.timestamp,
            &offline.account(),
            &offline.public_key(),
        )
        .unwrap();
        assert_eq!(
            actor
                .receive(
                    &fixture.context,
                    &registry,
                    request(
                        &actor,
                        &identity,
                        &old,
                        "advertise-offline-device",
                        &[added]
                    ),
                )
                .await,
            Err(Error::NetworkUnavailable)
        );
        let update = outgoing(&actor, OutgoingKind::Revocation).await;
        assert!(
            !actor
                .public_view(&fixture.context, vec![])
                .await
                .unwrap()
                .peers[0]
                .ready_for_payments
        );
        actor
            .receive(
                &fixture.context,
                &registry,
                acknowledgment(&actor, &identity, &old, &update.request_id, false),
            )
            .await
            .unwrap();
        assert!(
            actor
                .public_view(&fixture.context, vec![])
                .await
                .unwrap()
                .peers[0]
                .ready_for_payments
        );
        let wallet = registry.wallet(&fixture.context).await.unwrap();
        let card = wallet
            .seed_accepted_for_test(
                &fixture.context,
                payment_intent(&identity),
                fixture.timestamp,
                &memo(),
            )
            .await
            .unwrap();
        let transport = transport(&actor, &fixture, &identity);
        transport.accept(&card, memo()).await.unwrap();
        // Chat committed custody, then the process died before either wallet
        // acceptance write. Roster repair must work without a new spend review.
        wallet
            .seed_handoff_ready_for_test(&fixture.context, card.operation_id)
            .await
            .unwrap();
        let before = outgoing(&actor, OutgoingKind::Payment(card.operation_id)).await;
        let wire::V2StatementTransportData::MultiRequest(before_wire) =
            open_output(&actor, &identity, &before.statement, false, false)
        else {
            panic!("payment must be a native multi-device request")
        };
        let original = open_body(
            &actor,
            &old,
            &before_wire.encrypted_request,
            &before_wire.devices_info,
        );
        assert_eq!(
            before_wire
                .devices_info
                .iter()
                .map(|device| device.statement_account_id)
                .collect::<Vec<_>>(),
            vec![old.account()],
            "an unacknowledged active device must receive no payment key"
        );
        for wrap in &before_wire.devices_info {
            assert!(
                wire::unwrap_multi_device_key(
                    &offline.secret,
                    &actor.public.chat_public_key,
                    &wrap.encrypted_key
                )
                .is_err()
            );
        }
        assert_eq!(
            actor
                .receive(
                    &fixture.context,
                    &registry,
                    acknowledgment(&actor, &identity, &offline, &before.request_id, false),
                )
                .await,
            Err(Error::InvalidStatement),
            "an excluded device cannot falsely acknowledge payment custody"
        );
        assert_eq!(wallet.views(PRODUCT).await.unwrap(), vec![card.clone()]);

        let controls = vec![
            wire::encode_device_added_message(
                "add-replacement",
                fixture.timestamp,
                &new.account(),
                &new.public_key(),
            )
            .unwrap(),
            wire::encode_device_removed_message(
                "remove-old",
                fixture.timestamp + 1,
                &old.account(),
            )
            .unwrap(),
        ];
        assert_eq!(
            actor
                .receive(
                    &fixture.context,
                    &registry,
                    request(&actor, &identity, &old, "signed-roster-change", &controls)
                )
                .await,
            Err(Error::NetworkUnavailable)
        );
        let public = actor.public_view(&fixture.context, vec![]).await.unwrap();
        assert_eq!(
            public.peers[0]
                .devices
                .iter()
                .map(|device| device.account_id)
                .collect::<std::collections::BTreeSet<_>>(),
            [offline.account(), new.account()].into_iter().collect()
        );
        assert!(!public.peers[0].ready_for_payments);
        let revocation = outgoing(&actor, OutgoingKind::Revocation).await;
        actor
            .receive(
                &fixture.context,
                &registry,
                acknowledgment(&actor, &identity, &new, &revocation.request_id, false),
            )
            .await
            .unwrap();
        assert!(
            actor
                .public_view(&fixture.context, vec![])
                .await
                .unwrap()
                .peers[0]
                .ready_for_payments
        );
        assert!(
            wallet
                .pending_handoffs(&fixture.context, PRODUCT, &[])
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            actor.reconcile(&fixture.context, &registry).await,
            Err(Error::NetworkUnavailable)
        );
        let after = outgoing(&actor, OutgoingKind::Payment(card.operation_id)).await;
        let wire::V2StatementTransportData::MultiRequest(after_wire) =
            open_output(&actor, &identity, &after.statement, false, false)
        else {
            panic!("repaired payment must remain native multi-device transport")
        };
        let repaired = open_body(
            &actor,
            &new,
            &after_wire.encrypted_request,
            &after_wire.devices_info,
        );
        assert_eq!(
            repaired.as_slice(),
            original.as_slice(),
            "rewrapping cannot mint a new message, amount, or spendable memo"
        );
        let decoded = wire::decode_message_exchange_request_plaintext(&repaired).unwrap();
        assert_eq!(
            decoded.request_id,
            format!("pay-{}", hex::encode(card.operation_id))
        );
        assert_eq!(decoded.messages.len(), 1);
        let message = wire::decode_message(&decoded.messages[0]).unwrap();
        assert_eq!(message.message_id, card.message_id);
        assert_eq!(message.timestamp, card.timestamp);
        let wire::V2ChatMessageContent::CoinageSend {
            total_value,
            coin_keys,
        } = message.content
        else {
            panic!("accepted memo must remain native CoinageSend")
        };
        assert_eq!(total_value, "250");
        assert_eq!(
            coin_keys,
            memo()
                .entries
                .iter()
                .map(|entry| entry.0.to_vec())
                .collect::<Vec<_>>()
        );
        assert_eq!(wallet.views(PRODUCT).await.unwrap(), vec![card]);
        assert_eq!(
            after_wire
                .devices_info
                .iter()
                .map(|device| device.statement_account_id)
                .collect::<Vec<_>>(),
            vec![new.account()]
        );
        for wrap in &after_wire.devices_info {
            assert!(
                wire::unwrap_multi_device_key(
                    &offline.secret,
                    &actor.public.chat_public_key,
                    &wrap.encrypted_key
                )
                .is_err(),
                "rewrapping must not include an active but unacknowledged device"
            );
            assert!(
                wire::unwrap_multi_device_key(
                    &old.secret,
                    &actor.public.chat_public_key,
                    &wrap.encrypted_key
                )
                .is_err(),
                "revoked device must not recover the new one-shot key even if it ignores recipient addressing"
            );
        }
        assert!(
            fixture
                .platform
                .main_purse_chat_payment_reviews
                .lock()
                .unwrap()
                .is_empty()
        );
    });
}

#[test]
fn revoked_expired_and_future_signed_packets_never_change_roster_or_emit_ack() {
    block_on(async {
        let fixture = Fixture::new();
        let actor = fixture.actor().await;
        let identity = IdentityFixture::new();
        let old = DeviceFixture::new(1);
        let current = DeviceFixture::new(2);
        let intruder = DeviceFixture::new(3);
        seed_peer(&actor, &identity, &[&old, &current]).await;
        let registry = NativeChatRegistry::default();
        let removal =
            wire::encode_device_removed_message("remove-old", fixture.timestamp, &old.account())
                .unwrap();
        assert_eq!(
            actor
                .receive(
                    &fixture.context,
                    &registry,
                    request(
                        &actor,
                        &identity,
                        &current,
                        "authorized-revocation",
                        &[removal]
                    )
                )
                .await,
            Err(Error::NetworkUnavailable)
        );
        let before = actor.public_view(&fixture.context, vec![]).await.unwrap();
        let queued = actor
            .store
            .read(|state| {
                state
                    .outbox
                    .iter()
                    .map(|entry| entry.request_id.clone())
                    .collect::<Vec<_>>()
            })
            .await
            .unwrap();
        let forged_control = wire::encode_device_added_message(
            "intruder",
            fixture.timestamp + 1,
            &intruder.account(),
            &intruder.public_key(),
        )
        .unwrap();
        let delayed = wire::encode_rich_text_message(
            "delayed",
            (current_unix_secs() - LIFETIME - 1) * 1000,
            Some("old but authentic plaintext"),
            None,
        )
        .unwrap();
        let future = wire::encode_rich_text_message(
            "future",
            (current_unix_secs() + CLOCK_SKEW + 60) * 1000,
            Some("invalid future plaintext"),
            None,
        )
        .unwrap();
        let expired = resign_with_expiry(
            &current,
            request(&actor, &identity, &current, "expired-request", &[delayed]),
            (current_unix_secs() - 1) << 32,
        );
        let mut tampered = request(
            &actor,
            &identity,
            &current,
            "invalid-signature",
            &[future.clone()],
        );
        tampered.data.as_mut().unwrap()[0] ^= 1;
        let resurrect = wire::encode_multi_chat_accepted_message(
            "late-acceptance",
            fixture.timestamp + 1,
            "old-invitation",
            &wire::V2PeerDevice {
                statement_account_id: old.account(),
                encryption_public_key: old.public_key(),
            },
        )
        .unwrap();
        let attacks = [
            request(
                &actor,
                &identity,
                &old,
                "revoked-control",
                &[forged_control],
            ),
            expired,
            request(&actor, &identity, &current, "future-request", &[future]),
            tampered,
            native_packet(
                &actor,
                &identity,
                &old,
                &wire::encode_transport_request_plaintext("revoked-root-acceptance", &[resurrect])
                    .unwrap(),
                false,
                true,
            ),
            acknowledgment(
                &actor,
                &identity,
                &old,
                &outgoing(&actor, OutgoingKind::Revocation).await.request_id,
                false,
            ),
        ];
        for packet in attacks {
            assert_eq!(
                actor.receive(&fixture.context, &registry, packet).await,
                Err(Error::InvalidStatement)
            );
            assert_eq!(
                actor.public_view(&fixture.context, vec![]).await.unwrap(),
                before
            );
            assert_eq!(
                actor
                    .store
                    .read(|state| state
                        .outbox
                        .iter()
                        .map(|entry| entry.request_id.clone())
                        .collect::<Vec<_>>())
                    .await
                    .unwrap(),
                queued,
                "an unauthenticated, expired, or future exchange must not enqueue an ACK"
            );
        }
    });
}

#[test]
fn ordinary_native_delivery_and_ack_work_but_guest_cannot_send_payment_or_control() {
    block_on(async {
        let fixture = Fixture::new();
        let actor = fixture.actor().await;
        let identity = IdentityFixture::new();
        let peer = DeviceFixture::new(1);
        seed_peer(&actor, &identity, &[&peer]).await;
        let registry = NativeChatRegistry::default();
        let before = actor.public_view(&fixture.context, vec![]).await.unwrap();
        let keys: Vec<_> = memo()
            .entries
            .iter()
            .map(|entry| entry.0.to_vec())
            .collect();
        let blocked = [
            wire::encode_coinage_send_message("guest-payment", fixture.timestamp, "250", &keys)
                .unwrap(),
            wire::encode_device_removed_message(
                "guest-revocation",
                fixture.timestamp,
                &peer.account(),
            )
            .unwrap(),
            wire::encode_device_added_message(
                "guest-admission",
                fixture.timestamp,
                &peer.account(),
                &peer.public_key(),
            )
            .unwrap(),
        ];
        let ordinary = wire::encode_rich_text_message(
            "ordinary-message",
            fixture.timestamp,
            Some("native hello"),
            None,
        )
        .unwrap();
        for (index, forbidden) in blocked.into_iter().enumerate() {
            assert_eq!(
                actor
                    .send(
                        &fixture.context,
                        identity.account,
                        format!("guest-injection-{index}"),
                        vec![ordinary.clone(), forbidden]
                    )
                    .await,
                Err(Error::InvalidRequest)
            );
            assert_eq!(
                actor.public_view(&fixture.context, vec![]).await.unwrap(),
                before
            );
            assert!(
                actor
                    .store
                    .read(|state| state.outbox.is_empty())
                    .await
                    .unwrap()
            );
        }
        assert_eq!(
            actor
                .send(
                    &fixture.context,
                    identity.account,
                    "ordinary-send".into(),
                    vec![ordinary.clone()]
                )
                .await,
            Err(Error::NetworkUnavailable)
        );
        let outgoing = outgoing(&actor, OutgoingKind::Ordinary).await;
        let wire::V2StatementTransportData::MultiRequest(native) =
            open_output(&actor, &identity, &outgoing.statement, false, false)
        else {
            panic!("ordinary guest send must use native multi-device request")
        };
        let body = open_body(
            &actor,
            &peer,
            &native.encrypted_request,
            &native.devices_info,
        );
        let native = wire::decode_message_exchange_request_plaintext(&body).unwrap();
        assert_eq!(native.request_id, outgoing.request_id);
        assert_ne!(
            native.request_id, "ordinary-send",
            "guest correlation IDs cannot select a Host transport operation"
        );
        assert_eq!(native.messages, vec![ordinary.clone()]);
        actor
            .receive(
                &fixture.context,
                &registry,
                acknowledgment(&actor, &identity, &peer, &native.request_id, false),
            )
            .await
            .unwrap();
        assert_eq!(
            actor
                .public_view(&fixture.context, vec![])
                .await
                .unwrap()
                .acknowledgments,
            vec![HostNativeChatAcknowledgment {
                peer_identity: identity.account,
                request_id: "ordinary-send".into(),
                response_code: 0
            }]
        );
        assert!(
            actor
                .store
                .read(|state| state.outbox.is_empty())
                .await
                .unwrap()
        );

        let incoming = request(
            &actor,
            &identity,
            &peer,
            "native-incoming",
            &[ordinary.clone()],
        );
        assert_eq!(
            actor
                .receive(&fixture.context, &registry, incoming.clone())
                .await,
            Err(Error::NetworkUnavailable)
        );
        let view = actor.public_view(&fixture.context, vec![]).await.unwrap();
        assert_eq!(
            view.messages,
            vec![HostNativeChatMessages {
                peer_identity: identity.account,
                incoming: true,
                request_id: "native-incoming".into(),
                messages: vec![ordinary],
            }]
        );
        let ack = actor
            .store
            .read(|state| {
                state
                    .outbox
                    .iter()
                    .find(|entry| entry.kind == OutgoingKind::Acknowledgment)
                    .unwrap()
                    .statement
                    .clone()
            })
            .await
            .unwrap();
        let wire::V2StatementTransportData::MultiResponse(native) =
            open_output(&actor, &identity, &ack, true, false)
        else {
            panic!("native incoming message must get a native multi-device ACK")
        };
        let body = open_body(
            &actor,
            &peer,
            &native.encrypted_response,
            &native.devices_info,
        );
        assert_eq!(
            wire::decode_message_exchange_response_plaintext(&body).unwrap(),
            wire::V2MessageExchangeResponse {
                request_id: "native-incoming".into(),
                response_code: 0,
            }
        );
        // An offline ACK persisted by the old Host used the requester's route.
        // Replaying the authenticated request must repair that queued response.
        let old_shared =
            wire::x25519_shared_secret(&peer.secret, &actor.public.identity_chat_public_key)
                .unwrap();
        let old_topic = route(
            &old_shared,
            &peer.account(),
            &actor.public.identity_account_id,
        );
        let old_data = wire::encrypt_multi_device_payload_with_nonce(
            &wire::hkdf_sha256_32(&old_shared).unwrap(),
            &wire::encode_transport_multi_response_plaintext(&native).unwrap(),
            [0x27; 12],
        )
        .unwrap();
        let signing_actor = actor.clone();
        actor
            .store
            .update(move |state| {
                let statement = signing_actor.sign(
                    state,
                    wire::chat_identity_response_topic(&old_topic).unwrap(),
                    vec![old_topic],
                    old_data,
                )?;
                state
                    .outbox
                    .iter_mut()
                    .find(|entry| entry.kind == OutgoingKind::Acknowledgment)
                    .unwrap()
                    .statement = statement;
                Ok(())
            })
            .await
            .unwrap();
        assert_eq!(
            actor.receive(&fixture.context, &registry, incoming).await,
            Err(Error::NetworkUnavailable)
        );
        assert_eq!(
            actor.public_view(&fixture.context, vec![]).await.unwrap(),
            view,
            "retrying authenticated native delivery cannot duplicate the conversation"
        );
        let repaired = self::outgoing(&actor, OutgoingKind::Acknowledgment).await;
        let wire::V2StatementTransportData::MultiResponse(native) =
            open_output(&actor, &identity, &repaired.statement, true, false)
        else {
            panic!("replayed request must produce a native-decodable response")
        };
        let body = open_body(
            &actor,
            &peer,
            &native.encrypted_response,
            &native.devices_info,
        );
        assert_eq!(
            wire::decode_message_exchange_response_plaintext(&body).unwrap(),
            wire::V2MessageExchangeResponse {
                request_id: "native-incoming".into(),
                response_code: 0,
            }
        );
        assert!(
            fixture
                .platform
                .main_purse_chat_payment_reviews
                .lock()
                .unwrap()
                .is_empty()
        );
    });
}

#[test]
fn refreshed_statement_delivers_old_admitted_peer_messages_without_rewriting_them() {
    block_on(async {
        let fixture = Fixture::new();
        let actor = fixture.actor().await;
        let identity = IdentityFixture::new();
        let peer = DeviceFixture::new(1);
        seed_peer(&actor, &identity, &[&peer]).await;
        let admitted_at = fixture.timestamp;
        actor
            .store
            .update(move |state| {
                state.peers[0].devices[0].timestamp = admitted_at;
                Ok(())
            })
            .await
            .unwrap();
        let registry = NativeChatRegistry::default();
        let message = wire::encode_rich_text_message(
            "queued-offline",
            fixture.timestamp - (LIFETIME + 86_400) * 1000,
            Some("delayed native message"),
            None,
        )
        .unwrap();
        let old_departure = wire::encode_left_chat_message(
            "earlier-departure",
            fixture.timestamp - (LIFETIME + 86_400) * 1000,
        )
        .unwrap();
        let packet = request(
            &actor,
            &identity,
            &peer,
            "offline-request",
            &[old_departure, message.clone()],
        );
        let expired = resign_with_expiry(&peer, packet, (current_unix_secs() - 1) << 32);
        assert_eq!(
            actor
                .receive(&fixture.context, &registry, expired.clone())
                .await,
            Err(Error::InvalidStatement)
        );
        assert!(
            actor
                .public_view(&fixture.context, vec![])
                .await
                .unwrap()
                .messages
                .is_empty()
        );
        assert!(
            actor
                .store
                .read(|state| state.outbox.is_empty())
                .await
                .unwrap()
        );
        let refreshed = resign_with_expiry(
            &peer,
            expired.clone(),
            (current_unix_secs() + LIFETIME) << 32,
        );
        assert_eq!(
            refreshed.data, expired.data,
            "refresh changes only the signed expiry, not old message content"
        );
        assert_eq!(
            actor
                .receive(&fixture.context, &registry, refreshed.clone())
                .await,
            Err(Error::NetworkUnavailable)
        );
        let view = actor.public_view(&fixture.context, vec![]).await.unwrap();
        assert_eq!(
            view.messages,
            vec![HostNativeChatMessages {
                peer_identity: identity.account,
                incoming: true,
                request_id: "offline-request".into(),
                messages: vec![message]
            }]
        );
        assert_eq!(
            view.peers[0].devices,
            vec![HostNativeChatPeerDevice {
                account_id: peer.account(),
                chat_public_key: peer.public_key(),
            }],
            "an old departure cannot roll back a later authenticated admission"
        );
        assert_eq!(
            outgoing(&actor, OutgoingKind::Acknowledgment)
                .await
                .request_id,
            "offline-request"
        );
        assert_eq!(
            actor.receive(&fixture.context, &registry, refreshed).await,
            Err(Error::NetworkUnavailable)
        );
        assert_eq!(
            actor.public_view(&fixture.context, vec![]).await.unwrap(),
            view
        );
    });
}

#[test]
fn native_acceptance_with_push_tokens_keeps_metadata_private_and_replay_bound() {
    block_on(async {
        let fixture = Fixture::new();
        let actor = fixture.actor().await;
        let identity = IdentityFixture::new();
        let peer = DeviceFixture::new(1);
        seed_outgoing_invitation(&actor, &identity, &[], fixture.timestamp).await;
        let registry = NativeChatRegistry::default();
        let accepted = wire::encode_multi_chat_accepted_message(
            "accepted",
            fixture.timestamp,
            "pending-invitation",
            &wire::V2PeerDevice {
                statement_account_id: peer.account(),
                encryption_public_key: peer.public_key(),
            },
        )
        .unwrap();
        let ordinary =
            wire::encode_rich_text_message("reply", fixture.timestamp, Some("native reply"), None)
                .unwrap();
        let token = wire::encode_token_message(
            "ios-token",
            fixture.timestamp,
            &[0xa1; 32],
            wire::V2PushPlatform::Ios,
        )
        .unwrap();
        let voip = wire::encode_token_message(
            "voip-token",
            fixture.timestamp,
            &[0xa2; 32],
            wire::V2PushPlatform::IosVoip,
        )
        .unwrap();
        let mut messages = vec![accepted, token, voip, ordinary.clone()];
        let packet = |messages: &[Vec<u8>]| {
            native_packet(
                &actor,
                &identity,
                &peer,
                &wire::encode_transport_request_plaintext("native-acceptance", messages).unwrap(),
                false,
                true,
            )
        };
        let before = actor.public_view(&fixture.context, vec![]).await.unwrap();
        messages[1].push(0);
        assert_eq!(
            actor
                .receive(&fixture.context, &registry, packet(&messages))
                .await,
            Err(Error::InvalidStatement)
        );
        assert_eq!(
            actor.public_view(&fixture.context, vec![]).await.unwrap(),
            before
        );
        messages[1].pop();
        let valid_packet = packet(&messages);
        assert_eq!(
            actor
                .receive(&fixture.context, &registry, valid_packet.clone())
                .await,
            Err(Error::NetworkUnavailable)
        );
        let view = actor.public_view(&fixture.context, vec![]).await.unwrap();
        assert_eq!(
            view.peers[0].devices,
            vec![HostNativeChatPeerDevice {
                account_id: peer.account(),
                chat_public_key: peer.public_key(),
            }]
        );
        assert_eq!(
            view.messages,
            vec![HostNativeChatMessages {
                peer_identity: identity.account,
                incoming: true,
                request_id: "native-acceptance".into(),
                messages: vec![ordinary],
            }]
        );
        assert_eq!(
            actor
                .receive(&fixture.context, &registry, valid_packet)
                .await,
            Err(Error::NetworkUnavailable)
        );
        assert_eq!(
            actor.public_view(&fixture.context, vec![]).await.unwrap(),
            view
        );
        messages[1] = wire::encode_token_message(
            "ios-token",
            fixture.timestamp,
            &[0xa3; 32],
            wire::V2PushPlatform::Ios,
        )
        .unwrap();
        assert_eq!(
            actor
                .receive(&fixture.context, &registry, packet(&messages))
                .await,
            Err(Error::InvalidStatement)
        );
        assert_eq!(
            actor.public_view(&fixture.context, vec![]).await.unwrap(),
            view
        );
    });
}

#[test]
fn acceptance_batch_authenticates_every_control_before_wallet_or_roster_effects() {
    block_on(async {
        let fixture = Fixture::new();
        let actor = fixture.actor().await;
        let identity = IdentityFixture::new();
        let peer = DeviceFixture::new(1);
        let intruder = DeviceFixture::new(2);
        seed_outgoing_invitation(&actor, &identity, &[], fixture.timestamp).await;
        let registry = NativeChatRegistry::default();
        let accepted = wire::encode_multi_chat_accepted_message(
            "accepted",
            fixture.timestamp,
            "pending-invitation",
            &wire::V2PeerDevice {
                statement_account_id: peer.account(),
                encryption_public_key: peer.public_key(),
            },
        )
        .unwrap();
        let ordinary = wire::encode_rich_text_message(
            "hello",
            fixture.timestamp,
            Some("not visible on rejection"),
            None,
        )
        .unwrap();
        let keys = memo()
            .entries
            .iter()
            .map(|entry| entry.0.to_vec())
            .collect::<Vec<_>>();
        let payment =
            wire::encode_coinage_send_message("private-payment", fixture.timestamp, "250", &keys)
                .unwrap();
        let rebound = wire::encode_device_added_message(
            "rebound-key",
            fixture.timestamp + 1,
            &peer.account(),
            &intruder.public_key(),
        )
        .unwrap();
        let before = actor.public_view(&fixture.context, vec![]).await.unwrap();
        let attacks = [
            native_packet(
                &actor,
                &identity,
                &peer,
                &wire::encode_transport_request_plaintext(
                    "late-invalid-control",
                    &[payment.clone(), ordinary.clone(), accepted.clone(), rebound],
                )
                .unwrap(),
                false,
                true,
            ),
            native_packet(
                &actor,
                &identity,
                &intruder,
                &wire::encode_transport_request_plaintext(
                    "wrong-acceptance-signer",
                    &[accepted, payment, ordinary],
                )
                .unwrap(),
                false,
                true,
            ),
        ];
        for packet in attacks {
            assert_eq!(
                actor.receive(&fixture.context, &registry, packet).await,
                Err(Error::InvalidStatement)
            );
            assert_eq!(
                actor.public_view(&fixture.context, vec![]).await.unwrap(),
                before
            );
            assert!(
                actor
                    .store
                    .read(|state| state.outbox.is_empty())
                    .await
                    .unwrap()
            );
            assert!(
                fixture.platform.chain_connects.lock().unwrap().is_empty(),
                "invalid controls must fail before Coinage effects"
            );
        }
    });
}

#[test]
fn contact_added_resolves_only_an_admitted_peers_pending_invitation() {
    block_on(async {
        let fixture = Fixture::new();
        let actor = fixture.actor().await;
        let identity = IdentityFixture::new();
        let peer = DeviceFixture::new(1);
        let intruder = DeviceFixture::new(2);
        seed_outgoing_invitation(&actor, &identity, &[&peer], fixture.timestamp).await;
        let registry = NativeChatRegistry::default();
        let contact =
            wire::encode_contact_added_message("contact-added", fixture.timestamp - 1000).unwrap();
        let forged = wire::encode_device_added_message(
            "self-admission",
            fixture.timestamp,
            &intruder.account(),
            &intruder.public_key(),
        )
        .unwrap();
        assert_eq!(
            actor
                .receive(
                    &fixture.context,
                    &registry,
                    native_packet(
                        &actor,
                        &identity,
                        &intruder,
                        &wire::encode_transport_request_plaintext(
                            "unbound-contact",
                            &[contact.clone(), forged]
                        )
                        .unwrap(),
                        false,
                        true
                    )
                )
                .await,
            Err(Error::InvalidStatement)
        );
        assert!(
            actor
                .store
                .read(|state| state.outbox.is_empty())
                .await
                .unwrap()
        );
        let too_new =
            wire::encode_contact_added_message("later-contact", fixture.timestamp + 1000).unwrap();
        assert_eq!(
            actor
                .receive(
                    &fixture.context,
                    &registry,
                    native_packet(
                        &actor,
                        &identity,
                        &peer,
                        &wire::encode_transport_request_plaintext(
                            "uncorrelated-contact",
                            &[too_new]
                        )
                        .unwrap(),
                        false,
                        true
                    )
                )
                .await,
            Err(Error::NetworkUnavailable)
        );
        assert_eq!(
            actor
                .store
                .read(|state| state.peers[0].invitation.clone())
                .await
                .unwrap()
                .as_deref(),
            Some("pending-invitation")
        );
        assert_eq!(
            actor
                .receive(
                    &fixture.context,
                    &registry,
                    native_packet(
                        &actor,
                        &identity,
                        &peer,
                        &wire::encode_transport_request_plaintext("correlated-contact", &[contact])
                            .unwrap(),
                        false,
                        true
                    )
                )
                .await,
            Err(Error::NetworkUnavailable)
        );
        let view = actor.public_view(&fixture.context, vec![]).await.unwrap();
        assert!(
            view.messages.is_empty(),
            "lifecycle notifications are not guest messages"
        );
        assert_eq!(
            view.peers[0].devices,
            vec![HostNativeChatPeerDevice {
                account_id: peer.account(),
                chat_public_key: peer.public_key()
            }]
        );
        assert_eq!(
            view.acknowledgments,
            vec![HostNativeChatAcknowledgment {
                peer_identity: identity.account,
                request_id: "pending-invitation".into(),
                response_code: 0,
            }]
        );
        assert!(
            actor
                .store
                .read(|state| state.peers[0].invitation.is_none()
                    && state.peers[0].invitation_timestamp.is_none())
                .await
                .unwrap()
        );
    });
}

#[test]
fn a_later_departure_blocks_contact_added_acceptance_in_the_same_batch() {
    block_on(async {
        let fixture = Fixture::new();
        let actor = fixture.actor().await;
        let identity = IdentityFixture::new();
        let peer = DeviceFixture::new(1);
        seed_outgoing_invitation(&actor, &identity, &[&peer], fixture.timestamp).await;
        let registry = NativeChatRegistry::default();
        let contact =
            wire::encode_contact_added_message("contact-added", fixture.timestamp - 1000).unwrap();
        let left = wire::encode_left_chat_message("left-chat", fixture.timestamp).unwrap();
        assert_eq!(
            actor
                .receive(
                    &fixture.context,
                    &registry,
                    native_packet(
                        &actor,
                        &identity,
                        &peer,
                        &wire::encode_transport_request_plaintext(
                            "contact-and-departure",
                            &[left, contact]
                        )
                        .unwrap(),
                        false,
                        true
                    )
                )
                .await,
            Err(Error::NetworkUnavailable)
        );
        let view = actor.public_view(&fixture.context, vec![]).await.unwrap();
        assert!(view.messages.is_empty());
        assert!(
            view.acknowledgments.is_empty(),
            "transport ACK must not imply invitation acceptance"
        );
        assert!(view.peers[0].devices.is_empty());
        assert_eq!(
            actor
                .store
                .read(|state| state.peers[0].invitation.clone())
                .await
                .unwrap()
                .as_deref(),
            Some("pending-invitation")
        );
    });
}

#[test]
fn mixed_acceptance_batch_waits_for_every_claim_plan_and_replays_after_reopen() {
    block_on(async {
        let fixture = Fixture::new();
        let actor = fixture.actor().await;
        let identity = IdentityFixture::new();
        let peer = DeviceFixture::new(1);
        let second_device = DeviceFixture::new(2);
        seed_outgoing_invitation(&actor, &identity, &[], fixture.timestamp).await;
        let registry = NativeChatRegistry::default();
        let wallet = registry.wallet(&fixture.context).await.unwrap();
        let first_memo = TransferMemo {
            entries: vec![MemoEntry(keypair(0x31).secret.to_bytes())],
            total_value: 160,
        };
        let second_memo = TransferMemo {
            entries: vec![MemoEntry(keypair(0x32).secret.to_bytes())],
            total_value: 80,
        };
        let first = wallet
            .seed_incoming_for_test(
                &fixture.context,
                PRODUCT,
                identity.account,
                "mixed-native-request",
                "first-payment",
                fixture.timestamp,
                &first_memo,
                true,
            )
            .await
            .unwrap();
        let second = wallet
            .seed_incoming_for_test(
                &fixture.context,
                PRODUCT,
                identity.account,
                "mixed-native-request",
                "second-payment",
                fixture.timestamp,
                &second_memo,
                false,
            )
            .await
            .unwrap();
        let accepted = wire::encode_multi_chat_accepted_message(
            "accepted",
            fixture.timestamp,
            "pending-invitation",
            &wire::V2PeerDevice {
                statement_account_id: peer.account(),
                encryption_public_key: peer.public_key(),
            },
        )
        .unwrap();
        let historical = wire::encode_multi_chat_accepted_message(
            "previous-acceptance",
            fixture.timestamp - 1000,
            "previous-invitation",
            &wire::V2PeerDevice {
                statement_account_id: peer.account(),
                encryption_public_key: peer.public_key(),
            },
        )
        .unwrap();
        let added = wire::encode_device_added_message(
            "second-device",
            fixture.timestamp + 1,
            &second_device.account(),
            &second_device.public_key(),
        )
        .unwrap();
        let ordinary = wire::encode_rich_text_message(
            "welcome",
            fixture.timestamp,
            Some("native batched greeting"),
            None,
        )
        .unwrap();
        let first_wire = wire::encode_coinage_send_message(
            "first-payment",
            fixture.timestamp,
            "160",
            &[first_memo.entries[0].0.to_vec()],
        )
        .unwrap();
        let second_wire = wire::encode_coinage_send_message(
            "second-payment",
            fixture.timestamp,
            "80",
            &[second_memo.entries[0].0.to_vec()],
        )
        .unwrap();
        let mut messages = vec![
            ordinary.clone(),
            first_wire,
            added,
            historical,
            accepted,
            second_wire,
        ];
        let packet = native_packet(
            &actor,
            &identity,
            &peer,
            &wire::encode_transport_request_plaintext("mixed-native-request", &messages).unwrap(),
            false,
            true,
        );
        let before = actor
            .public_view(&fixture.context, wallet.views(PRODUCT).await.unwrap())
            .await
            .unwrap();
        assert_eq!(
            actor
                .receive(&fixture.context, &registry, packet.clone())
                .await,
            Err(Error::NetworkUnavailable)
        );
        assert_eq!(
            actor
                .public_view(&fixture.context, wallet.views(PRODUCT).await.unwrap())
                .await
                .unwrap(),
            before,
            "durable first claim cannot admit a roster, expose text, or acknowledge an unplanned second claim"
        );
        assert!(
            actor
                .store
                .read(|state| state.outbox.is_empty())
                .await
                .unwrap()
        );

        wallet
            .persist_incoming_plan_for_test(&second_memo)
            .await
            .unwrap();
        assert_eq!(
            actor
                .receive(&fixture.context, &registry, packet.clone())
                .await,
            Err(Error::NetworkUnavailable)
        );
        let cards = wallet.views(PRODUCT).await.unwrap();
        assert_eq!(cards.len(), 2);
        assert!(
            cards.contains(&first) && cards.contains(&second),
            "same native request carries two independent payment identities"
        );
        let view = actor.public_view(&fixture.context, cards).await.unwrap();
        assert_eq!(
            view.messages,
            vec![HostNativeChatMessages {
                peer_identity: identity.account,
                incoming: true,
                request_id: "mixed-native-request".into(),
                messages: vec![ordinary]
            }]
        );
        assert_eq!(
            view.peers[0].devices,
            vec![
                HostNativeChatPeerDevice {
                    account_id: peer.account(),
                    chat_public_key: peer.public_key()
                },
                HostNativeChatPeerDevice {
                    account_id: second_device.account(),
                    chat_public_key: second_device.public_key()
                },
            ]
        );
        assert_eq!(
            view.acknowledgments,
            vec![HostNativeChatAcknowledgment {
                peer_identity: identity.account,
                request_id: "pending-invitation".into(),
                response_code: 0,
            }]
        );
        let ack = outgoing(&actor, OutgoingKind::Acknowledgment).await;
        assert_eq!(
            open_output(&actor, &identity, &ack.statement, true, true),
            wire::V2StatementTransportData::Response {
                request_id: "mixed-native-request".into(),
                response_code: 0
            }
        );
        messages[0] = wire::encode_rich_text_message(
            "welcome",
            fixture.timestamp,
            Some("mutated retry"),
            None,
        )
        .unwrap();
        assert_eq!(
            actor
                .receive(
                    &fixture.context,
                    &registry,
                    native_packet(
                        &actor,
                        &identity,
                        &peer,
                        &wire::encode_transport_request_plaintext(
                            "mixed-native-request",
                            &messages
                        )
                        .unwrap(),
                        false,
                        true
                    )
                )
                .await,
            Err(Error::InvalidStatement)
        );
        assert_eq!(
            actor
                .public_view(&fixture.context, wallet.views(PRODUCT).await.unwrap())
                .await
                .unwrap(),
            view
        );
        fixture.tasks.stop();
        drop(wallet);
        drop(registry);
        drop(actor);

        let restarted = Fixture::on_platform(fixture.platform.clone());
        let actor = restarted.actor().await;
        let registry = NativeChatRegistry::default();
        let wallet = registry.wallet(&restarted.context).await.unwrap();
        assert_eq!(
            actor
                .public_view(&restarted.context, wallet.views(PRODUCT).await.unwrap())
                .await
                .unwrap(),
            view
        );
        assert_eq!(
            actor.receive(&restarted.context, &registry, packet).await,
            Err(Error::NetworkUnavailable)
        );
        assert_eq!(
            actor
                .public_view(&restarted.context, wallet.views(PRODUCT).await.unwrap())
                .await
                .unwrap(),
            view,
            "restored batch retries cannot duplicate payments, acceptance, or visible conversation"
        );
    });
}

#[test]
fn an_ordinary_ack_cannot_acknowledge_a_payment_with_a_colliding_guest_request_id() {
    block_on(async {
        let fixture = Fixture::new();
        let actor = fixture.actor().await;
        let identity = IdentityFixture::new();
        let peer = DeviceFixture::new(1);
        seed_peer(&actor, &identity, &[&peer]).await;
        let registry = NativeChatRegistry::default();
        let wallet = registry.wallet(&fixture.context).await.unwrap();
        let card = wallet
            .seed_accepted_for_test(
                &fixture.context,
                payment_intent(&identity),
                fixture.timestamp,
                &memo(),
            )
            .await
            .unwrap();
        transport(&actor, &fixture, &identity)
            .accept(&card, memo())
            .await
            .unwrap();
        let payment = outgoing(&actor, OutgoingKind::Payment(card.operation_id)).await;
        let ordinary_message = wire::encode_rich_text_message(
            "ordinary-collision",
            fixture.timestamp,
            Some("not a payment"),
            None,
        )
        .unwrap();
        assert_eq!(
            actor
                .send(
                    &fixture.context,
                    identity.account,
                    payment.request_id.clone(),
                    vec![ordinary_message]
                )
                .await,
            Err(Error::NetworkUnavailable)
        );
        let ordinary = outgoing(&actor, OutgoingKind::Ordinary).await;
        assert_ne!(ordinary.request_id, payment.request_id);
        actor
            .receive(
                &fixture.context,
                &registry,
                acknowledgment(&actor, &identity, &peer, &ordinary.request_id, false),
            )
            .await
            .unwrap();
        assert_eq!(
            wallet.views(PRODUCT).await.unwrap(),
            vec![card.clone()],
            "acknowledging ordinary text cannot mark the colliding payment delivered"
        );
        assert_eq!(
            outgoing(&actor, OutgoingKind::Payment(card.operation_id))
                .await
                .request_id,
            payment.request_id
        );
        assert!(
            actor
                .store
                .read(|state| state
                    .outbox
                    .iter()
                    .all(|entry| entry.kind != OutgoingKind::Ordinary))
                .await
                .unwrap()
        );
        assert_eq!(
            actor
                .public_view(&fixture.context, vec![])
                .await
                .unwrap()
                .acknowledgments,
            vec![HostNativeChatAcknowledgment {
                peer_identity: identity.account,
                request_id: payment.request_id.clone(),
                response_code: 0
            }]
        );
        actor
            .receive(
                &fixture.context,
                &registry,
                acknowledgment(&actor, &identity, &peer, &payment.request_id, false),
            )
            .await
            .unwrap();
        assert_eq!(
            wallet.views(PRODUCT).await.unwrap(),
            vec![HostNativeChatPayment {
                state: HostNativeChatPaymentState::Delivered,
                ..card
            }]
        );
        assert!(
            actor
                .store
                .read(|state| state.outbox.is_empty())
                .await
                .unwrap()
        );
    });
}

async fn set_background_grants(
    platform: &StubPlatform,
    submit: truapi_platform::PermissionAuthorizationStatus,
) {
    set_product_background_grants(platform, PRODUCT, submit).await;
}

async fn set_product_background_grants(
    platform: &StubPlatform,
    product: &str,
    submit: truapi_platform::PermissionAuthorizationStatus,
) {
    use crate::host_logic::permissions::PermissionsService;
    use truapi_platform::{PermissionAuthorizationRequest, PermissionAuthorizationStatus};
    let permissions = PermissionsService::new(platform, platform, product);
    permissions
        .set_authorization_status(
            &PermissionAuthorizationRequest::ChatAuthority,
            PermissionAuthorizationStatus::Authorized,
        )
        .await
        .unwrap();
    permissions
        .set_authorization_status(
            &PermissionAuthorizationRequest::Remote(RemotePermissionRequest {
                permission: RemotePermission::StatementSubmit,
            }),
            submit,
        )
        .await
        .unwrap();
}

fn notification(statement: SignedStatement) -> serde_json::Value {
    serde_json::json!({
        "event": "newStatements",
        "data": {
            "statements": [format!("0x{}", hex::encode(signed_statement_to_scale(statement).unwrap()))],
            "remaining": 0,
        }
    })
}

#[test]
fn background_notification_keeps_claim_before_ack_and_rechecks_grants() {
    block_on(async {
        use super::super::background::receive_notification;
        use truapi_platform::PermissionAuthorizationStatus;
        let fixture = Fixture::new();
        set_background_grants(&fixture.platform, PermissionAuthorizationStatus::Authorized).await;
        let actor = fixture.actor().await;
        let identity = IdentityFixture::new();
        let peer = DeviceFixture::new(1);
        seed_peer(&actor, &identity, &[&peer]).await;
        let registry = NativeChatRegistry::default();
        let wallet = registry.wallet(&fixture.context).await.unwrap();
        // Two source keys correspond to the 160 + 80 native denominations.
        let mut memo = memo();
        memo.total_value = 240;
        let card = wallet
            .seed_incoming_for_test(
                &fixture.context,
                PRODUCT,
                identity.account,
                "background-request",
                "background-payment",
                fixture.timestamp,
                &memo,
                false,
            )
            .await
            .unwrap();
        let payment = wire::encode_coinage_send_message(
            "background-payment",
            fixture.timestamp,
            &memo.total_value.to_string(),
            &memo
                .entries
                .iter()
                .map(|entry| entry.0.to_vec())
                .collect::<Vec<_>>(),
        )
        .unwrap();
        let ordinary = wire::encode_rich_text_message(
            "background-text",
            fixture.timestamp,
            Some("received without a guest"),
            None,
        )
        .unwrap();
        let packet = notification(request(
            &actor,
            &identity,
            &peer,
            "background-request",
            &[payment, ordinary.clone()],
        ));
        receive_notification(&fixture.context, PRODUCT, &actor, &registry, packet.clone())
            .await
            .unwrap();
        assert!(
            actor
                .public_view(&fixture.context, vec![])
                .await
                .unwrap()
                .messages
                .is_empty()
        );
        assert!(
            actor
                .store
                .read(|state| state.outbox.is_empty())
                .await
                .unwrap(),
            "an unplanned private claim must not emit an ACK"
        );
        wallet.persist_incoming_plan_for_test(&memo).await.unwrap();
        receive_notification(&fixture.context, PRODUCT, &actor, &registry, packet.clone())
            .await
            .unwrap();
        let view = actor
            .public_view(&fixture.context, wallet.views(PRODUCT).await.unwrap())
            .await
            .unwrap();
        assert_eq!(view.payments, vec![card]);
        assert_eq!(
            view.messages,
            vec![HostNativeChatMessages {
                peer_identity: identity.account,
                incoming: true,
                request_id: "background-request".into(),
                messages: vec![ordinary],
            }]
        );
        let ack = outgoing(&actor, OutgoingKind::Acknowledgment).await;
        assert_eq!(ack.request_id, "background-request");
        receive_notification(&fixture.context, PRODUCT, &actor, &registry, packet.clone())
            .await
            .unwrap();
        assert_eq!(
            actor
                .public_view(&fixture.context, wallet.views(PRODUCT).await.unwrap())
                .await
                .unwrap(),
            view,
            "replayed subscription pages must use durable receipts"
        );
        set_background_grants(&fixture.platform, PermissionAuthorizationStatus::Denied).await;
        assert_eq!(
            receive_notification(&fixture.context, PRODUCT, &actor, &registry, packet.clone())
                .await,
            Err(Error::AccessNotGranted)
        );
        fixture.tasks.live.store(false, Ordering::Release);
        assert_eq!(
            receive_notification(&fixture.context, PRODUCT, &actor, &registry, packet).await,
            Err(Error::NotConnected)
        );
        assert!(
            fixture
                .platform
                .remote_permission_requests
                .lock()
                .unwrap()
                .is_empty()
        );
    });
}

async fn await_background<T>(future: impl Future<Output = T>) -> T {
    use futures::future::{Either, select};
    let deadline = futures_timer::Delay::new(std::time::Duration::from_secs(5));
    futures::pin_mut!(future, deadline);
    match select(future, deadline).await {
        Either::Left((result, _)) => result,
        Either::Right(_) => panic!("owned background task did not reach the expected state"),
    }
}

#[test]
fn owned_subscription_survives_guest_return_and_stops_on_revocation_and_replacement() {
    block_on(async {
        use truapi_platform::PermissionAuthorizationStatus;
        let seed = Fixture::new();
        let actor = seed.actor().await;
        let identity = IdentityFixture::new();
        let peer = DeviceFixture::new(1);
        seed_peer(&actor, &identity, &[&peer]).await;
        let ordinary = wire::encode_rich_text_message(
            "owned-text",
            seed.timestamp,
            Some("arrived after Initialize returned"),
            None,
        )
        .unwrap();
        let statement = request(
            &actor,
            &identity,
            &peer,
            "owned-request",
            &[ordinary.clone()],
        );
        let platform = Arc::new(StubPlatform {
            local_storage: seed.platform.local_storage.clone(),
            rpc_responses: vec![
                crate::test_support::subscribe_ack_frame("truapi:1", "owned-chat"),
                crate::test_support::new_statements_frame(
                    "owned-chat",
                    vec![signed_statement_to_scale(statement).unwrap()],
                ),
            ],
            ..Default::default()
        });
        drop(actor);
        seed.tasks.stop();
        let fixture = Fixture::on_platform(platform.clone());
        set_background_grants(&platform, PermissionAuthorizationStatus::Authorized).await;
        let registry = NativeChatRegistry::default();
        let first = registry
            .execute(
                fixture.context.clone(),
                PRODUCT.into(),
                HostProductDeviceChatRequest::Initialize,
            )
            .await
            .unwrap();
        let actor = registry.chat(&fixture.context, PRODUCT).await.unwrap();
        // No product connection or Receive call exists for the incoming packet.
        let view = await_background(async {
            loop {
                let view = actor.public_view(&fixture.context, vec![]).await.unwrap();
                if !view.messages.is_empty() {
                    break view;
                }
                futures_timer::Delay::new(std::time::Duration::from_millis(1)).await;
            }
        })
        .await;
        assert_eq!(view.device, first.device);
        assert_eq!(
            view.messages,
            vec![HostNativeChatMessages {
                peer_identity: identity.account,
                incoming: true,
                request_id: "owned-request".into(),
                messages: vec![ordinary],
            }]
        );
        registry
            .execute(
                fixture.context.clone(),
                PRODUCT.into(),
                HostProductDeviceChatRequest::Initialize,
            )
            .await
            .unwrap();
        set_background_grants(&platform, PermissionAuthorizationStatus::Denied).await;
        await_background(async {
            while fixture.context.services.worker_ledger.count(PRODUCT) != 0 {
                futures_timer::Delay::new(std::time::Duration::from_millis(1)).await;
            }
        })
        .await;
        let subscriptions = || {
            platform
                .sent_rpc
                .lock()
                .unwrap()
                .iter()
                .filter(|request| {
                    serde_json::from_str::<serde_json::Value>(request).unwrap()["method"]
                        == "statement_subscribeStatement"
                })
                .count()
        };
        assert_eq!(
            subscriptions(),
            1,
            "repeated Initialize must not duplicate subscriptions"
        );
        assert!(
            platform
                .remote_permission_requests
                .lock()
                .unwrap()
                .is_empty()
        );
        set_background_grants(&platform, PermissionAuthorizationStatus::Authorized).await;
        let mut replacement = fixture.context.clone();
        replacement.session.validation_id = vec![2];
        let live = Arc::new(AtomicBool::new(true));
        let valid = live.clone();
        replacement.session_valid = Arc::new(move || valid.load(Ordering::Acquire));
        fixture.tasks.live.store(false, Ordering::Release);
        registry.stop_receiving();
        registry.resume_receiving(replacement.clone());
        await_background(async {
            while subscriptions() != 2 {
                futures_timer::Delay::new(std::time::Duration::from_millis(1)).await;
            }
        })
        .await;
        assert_eq!(
            actor
                .public_view(&replacement, vec![])
                .await
                .unwrap()
                .messages,
            view.messages
        );
        live.store(false, Ordering::Release);
        registry.stop_receiving();
        await_background(async {
            while replacement.services.worker_ledger.count(PRODUCT) != 0 {
                futures_timer::Delay::new(std::time::Duration::from_millis(1)).await;
            }
        })
        .await;
    });
}

#[test]
fn cold_restart_restores_consented_receive_without_product_launch_and_honors_forget() {
    block_on(async {
        use truapi_platform::PermissionAuthorizationStatus;
        let seed = Fixture::new();
        set_background_grants(&seed.platform, PermissionAuthorizationStatus::Authorized).await;
        let first_registry = NativeChatRegistry::default();
        let first = first_registry
            .execute(
                seed.context.clone(),
                PRODUCT.into(),
                HostProductDeviceChatRequest::Initialize,
            )
            .await
            .unwrap();
        let actor = first_registry.chat(&seed.context, PRODUCT).await.unwrap();
        let identity = IdentityFixture::new();
        let peer = DeviceFixture::new(1);
        seed_peer(&actor, &identity, &[&peer]).await;
        let ordinary = wire::encode_rich_text_message(
            "restart-text",
            seed.timestamp,
            Some("arrived before any product was reopened"),
            None,
        )
        .unwrap();
        let statement = request(
            &actor,
            &identity,
            &peer,
            "restart-request",
            &[ordinary.clone()],
        );
        let platform = Arc::new(StubPlatform {
            local_storage: seed.platform.local_storage.clone(),
            rpc_responses: vec![
                crate::test_support::subscribe_ack_frame("truapi:1", "restarted-chat"),
                crate::test_support::new_statements_frame(
                    "restarted-chat",
                    vec![signed_statement_to_scale(statement).unwrap()],
                ),
            ],
            ..Default::default()
        });
        first_registry.stop_receiving();
        seed.tasks.stop();
        drop(actor);
        drop(first_registry);

        let fixture = Fixture::on_platform(platform.clone());
        let restored = NativeChatRegistry::default();
        // An actual new registry: no guest execution or Initialize after unlock.
        restored.restore_receiving(&fixture.context).await.unwrap();
        let actor = restored.chat(&fixture.context, PRODUCT).await.unwrap();
        let view = await_background(async {
            loop {
                let view = actor.public_view(&fixture.context, vec![]).await.unwrap();
                if !view.messages.is_empty() {
                    break view;
                }
                futures_timer::Delay::new(std::time::Duration::from_millis(1)).await;
            }
        })
        .await;
        assert_eq!(view.device, first.device);
        assert_eq!(
            view.messages,
            vec![HostNativeChatMessages {
                peer_identity: identity.account,
                incoming: true,
                request_id: "restart-request".into(),
                messages: vec![ordinary],
            }]
        );
        assert!(
            platform
                .remote_permission_requests
                .lock()
                .unwrap()
                .is_empty()
        );

        restored
            .forget_product(&fixture.context, PRODUCT)
            .await
            .unwrap();
        await_background(async {
            while fixture.context.services.worker_ledger.count(PRODUCT) != 0 {
                futures_timer::Delay::new(std::time::Duration::from_millis(1)).await;
            }
        })
        .await;
        let forgotten = NativeChatRegistry::default();
        forgotten.restore_receiving(&fixture.context).await.unwrap();
        assert_eq!(fixture.context.services.worker_ledger.count(PRODUCT), 0);
        assert_eq!(
            actor
                .public_view(&fixture.context, vec![])
                .await
                .unwrap()
                .messages,
            view.messages,
            "forgetting reception must not erase durable history or custody"
        );
    });
}

#[test]
fn cold_restart_does_not_restore_revoked_or_missing_chat_devices() {
    block_on(async {
        use truapi_platform::PermissionAuthorizationStatus;
        let fixture = Fixture::new();
        let registry = NativeChatRegistry::default();
        registry
            .execute(
                fixture.context.clone(),
                PRODUCT.into(),
                HostProductDeviceChatRequest::Initialize,
            )
            .await
            .unwrap();
        set_background_grants(&fixture.platform, PermissionAuthorizationStatus::Denied).await;
        let restored = NativeChatRegistry::default();
        restored.restore_receiving(&fixture.context).await.unwrap();
        assert_eq!(fixture.context.services.worker_ledger.count(PRODUCT), 0);
        assert!(
            fixture
                .platform
                .remote_permission_requests
                .lock()
                .unwrap()
                .is_empty()
        );

        set_background_grants(&fixture.platform, PermissionAuthorizationStatus::Authorized).await;
        truapi_platform::CoreStorage::clear_core_storage(
            fixture.platform.as_ref(),
            CoreStorageKey::NativeChatDevice {
                root_public_key: fixture.context.session.public_key,
                genesis_hash: fixture.context.genesis_hash,
                product_id: PRODUCT.into(),
            },
        )
        .await
        .unwrap();
        assert!(matches!(
            restored.restore_receiving(&fixture.context).await,
            Err(Error::StorageUnavailable)
        ));
        assert_eq!(fixture.context.services.worker_ledger.count(PRODUCT), 0);
    });
}

#[test]
fn retryable_open_and_session_release_preserve_identity_without_retaining_owners() {
    block_on(async {
        let fixture = Fixture::new();
        let registry = NativeChatRegistry::default();
        let key = core_storage_test_key(CoreStorageKey::NativeChatDevice {
            root_public_key: fixture.context.session.public_key,
            genesis_hash: fixture.context.genesis_hash,
            product_id: PRODUCT.into(),
        });
        fixture.platform.core_read_failures.lock().insert(key);
        assert!(matches!(
            registry.chat(&fixture.context, PRODUCT).await,
            Err(Error::StorageUnavailable)
        ));
        fixture.platform.core_read_failures.lock().clear();
        let actor = registry.chat(&fixture.context, PRODUCT).await.unwrap();
        let device = actor.public.clone();
        let wallet = registry.wallet(&fixture.context).await.unwrap();
        let actor_lifetime = Arc::downgrade(&actor);
        let wallet_lifetime = Arc::downgrade(&wallet);
        fixture.tasks.live.store(false, Ordering::Release);
        registry.release();
        assert!(matches!(
            registry.chat(&fixture.context, PRODUCT).await,
            Err(Error::NotConnected)
        ));
        let restarted = Fixture::on_platform(fixture.platform.clone());
        // An old operation retaining either owner must exclude a new allocator.
        assert!(matches!(
            registry.chat(&restarted.context, PRODUCT).await,
            Err(Error::StorageUnavailable)
        ));
        assert!(matches!(
            registry.wallet(&restarted.context).await,
            Err(Error::StorageUnavailable)
        ));
        drop(actor);
        drop(wallet);
        assert!(actor_lifetime.upgrade().is_none());
        assert!(wallet_lifetime.upgrade().is_none());
        assert_eq!(
            registry
                .chat(&restarted.context, PRODUCT)
                .await
                .unwrap()
                .public,
            device
        );
        registry.wallet(&restarted.context).await.unwrap();
    });
}

#[test]
fn cold_restore_reports_bad_devices_but_starts_every_valid_product() {
    block_on(async {
        use truapi_platform::PermissionAuthorizationStatus;
        let fixture = Fixture::new();
        let registry = NativeChatRegistry::default();
        for product in ["aaa.dot", "bbb.dot", PRODUCT] {
            set_product_background_grants(
                &fixture.platform,
                product,
                PermissionAuthorizationStatus::Authorized,
            )
            .await;
            registry.chat(&fixture.context, product).await.unwrap();
            registry
                .remember_product(&fixture.context, product)
                .await
                .unwrap();
        }
        let device = registry
            .chat(&fixture.context, PRODUCT)
            .await
            .unwrap()
            .public
            .clone();
        registry.release();
        let key = |product: &str| {
            core_storage_test_key(CoreStorageKey::NativeChatDevice {
                root_public_key: fixture.context.session.public_key,
                genesis_hash: fixture.context.genesis_hash,
                product_id: product.into(),
            })
        };
        fixture
            .platform
            .local_storage
            .lock()
            .unwrap()
            .remove(&key("aaa.dot"));
        fixture
            .platform
            .local_storage
            .lock()
            .unwrap()
            .insert(key("bbb.dot"), vec![0xff]);
        assert_eq!(
            registry.restore_receiving(&fixture.context).await,
            Err(Error::StorageUnavailable)
        );
        await_background(async {
            while fixture.context.services.worker_ledger.count(PRODUCT) != 1 {
                futures_timer::Delay::new(std::time::Duration::from_millis(1)).await;
            }
        })
        .await;
        assert_eq!(
            registry
                .chat(&fixture.context, PRODUCT)
                .await
                .unwrap()
                .public,
            device
        );
        assert_eq!(fixture.context.services.worker_ledger.count("aaa.dot"), 0);
        assert_eq!(fixture.context.services.worker_ledger.count("bbb.dot"), 0);
        let stored = fixture.platform.local_storage.lock().unwrap();
        assert!(!stored.contains_key(&key("aaa.dot")));
        assert_eq!(stored.get(&key("bbb.dot")), Some(&vec![0xff]));
    });
}

#[test]
fn authorization_storage_outage_pauses_and_recovers_without_reopening_then_denial_stops() {
    block_on(async {
        use super::super::background::{require_authorized, require_upload_authorized};
        use truapi_platform::PermissionAuthorizationStatus;
        let platform = Arc::new(StubPlatform {
            chain_connect_pending: true,
            ..Default::default()
        });
        let fixture = Fixture::on_platform(platform.clone());
        set_background_grants(&platform, PermissionAuthorizationStatus::Authorized).await;
        let registry = NativeChatRegistry::default();
        registry
            .execute(
                fixture.context.clone(),
                PRODUCT.into(),
                HostProductDeviceChatRequest::Initialize,
            )
            .await
            .unwrap();
        await_background(async {
            while platform.chain_connects.lock().unwrap().len() != 1 {
                futures_timer::Delay::new(std::time::Duration::from_millis(1)).await;
            }
        })
        .await;
        platform
            .core_read_failures
            .lock()
            .insert(core_storage_test_key(
                CoreStorageKey::remote_permission_authorization(
                    PRODUCT,
                    &RemotePermissionRequest {
                        permission: RemotePermission::StatementSubmit,
                    },
                ),
            ));
        assert_eq!(
            require_authorized(&fixture.context, PRODUCT).await,
            Err(Error::StorageUnavailable)
        );
        assert_eq!(
            require_upload_authorized(&fixture.context, PRODUCT).await,
            Err(Error::StorageUnavailable)
        );
        await_background(async {
            while !platform.pending_connect_dropped.load(Ordering::Acquire) {
                futures_timer::Delay::new(std::time::Duration::from_millis(1)).await;
            }
        })
        .await;
        assert_eq!(fixture.context.services.worker_ledger.count(PRODUCT), 1);
        platform.core_read_failures.lock().clear();
        await_background(async {
            while platform.chain_connects.lock().unwrap().len() != 2 {
                futures_timer::Delay::new(std::time::Duration::from_millis(1)).await;
            }
        })
        .await;
        platform
            .core_read_failures
            .lock()
            .insert(core_storage_test_key(
                CoreStorageKey::remote_permission_authorization(
                    PRODUCT,
                    &RemotePermissionRequest {
                        permission: RemotePermission::PreimageSubmit,
                    },
                ),
            ));
        assert_eq!(require_authorized(&fixture.context, PRODUCT).await, Ok(()));
        assert_eq!(
            require_upload_authorized(&fixture.context, PRODUCT).await,
            Err(Error::StorageUnavailable)
        );
        platform.core_read_failures.lock().clear();
        set_background_grants(&platform, PermissionAuthorizationStatus::Denied).await;
        assert_eq!(
            require_authorized(&fixture.context, PRODUCT).await,
            Err(Error::AccessNotGranted)
        );
        await_background(async {
            while fixture.context.services.worker_ledger.count(PRODUCT) != 0 {
                futures_timer::Delay::new(std::time::Duration::from_millis(1)).await;
            }
        })
        .await;
        assert!(
            platform
                .remote_permission_requests
                .lock()
                .unwrap()
                .is_empty()
        );
    });
}

#[test]
fn failed_product_forget_rebinds_unrelated_receivers_without_readable_or_writable_index() {
    block_on(async {
        use truapi_platform::PermissionAuthorizationStatus;
        for fail_read in [true, false] {
            let platform = Arc::new(StubPlatform {
                chain_connect_pending: true,
                ..Default::default()
            });
            let fixture = Fixture::on_platform(platform.clone());
            let registry = NativeChatRegistry::default();
            for product in ["aaa.dot", PRODUCT] {
                set_product_background_grants(
                    &platform,
                    product,
                    PermissionAuthorizationStatus::Authorized,
                )
                .await;
                registry
                    .execute(
                        fixture.context.clone(),
                        product.into(),
                        HostProductDeviceChatRequest::Initialize,
                    )
                    .await
                    .unwrap();
            }
            await_background(async {
                while platform.chain_connects.lock().unwrap().len() != 2 {
                    futures_timer::Delay::new(std::time::Duration::from_millis(1)).await;
                }
            })
            .await;
            set_product_background_grants(
                &platform,
                "aaa.dot",
                PermissionAuthorizationStatus::Denied,
            )
            .await;
            let mut replacement = fixture.context.clone();
            replacement.session.validation_id = vec![2];
            replacement.session_valid = Arc::new(|| true);
            fixture.tasks.live.store(false, Ordering::Release);
            let index = core_storage_test_key(NativeChatRegistry::products_key(&replacement));
            if fail_read {
                platform.core_read_failures.lock().insert(index);
            } else {
                platform.core_write_failures.lock().insert(index);
            }
            assert_eq!(
                registry.forget_product(&replacement, "aaa.dot").await,
                Err(Error::StorageUnavailable)
            );
            await_background(async {
                while platform.chain_connects.lock().unwrap().len() < 3
                    || replacement.services.worker_ledger.count(PRODUCT) != 1
                    || replacement.services.worker_ledger.count("aaa.dot") != 0
                {
                    futures_timer::Delay::new(std::time::Duration::from_millis(1)).await;
                }
            })
            .await;
            assert!(
                platform
                    .remote_permission_requests
                    .lock()
                    .unwrap()
                    .is_empty()
            );
        }
    });
}
