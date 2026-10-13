// SPDX-License-Identifier: AGPL-3.0-only
//! Native signed/encrypted packets against the real actor and encrypted stores.
#![cfg(not(target_arch = "wasm32"))]

mod contacts;
mod hop_history;
mod native_wallet;

use super::*;
use crate::platform::CoreStorageKey;
use crate::{
    host_logic::statement_store::decode_verified_statement_data,
    runtime::{authority::AuthoritySession, services::RuntimeServices},
    subscription::Spawner,
    test_support::{StubPlatform, core_storage_test_key, wait_until},
    versioned::IntoLatest,
};
use futures::{
    executor::block_on,
    future::{AbortHandle, Abortable},
};
use parking_lot::Mutex;
use truapi_coinage::{MemoEntry, TransferMemo};

const PRODUCT: &str = "chat.dot";

// Persistence tasks belong to the fixture session and are aborted and joined
// at logout/drop, including on assertion failure.
type SessionTask = (AbortHandle, std::thread::JoinHandle<()>);

struct SessionTasks {
    live: Arc<AtomicBool>,
    tasks: Arc<Mutex<Vec<SessionTask>>>,
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
        native_wallet: Option<Arc<dyn crate::platform::CoinageWalletHost>>,
    ) -> Self {
        let tasks = SessionTasks::new();
        let live = tasks.live.clone();
        let context = NativeChatContext {
            services: RuntimeServices::with_chat_platform(
                platform.clone(),
                crate::platform::HostInfo {
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
            chat_session_granted: Arc::new(|_| false),
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
    submit: crate::platform::PermissionAuthorizationStatus,
) {
    use crate::host_internal::permissions::PermissionsService;
    use crate::platform::{PermissionAuthorizationRequest, PermissionAuthorizationStatus};
    let product = crate::platform::ProductContext::new_with_execution(
        product.to_owned(),
        crate::platform::ProductExecutionKind::Worker,
    )
    .expect("test product id is valid");
    let permissions = PermissionsService::new(platform, platform, &product);
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
        let packet = request(
            &actor,
            &identity,
            &peer,
            "routing",
            std::slice::from_ref(&message),
        );
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
fn identity_route_rejects_first_contact_plaintext() {
    block_on(async {
        let fixture = Fixture::new();
        let actor = fixture.actor().await;
        let identity = IdentityFixture::new();
        let peer = DeviceFixture::new(1);
        seed_peer(&actor, &identity, &[&peer]).await;
        let registry = NativeChatRegistry::default();
        let invitation = wire::encode_chat_request_v2(&wire::V2ChatRequestV2 {
            message: wire::V2ChatRequestMessageV2 {
                message_id: "first-contact".into(),
                timestamp: fixture.timestamp,
                content: wire::V2ChatRequestContentV2 {
                    identity_proof: wire::V2ChatRequestIdentityProof {
                        identity_account_id: identity.account,
                        proof: [0x08; 32],
                    },
                    device_enc_pub_key: peer.public_key(),
                    push_token: None,
                    welcome_text: None,
                },
            },
            proof: wire::V2ChatRequestProof {
                signature: vec![0x09; 64],
                signer: peer.account().to_vec(),
            },
        })
        .unwrap();
        for response in [false, true] {
            let packet = native_packet(&actor, &identity, &peer, &invitation, response, true);
            assert_eq!(
                actor
                    .open_statement(&fixture.context, &registry, packet)
                    .await,
                Err(Error::InvalidStatement)
            );
        }
        let message =
            wire::encode_rich_text_message("text", fixture.timestamp, Some("root"), None).unwrap();
        let packet = native_packet(
            &actor,
            &identity,
            &peer,
            &wire::encode_transport_request_plaintext("root", &[message]).unwrap(),
            false,
            true,
        );
        let (opened, _) = actor
            .open_statement(&fixture.context, &registry, packet)
            .await
            .unwrap();
        assert_eq!(opened.len(), 1);
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
                crate::platform::PermissionAuthorizationStatus::NotDetermined,
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
                            std::slice::from_ref(&message),
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
        let plaintext = wire::encode_transport_request_plaintext(
            "guest-request",
            std::slice::from_ref(&message),
        )
        .unwrap();
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
                wire::encode_transport_request_plaintext(
                    "legacy-outgoing",
                    std::slice::from_ref(&message),
                )
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

/// A snapshot written before frames were ordered carried watermarks without a
/// timestamp or withdrawal marker. It still opens, keeps everything else, and
/// the next publish sends the disclosure again.
#[test]
fn a_snapshot_with_legacy_profile_watermarks_opens_and_resends() {
    block_on(async {
        let fixture = Fixture::new();
        set_product_grants(
            &fixture.platform,
            PRODUCT,
            crate::platform::PermissionAuthorizationStatus::Authorized,
        )
        .await;
        let actor = fixture.actor().await;
        let identity = IdentityFixture::new();
        seed_peer(&actor, &identity, &[&DeviceFixture::new(1)]).await;
        disclose_for(&fixture).await;
        assert!(
            actor
                .publish_profile_reference(&fixture.context)
                .await
                .unwrap()
        );
        actor
            .store
            .update(|state| {
                let current = state.encode();
                let reopened = State::decode(&mut current.as_slice()).unwrap();
                assert_eq!(
                    reopened.profile_shared.len(),
                    1,
                    "the current layout keeps its watermarks"
                );

                // The same state with its watermarks in the legacy layout.
                let trailing = state.profile_shared.encode();
                let mut legacy = current[..current.len() - trailing.len() - 4].to_vec();
                legacy.extend(
                    state
                        .profile_shared
                        .iter()
                        .map(|watermark| {
                            (
                                watermark.peer,
                                watermark.digest.expect("a disclosure was sent"),
                                watermark.discloser_product_id.clone(),
                            )
                        })
                        .collect::<Vec<_>>()
                        .encode(),
                );
                let mut input = legacy.as_slice();
                let decoded = State::decode(&mut input).unwrap();
                assert!(input.is_empty());
                assert!(decoded.profile_shared.is_empty());
                assert_eq!(decoded.peers.len(), state.peers.len());
                assert_eq!(decoded.outbox.len(), state.outbox.len());
                assert_eq!(
                    (decoded.secret.0, decoded.index, decoded.last_expiry),
                    (state.secret.0, state.index, state.last_expiry)
                );
                assert!(!decoded.boundary.legacy_pending);

                let mut corrupt = legacy.clone();
                corrupt.push(0);
                assert!(State::decode(&mut corrupt.as_slice()).is_err());

                *state = decoded;
                Ok(())
            })
            .await
            .unwrap();
        assert!(
            actor
                .publish_profile_reference(&fixture.context)
                .await
                .unwrap(),
            "the contact is sent the disclosure again"
        );
        let view = actor.public_view(&fixture.context, vec![]).await.unwrap();
        assert_eq!(view.prepared.len(), 1, "one reference, not two");
    });
}

/// A snapshot written before lapsed frames were resent carried watermarks with
/// no attempt count. They open as one attempt that did not lapse, so the
/// contact is not sent the same disclosure again.
#[test]
fn a_snapshot_with_single_attempt_profile_watermarks_keeps_them() {
    block_on(async {
        let fixture = Fixture::new();
        set_product_grants(
            &fixture.platform,
            PRODUCT,
            crate::platform::PermissionAuthorizationStatus::Authorized,
        )
        .await;
        let actor = fixture.actor().await;
        let identity = IdentityFixture::new();
        seed_peer(&actor, &identity, &[&DeviceFixture::new(1)]).await;
        disclose_for(&fixture).await;
        assert!(
            actor
                .publish_profile_reference(&fixture.context)
                .await
                .unwrap()
        );
        actor
            .store
            .update(|state| {
                let current = state.encode();
                let trailing = state.profile_shared.encode();
                let mut single = current[..current.len() - trailing.len() - 4].to_vec();
                single.extend(
                    state
                        .profile_shared
                        .iter()
                        .map(|watermark| {
                            (
                                watermark.peer,
                                watermark.digest,
                                watermark.discloser_product_id.clone(),
                                watermark.timestamp,
                            )
                        })
                        .collect::<Vec<_>>()
                        .encode(),
                );
                let mut input = single.as_slice();
                let decoded = State::decode(&mut input).unwrap();
                assert!(input.is_empty());
                assert_eq!(decoded.profile_shared.len(), 1);
                assert!(
                    decoded.profile_shared == state.profile_shared,
                    "kept as one attempt that has not lapsed"
                );
                assert_eq!(decoded.outbox.len(), state.outbox.len());
                *state = decoded;
                Ok(())
            })
            .await
            .unwrap();
        assert!(
            !actor
                .publish_profile_reference(&fixture.context)
                .await
                .unwrap(),
            "the contact is not sent it again"
        );
    });
}

const PROFILE_REFERENCE: &str = "seity-contacts:v1:5c9584ba6e565351723d57394780b31b4c2156123e1269c4724ae5f01258bb535c9584ba6e565351723d57394780b31b4c2156123e1269c4724ae5f01258bb53";

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

#[test]
fn a_disclosed_profile_reference_is_sealed_once_per_peer_and_withdrawn_on_retract() {
    block_on(async {
        use crate::runtime::profile::{Disclosure, clear_disclosure, write_disclosure};
        let fixture = Fixture::new();
        let owner = profile::profile_owner(&fixture.context);
        set_product_grants(
            &fixture.platform,
            PRODUCT,
            crate::platform::PermissionAuthorizationStatus::Authorized,
        )
        .await;
        let actor = fixture.actor().await;
        let identity = IdentityFixture::new();
        let peer = DeviceFixture::new(1);
        seed_peer(&actor, &identity, &[&peer]).await;
        let profile_entries = |state: &State| {
            state
                .outbox
                .iter()
                .filter(|entry| matches!(entry.kind, OutgoingKind::ProfileReference(_)))
                .count()
        };

        assert!(
            !actor
                .publish_profile_reference(&fixture.context)
                .await
                .unwrap(),
            "nothing disclosed, nothing sent"
        );

        write_disclosure(
            fixture.platform.as_ref(),
            owner,
            &Disclosure {
                product_id: "seity.dot".into(),
                reference: PROFILE_REFERENCE.into(),
                revision: 1,
                all_chat_apps: true,
                app_products: Vec::new(),
                contacts: Vec::new(),
            },
        )
        .await
        .unwrap();
        assert!(
            actor
                .publish_profile_reference(&fixture.context)
                .await
                .unwrap()
        );
        let view = actor.public_view(&fixture.context, vec![]).await.unwrap();
        assert_eq!(
            view.prepared.len(),
            1,
            "one opaque statement for the product to submit"
        );
        assert_eq!(view.prepared[0].peer_identity, identity.account);
        assert!(view.prepared[0].requires_ack);
        let first_request = view.prepared[0].request_id.clone();
        let predicted = actor.store.read(|state| {
            let watermark = &state.profile_shared[0];
            hash(&(identity.account, "seity.dot", Some(PROFILE_REFERENCE), watermark.timestamp).encode())
        }).await.unwrap();
        assert_ne!(
            first_request, format!("profile-{}", hex::encode(&predicted[..8])),
            "the public request id must not be an offline reference oracle"
        );
        assert!(
            !contains(
                &view.prepared[0].statement.encode(),
                PROFILE_REFERENCE.as_bytes()
            ),
            "the product carries ciphertext, never the reference"
        );
        assert!(
            !actor
                .publish_profile_reference(&fixture.context)
                .await
                .unwrap(),
            "the watermark stops a second send of the same disclosure"
        );

        write_disclosure(
            fixture.platform.as_ref(),
            owner,
            &Disclosure {
                product_id: "seity.dot".into(),
                reference: format!("{PROFILE_REFERENCE}ff"),
                revision: 1,
                all_chat_apps: true,
                app_products: Vec::new(),
                contacts: Vec::new(),
            },
        )
        .await
        .unwrap();
        assert!(
            actor
                .publish_profile_reference(&fixture.context)
                .await
                .unwrap()
        );
        assert_eq!(
            actor.store.read(profile_entries).await.unwrap(),
            1,
            "a replacement supersedes the queued disclosure"
        );

        clear_disclosure(fixture.platform.as_ref(), owner)
            .await
            .unwrap();
        assert!(
            actor
                .publish_profile_reference(&fixture.context)
                .await
                .unwrap(),
            "a holder is sent the withdrawal"
        );
        assert!(
            actor
                .store
                .read(|state| state
                    .profile_shared
                    .iter()
                    .all(|watermark| watermark.digest.is_none()))
                .await
                .unwrap(),
            "the withdrawal is remembered, not forgotten"
        );
        assert!(
            !actor
                .publish_profile_reference(&fixture.context)
                .await
                .unwrap(),
            "a withdrawal is sent once"
        );

        // Disclosing the first reference again is a new message, not a replay
        // of the first one, which the peer's host would refuse as a conflict.
        write_disclosure(
            fixture.platform.as_ref(),
            owner,
            &Disclosure {
                product_id: "seity.dot".into(),
                reference: PROFILE_REFERENCE.into(),
                revision: 1,
                all_chat_apps: true,
                app_products: Vec::new(),
                contacts: Vec::new(),
            },
        )
        .await
        .unwrap();
        assert!(
            actor
                .publish_profile_reference(&fixture.context)
                .await
                .unwrap()
        );
        let view = actor.public_view(&fixture.context, vec![]).await.unwrap();
        assert_eq!(view.prepared.len(), 1);
        assert_ne!(view.prepared[0].request_id, first_request);
        let redisclosed_from = view.prepared[0].request_id.clone();
        assert!(
            !actor
                .publish_profile_reference(&fixture.context)
                .await
                .unwrap(),
            "an automatic publish does not resend the revision already sent"
        );

        // The user updates what contacts see: the same reference, a new
        // disclose call. Every ready peer is sent a fresh frame.
        write_disclosure(
            fixture.platform.as_ref(),
            owner,
            &Disclosure {
                product_id: "seity.dot".into(),
                reference: PROFILE_REFERENCE.into(),
                revision: 2,
                all_chat_apps: true,
                app_products: Vec::new(),
                contacts: Vec::new(),
            },
        )
        .await
        .unwrap();
        assert!(
            actor
                .publish_profile_reference(&fixture.context)
                .await
                .unwrap(),
            "a new disclose of the same reference starts a new round"
        );
        let view = actor.public_view(&fixture.context, vec![]).await.unwrap();
        assert_eq!(view.prepared.len(), 1);
        assert_ne!(view.prepared[0].request_id, redisclosed_from);
        assert!(
            actor
                .store
                .read(|state| state
                    .profile_shared
                    .iter()
                    .all(|watermark| watermark.attempts == 1))
                .await
                .unwrap(),
            "the new round starts its attempts afresh"
        );
        assert!(
            !actor
                .publish_profile_reference(&fixture.context)
                .await
                .unwrap(),
            "and is sent once"
        );
    });
}

#[test]
fn profile_delivery_tracks_the_contacts_current_device_roster() {
    block_on(async {
        for acknowledged in [false, true] {
            let fixture = Fixture::new();
            set_product_grants(&fixture.platform, PRODUCT,
                crate::platform::PermissionAuthorizationStatus::Authorized).await;
            let actor = fixture.actor().await;
            let identity = IdentityFixture::new();
            let old = DeviceFixture::new(1);
            let new = DeviceFixture::new(2);
            seed_peer(&actor, &identity, &[&old]).await;
            crate::runtime::profile::write_disclosure(
                fixture.platform.as_ref(), profile::profile_owner(&fixture.context),
                &crate::runtime::profile::Disclosure {
                    product_id: "seity.dot".into(), reference: PROFILE_REFERENCE.into(), revision: 1,
                    all_chat_apps: true, app_products: Vec::new(), contacts: Vec::new(),
                },
            ).await.unwrap();
            assert!(actor.publish_profile_reference(&fixture.context).await.unwrap());
            let previous = actor.public_view(&fixture.context, vec![]).await.unwrap().prepared[0]
                .request_id.clone();
            let account = identity.account;
            let replacement = DeviceRecord {
                account: new.account(), key: Some(new.public_key()), active: true,
                timestamp: fixture.timestamp, message_id: "device-handover".into(),
            };
            actor.store.update(move |state| {
                if acknowledged {
                    state.outbox.retain(|entry| entry.kind.profile_scope().is_none());
                }
                let peer = state.peer_mut(&account)?;
                peer.revocation_acks = vec![replacement.account];
                peer.devices = vec![replacement];
                peer.revision += 1;
                Ok(())
            }).await.unwrap();
            assert!(actor.public_view(&fixture.context, vec![]).await.unwrap().prepared.is_empty(),
                "no statement sealed to the revoked device can still reach the product");
            assert!(actor.publish_profile_reference(&fixture.context).await.unwrap());
            let prepared = actor.public_view(&fixture.context, vec![]).await.unwrap().prepared;
            assert_eq!(prepared.len(), 1);
            assert_ne!(prepared[0].request_id, previous);
            let wire::V2StatementTransportData::MultiRequest(sealed) =
                open_output(&actor, &identity, &prepared[0].statement, false, false)
            else { panic!("profile must use authenticated multi-device transport") };
            assert_eq!(sealed.devices_info.iter().map(|device| device.statement_account_id)
                .collect::<Vec<_>>(), vec![new.account()]);
            let opened = open_body(&actor, &new, &sealed.encrypted_request, &sealed.devices_info);
            assert!(contains(&opened, PROFILE_REFERENCE.as_bytes()));
        }
    });
}

#[test]
fn a_received_profile_reference_is_kept_by_the_host_and_cut_from_what_the_product_opens() {
    block_on(async {
        use crate::runtime::profile::received_reference;
        let fixture = Fixture::new();
        let actor = fixture.actor().await;
        let identity = IdentityFixture::new();
        let peer = DeviceFixture::new(1);
        seed_peer(&actor, &identity, &[&peer]).await;
        let registry = NativeChatRegistry::default();

        let text = wire::encode_text_message("hello", fixture.timestamp, "hi").unwrap();
        let frame = wire::encode_profile_reference_message(
            "profile-1",
            fixture.timestamp,
            "seity.dot",
            Some(PROFILE_REFERENCE),
        )
        .unwrap();
        let plaintext =
            wire::encode_transport_request_plaintext("incoming-profile", &[frame, text.clone()])
                .unwrap();
        let packet = native_packet(&actor, &identity, &peer, &plaintext, false, false);
        let (opened, _) = actor
            .open_statement(&fixture.context, &registry, packet)
            .await
            .unwrap();
        assert_eq!(opened.len(), 1);
        assert!(!contains(
            &opened[0].plaintext,
            PROFILE_REFERENCE.as_bytes()
        ));
        let wire::V2StatementTransportData::Request { messages, .. } =
            wire::decode_transport_plaintext(&opened[0].plaintext).unwrap()
        else {
            panic!("the product still receives the request to acknowledge");
        };
        assert_eq!(
            messages,
            vec![text],
            "ordinary content passes through untouched"
        );
        let owner = profile::profile_owner(&fixture.context);
        let held = received_reference(fixture.platform.as_ref(), owner, PRODUCT, &identity.account)
            .await
            .unwrap()
            .expect("the host keeps what the contact disclosed");
        assert_eq!(held.reference.as_deref(), Some(PROFILE_REFERENCE));
        assert_eq!(held.discloser_product_id, "seity.dot");

        open_profile_frame(
            &fixture,
            &actor,
            &identity,
            &peer,
            "incoming-withdrawal",
            fixture.timestamp + 1,
            None,
        )
        .await;
        assert_eq!(
            received_reference(fixture.platform.as_ref(), owner, PRODUCT, &identity.account)
                .await
                .unwrap()
                .and_then(|held| held.reference),
            None
        );
    });
}

#[test]
fn contact_avatars_over_the_product_follow_what_the_contact_shares() {
    block_on(async {
        use crate::runtime::profile::avatars::ContactAvatarPlacement;
        let fixture = Fixture::new();
        let actor = fixture.actor().await;
        let identity = IdentityFixture::new();
        let peer = DeviceFixture::new(1);
        seed_peer(&actor, &identity, &[&peer]).await;
        let host = Arc::new(crate::test_support::RecordingAvatarHost::default());
        let placement = fixture.context.services.contact_avatars.for_runtime(1, || {
            ContactAvatarPlacement::new(
                host.clone(),
                fixture.platform.clone(),
                crate::platform::ProductContext::new(PRODUCT.to_string()).unwrap(),
                Arc::downgrade(&fixture.context.services),
            )
        });
        let rect = truapi::v01::AvatarRect {
            x: 16,
            y: 80,
            width: 44,
            height: 44,
        };
        let clip = truapi::v01::AvatarRect {
            x: 0,
            y: 64,
            width: 360,
            height: 576,
        };
        placement
            .place(
                profile::profile_owner(&fixture.context),
                truapi::versioned::profile::HostProfilePlaceContactAvatarsRequest::V2(
                    truapi::v02::HostProfilePlaceContactAvatarsRequest {
                        surface_width: 360,
                        surface_height: 640,
                        own: None,
                        slots: vec![truapi::v01::ContactAvatarSlot {
                            slot: 7,
                            peer_identity: identity.account,
                            rect,
                            clip,
                        }],
                    },
                )
                .into_latest(),
                None,
            )
            .await
            .unwrap();
        let placed = |avatars| {
            (
                PRODUCT.to_string(),
                crate::platform::PlacedAvatars {
                    surface_width: 360,
                    surface_height: 640,
                    avatars,
                },
            )
        };

        // The product placed the avatar once, before the contact shared; the
        // host redraws it when the reference arrives and when it is withdrawn.
        open_profile_frame(
            &fixture,
            &actor,
            &identity,
            &peer,
            "incoming-profile",
            fixture.timestamp,
            Some(PROFILE_REFERENCE),
        )
        .await;
        let avatar = |shared_at| crate::platform::PlacedAvatar {
            slot: 7,
            rect,
            clip,
            reference: PROFILE_REFERENCE.to_string(),
            shared_at,
        };
        assert_eq!(
            host.wait_for(2),
            vec![placed(Vec::new()), placed(vec![avatar(fixture.timestamp)]),]
        );
        // The contact re-shares the same reference (its record changed): the
        // host is told, with the newer frame's time, so it drops its cache.
        open_profile_frame(
            &fixture,
            &actor,
            &identity,
            &peer,
            "incoming-reshare",
            fixture.timestamp + 1,
            Some(PROFILE_REFERENCE),
        )
        .await;
        assert_eq!(
            host.wait_for(3)[2],
            placed(vec![avatar(fixture.timestamp + 1)])
        );
        open_profile_frame(
            &fixture,
            &actor,
            &identity,
            &peer,
            "incoming-withdrawal",
            fixture.timestamp + 2,
            None,
        )
        .await;
        assert_eq!(host.wait_for(4)[3], placed(Vec::new()));
    });
}

/// Open one statement from `identity` carrying a single profile frame.
async fn open_profile_frame(
    fixture: &Fixture,
    actor: &Arc<NativeChatActor>,
    identity: &IdentityFixture,
    peer: &DeviceFixture,
    request_id: &str,
    timestamp: u64,
    reference: Option<&str>,
) {
    let frame = wire::encode_profile_reference_message(
        &format!("{request_id}-frame"),
        timestamp,
        "seity.dot",
        reference,
    )
    .unwrap();
    let plaintext = wire::encode_transport_request_plaintext(request_id, &[frame]).unwrap();
    let packet = native_packet(actor, identity, peer, &plaintext, false, false);
    actor
        .open_statement(&fixture.context, &NativeChatRegistry::default(), packet)
        .await
        .unwrap();
}

/// The reference the host holds for `identity`, a withdrawal reading as `None`.
async fn held_reference(fixture: &Fixture, identity: &IdentityFixture) -> Option<String> {
    crate::runtime::profile::received_reference(
        fixture.platform.as_ref(),
        profile::profile_owner(&fixture.context),
        PRODUCT,
        &identity.account,
    )
    .await
    .unwrap()
    .and_then(|held| held.reference)
}

#[test]
fn a_withdrawal_opened_before_an_older_disclosure_stays_withdrawn() {
    block_on(async {
        let fixture = Fixture::new();
        let actor = fixture.actor().await;
        let identity = IdentityFixture::new();
        let peer = DeviceFixture::new(1);
        seed_peer(&actor, &identity, &[&peer]).await;

        // The product chooses the order it opens fetched statements in: here
        // the contact's withdrawal first, then the disclosure it withdrew.
        open_profile_frame(
            &fixture,
            &actor,
            &identity,
            &peer,
            "withdrawal",
            fixture.timestamp + 1,
            None,
        )
        .await;
        open_profile_frame(
            &fixture,
            &actor,
            &identity,
            &peer,
            "disclosure",
            fixture.timestamp,
            Some(PROFILE_REFERENCE),
        )
        .await;
        assert_eq!(
            held_reference(&fixture, &identity).await,
            None,
            "the older disclosure cannot return"
        );

        let replacement = format!("{PROFILE_REFERENCE}ff");
        open_profile_frame(
            &fixture,
            &actor,
            &identity,
            &peer,
            "replacement",
            fixture.timestamp + 2,
            Some(&replacement),
        )
        .await;
        assert_eq!(
            held_reference(&fixture, &identity).await,
            Some(replacement.clone()),
            "a later one does"
        );
        // Re-opening the withdrawal changes nothing either.
        open_profile_frame(
            &fixture,
            &actor,
            &identity,
            &peer,
            "withdrawal",
            fixture.timestamp + 1,
            None,
        )
        .await;
        assert_eq!(held_reference(&fixture, &identity).await, Some(replacement));
    });
}

fn filler(peer: [u8; 32], request_id: String, kind: OutgoingKind) -> Outgoing {
    Outgoing {
        peer,
        request_id,
        digest: [0; 32],
        kind,
        roster_revision: 1,
        statement: signed_packet(&DeviceFixture::new(9), [9; 32], false, vec![1]),
        last_attempt: 0,
    }
}

async fn disclose_for(fixture: &Fixture) {
    crate::runtime::profile::write_disclosure(
        fixture.platform.as_ref(),
        profile::profile_owner(&fixture.context),
        &crate::runtime::profile::Disclosure {
            product_id: "seity.dot".into(),
            reference: PROFILE_REFERENCE.into(),
            revision: 1,
            all_chat_apps: true,
            app_products: Vec::new(),
            contacts: Vec::new(),
        },
    )
    .await
    .unwrap();
}

#[test]
fn a_full_outbox_neither_blocks_profile_references_nor_is_blocked_by_them() {
    block_on(async {
        let fixture = Fixture::new();
        set_product_grants(
            &fixture.platform,
            PRODUCT,
            crate::platform::PermissionAuthorizationStatus::Authorized,
        )
        .await;
        let actor = fixture.actor().await;
        let identity = IdentityFixture::new();
        seed_peer(&actor, &identity, &[&DeviceFixture::new(1)]).await;
        disclose_for(&fixture).await;
        let peer = identity.account;
        actor
            .store
            .update(move |state| {
                for index in 0..MAX_OUTBOX {
                    state.queue(filler(
                        peer,
                        format!("rich-{index}"),
                        OutgoingKind::Rich([0; 32]),
                    ))?;
                }
                assert!(matches!(
                    state.queue(filler(
                        peer,
                        "rich-extra".into(),
                        OutgoingKind::Rich([0; 32])
                    )),
                    Err(Error::StorageUnavailable)
                ));
                Ok(())
            })
            .await
            .unwrap();
        assert!(
            actor
                .publish_profile_reference(&fixture.context)
                .await
                .unwrap(),
            "user traffic filling the outbox does not stop the reference"
        );

        // References filling their own budget leave the rest to user traffic.
        actor
            .store
            .update(move |state| {
                state.outbox.retain(|entry| entry.request_id == "rich-0");
                for index in 0..MAX_PROFILE_OUTBOX {
                    state.queue(filler(
                        [index as u8; 32],
                        format!("profile-{index}"),
                        OutgoingKind::ProfileReference([0; 32]),
                    ))?;
                }
                state.queue(filler(peer, "rich-1".into(), OutgoingKind::Rich([0; 32])))
            })
            .await
            .unwrap();
    });
}

#[test]
fn obsolete_roster_profile_references_release_outbox_room() {
    block_on(async {
        let fixture = Fixture::new();
        set_product_grants(
            &fixture.platform,
            PRODUCT,
            crate::platform::PermissionAuthorizationStatus::Authorized,
        )
        .await;
        let actor = fixture.actor().await;
        let identity = IdentityFixture::new();
        seed_peer(&actor, &identity, &[&DeviceFixture::new(1)]).await;
        disclose_for(&fixture).await;
        actor
            .store
            .update(|state| {
                for index in 0..MAX_PROFILE_OUTBOX {
                    state.queue(filler(
                        [index as u8; 32],
                        format!("obsolete-{index}"),
                        OutgoingKind::ProfileReference([0; 32]),
                    ))?;
                }
                Ok(())
            })
            .await
            .unwrap();
        assert!(actor.publish_profile_reference(&fixture.context).await.unwrap());
        actor
            .store
            .read(move |state| {
                assert_eq!(state.outbox_used(true), 1);
                assert!(state.outbox.iter().all(|entry| entry.peer == identity.account));
                assert_eq!(state.profile_shared.len(), 1);
                assert_eq!(state.profile_shared[0].peer, identity.account);
            })
            .await
            .unwrap();
    });
}

#[test]
fn a_reference_with_no_outbox_room_waits_for_a_later_reconcile() {
    block_on(async {
        let fixture = Fixture::new();
        set_product_grants(
            &fixture.platform,
            PRODUCT,
            crate::platform::PermissionAuthorizationStatus::Authorized,
        )
        .await;
        let actor = fixture.actor().await;
        let identity = IdentityFixture::new();
        seed_peer(&actor, &identity, &[&DeviceFixture::new(1)]).await;
        disclose_for(&fixture).await;
        let peer = identity.account;
        let (revision, disclosure) = crate::runtime::profile::read_disclosure_state(
            fixture.platform.as_ref(),
            profile::profile_owner(&fixture.context),
        )
        .await
        .unwrap();
        let disclosure = disclosure.unwrap();
        let digest = profile::disclosure_digest(&disclosure);
        // Current App shares and Personal withdrawals fill the independent
        // profile budget. Absent peers or old rosters are retired before queueing
        // and therefore cannot exercise backpressure.
        actor
            .store
            .update(move |state| {
                let template = state.peer(&peer)?.clone();
                for index in 0..MAX_PROFILE_OUTBOX / 2 {
                    let mut recipient = template.clone();
                    recipient.identity = [index as u8; 32];
                    for scope in [
                        crate::runtime::profile::ProfileScope::App,
                        crate::runtime::profile::ProfileScope::Personal,
                    ] {
                        let personal = scope == crate::runtime::profile::ProfileScope::Personal;
                        state.queue(filler(
                            recipient.identity,
                            format!("current-{index}-{scope:?}"),
                            if personal {
                                OutgoingKind::PersonalProfileReference([0; 32])
                            } else {
                                OutgoingKind::ProfileReference([0; 32])
                            },
                        ))?;
                        state.profile_shared.push(profile::ProfileWatermark {
                            peer: recipient.identity,
                            scope,
                            revision: if personal { revision } else { 0 },
                            digest: if personal { None } else { Some(digest) },
                            discloser_product_id: disclosure.product_id.clone(),
                            timestamp: 1,
                            attempts: 1,
                            lapsed: false,
                            roster_revision: recipient.revision,
                        });
                    }
                    state.peers.push(recipient);
                }
                assert_eq!(state.outbox_used(true), MAX_PROFILE_OUTBOX);
                Ok(())
            })
            .await
            .unwrap();

        assert!(
            !actor
                .publish_profile_reference(&fixture.context)
                .await
                .unwrap(),
            "no room is not a failure"
        );
        assert!(
            actor
                .store
                .read(move |state| state
                    .profile_shared
                    .iter()
                    .all(|watermark| watermark.peer != peer))
                .await
                .unwrap(),
            "the peer is left unsent"
        );

        actor
            .store
            .update(|state| {
                state.outbox.clear();
                Ok(())
            })
            .await
            .unwrap();
        assert!(
            actor
                .publish_profile_reference(&fixture.context)
                .await
                .unwrap(),
            "a later reconcile sends it"
        );
    });
}

/// A peer host that predates the content type never acknowledges a
/// reference; let its statement lifetime pass.
async fn lapse_profile_references(actor: &Arc<NativeChatActor>) {
    actor
        .store
        .update(|state| {
            for entry in &mut state.outbox {
                entry.statement.expiry = Some(1 << 32);
            }
            Ok(())
        })
        .await
        .unwrap();
}

#[test]
fn an_unacknowledged_reference_is_resent_a_bounded_number_of_times_per_disclosure() {
    block_on(async {
        let fixture = Fixture::new();
        set_product_grants(
            &fixture.platform,
            PRODUCT,
            crate::platform::PermissionAuthorizationStatus::Authorized,
        )
        .await;
        let actor = fixture.actor().await;
        let identity = IdentityFixture::new();
        seed_peer(&actor, &identity, &[&DeviceFixture::new(1)]).await;
        disclose_for(&fixture).await;
        let registry = NativeChatRegistry::default();
        assert!(
            actor
                .publish_profile_reference(&fixture.context)
                .await
                .unwrap()
        );
        let view = actor.public_view(&fixture.context, vec![]).await.unwrap();
        let mut request_ids = vec![view.prepared[0].request_id.clone()];

        for attempt in 2..=profile::MAX_PROFILE_ATTEMPTS {
            lapse_profile_references(&actor).await;
            actor.reconcile(&fixture.context, &registry).await.unwrap();
            let view = actor.public_view(&fixture.context, vec![]).await.unwrap();
            assert_eq!(view.prepared.len(), 1, "attempt {attempt} is offered");
            let resent = &view.prepared[0];
            assert!(
                !request_ids.contains(&resent.request_id),
                "attempt {attempt} is a new frame"
            );
            assert!(
                resent
                    .statement
                    .expiry
                    .is_some_and(|expiry| (expiry >> 32) > current_unix_secs()),
                "attempt {attempt} is signed for a new lifetime"
            );
            request_ids.push(resent.request_id.clone());
        }

        lapse_profile_references(&actor).await;
        actor.reconcile(&fixture.context, &registry).await.unwrap();
        let view = actor.public_view(&fixture.context, vec![]).await.unwrap();
        assert!(
            view.prepared.is_empty(),
            "the last attempt is dropped, not re-signed"
        );
        assert!(
            !actor
                .publish_profile_reference(&fixture.context)
                .await
                .unwrap(),
            "and nothing is queued again while the disclosure is unchanged"
        );

        crate::runtime::profile::write_disclosure(
            fixture.platform.as_ref(),
            profile::profile_owner(&fixture.context),
            &crate::runtime::profile::Disclosure {
                product_id: "seity.dot".into(),
                reference: format!("{PROFILE_REFERENCE}ff"),
                revision: 1,
                all_chat_apps: true,
                app_products: Vec::new(),
                contacts: Vec::new(),
            },
        )
        .await
        .unwrap();
        assert!(
            actor
                .publish_profile_reference(&fixture.context)
                .await
                .unwrap(),
            "a new disclosure is sent"
        );
        lapse_profile_references(&actor).await;
        actor.reconcile(&fixture.context, &registry).await.unwrap();
        let view = actor.public_view(&fixture.context, vec![]).await.unwrap();
        assert_eq!(
            view.prepared.len(),
            1,
            "and has attempts of its own when it lapses"
        );
    });
}

#[test]
fn a_reconcile_sends_a_reference_no_trigger_queued() {
    block_on(async {
        let fixture = Fixture::new();
        set_product_grants(
            &fixture.platform,
            PRODUCT,
            crate::platform::PermissionAuthorizationStatus::Authorized,
        )
        .await;
        let actor = fixture.actor().await;
        let identity = IdentityFixture::new();
        seed_peer(&actor, &identity, &[&DeviceFixture::new(1)]).await;
        // Stored with no Chat told, as by a host that stopped before relaying.
        disclose_for(&fixture).await;
        actor
            .reconcile(&fixture.context, &NativeChatRegistry::default())
            .await
            .unwrap();
        let view = actor.public_view(&fixture.context, vec![]).await.unwrap();
        assert_eq!(view.prepared.len(), 1);
        assert_eq!(view.prepared[0].peer_identity, identity.account);
        assert!(view.prepared[0].request_id.starts_with("profile-"));
    });
}

#[test]
fn a_peer_that_becomes_ready_is_sent_the_disclosure_in_that_request() {
    block_on(async {
        let fixture = Fixture::new();
        set_product_grants(
            &fixture.platform,
            PRODUCT,
            crate::platform::PermissionAuthorizationStatus::Authorized,
        )
        .await;
        let registry = NativeChatRegistry::default();
        let actor = registry.chat(&fixture.context, PRODUCT).await.unwrap();
        let identity = IdentityFixture::new();
        let peer = DeviceFixture::new(1);
        seed_peer(&actor, &identity, &[&peer]).await;
        // Retiring the legacy device leaves the peer unready until it
        // acknowledges.
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
        actor
            .prepare(
                &fixture.context,
                identity.account,
                HostNativeChatRoute::Device,
                wire::encode_transport_request_plaintext("retire-legacy", &[added, removed])
                    .unwrap(),
            )
            .await
            .unwrap();
        disclose_for(&fixture).await;
        let initialized = registry
            .execute(
                fixture.context.clone(),
                PRODUCT.into(),
                HostProductDeviceChatRequest::Initialize,
            )
            .await
            .unwrap();
        assert!(
            initialized.prepared.is_empty(),
            "a peer that is not ready is sent nothing"
        );

        let opened = registry
            .execute(
                fixture.context.clone(),
                PRODUCT.into(),
                HostProductDeviceChatRequest::Open {
                    statement: acknowledgment(&actor, &identity, &peer, "retire-legacy", false),
                },
            )
            .await
            .unwrap();
        assert_eq!(
            opened.prepared.len(),
            1,
            "the acknowledgement that makes it ready brings the reference"
        );
        assert_eq!(opened.prepared[0].peer_identity, identity.account);
        assert!(opened.prepared[0].request_id.starts_with("profile-"));
    });
}

/// A contact is named from the verified roster of the Chat it is a contact
/// of; a peer the roster does not hold, or another product's contact, gets
/// no name when the directory cannot supply one.
#[test]
fn a_contact_is_named_by_the_roster_of_its_own_chat() {
    block_on(async {
        let fixture = Fixture::new();
        let registry = NativeChatRegistry::default();
        set_product_grants(
            &fixture.platform,
            PRODUCT,
            crate::platform::PermissionAuthorizationStatus::Authorized,
        )
        .await;
        let chat = registry.chat(&fixture.context, PRODUCT).await.unwrap();
        let identity = IdentityFixture::new();
        seed_peer(&chat, &identity, &[&DeviceFixture::new(1)]).await;

        assert_eq!(
            registry
                .contact_username(&fixture.context, PRODUCT, identity.account)
                .await
                .as_deref(),
            Some("peer.dot"),
            "the name the host verified when the contact was added"
        );
        assert_eq!(
            registry
                .contact_username(&fixture.context, PRODUCT, [0xee; 32])
                .await,
            None,
            "a peer not on the roster does not borrow a contact's name"
        );
        assert_eq!(
            registry
                .contact_username(&fixture.context, "other.dot", identity.account)
                .await,
            None,
            "another product's Chat does not lend its roster"
        );
    });
}

/// Each open Chat of the wallet whose disclosure changed relays it, whichever
/// product it belongs to; another wallet's Chat relays nothing.
#[test]
fn a_changed_disclosure_is_relayed_by_the_open_chats_of_its_wallet() {
    use crate::runtime::profile::{Disclosure, clear_disclosure, write_disclosure};
    let fixture = Fixture::new();
    let mut other_wallet = fixture.context.clone();
    other_wallet.session.public_key = [9; 32];
    let registry = NativeChatRegistry::default();
    let identity = IdentityFixture::new();
    let open = |context: &NativeChatContext, product: &str| {
        block_on(async {
            set_product_grants(
                &fixture.platform,
                product,
                crate::platform::PermissionAuthorizationStatus::Authorized,
            )
            .await;
            let chat = registry.chat(context, product).await.unwrap();
            seed_peer(&chat, &identity, &[&DeviceFixture::new(1)]).await;
            chat
        })
    };
    let chats = [
        open(&fixture.context, PRODUCT),
        open(&fixture.context, "other.dot"),
    ];
    let bystander = open(&other_wallet, PRODUCT);
    let relayed = |chat: &Arc<NativeChatActor>, withdrawn: bool| {
        block_on(chat.store.read(|state| {
            state.profile_shared.iter().any(|watermark| {
                watermark.peer == identity.account && watermark.digest.is_none() == withdrawn
            }) && state
                .outbox
                .iter()
                .any(|entry| matches!(entry.kind, OutgoingKind::ProfileReference(_)))
        }))
        .unwrap()
    };
    // Both wallets hold a disclosure; only the first changed it.
    for context in [&fixture.context, &other_wallet] {
        block_on(write_disclosure(
            fixture.platform.as_ref(),
            profile::profile_owner(context),
            &Disclosure {
                product_id: "seity.dot".into(),
                reference: PROFILE_REFERENCE.into(),
                revision: 1,
                all_chat_apps: true,
                app_products: Vec::new(),
                contacts: Vec::new(),
            },
        ))
        .unwrap();
    }

    registry.relay_profile_disclosure(fixture.context.clone());
    wait_until(
        || chats.iter().all(|chat| relayed(chat, false)),
        "every open Chat of the wallet relays the disclosure",
    );
    block_on(clear_disclosure(
        fixture.platform.as_ref(),
        profile::profile_owner(&fixture.context),
    ))
    .unwrap();
    registry.relay_profile_disclosure(fixture.context.clone());
    wait_until(
        || chats.iter().all(|chat| relayed(chat, true)),
        "and then its withdrawal",
    );
    assert!(
        block_on(
            bystander
                .store
                .read(|state| state.profile_shared.is_empty())
        )
        .unwrap(),
        "another wallet's Chat is not told"
    );
}

async fn open_personal_profile_frame(
    fixture: &Fixture,
    actor: &Arc<NativeChatActor>,
    identity: &IdentityFixture,
    peer: &DeviceFixture,
    request_id: &str,
    (revision, timestamp): (u64, u64),
    reference: Option<&str>,
) {
    let frame = wire::encode_personal_profile_reference_message(
        &format!("{request_id}-frame"),
        timestamp,
        revision,
        "seity.dot",
        reference,
    )
    .unwrap();
    let plaintext = wire::encode_transport_request_plaintext(request_id, &[frame]).unwrap();
    let packet = native_packet(actor, identity, peer, &plaintext, false, false);
    let (opened, _) = actor
        .open_statement(&fixture.context, &NativeChatRegistry::default(), packet)
        .await
        .unwrap();
    assert!(
        opened
            .iter()
            .all(|opened| !contains(&opened.plaintext, PROFILE_REFERENCE.as_bytes()))
    );
}

async fn queued_profile_contents(
    fixture: &Fixture,
    actor: &Arc<NativeChatActor>,
    identity: &IdentityFixture,
    device: &DeviceFixture,
) -> Vec<wire::V2ChatMessageContent> {
    actor
        .public_view(&fixture.context, Vec::new())
        .await
        .unwrap()
        .prepared
        .into_iter()
        .filter(|entry| entry.peer_identity == identity.account)
        .map(|entry| {
            let wire::V2StatementTransportData::MultiRequest(native) =
                open_output(actor, identity, &entry.statement, false, false)
            else {
                panic!("profile must use authenticated multi-device transport");
            };
            let body = open_body(
                actor,
                device,
                &native.encrypted_request,
                &native.devices_info,
            );
            let request = wire::decode_message_exchange_request_plaintext(&body).unwrap();
            assert_eq!(request.messages.len(), 1);
            wire::decode_message(&request.messages[0]).unwrap().content
        })
        .collect()
}

#[test]
fn profile_audiences_select_exact_identity_accounts_and_keep_scopes_independent_after_restart() {
    block_on(async {
        use crate::runtime::profile::{Disclosure, read_disclosure_state, write_disclosure};
        let fixture = Fixture::new();
        for product in [PRODUCT, "other.dot"] {
            set_product_grants(
                &fixture.platform,
                product,
                crate::platform::PermissionAuthorizationStatus::Authorized,
            )
            .await;
        }
        let actor = fixture.actor().await;
        let other = NativeChatActor::open(&fixture.context, "other.dot")
            .await
            .unwrap();
        let selected = IdentityFixture::new();
        let bystander = IdentityFixture {
            account: keypair(0x72).public.to_bytes(),
            secret: [0x73; 32],
        };
        let device = DeviceFixture::new(1);
        let other_device = DeviceFixture::new(2);
        for chat in [&actor, &other] {
            seed_peer(chat, &selected, &[&device]).await;
            seed_peer(chat, &bystander, &[&other_device]).await;
        }
        let owner = profile::profile_owner(&fixture.context);
        let mut disclosure = Disclosure {
            product_id: "seity.dot".into(),
            reference: PROFILE_REFERENCE.into(),
            revision: 1,
            all_chat_apps: false,
            app_products: vec![PRODUCT.into()],
            contacts: vec![selected.account, other_device.account()],
        };
        write_disclosure(fixture.platform.as_ref(), owner, &disclosure)
            .await
            .unwrap();
        assert!(
            actor
                .publish_profile_reference(&fixture.context)
                .await
                .unwrap()
        );
        assert!(
            other
                .publish_profile_reference(&fixture.context)
                .await
                .unwrap()
        );
        let personal_share = wire::V2ChatMessageContent::PersonalProfileReference {
            discloser_product_id: "seity.dot".into(),
            reference: Some(PROFILE_REFERENCE.into()),
            revision: 1,
        };
        assert_eq!(
            queued_profile_contents(&fixture, &other, &selected, &device).await,
            vec![personal_share.clone()]
        );
        assert_eq!(
            queued_profile_contents(&fixture, &actor, &selected, &device).await,
            vec![
                wire::V2ChatMessageContent::ProfileReference {
                    discloser_product_id: "seity.dot".into(),
                    reference: Some(PROFILE_REFERENCE.into()),
                },
                personal_share
            ]
        );
        assert_eq!(
            queued_profile_contents(&fixture, &actor, &bystander, &other_device).await,
            vec![wire::V2ChatMessageContent::ProfileReference {
                discloser_product_id: "seity.dot".into(),
                reference: Some(PROFILE_REFERENCE.into()),
            }]
        );
        assert!(
            queued_profile_contents(&fixture, &other, &bystander, &other_device)
                .await
                .is_empty(),
            "a selected device account must not be translated to its peer identity"
        );

        disclosure.contacts.clear();
        write_disclosure(fixture.platform.as_ref(), owner, &disclosure)
            .await
            .unwrap();
        assert!(
            queued_profile_contents(&fixture, &actor, &selected, &device)
                .await
                .is_empty(),
            "a response cannot expose superseded pending shares before the relay runs"
        );
        let platform = fixture.platform.clone();
        fixture.tasks.stop();
        drop(actor);
        drop(other);
        drop(fixture);
        let fixture = Fixture::on_platform(platform);
        let actor = fixture.actor().await;
        assert!(
            actor
                .publish_profile_reference(&fixture.context)
                .await
                .unwrap()
        );
        let revision = read_disclosure_state(fixture.platform.as_ref(), owner)
            .await
            .unwrap()
            .0;
        assert_eq!(
            queued_profile_contents(&fixture, &actor, &selected, &device).await,
            vec![
                wire::V2ChatMessageContent::ProfileReference {
                    discloser_product_id: "seity.dot".into(),
                    reference: Some(PROFILE_REFERENCE.into()),
                },
                wire::V2ChatMessageContent::PersonalProfileReference {
                    discloser_product_id: "seity.dot".into(),
                    reference: None,
                    revision,
                },
            ],
            "a never-delivered personal share still requires a durable withdrawal"
        );

        disclosure.app_products.clear();
        disclosure.contacts = vec![selected.account];
        write_disclosure(fixture.platform.as_ref(), owner, &disclosure)
            .await
            .unwrap();
        assert!(
            actor
                .publish_profile_reference(&fixture.context)
                .await
                .unwrap()
        );
        let revision = read_disclosure_state(fixture.platform.as_ref(), owner)
            .await
            .unwrap()
            .0;
        assert_eq!(
            queued_profile_contents(&fixture, &actor, &selected, &device).await,
            vec![
                wire::V2ChatMessageContent::ProfileReference {
                    discloser_product_id: "seity.dot".into(),
                    reference: None,
                },
                wire::V2ChatMessageContent::PersonalProfileReference {
                    discloser_product_id: "seity.dot".into(),
                    reference: Some(PROFILE_REFERENCE.into()),
                    revision,
                },
            ],
            "a personal share cannot overwrite a pending app withdrawal"
        );
    });
}

#[test]
fn personal_references_render_across_apps_with_independent_withdrawals_and_global_replay_order() {
    block_on(async {
        use crate::runtime::profile::{avatars::ContactAvatarPlacement, received_reference};
        let fixture = Fixture::new();
        let actor = fixture.actor().await;
        let other = NativeChatActor::open(&fixture.context, "other.dot")
            .await
            .unwrap();
        let identity = IdentityFixture::new();
        let device = DeviceFixture::new(1);
        seed_peer(&actor, &identity, &[&device]).await;
        seed_peer(&other, &identity, &[&device]).await;
        let owner = profile::profile_owner(&fixture.context);
        let host = Arc::new(crate::test_support::RecordingAvatarHost::default());
        let placement = fixture
            .context
            .services
            .contact_avatars
            .for_runtime(41, || {
                ContactAvatarPlacement::new(
                    host.clone(),
                    fixture.platform.clone(),
                    crate::platform::ProductContext::new("not-chat.dot".into()).unwrap(),
                    Arc::downgrade(&fixture.context.services),
                )
            });
        placement
            .place(
                owner,
                truapi::versioned::profile::HostProfilePlaceContactAvatarsRequest::V2(
                    truapi::v02::HostProfilePlaceContactAvatarsRequest {
                        surface_width: 100,
                        surface_height: 100,
                        own: None,
                        slots: vec![truapi::v01::ContactAvatarSlot {
                            slot: 1,
                            peer_identity: identity.account,
                            rect: truapi::v01::AvatarRect {
                                x: 0,
                                y: 0,
                                width: 40,
                                height: 40,
                            },
                            clip: truapi::v01::AvatarRect {
                                x: 0,
                                y: 0,
                                width: 100,
                                height: 100,
                            },
                        }],
                    },
                )
                .into_latest(),
                None,
            )
            .await
            .unwrap();
        open_personal_profile_frame(
            &fixture,
            &other,
            &identity,
            &device,
            "personal-share",
            (10, fixture.timestamp),
            Some(PROFILE_REFERENCE),
        )
        .await;
        let draws = host.wait_for(2);
        assert_eq!(draws[1].0, "not-chat.dot");
        assert_eq!(draws[1].1.avatars[0].reference, PROFILE_REFERENCE);
        for product in [PRODUCT, "other.dot", "not-chat.dot"] {
            assert_eq!(
                received_reference(fixture.platform.as_ref(), owner, product, &identity.account)
                    .await
                    .unwrap()
                    .unwrap()
                    .reference
                    .as_deref(),
                Some(PROFILE_REFERENCE)
            );
        }
        let app_reference = format!("{PROFILE_REFERENCE}ff");
        open_profile_frame(
            &fixture,
            &actor,
            &identity,
            &device,
            "app-share",
            fixture.timestamp + 1,
            Some(&app_reference),
        )
        .await;
        assert_eq!(
            held_reference(&fixture, &identity).await.as_deref(),
            Some(app_reference.as_str())
        );
        open_personal_profile_frame(
            &fixture,
            &actor,
            &identity,
            &device,
            "personal-withdrawal",
            (11, fixture.timestamp + 2),
            None,
        )
        .await;
        assert_eq!(
            held_reference(&fixture, &identity).await.as_deref(),
            Some(app_reference.as_str()),
            "a personal withdrawal cannot remove an app grant"
        );
        assert!(host.wait_for(3)[2].1.avatars.is_empty());
        open_personal_profile_frame(
            &fixture,
            &other,
            &identity,
            &device,
            "late-stale-share",
            (10, fixture.timestamp + 100),
            Some(PROFILE_REFERENCE),
        )
        .await;
        assert!(
            received_reference(
                fixture.platform.as_ref(),
                owner,
                "not-chat.dot",
                &identity.account
            )
            .await
            .unwrap()
            .unwrap()
            .reference
            .is_none(),
            "relay time cannot defeat a cross-app tombstone"
        );
        open_personal_profile_frame(
            &fixture,
            &other,
            &identity,
            &device,
            "personal-regrant",
            (12, fixture.timestamp),
            Some(PROFILE_REFERENCE),
        )
        .await;
        let redraws = host.wait_for(4);
        assert!(
            redraws[3].1.avatars[0].shared_at > draws[1].1.avatars[0].shared_at,
            "a newer personal revision refreshes the profile despite another actor's older clock"
        );
        open_profile_frame(
            &fixture,
            &actor,
            &identity,
            &device,
            "app-withdrawal",
            fixture.timestamp + 4,
            None,
        )
        .await;
        assert_eq!(
            held_reference(&fixture, &identity).await.as_deref(),
            Some(PROFILE_REFERENCE),
            "an app withdrawal exposes the independent personal fallback"
        );
        open_personal_profile_frame(
            &fixture,
            &actor,
            &identity,
            &device,
            "late-stale-withdrawal",
            (11, fixture.timestamp + 101),
            None,
        )
        .await;
        assert_eq!(
            held_reference(&fixture, &identity).await.as_deref(),
            Some(PROFILE_REFERENCE)
        );
        let platform = fixture.platform.clone();
        fixture.tasks.stop();
        drop(actor);
        drop(other);
        drop(fixture);
        let fixture = Fixture::on_platform(platform);
        let actor = fixture.actor().await;
        open_personal_profile_frame(
            &fixture,
            &actor,
            &identity,
            &device,
            "restart-stale-share",
            (10, fixture.timestamp + 200),
            Some(&app_reference),
        )
        .await;
        assert_eq!(
            held_reference(&fixture, &identity).await.as_deref(),
            Some(PROFILE_REFERENCE)
        );
        let other_owner = crate::runtime::profile::ProfileOwner {
            genesis_hash: [99; 32],
            ..owner
        };
        assert!(
            received_reference(
                fixture.platform.as_ref(),
                other_owner,
                PRODUCT,
                &identity.account
            )
            .await
            .unwrap()
            .is_none()
        );
    });
}

#[test]
fn profile_storage_migrates_legacy_audiences_and_retains_retraction_sequence() {
    block_on(async {
        use crate::runtime::profile::{
            clear_disclosure, read_disclosure, read_disclosure_state, write_disclosure,
        };
        let fixture = Fixture::new();
        let owner = profile::profile_owner(&fixture.context);
        for (raw, revision) in [
            (
                ("seity.dot".to_string(), PROFILE_REFERENCE.to_string()).encode(),
                0,
            ),
            (
                (
                    "seity.dot".to_string(),
                    PROFILE_REFERENCE.to_string(),
                    42u64,
                )
                    .encode(),
                42,
            ),
        ] {
            crate::platform::CoreStorage::write_core_storage(
                fixture.platform.as_ref(),
                owner.disclosure_key(),
                raw,
            )
            .await
            .unwrap();
            let legacy = read_disclosure(fixture.platform.as_ref(), owner)
                .await
                .unwrap()
                .unwrap();
            assert!(legacy.all_chat_apps);
            assert!(legacy.app_products.is_empty() && legacy.contacts.is_empty());
            assert_eq!(legacy.revision, revision);
            clear_disclosure(fixture.platform.as_ref(), owner)
                .await
                .unwrap();
            assert_eq!(
                read_disclosure_state(fixture.platform.as_ref(), owner)
                    .await
                    .unwrap(),
                (revision + 1, None)
            );
            write_disclosure(fixture.platform.as_ref(), owner, &legacy)
                .await
                .unwrap();
            assert_eq!(
                read_disclosure_state(fixture.platform.as_ref(), owner)
                    .await
                    .unwrap()
                    .0,
                revision + 2
            );
        }
    });
}

#[test]
fn personal_pending_shares_are_removed_while_unready_and_withdrawn_when_ready() {
    block_on(async {
        use crate::runtime::profile::{
            Disclosure, ProfileScope, clear_disclosure, write_disclosure,
        };
        let fixture = Fixture::new();
        set_product_grants(
            &fixture.platform,
            PRODUCT,
            crate::platform::PermissionAuthorizationStatus::Authorized,
        )
        .await;
        let actor = fixture.actor().await;
        let identity = IdentityFixture::new();
        let device = DeviceFixture::new(1);
        seed_peer(&actor, &identity, &[&device]).await;
        let owner = profile::profile_owner(&fixture.context);
        write_disclosure(
            fixture.platform.as_ref(),
            owner,
            &Disclosure {
                product_id: "seity.dot".into(),
                reference: PROFILE_REFERENCE.into(),
                revision: 1,
                all_chat_apps: false,
                app_products: Vec::new(),
                contacts: vec![identity.account],
            },
        )
        .await
        .unwrap();
        actor
            .publish_profile_reference(&fixture.context)
            .await
            .unwrap();
        let peer = identity.account;
        actor
            .store
            .update(move |state| {
                state.peer_mut(&peer)?.established = false;
                Ok(())
            })
            .await
            .unwrap();
        clear_disclosure(fixture.platform.as_ref(), owner)
            .await
            .unwrap();
        assert!(
            !actor
                .publish_profile_reference(&fixture.context)
                .await
                .unwrap()
        );
        actor
            .store
            .read(|state| {
                assert!(
                    state
                        .outbox
                        .iter()
                        .all(|entry| entry.kind.profile_scope() != Some(ProfileScope::Personal))
                );
                assert_eq!(state.profile_shared[0].scope, ProfileScope::Personal);
                assert!(state.profile_shared[0].digest.is_some());
            })
            .await
            .unwrap();
        actor
            .store
            .update(move |state| {
                state.peer_mut(&peer)?.established = true;
                Ok(())
            })
            .await
            .unwrap();
        actor
            .publish_profile_reference(&fixture.context)
            .await
            .unwrap();
        assert_eq!(
            queued_profile_contents(&fixture, &actor, &identity, &device).await,
            vec![wire::V2ChatMessageContent::PersonalProfileReference {
                discloser_product_id: "seity.dot".into(),
                reference: None,
                revision: 2,
            }]
        );
        for _ in 1..profile::MAX_PROFILE_ATTEMPTS {
            lapse_profile_references(&actor).await;
            assert!(
                actor
                    .publish_profile_reference(&fixture.context)
                    .await
                    .unwrap()
            );
        }
        lapse_profile_references(&actor).await;
        assert!(
            !actor
                .publish_profile_reference(&fixture.context)
                .await
                .unwrap()
        );
        assert!(
            queued_profile_contents(&fixture, &actor, &identity, &device)
                .await
                .is_empty()
        );
    });
}
