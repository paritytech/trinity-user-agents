// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use crate::runtime::notification_envelope::verify_frames;

pub fn assert_wake(wake: &truapi::latest::HostNativeChatWake, context: &NativeChatContext) {
    let frames = verify_frames(
        wake.statement.data.as_deref().unwrap(),
        &hex::encode(context.genesis_hash),
        &hex::encode(wake.statement.channel.unwrap()),
        &wake.statement.topics.iter().map(hex::encode).collect::<Vec<_>>(),
        current_unix_secs() * 1000,
    ).unwrap();
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].header.v, 2);
    assert_eq!(frames[0].header.product, PRODUCT);
}

#[test]
fn wake_is_separate_authenticated_sr25519_and_preserves_chat_ciphertext() {
    block_on(async {
        let fixture = Fixture::new();
        let actor = fixture.actor().await;
        let identity = IdentityFixture::new();
        let peer = DeviceFixture::new(1);
        seed_peer(&actor, &identity, &[&peer]).await;
        let message = wire::encode_rich_text_message("message", fixture.timestamp, Some("private"), None).unwrap();
        let plaintext = wire::encode_transport_request_plaintext("request", &[message.clone()]).unwrap();
        assert_eq!(notification_message(HostNativeChatRoute::Device, &plaintext).as_deref(), Some("request"));
        let prepared = actor.prepare(&fixture.context, identity.account, HostNativeChatRoute::Device, plaintext).await.unwrap();
        let original = prepared[0].statement.clone();
        let wake = actor.notification_wake(&fixture.context, &prepared[0]).await.unwrap().unwrap();
        assert_eq!(prepared[0].statement, original);
        assert_ne!(wake.statement.channel, original.channel);
        assert!(wake.statement.topics.iter().all(|topic| !original.topics.contains(topic)));
        let carrier = wake.statement.data.as_ref().unwrap();
        assert!(carrier.len() < 4096);
        // The minimal Gordian subject is exactly one opaque 32-byte witness,
        // not native ciphertext or an exported encryption key.
        assert_eq!(&carrier[..7], &[0xd8, 200, 0x82, 0xd8, 201, 0x58, 32]);
        assert_eq!(&carrier[7..39], &hash(original.data.as_deref().unwrap()));
        let channel = hex::encode(wake.statement.channel.unwrap());
        let topics: Vec<_> = wake.statement.topics.iter().map(hex::encode).collect();
        let frames = verify_frames(carrier, &hex::encode(fixture.context.genesis_hash), &channel, &topics, fixture.timestamp).unwrap();
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].header.v, 2);
        assert_eq!(frames[0].header.product, PRODUCT);
        assert_eq!(frames[0].header.sender_key, hex::encode(actor.public.account_id));
        assert_eq!(frames[0].header.event_id, hex::encode(hash(&("truapi:chat:notification-event:v2", &actor.public.account_id, "request").encode())));
        assert_eq!(frames[0].header.expires_at - frames[0].header.created_at, 86_400_000);
        assert!(verify_frames(carrier, &hex::encode(fixture.context.genesis_hash), &hex::encode([99; 32]), &topics, fixture.timestamp).unwrap().is_empty());
        let wire::V2StatementTransportData::MultiRequest(native) = open_output(&actor, &identity, &original, false, false) else { panic!("unchanged native carrier required") };
        let body = open_body(&actor, &peer, &native.encrypted_request, &native.devices_info);
        let decoded = wire::decode_message_exchange_request_plaintext(&body).unwrap();
        assert_eq!(decoded.request_id, "request");
        assert_eq!(decoded.messages, vec![message]);
    });
}

