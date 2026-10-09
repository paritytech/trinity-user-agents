use super::*;
use crate::platform::{
    PermissionAuthorizationRequest, PermissionAuthorizationStatus, ProductContext,
};
use crate::runtime::{NativeChatContact, NativeChatContactsSnapshot};

async fn authorize(fixture: &Fixture, product: &str) {
    set_product_grants(
        &fixture.platform,
        product,
        PermissionAuthorizationStatus::NotDetermined,
    )
    .await;
}

fn snapshot(
    context: &NativeChatContext,
    contacts: Vec<NativeChatContact>,
) -> NativeChatContactsSnapshot {
    NativeChatContactsSnapshot {
        wallet_public_key: format!("0x{}", hex::encode(context.session.public_key)),
        genesis_hash: format!("0x{}", hex::encode(context.genesis_hash)),
        contacts,
    }
}

fn contact(identity: &IdentityFixture, username: Option<&str>) -> NativeChatContact {
    NativeChatContact {
        peer_identity: format!("0x{}", hex::encode(identity.account)),
        username: username.map(str::to_owned),
    }
}

#[test]
fn contacts_restore_authenticated_rosters_and_isolate_wallet_network_and_grants() {
    block_on(async {
        let fixture = Fixture::new();
        let registry = NativeChatRegistry::default();
        authorize(&fixture, PRODUCT).await;
        let actor = registry.chat(&fixture.context, PRODUCT).await.unwrap();
        let identity = IdentityFixture::new();
        seed_peer(&actor, &identity, &[&DeviceFixture::new(1)]).await;
        let expected = snapshot(&fixture.context, vec![contact(&identity, Some("peer.dot"))]);
        assert_eq!(registry.contacts(&fixture.context).await.unwrap(), expected);
        drop(actor);
        registry.release();
        let restored = NativeChatRegistry::default();
        assert_eq!(restored.contacts(&fixture.context).await.unwrap(), expected);

        let mut other_wallet = fixture.context.clone();
        other_wallet.session.public_key = [0x81; 32];
        other_wallet.entropy = Zeroizing::new(vec![0x82; 16]);
        assert_eq!(
            restored.contacts(&other_wallet).await.unwrap(),
            snapshot(&other_wallet, vec![])
        );
        let mut other_network = fixture.context.clone();
        other_network.genesis_hash = [0x83; 32];
        assert_eq!(
            restored.contacts(&other_network).await.unwrap(),
            snapshot(&other_network, vec![])
        );

        let product = ProductContext::new_with_execution(
            PRODUCT.into(),
            crate::platform::ProductExecutionKind::Worker,
        )
        .unwrap();
        crate::host_internal::permissions::PermissionsService::new(
            fixture.platform.as_ref(),
            fixture.platform.as_ref(),
            &product,
        )
        .set_authorization_status(
            &PermissionAuthorizationRequest::ChatAuthority,
            PermissionAuthorizationStatus::Denied,
        )
        .await
        .unwrap();
        assert_eq!(
            restored.contacts(&fixture.context).await.unwrap(),
            snapshot(&fixture.context, vec![])
        );
        fixture.tasks.live.store(false, Ordering::Release);
        assert_eq!(
            restored.contacts(&fixture.context).await,
            Err(Error::NotConnected)
        );
    });
}

