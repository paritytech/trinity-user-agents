// SPDX-License-Identifier: AGPL-3.0-only
//! Native signed/encrypted packets against the real actor and encrypted stores.
#![cfg(not(target_arch = "wasm32"))]

mod hop_history;
mod native_wallet;

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

// Persistence tasks belong to the fixture session and are aborted and joined
// at logout/drop, including on assertion failure.
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
        Self::with_native_wallet(platform, None)
    }

    fn with_native_wallet(
        platform: Arc<StubPlatform>,
        native_wallet: Option<Arc<dyn truapi_platform::CoinageWalletHost>>,
    ) -> Self {
        let tasks = SessionTasks::new();
        let live = tasks.live.clone();
        let context = NativeChatContext {
            services: RuntimeServices::with_chat_platform(
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
                None,
                native_wallet,
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

async fn rust_wallet(
    registry: &NativeChatRegistry,
    context: &NativeChatContext,
) -> Arc<crate::runtime::native_chat::payments::WalletCoinage> {
    match registry.wallet(context).await.unwrap().as_ref() {
        crate::runtime::native_chat::wallet::SelectedWallet::Rust(wallet) => wallet.clone(),
        _ => panic!("Rust custody fixture selected another owner"),
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

async fn set_product_grants(
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

#[test]
fn outgoing_payment_ciphertext_is_not_an_open_oracle() {
    block_on(async {
        let fixture = Fixture::new();
        let actor = fixture.actor().await;
        let identity = IdentityFixture::new();
        let peer = DeviceFixture::new(1);
        seed_peer(&actor, &identity, &[&peer]).await;
        let registry = NativeChatRegistry::default();
        let wallet = rust_wallet(&registry, &fixture.context).await;
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
        let packet = outgoing(&actor, OutgoingKind::Payment(card.operation_id)).await;
        // This is a real, valid Host-signed payment, not malformed ciphertext.
        let wire::V2StatementTransportData::MultiRequest(native) =
            open_output(&actor, &identity, &packet.statement, false, false)
        else {
            panic!("payment must be a native multi-device request")
        };
        let body = open_body(
            &actor,
            &peer,
            &native.encrypted_request,
            &native.devices_info,
        );
        let decoded = wire::decode_message_exchange_request_plaintext(&body).unwrap();
        let keys: Vec<_> = memo()
            .entries
            .iter()
            .map(|entry| entry.0.to_vec())
            .collect();
        assert_eq!(
            decoded.messages,
            vec![
                wire::encode_coinage_send_message(&card.message_id, card.timestamp, "250", &keys,)
                    .unwrap()
            ]
        );
        assert_eq!(
            actor
                .open_statement(&fixture.context, &registry, packet.statement.clone())
                .await,
            Err(Error::InvalidStatement)
        );
        // An admitted peer's signature over exactly that outgoing ciphertext
        // cannot change its direction or turn it into incoming material.
        let shared =
            wire::x25519_shared_secret(&peer.secret, &actor.public.identity_chat_public_key)
                .unwrap();
        let incoming = route(&shared, &peer.account(), &actor.public.identity_account_id);
        let reflected = signed_packet(
            &peer,
            incoming,
            false,
            packet.statement.data.clone().unwrap(),
        );
        assert_eq!(
            actor
                .open_statement(&fixture.context, &registry, reflected)
                .await,
            Err(Error::InvalidStatement)
        );
        assert_eq!(wallet.views(PRODUCT).await.unwrap(), vec![card.clone()]);
        assert_eq!(
            outgoing(&actor, OutgoingKind::Payment(card.operation_id))
                .await
                .statement,
            packet.statement
        );
    });
}

#[test]
fn unsigned_routing_changes_are_rejected_before_plaintext_is_returned() {
    block_on(async {
        let fixture = Fixture::new();
        let actor = fixture.actor().await;
        let identity = IdentityFixture::new();
        let peer = DeviceFixture::new(1);
        seed_peer(&actor, &identity, &[&peer]).await;
        let registry = NativeChatRegistry::default();
        let message =
            wire::encode_rich_text_message("text", fixture.timestamp, Some("authenticated"), None)
                .unwrap();
        let packet = request(&actor, &identity, &peer, "routing", &[message.clone()]);
        let mut changed_topic = packet.clone();
        changed_topic.topics = vec![[0x99; 32]];
        let mut changed_channel = packet.clone();
        changed_channel.channel =
            Some(wire::chat_identity_response_topic(&packet.topics[0]).unwrap());
        for changed in [changed_topic, changed_channel] {
            assert_eq!(
                actor
                    .open_statement(&fixture.context, &registry, changed)
                    .await,
                Err(Error::InvalidStatement)
            );
        }
        let (opened, page) = actor
            .open_statement(&fixture.context, &registry, packet)
            .await
            .unwrap();
        assert_eq!(page, None);
        assert_eq!(opened.len(), 1);
        assert_eq!(
            opened[0].plaintext,
            wire::encode_transport_request_plaintext("routing", &[message]).unwrap()
        );
    });
}

#[test]
fn incoming_payment_is_returned_intact_without_import_or_acknowledgment() {
    block_on(async {
        let fixture = Fixture::new();
        let actor = fixture.actor().await;
        let identity = IdentityFixture::new();
        let peer = DeviceFixture::new(1);
        seed_peer(&actor, &identity, &[&peer]).await;
        let registry = NativeChatRegistry::default();
        let wallet = rust_wallet(&registry, &fixture.context).await;
        let slot = core_storage_test_key(CoreStorageKey::MainPurseCoinage {
            root_public_key: fixture.context.session.public_key,
            genesis_hash: fixture.context.genesis_hash,
        });
        let wallet_before = fixture
            .platform
            .local_storage
            .lock()
            .unwrap()
            .get(&slot)
            .cloned();
        let keys: Vec<_> = memo()
            .entries
            .iter()
            .map(|entry| entry.0.to_vec())
            .collect();
        let payment =
            wire::encode_coinage_send_message("incoming-coins", fixture.timestamp, "250", &keys)
                .unwrap();
        let plaintext =
            wire::encode_transport_request_plaintext("incoming-payment", &[payment]).unwrap();
        let packet = native_packet(&actor, &identity, &peer, &plaintext, false, false);
        for _ in 0..2 {
            let (opened, page) = actor
                .open_statement(&fixture.context, &registry, packet.clone())
                .await
                .unwrap();
            assert_eq!(page, None);
            assert_eq!(opened.len(), 1);
            assert_eq!(opened[0].peer_identity, identity.account);
            assert_eq!(opened[0].sender_account_id, peer.account());
            assert_eq!(opened[0].route, HostNativeChatRoute::Device);
            assert_eq!(opened[0].plaintext, plaintext);
        }
        assert!(wallet.views(PRODUCT).await.unwrap().is_empty());
        assert_eq!(
            fixture
                .platform
                .local_storage
                .lock()
                .unwrap()
                .get(&slot)
                .cloned(),
            wallet_before
        );
        assert!(fixture.platform.chain_connects.lock().unwrap().is_empty());
        let view = actor.public_view(&fixture.context, vec![]).await.unwrap();
        assert!(view.prepared.is_empty());
        assert!(
            actor
                .store
                .read(|state| state.outbox.is_empty()
                    && state.messages.is_empty()
                    && state.acknowledgments.is_empty())
                .await
                .unwrap()
        );
    });
}

#[test]
fn ordinary_polling_and_opening_do_not_activate_an_absent_or_guarded_purse() {
    block_on(async {
        for guarded in [false, true] {
            let fixture = Fixture::new();
            set_product_grants(
                fixture.platform.as_ref(),
                PRODUCT,
                truapi_platform::PermissionAuthorizationStatus::NotDetermined,
            )
            .await;
            let slot = core_storage_test_key(CoreStorageKey::MainPurseCoinage {
                root_public_key: fixture.context.session.public_key,
                genesis_hash: fixture.context.genesis_hash,
            });
            if guarded {
                fixture
                    .platform
                    .core_read_failures
                    .lock()
                    .insert(slot.clone());
            }
            let registry = NativeChatRegistry::default();
            let actor = registry.chat(&fixture.context, PRODUCT).await.unwrap();
            let identity = IdentityFixture::new();
            let peer = DeviceFixture::new(1);
            seed_peer(&actor, &identity, &[&peer]).await;
            for operation in [
                HostProductDeviceChatRequest::Initialize,
                HostProductDeviceChatRequest::ReconcilePayments,
            ] {
                let response = registry
                    .execute(fixture.context.clone(), PRODUCT.into(), operation)
                    .await
                    .unwrap();
                assert!(response.payments.is_empty());
                assert_eq!(response.coinage_cents_unit, None);
            }
            let message =
                wire::encode_rich_text_message("text", fixture.timestamp, Some("ordinary"), None)
                    .unwrap();
            let response = registry
                .execute(
                    fixture.context.clone(),
                    PRODUCT.into(),
                    HostProductDeviceChatRequest::Open {
                        statement: request(
                            &actor,
                            &identity,
                            &peer,
                            "ordinary",
                            &[message.clone()],
                        ),
                    },
                )
                .await
                .unwrap();
            assert_eq!(
                response.opened[0].plaintext,
                wire::encode_transport_request_plaintext("ordinary", &[message]).unwrap()
            );
            assert!(
                !fixture
                    .platform
                    .local_storage
                    .lock()
                    .unwrap()
                    .contains_key(&slot)
            );
            assert!(fixture.platform.chain_connects.lock().unwrap().is_empty());
        }
    });
}

#[test]
fn unavailable_purse_does_not_acknowledge_or_hide_private_payment_dependencies() {
    block_on(async {
        let fixture = Fixture::new();
        let registry = NativeChatRegistry::default();
        let actor = registry.chat(&fixture.context, PRODUCT).await.unwrap();
        actor
            .store
            .update(|state| {
                state.payment_acknowledgments.push([0x77; 32]);
                Ok(())
            })
            .await
            .unwrap();
        let slot = core_storage_test_key(CoreStorageKey::MainPurseCoinage {
            root_public_key: fixture.context.session.public_key,
            genesis_hash: fixture.context.genesis_hash,
        });
        fixture
            .platform
            .core_read_failures
            .lock()
            .insert(slot.clone());
        assert_eq!(
            actor.reconcile(&fixture.context, &registry).await,
            Err(Error::StorageUnavailable)
        );
        assert_eq!(
            actor
                .store
                .read(|state| state.payment_acknowledgments.clone())
                .await
                .unwrap(),
            vec![[0x77; 32]]
        );
        assert!(
            !fixture
                .platform
                .local_storage
                .lock()
                .unwrap()
                .contains_key(&slot)
        );
    });
}

#[test]
fn prepare_ordinary_preserves_native_plaintext_without_retaining_history_or_delivery() {
    block_on(async {
        let fixture = Fixture::new();
        let actor = fixture.actor().await;
        let identity = IdentityFixture::new();
        let peer = DeviceFixture::new(1);
        seed_peer(&actor, &identity, &[&peer]).await;
        let message = wire::encode_rich_text_message(
            "guest-message",
            fixture.timestamp,
            Some("product-owned text"),
            None,
        )
        .unwrap();
        let plaintext =
            wire::encode_transport_request_plaintext("guest-request", &[message.clone()]).unwrap();
        let prepared = actor
            .prepare(
                &fixture.context,
                identity.account,
                HostNativeChatRoute::Device,
                plaintext,
            )
            .await
            .unwrap();
        assert_eq!(prepared.len(), 1);
        assert_eq!(prepared[0].peer_identity, identity.account);
        assert_eq!(prepared[0].request_id, "guest-request");
        assert!(prepared[0].requires_ack);
        let wire::V2StatementTransportData::MultiRequest(native) =
            open_output(&actor, &identity, &prepared[0].statement, false, false)
        else {
            panic!("ordinary preparation must use native multi-device encryption")
        };
        let body = open_body(
            &actor,
            &peer,
            &native.encrypted_request,
            &native.devices_info,
        );
        let decoded = wire::decode_message_exchange_request_plaintext(&body).unwrap();
        assert_eq!(decoded.request_id, "guest-request");
        assert_eq!(decoded.messages, vec![message]);
        assert!(
            actor
                .store
                .read(|state| state.outbox.is_empty()
                    && state.messages.is_empty()
                    && state.sent.is_empty()
                    && state.acknowledgments.is_empty())
                .await
                .unwrap()
        );
        assert!(
            actor
                .public_view(&fixture.context, vec![])
                .await
                .unwrap()
                .prepared
                .is_empty()
        );
        assert!(fixture.platform.chain_connects.lock().unwrap().is_empty());
    });
}

#[test]
fn prepare_rejects_injected_keys_and_requires_authenticated_revocation_ack() {
    block_on(async {
        let fixture = Fixture::new();
        let actor = fixture.actor().await;
        let identity = IdentityFixture::new();
        let peer = DeviceFixture::new(1);
        let stranger = DeviceFixture::new(2);
        seed_peer(&actor, &identity, &[&peer]).await;
        let added = wire::encode_device_added_message(
            "own-device",
            fixture.timestamp,
            &actor.public.account_id,
            &actor.public.chat_public_key,
        )
        .unwrap();
        let removed = wire::encode_device_removed_message(
            "legacy-device",
            fixture.timestamp,
            &actor.legacy_account,
        )
        .unwrap();
        let keys: Vec<_> = memo()
            .entries
            .iter()
            .map(|entry| entry.0.to_vec())
            .collect();
        let forbidden = vec![
            vec![
                wire::encode_device_added_message(
                    "foreign-account",
                    fixture.timestamp,
                    &stranger.account(),
                    &actor.public.chat_public_key,
                )
                .unwrap(),
                removed.clone(),
            ],
            vec![
                wire::encode_device_added_message(
                    "foreign-key",
                    fixture.timestamp,
                    &actor.public.account_id,
                    &stranger.public_key(),
                )
                .unwrap(),
                removed.clone(),
            ],
            vec![
                added.clone(),
                wire::encode_device_removed_message(
                    "peer-revocation",
                    fixture.timestamp,
                    &peer.account(),
                )
                .unwrap(),
            ],
            vec![added.clone()],
            vec![removed.clone()],
            vec![
                wire::encode_multi_chat_accepted_message(
                    "untrusted-invitation",
                    fixture.timestamp,
                    "never-received",
                    &wire::V2PeerDevice {
                        statement_account_id: actor.public.account_id,
                        encryption_public_key: actor.public.chat_public_key,
                    },
                )
                .unwrap(),
            ],
            vec![
                wire::encode_coinage_send_message("guest-coins", fixture.timestamp, "250", &keys)
                    .unwrap(),
            ],
        ];
        for (index, messages) in forbidden.into_iter().enumerate() {
            let route = if index == 5 {
                HostNativeChatRoute::Identity
            } else {
                HostNativeChatRoute::Device
            };
            assert_eq!(
                actor
                    .prepare(
                        &fixture.context,
                        identity.account,
                        route,
                        wire::encode_transport_request_plaintext(
                            &format!("injected-{index}"),
                            &messages
                        )
                        .unwrap()
                    )
                    .await,
                Err(Error::InvalidRequest)
            );
            assert!(
                actor
                    .public_view(&fixture.context, vec![])
                    .await
                    .unwrap()
                    .peers[0]
                    .ready_for_payments
            );
        }
        let plaintext =
            wire::encode_transport_request_plaintext("retire-legacy", &[added, removed]).unwrap();
        actor
            .prepare(
                &fixture.context,
                identity.account,
                HostNativeChatRoute::Device,
                plaintext,
            )
            .await
            .unwrap();
        assert_eq!(
            actor
                .payment(&fixture.context, identity.account, "blocked".into(), 25)
                .await
                .err(),
            Some(Error::PeerNotReady)
        );
        let registry = NativeChatRegistry::default();
        for packet in [
            acknowledgment(&actor, &identity, &stranger, "retire-legacy", false),
            acknowledgment(&actor, &identity, &peer, "different-request", false),
            native_packet(
                &actor,
                &identity,
                &peer,
                &wire::encode_transport_response_plaintext("retire-legacy", 1).unwrap(),
                true,
                false,
            ),
        ] {
            let _ = actor
                .open_statement(&fixture.context, &registry, packet)
                .await;
            assert!(
                !actor
                    .public_view(&fixture.context, vec![])
                    .await
                    .unwrap()
                    .peers[0]
                    .ready_for_payments
            );
        }
        actor
            .open_statement(
                &fixture.context,
                &registry,
                acknowledgment(&actor, &identity, &peer, "retire-legacy", false),
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
        let wallet = rust_wallet(&registry, &fixture.context).await;
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
                .open_statement(
                    &fixture.context,
                    &registry,
                    acknowledgment(&actor, &identity, &peer, &packet.request_id, false)
                )
                .await,
            Err(Error::StorageUnavailable)
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
        // No packet is re-received. Reconcile must repair delivery before its
        // independent finalized-chain observation encounters the offline RPC.
        assert_eq!(
            actor.reconcile(&restarted.context, &registry).await,
            Err(Error::NetworkUnavailable)
        );
        let wallet = rust_wallet(&registry, &restarted.context).await;
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

// The old SCALE snapshot ended after rich_messages, without an extension tag.
// Encode that historical prefix structurally so this test never relies on offsets.
fn legacy_snapshot(state: &State) -> Vec<u8> {
    (
        &state.secret,
        state.index,
        &state.peers,
        &state.invitations,
        &state.outbox,
        &state.received,
        &state.sent,
        &state.accepted_payments,
        &state.payment_acknowledgments,
        &state.messages,
        &state.acknowledgments,
        state.last_expiry,
        &state.history_imports,
        &state.files,
        &state.rich_messages,
    )
        .encode()
}

#[test]
fn legacy_scale_snapshot_migrates_once_without_losing_keys_custody_or_files() {
    block_on(async {
        let fixture = Fixture::new();
        let actor = fixture.actor().await;
        let identity = IdentityFixture::new();
        let peer = DeviceFixture::new(1);
        seed_peer(&actor, &identity, &[&peer]).await;
        let registry = NativeChatRegistry::default();
        let wallet = rust_wallet(&registry, &fixture.context).await;
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
        let payment_statement = outgoing(&actor, OutgoingKind::Payment(card.operation_id))
            .await
            .statement;
        let message = wire::encode_rich_text_message(
            "legacy-text",
            fixture.timestamp,
            Some("preserve conversation"),
            None,
        )
        .unwrap();
        let ordinary = actor
            .prepare(
                &fixture.context,
                identity.account,
                HostNativeChatRoute::Device,
                wire::encode_transport_request_plaintext("legacy-outgoing", &[message.clone()])
                    .unwrap(),
            )
            .await
            .unwrap()
            .remove(0);
        let ordinary_statement = ordinary.statement.clone();
        let peer_identity = identity.account;
        let timestamp = fixture.timestamp;
        let invitation = Invitation {
            id: [0x13; 32],
            peer: peer_identity,
            root_key: identity.public_key(),
            username: Some("peer.dot".into()),
            device_account: peer.account(),
            device_key: peer.public_key(),
            message_id: "legacy-invitation".into(),
            timestamp,
            text: "preserve invitation".into(),
        };
        let messages = vec![HostNativeChatMessages {
            peer_identity,
            incoming: false,
            request_id: "legacy-outgoing".into(),
            messages: vec![message.clone()],
        }];
        let expected_messages = messages.clone();
        let acknowledgment = HostNativeChatAcknowledgment {
            peer_identity,
            request_id: "older-send".into(),
            response_code: 0,
        };
        let expected_acknowledgment = acknowledgment.clone();
        // Private file records remain Host-owned. Decode their old wire shape to
        // include a real pending ticket/progress slot without exposing its fields.
        let metadata = HostNativeChatAttachmentMetadata {
            mime_type: "image/png".into(),
            size_bytes: 4,
            kind: HostNativeChatAttachmentKind::Image {
                width: 1,
                height: 1,
                thumbnail: None,
            },
        };
        let file_id = [0x41u8; 32];
        let file_bytes = (
            (
                file_id,
                [0x42u8; 32],
                peer_identity,
                true,
                metadata.clone(),
                Secret32([0x43; 32]),
                "wss://hop.example.test".to_owned(),
                Some([0x44u8; 32]),
                None::<String>,
            ),
            (
                Vec::<u8>::new(),
                None::<files::PreparedEntry>,
                None::<Vec<u8>>,
                Vec::<u8>::new(),
                0u32,
                0u64,
                None::<Vec<u8>>,
                false,
                false,
            ),
        )
            .encode();
        let file = files::FileRecord::decode(&mut file_bytes.as_slice()).unwrap();
        let rich_bytes = (
            [0x45u8; 32],
            peer_identity,
            true,
            None::<String>,
            "legacy-file".to_owned(),
            "legacy-image".to_owned(),
            timestamp,
            HostNativeChatRichMessageKind::Message,
            Some("preserve attachment".to_owned()),
            vec![file_id],
            [0x46u8; 32],
            false,
            true,
        )
            .encode();
        let rich = files::RichRecord::decode(&mut rich_bytes.as_slice()).unwrap();
        actor
            .store
            .update(move |state| {
                state.invitations.push(invitation);
                state.messages = messages;
                state.acknowledgments.push(acknowledgment);
                state.sent.push(SentReceipt {
                    peer: peer_identity,
                    request_id: "legacy-client-request".into(),
                    wire_request_id: "legacy-outgoing".into(),
                    digest: hash(&message),
                });
                state.queue(Outgoing {
                    peer: peer_identity,
                    request_id: ordinary.request_id,
                    digest: hash(&message),
                    kind: OutgoingKind::Ordinary,
                    roster_revision: 1,
                    statement: ordinary.statement,
                    last_attempt: 0,
                })?;
                state.files.push(file);
                state.rich_messages.push(rich);
                let bytes = legacy_snapshot(state);
                let mut input = bytes.as_slice();
                *state = State::decode(&mut input).unwrap();
                assert!(input.is_empty());
                assert!(state.boundary.legacy_pending);
                Ok(())
            })
            .await
            .unwrap();
        let retained = actor
            .store
            .read(|state| {
                (
                    state.secret.clone(),
                    state.index,
                    state.accepted_payments.clone(),
                    state.payment_acknowledgments.clone(),
                    state.files.clone(),
                    state.rich_messages.clone(),
                )
                    .encode()
            })
            .await
            .unwrap();
        let view = actor
            .public_view(&fixture.context, vec![card.clone()])
            .await
            .unwrap();
        let migration_id = view.migration_id.unwrap();
        let snapshot = view.migration.as_ref().unwrap();
        assert_eq!(snapshot.messages, expected_messages);
        assert_eq!(snapshot.acknowledgments, vec![expected_acknowledgment]);
        assert_eq!(snapshot.payments, vec![card.clone()]);
        assert_eq!(snapshot.invitations[0].text, "preserve invitation");
        assert_eq!(snapshot.rich_messages[0].attachments[0].metadata, metadata);
        assert_eq!(
            snapshot.rich_messages[0].attachments[0].state,
            HostNativeChatAttachmentState::Downloading {
                downloaded_bytes: 0
            }
        );
        assert_eq!(
            view.migration_invitations[0].request_id,
            "legacy-invitation"
        );
        assert!(
            view.prepared
                .iter()
                .any(|item| item.statement == ordinary_statement
                    && item.request_id == "legacy-outgoing"
                    && item.peer_identity == peer_identity
                    && item.requires_ack
                    && item.client_request_id.as_deref() == Some("legacy-client-request"))
        );
        assert!(
            view.prepared
                .iter()
                .any(|item| item.statement == payment_statement)
        );
        assert_eq!(
            actor
                .prepare(
                    &fixture.context,
                    peer_identity,
                    HostNativeChatRoute::Device,
                    wire::encode_transport_response_plaintext("guest-not-persisted", 0).unwrap()
                )
                .await,
            Err(Error::OperationConflict)
        );
        assert!(!actor.drive_files(&fixture.context).await.unwrap());
        assert_eq!(
            actor
                .public_view(&fixture.context, vec![])
                .await
                .unwrap()
                .migration,
            view.migration
        );
        assert_eq!(
            actor.commit_migration(&fixture.context, [0xff; 32]).await,
            Err(Error::OperationConflict)
        );
        fixture.tasks.stop();
        drop(wallet);
        drop(registry);
        drop(actor);
        let restarted = Fixture::on_platform(fixture.platform.clone());
        let actor = restarted.actor().await;
        // Once exposed, the migration view remains the same across retries and
        // restart even if the caller's current wallet projection has changed.
        let reopened = actor.public_view(&restarted.context, vec![]).await.unwrap();
        assert_eq!(reopened.migration, view.migration);
        assert_eq!(reopened.migration_id, Some(migration_id));
        assert_eq!(reopened.prepared, view.prepared);
        for _ in 0..2 {
            actor
                .commit_migration(&restarted.context, migration_id)
                .await
                .unwrap();
        }
        let committed = actor.public_view(&restarted.context, vec![]).await.unwrap();
        assert_eq!(committed.device, view.device);
        assert_eq!(committed.migration, None);
        assert_eq!(committed.migration_id, None);
        assert!(committed.migration_invitations.is_empty());
        assert_eq!(committed.rich_messages, view.rich_messages);
        assert_eq!(committed.prepared.len(), 1);
        assert_eq!(committed.prepared[0].statement, payment_statement);
        assert_eq!(
            actor
                .store
                .read(|state| (
                    state.secret.clone(),
                    state.index,
                    state.accepted_payments.clone(),
                    state.payment_acknowledgments.clone(),
                    state.files.clone(),
                    state.rich_messages.clone()
                )
                    .encode())
                .await
                .unwrap(),
            retained
        );
        assert!(
            actor
                .store
                .read(|state| state.messages.is_empty()
                    && state.sent.is_empty()
                    && state.acknowledgments.is_empty())
                .await
                .unwrap()
        );
        let registry = NativeChatRegistry::default();
        assert_eq!(
            registry
                .wallet(&restarted.context)
                .await
                .unwrap()
                .views(&restarted.context, PRODUCT)
                .await
                .unwrap(),
            vec![card]
        );
        restarted.tasks.stop();
        drop(registry);
        drop(actor);
        let again = Fixture::on_platform(restarted.platform.clone());
        let actor = again.actor().await;
        actor
            .commit_migration(&again.context, migration_id)
            .await
            .unwrap();
        assert_eq!(
            actor.public_view(&again.context, vec![]).await.unwrap(),
            committed
        );
    });
}

#[test]
fn state_decode_accepts_old_prefix_and_tagged_extension_but_rejects_corruption() {
    let state = State::initial().unwrap();
    let legacy = legacy_snapshot(&state);
    let old = State::decode(&mut legacy.as_slice()).unwrap();
    assert!(old.boundary.legacy_pending);
    assert_eq!(legacy_snapshot(&old), legacy);
    let current = state.encode();
    let new = State::decode(&mut current.as_slice()).unwrap();
    assert!(!new.boundary.legacy_pending);
    assert_eq!(new.encode(), current);
    let mut bad_marker = legacy.clone();
    bad_marker.extend_from_slice(b"BAD!");
    bad_marker.extend_from_slice(&BoundaryState::default().encode());
    assert!(State::decode(&mut bad_marker.as_slice()).is_err());
    let mut truncated_extension = legacy.clone();
    truncated_extension.extend_from_slice(b"HCN3");
    assert!(State::decode(&mut truncated_extension.as_slice()).is_err());
    assert!(State::decode(&mut &legacy[..legacy.len() - 1]).is_err());
}