#[test]
fn watches_use_only_authenticated_active_device_signers_and_revocation_removes_them() {
    block_on(async {
        let fixture = Fixture::new();
        let actor = fixture.actor().await;
        assert!(actor.notification_watches(&fixture.context).await.unwrap().is_empty());
        let identity = IdentityFixture::new();
        let peer = DeviceFixture::new(1);
        seed_peer(&actor, &identity, &[&peer]).await;
        let before = current_unix_secs() * 1000;
        let watches = actor.notification_watches(&fixture.context).await.unwrap();
        let after = current_unix_secs() * 1000;
        assert_eq!(watches.len(), 1);
        assert_eq!(watches[0].senders, vec![format!("sr25519:{}", hex::encode(peer.account()))]);
        assert_eq!(watches[0].route, format!("/peer/{}", hex::encode(identity.account)));
        assert_eq!(watches[0].id.len(), 128);
        assert!((before + 7 * 86_400_000..=after + 7 * 86_400_000).contains(&watches[0].expires_at));
        assert_eq!(watches[0].muted_until, 0);
        let shared = chat_shared_secret(&peer.secret, &actor.public.identity_chat_public_key).unwrap();
        let session = chat_identity_session_id(&shared, &peer.account(), &actor.public.identity_account_id);
        assert_eq!(watches[0].topics, vec![hex::encode(hash(&("truapi:chat:notification-topic:v2", &session).encode()))]);
        assert_eq!(watches[0].channel, hex::encode(hash(&("truapi:chat:notification-channel:v2", &session).encode())));
        actor.store.update(move |state| { state.peers[0].devices[0].active = false; Ok(()) }).await.unwrap();
        assert!(actor.notification_watches(&fixture.context).await.unwrap().is_empty());
    });
}

#[test]
fn unestablished_peers_and_unkeyed_devices_never_grant_watches() {
    block_on(async {
        let fixture = Fixture::new();
        let actor = fixture.actor().await;
        let identity = IdentityFixture::new();
        let peer = DeviceFixture::new(1);
        seed_peer(&actor, &identity, &[&peer]).await;
        actor.store.update(|state| {
            state.peers[0].established = false;
            state.peers[0].invitation = Some("pending-invitation".into());
            Ok(())
        }).await.unwrap();
        assert!(actor.notification_watches(&fixture.context).await.unwrap().is_empty());
        actor.store.update(|state| {
            state.peers[0].established = true;
            state.peers[0].invitation = None;
            state.peers[0].devices[0].active = false;
            state.peers[0].devices[0].key = None;
            Ok(())
        }).await.unwrap();
        assert!(actor.notification_watches(&fixture.context).await.unwrap().is_empty());
    });
}

#[test]
fn watch_limit_fails_closed_instead_of_truncating_authenticated_peers() {
    block_on(async {
        let fixture = Fixture::new();
        let actor = fixture.actor().await;
        let identity = IdentityFixture::new();
        let peer = DeviceFixture::new(1);
        seed_peer(&actor, &identity, &[&peer]).await;
        actor.store.update(|state| {
            let template = state.peers[0].clone();
            state.peers = (0u16..256).map(|index| {
                let mut peer = template.clone();
                peer.identity = [0x80; 32];
                peer.identity[..2].copy_from_slice(&index.to_le_bytes());
                peer
            }).collect();
            Ok(())
        }).await.unwrap();
        assert_eq!(actor.notification_watches(&fixture.context).await.unwrap().len(), 256);
        let second_device = DeviceFixture::new(2);
        actor.store.update(move |state| {
            state.peers[0].devices.push(DeviceRecord {
                account: second_device.account(),
                key: Some(second_device.public_key()),
                active: true,
                timestamp: 0,
                message_id: "authenticated-second-device".into(),
            });
            Ok(())
        }).await.unwrap();
        assert_eq!(actor.notification_watches(&fixture.context).await, Err(Error::InvalidRequest));
    });
}

#[test]
fn acknowledgments_and_lifecycle_never_request_wakes() {
    let acknowledgment = wire::encode_transport_response_plaintext("request", 0).unwrap();
    assert!(notification_message(HostNativeChatRoute::Device, &acknowledgment).is_none());
    let device = DeviceFixture::new(1);
    let control = wire::encode_device_added_message("control", current_unix_secs() * 1000, &device.account(), &device.public_key()).unwrap();
    let request = wire::encode_transport_request_plaintext("control-request", &[control]).unwrap();
    assert!(notification_message(HostNativeChatRoute::Device, &request).is_none());
    let text = wire::encode_rich_text_message("message", current_unix_secs() * 1000, Some("hello"), None).unwrap();
    let request = wire::encode_transport_request_plaintext("request", &[text]).unwrap();
    assert!(notification_message(HostNativeChatRoute::Identity, &request).is_none());
    assert!(notification_message(HostNativeChatRoute::Invitation, &request).is_none());
}