#[test]
fn contacts_exclude_unready_peers_and_never_promote_a_malicious_sender() {
    block_on(async {
        let fixture = Fixture::new();
        let registry = NativeChatRegistry::default();
        authorize(&fixture, PRODUCT).await;
        let actor = registry.chat(&fixture.context, PRODUCT).await.unwrap();
        let identity = IdentityFixture::new();
        let device = DeviceFixture::new(1);
        seed_peer(&actor, &identity, &[&device]).await;
        let ready = actor
            .store
            .read(|state| state.peers[0].clone())
            .await
            .unwrap();
        let invitation = Invitation {
            id: [0x90; 32],
            peer: keypair(0x91).public.to_bytes(),
            root_key: wire::x25519_public_key(&[0x92; 32]),
            username: Some("unaccepted.dot".into()),
            device_account: DeviceFixture::new(3).account(),
            device_key: DeviceFixture::new(3).public_key(),
            message_id: "unaccepted".into(),
            timestamp: fixture.timestamp,
            text: "not a contact".into(),
        };
        actor
            .store
            .update(move |state| {
                state.invitations.push(invitation);
                Ok(())
            })
            .await
            .unwrap();
        for condition in 0..3 {
            let mut peer = ready.clone();
            match condition {
                0 => peer.established = false,
                1 => peer.revocation_acked = false,
                _ => peer.devices[0].active = false,
            }
            actor
                .store
                .update(move |state| {
                    state.peers[0] = peer;
                    Ok(())
                })
                .await
                .unwrap();
            assert_eq!(
                registry.contacts(&fixture.context).await.unwrap(),
                snapshot(&fixture.context, vec![])
            );
        }
        actor
            .store
            .update(move |state| {
                state.peers[0] = ready;
                Ok(())
            })
            .await
            .unwrap();
        let stranger = DeviceFixture::new(2);
        let added = wire::encode_device_added_message(
            "forged-admission",
            fixture.timestamp,
            &stranger.account(),
            &stranger.public_key(),
        )
        .unwrap();
        let packet = native_packet(
            &actor,
            &identity,
            &stranger,
            &wire::encode_transport_request_plaintext("forged", &[added]).unwrap(),
            false,
            true,
        );
        assert_eq!(
            actor
                .open_statement(&fixture.context, &registry, packet)
                .await,
            Err(Error::InvalidStatement)
        );
        assert_eq!(
            registry.contacts(&fixture.context).await.unwrap(),
            snapshot(&fixture.context, vec![contact(&identity, Some("peer.dot"))],)
        );
    });
}

#[test]
fn contacts_deduplicate_and_omit_ambiguous_verified_names() {
    block_on(async {
        let fixture = Fixture::new();
        let registry = NativeChatRegistry::default();
        let identity = IdentityFixture::new();
        let second = IdentityFixture {
            account: keypair(0x72).public.to_bytes(),
            secret: [0x73; 32],
        };
        for (product, username) in [
            (PRODUCT, "peer.dot"),
            ("other.dot", "renamed.dot"),
            ("third.dot", "peer.dot"),
        ] {
            authorize(&fixture, product).await;
            let actor = registry.chat(&fixture.context, product).await.unwrap();
            seed_peer(&actor, &identity, &[&DeviceFixture::new(1)]).await;
            actor
                .store
                .update(move |state| {
                    state.peers[0].username = Some(username.into());
                    Ok(())
                })
                .await
                .unwrap();
            if product == PRODUCT {
                seed_peer(&actor, &second, &[&DeviceFixture::new(2)]).await;
            }
        }
        let mut expected = vec![contact(&identity, None), contact(&second, Some("peer.dot"))];
        expected.sort_by(|a, b| a.peer_identity.cmp(&b.peer_identity));
        assert_eq!(
            registry.contacts(&fixture.context).await.unwrap(),
            snapshot(&fixture.context, expected)
        );
        registry
            .forget_product(&fixture.context, PRODUCT)
            .await
            .unwrap();
        assert_eq!(
            registry.contacts(&fixture.context).await.unwrap(),
            snapshot(&fixture.context, vec![contact(&identity, None)],)
        );
    });
}