#[test]
fn explicit_wire_operation_produces_a_wake_without_changing_legacy_response() {
    block_on(async {
        let fixture = Fixture::new();
        set_product_grants(
            &fixture.platform,
            PRODUCT,
            crate::platform::PermissionAuthorizationStatus::Authorized,
        ).await;
        let registry = NativeChatRegistry::default();
        let actor = registry.chat(&fixture.context, PRODUCT).await.unwrap();
        let identity = IdentityFixture::new();
        let peer = DeviceFixture::new(1);
        seed_peer(&actor, &identity, &[&peer]).await;
        let message = wire::encode_rich_text_message("message", fixture.timestamp, Some("hello"), None).unwrap();
        let plaintext = wire::encode_transport_request_plaintext("request", &[message]).unwrap();
        let response = registry.execute_response(fixture.context.clone(), PRODUCT.into(),
            HostProductDeviceChatRequest::PrepareWithNotification {
                peer_identity: identity.account, route: HostNativeChatRoute::Device, plaintext: plaintext.clone(),
            }).await.unwrap();
        use truapi::versioned::{FromLatest, IntoLatest};
        use truapi::versioned::account::HostProductDeviceChatResponse as WireResponse;
        // The generated dispatcher normalizes and negotiates every response.
        let response = WireResponse::from_latest(response.into_latest(), 3);
        let WireResponse::V3(response) = response else { panic!("explicit notification response required") };
        assert_eq!(response.chat.prepared.len(), 1);
        assert_eq!(response.wakes.len(), 1);
        assert_wake(&response.wakes[0], &fixture.context);
        let legacy = registry.execute_response(fixture.context.clone(), PRODUCT.into(),
            HostProductDeviceChatRequest::Prepare {
                peer_identity: identity.account, route: HostNativeChatRoute::Device, plaintext,
            }).await.unwrap();
        let legacy_bytes = legacy.encode();
        let legacy = WireResponse::from_latest(legacy.into_latest(), 2);
        assert_eq!(legacy.encode(), legacy_bytes, "negotiation must preserve legacy response bytes");
        let WireResponse::V2(legacy) = legacy else { panic!("legacy response must remain V2") };
        assert_eq!(legacy.prepared[0].request_id, "request");
        assert!(actor.retained_notification(&fixture.context, identity.account, "request").await.unwrap().is_none());
        let watches = registry.execute_response(fixture.context.clone(), PRODUCT.into(),
            HostProductDeviceChatRequest::NotificationWatches).await.unwrap();
        let WireResponse::V3(watches) = watches else { panic!("notification response required") };
        assert_eq!(watches.watches.len(), 1);
        assert!(watches.wakes.is_empty());
        assert!(watches.chat.state_page.is_some(), "notification-only operations must use the bounded state snapshot");
    });
}

#[test]
fn optional_wake_signing_failure_preserves_ordinary_preparation() {
    block_on(async {
        let fixture = Fixture::new();
        let registry = NativeChatRegistry::default();
        let actor = registry.chat(&fixture.context, PRODUCT).await.unwrap();
        let identity = IdentityFixture::new();
        let peer = DeviceFixture::new(1);
        seed_peer(&actor, &identity, &[&peer]).await;
        // The ordinary packet can allocate its final expiry. Only the optional
        // second signature fails, without a storage/RPC mock or delivery change.
        actor.store.update(|state| {
            state.last_expiry = u64::MAX - 1;
            Ok(())
        }).await.unwrap();
        let message = wire::encode_rich_text_message("message", fixture.timestamp, Some("hello"), None).unwrap();
        let plaintext = wire::encode_transport_request_plaintext("request", &[message.clone()]).unwrap();
        let response = registry.execute_response(fixture.context.clone(), PRODUCT.into(),
            HostProductDeviceChatRequest::PrepareWithNotification {
                peer_identity: identity.account,
                route: HostNativeChatRoute::Device,
                plaintext,
            }).await.unwrap();
        let truapi::versioned::account::HostProductDeviceChatResponse::V3(response) = response
            else { panic!("notification-aware response required") };
        assert!(response.wakes.is_empty());
        assert_eq!(response.chat.prepared.len(), 1);
        let prepared = &response.chat.prepared[0];
        assert_eq!(prepared.statement.expiry, Some(u64::MAX));
        let wire::V2StatementTransportData::MultiRequest(native) =
            open_output(&actor, &identity, &prepared.statement, false, false)
            else { panic!("ordinary ciphertext must remain deliverable") };
        let body = open_body(&actor, &peer, &native.encrypted_request, &native.devices_info);
        let decoded = wire::decode_message_exchange_request_plaintext(&body).unwrap();
        assert_eq!(decoded.request_id, "request");
        assert_eq!(decoded.messages, vec![message]);
    });
}