#[test]
fn contacts_device_removal_invalidates_handles_and_survives_restart() {
    block_on(async {
        let fixture = Fixture::new();
        let registry = NativeChatRegistry::default();
        authorize(&fixture, PRODUCT).await;
        let actor = registry.chat(&fixture.context, PRODUCT).await.unwrap();
        let identity = IdentityFixture::new();
        let device = DeviceFixture::new(1);
        seed_peer(&actor, &identity, &[&device]).await;
        let handles = crate::runtime::contacts::ContactHandles::from_handle_key([0x55; 32]);
        let handle = handles.mint(&identity.account);
        let cache = &fixture.context.services.contact_handles;
        let generation = cache.generation();
        cache.insert(handle, identity.account, generation);
        assert_eq!(cache.get(&handle, &handles), Some(identity.account));
        let removal = wire::encode_device_removed_message(
            "removed-device",
            fixture.timestamp,
            &device.account(),
        )
        .unwrap();
        let packet = request(&actor, &identity, &device, "remove", &[removal]);
        actor
            .open_statement(&fixture.context, &registry, packet)
            .await
            .unwrap();
        cache.insert(handle, identity.account, generation);
        assert_eq!(cache.get(&handle, &handles), None);
        assert_eq!(
            registry.contacts(&fixture.context).await.unwrap(),
            snapshot(&fixture.context, vec![])
        );
        drop(actor);
        registry.release();
        assert_eq!(
            NativeChatRegistry::default()
                .contacts(&fixture.context)
                .await
                .unwrap(),
            snapshot(&fixture.context, vec![])
        );
    });
}

#[test]
fn contacts_reject_a_session_change_while_waiting_for_actor_commit() {
    block_on(async {
        let fixture = Fixture::new();
        let registry = NativeChatRegistry::default();
        authorize(&fixture, PRODUCT).await;
        let actor = registry.chat(&fixture.context, PRODUCT).await.unwrap();
        seed_peer(&actor, &IdentityFixture::new(), &[&DeviceFixture::new(1)]).await;
        let (entered, waiting) = futures::channel::oneshot::channel();
        let (resume, paused) = std::sync::mpsc::channel();
        let mut mutation = Box::pin(actor.store.update(move |state| {
            let _ = entered.send(());
            let _ = paused.recv();
            state.peers[0].revocation_acked = false;
            Ok(())
        }));
        assert!(futures::poll!(mutation.as_mut()).is_pending());
        waiting.await.unwrap();
        let mut read = Box::pin(registry.contacts(&fixture.context));
        assert!(futures::poll!(read.as_mut()).is_pending());
        fixture.tasks.live.store(false, Ordering::Release);
        resume.send(()).unwrap();
        mutation.await.unwrap();
        assert_eq!(read.await, Err(Error::NotConnected));
    });
}

#[test]
fn contacts_reject_tampered_persisted_roster_without_empty_fallback() {
    block_on(async {
        let fixture = Fixture::new();
        let registry = NativeChatRegistry::default();
        authorize(&fixture, PRODUCT).await;
        let actor = registry.chat(&fixture.context, PRODUCT).await.unwrap();
        seed_peer(&actor, &IdentityFixture::new(), &[&DeviceFixture::new(1)]).await;
        drop(actor);
        registry.release();
        let key = core_storage_test_key(CoreStorageKey::NativeChatDevice {
            root_public_key: fixture.context.session.public_key,
            genesis_hash: fixture.context.genesis_hash,
            product_id: PRODUCT.into(),
        });
        *fixture
            .platform
            .local_storage
            .lock()
            .unwrap()
            .get_mut(&key)
            .unwrap()
            .last_mut()
            .unwrap() ^= 1;
        assert_eq!(
            registry.contacts(&fixture.context).await,
            Err(Error::StorageUnavailable)
        );
    });
}

#[test]
fn contacts_do_not_label_two_identities_with_the_same_roster_name() {
    block_on(async {
        let fixture = Fixture::new();
        let registry = NativeChatRegistry::default();
        authorize(&fixture, PRODUCT).await;
        let actor = registry.chat(&fixture.context, PRODUCT).await.unwrap();
        let first = IdentityFixture::new();
        let second = IdentityFixture {
            account: keypair(0x72).public.to_bytes(),
            secret: [0x73; 32],
        };
        seed_peer(&actor, &first, &[&DeviceFixture::new(1)]).await;
        seed_peer(&actor, &second, &[&DeviceFixture::new(2)]).await;
        let mut expected = vec![contact(&first, None), contact(&second, None)];
        expected.sort_by(|a, b| a.peer_identity.cmp(&b.peer_identity));
        assert_eq!(
            registry.contacts(&fixture.context).await.unwrap(),
            snapshot(&fixture.context, expected),
        );
    });
}
