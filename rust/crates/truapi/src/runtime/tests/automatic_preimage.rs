use super::*;

fn upload_host(platform: Arc<StubPlatform>, product_id: &str) -> ProductRuntimeHost {
    let host = ProductRuntimeHost::new(platform, runtime_config(product_id), test_spawner());
    install_pairing_session(&host, session_info());
    host
}

fn permission(session: &AuthoritySession) -> PermissionAuthorizationRequest {
    PermissionAuthorizationRequest::AutomaticPreimageSubmit {
        root_public_key: session.public_key,
    }
}

#[test]
fn once_is_not_durable_and_revocation_fences_an_approved_upload() {
    futures::executor::block_on(async {
        let platform = Arc::new(StubPlatform::default());
        platform
            .permission_confirmation_decisions
            .lock()
            .unwrap()
            .extend([
                PermissionDecision::AllowOnce,
                PermissionDecision::AllowAlways,
                PermissionDecision::AllowOnce,
            ]);
        let host = upload_host(platform.clone(), "peopl.paseo");
        let session = host.authority.current_session().unwrap();
        host.approve_preimage_upload(&session, 1024).await.unwrap();
        assert_eq!(
            host.permission_authorization_status(permission(&session))
                .await
                .unwrap(),
            PermissionAuthorizationStatus::NotDetermined
        );
        host.approve_preimage_upload(&session, 1024).await.unwrap();
        let approval = host
            .approve_preimage_upload(&session, 262_144)
            .await
            .unwrap();
        assert_eq!(platform.preimage_reviews.lock().len(), 2);
        host.set_permission_authorization_status(
            permission(&session),
            PermissionAuthorizationStatus::NotDetermined,
        )
        .await
        .unwrap();
        assert!(
            host.validate_upload_approval(&session, &approval)
                .await
                .is_err()
        );
        host.approve_preimage_upload(&session, 1024).await.unwrap();
        assert_eq!(platform.preimage_reviews.lock().len(), 3);
        assert_eq!(
            host.permission_authorization_status(permission(&session))
                .await
                .unwrap(),
            PermissionAuthorizationStatus::NotDetermined
        );
    });
}

#[test]
fn automatic_quota_is_shared_by_connections_and_survives_restart_and_regrant() {
    futures::executor::block_on(async {
        let platform = Arc::new(StubPlatform::default());
        let host = upload_host(platform.clone(), "upload.dot");
        let session = host.authority.current_session().unwrap();
        host.set_permission_authorization_status(
            permission(&session),
            PermissionAuthorizationStatus::Authorized,
        )
        .await
        .unwrap();
        let other = ProductRuntimeHost::from_services(
            host.services.clone(),
            crate::host_core::ConnectionAdapters::from_services(&host.services),
            host.authority.clone(),
            host.product.clone(),
        );
        let calls = (0..8).map(|i| {
            let target = if i % 2 == 0 { &host } else { &other };
            target.approve_preimage_upload(&session, 262_144)
        });
        for approval in futures::future::join_all(calls).await {
            approval.unwrap();
        }
        // Only four requests may bypass the explicit one-shot prompt.
        assert_eq!(platform.preimage_reviews.lock().len(), 4);
        drop(other);
        drop(host);
        let restored = upload_host(platform.clone(), "upload.dot");
        restored
            .set_permission_authorization_status(
                permission(&session),
                PermissionAuthorizationStatus::Denied,
            )
            .await
            .unwrap();
        restored
            .set_permission_authorization_status(
                permission(&session),
                PermissionAuthorizationStatus::Authorized,
            )
            .await
            .unwrap();
        platform
            .permission_confirmation_decisions
            .lock()
            .unwrap()
            .push_back(PermissionDecision::Deny);
        let session = restored.authority.current_session().unwrap();
        assert!(
            restored
                .approve_preimage_upload(&session, 1024)
                .await
                .is_err()
        );
        assert_eq!(platform.preimage_reviews.lock().len(), 5);
    });
}

#[test]
fn independent_runtimes_cannot_overspend_a_stale_quota_snapshot() {
    futures::executor::block_on(async {
        let platform = Arc::new(StubPlatform::default());
        let host = upload_host(platform.clone(), "upload.dot");
        let other = upload_host(platform.clone(), "upload.dot");
        let session = host.authority.current_session().unwrap();
        let other_session = other.authority.current_session().unwrap();
        host.set_permission_authorization_status(
            permission(&session),
            PermissionAuthorizationStatus::Authorized,
        )
        .await
        .unwrap();
        let (release, gate) = futures::channel::oneshot::channel();
        *platform.preimage_read_gate.lock() = Some(gate);
        let pending = host.approve_preimage_upload(&session, 1);
        futures::pin_mut!(pending);
        assert!(futures::poll!(pending.as_mut()).is_pending());
        for _ in 0..4 {
            other
                .approve_preimage_upload(&other_session, 1)
                .await
                .unwrap();
        }
        assert!(platform.preimage_reviews.lock().is_empty());
        release.send(()).unwrap();
        pending.await.unwrap();
        // The fifth request must prompt after its stale reservation loses CAS.
        assert_eq!(platform.preimage_reviews.lock().len(), 1);
        other
            .approve_preimage_upload(&other_session, 1)
            .await
            .unwrap();
        assert_eq!(platform.preimage_reviews.lock().len(), 2);
    });
}

#[test]
fn independent_runtime_revocation_rejects_a_stale_prompt_commit() {
    futures::executor::block_on(async {
        let platform = Arc::new(StubPlatform::default());
        platform
            .permission_confirmation_decisions
            .lock()
            .unwrap()
            .push_back(PermissionDecision::AllowAlways);
        let (answer, confirmation_gate) = futures::channel::oneshot::channel();
        *platform.preimage_confirmation_gate.lock() = Some(confirmation_gate);
        let host = upload_host(platform.clone(), "upload.dot");
        let other = upload_host(platform.clone(), "upload.dot");
        let session = host.authority.current_session().unwrap();
        let other_session = other.authority.current_session().unwrap();
        let pending = host.approve_preimage_upload(&session, 1);
        futures::pin_mut!(pending);
        assert!(futures::poll!(pending.as_mut()).is_pending());
        let (release, read_gate) = futures::channel::oneshot::channel();
        *platform.preimage_read_gate.lock() = Some(read_gate);
        answer.send(()).unwrap();
        assert!(futures::poll!(pending.as_mut()).is_pending());
        other
            .set_permission_authorization_status(
                permission(&other_session),
                PermissionAuthorizationStatus::Denied,
            )
            .await
            .unwrap();
        release.send(()).unwrap();
        assert!(pending.await.is_err());
        assert_eq!(
            host.permission_authorization_status(permission(&session))
                .await
                .unwrap(),
            PermissionAuthorizationStatus::Denied,
        );
    });
}

#[test]
fn oversized_upload_requires_review_without_spending_the_small_upload_quota() {
    futures::executor::block_on(async {
        let platform = Arc::new(StubPlatform::default());
        let host = upload_host(platform.clone(), "upload.dot");
        let session = host.authority.current_session().unwrap();
        host.set_permission_authorization_status(
            permission(&session),
            PermissionAuthorizationStatus::Authorized,
        )
        .await
        .unwrap();
        host.approve_preimage_upload(&session, 262_145)
            .await
            .unwrap();
        for _ in 0..4 {
            host.approve_preimage_upload(&session, 262_144)
                .await
                .unwrap();
        }
        assert_eq!(platform.preimage_reviews.lock().len(), 1);
        host.approve_preimage_upload(&session, 1).await.unwrap();
        assert_eq!(platform.preimage_reviews.lock().len(), 2);
    });
}

#[test]
fn upload_consent_isolated_by_product_root_and_bulletin_network() {
    futures::executor::block_on(async {
        let platform = Arc::new(StubPlatform::default());
        let host = upload_host(platform.clone(), "upload.dot");
        let session = host.authority.current_session().unwrap();
        host.set_permission_authorization_status(
            permission(&session),
            PermissionAuthorizationStatus::Authorized,
        )
        .await
        .unwrap();
        let other_product = upload_host(platform.clone(), "other.dot");
        assert_eq!(
            other_product
                .permission_authorization_status(permission(&session))
                .await
                .unwrap(),
            PermissionAuthorizationStatus::NotDetermined
        );
        let (mut config, product) = runtime_config("upload.dot");
        config.bulletin_chain_genesis_hash = [0x99; 32];
        let other_network = ProductRuntimeHost::new(platform, (config, product), test_spawner());
        install_pairing_session(&other_network, session_info());
        assert_eq!(
            other_network
                .permission_authorization_status(permission(&session))
                .await
                .unwrap(),
            PermissionAuthorizationStatus::NotDetermined
        );
        let mut new_account = session_info();
        new_account.public_key = [0x88; 32];
        install_pairing_session(&host, new_account);
        assert!(
            host.set_permission_authorization_status(
                permission(&session),
                PermissionAuthorizationStatus::Authorized
            )
            .await
            .is_err()
        );
        let new_session = host.authority.current_session().unwrap();
        assert_eq!(
            host.permission_authorization_status(permission(&new_session))
                .await
                .unwrap(),
            PermissionAuthorizationStatus::NotDetermined
        );
        install_pairing_session(&host, session_info());
        assert_eq!(
            host.permission_authorization_status(permission(&session))
                .await
                .unwrap(),
            PermissionAuthorizationStatus::Authorized
        );
    });
}

#[test]
fn revoking_while_the_consent_sheet_is_open_cannot_restore_the_grant() {
    futures::executor::block_on(async {
        let (release, gate) = futures::channel::oneshot::channel();
        let platform = Arc::new(StubPlatform::default());
        *platform.preimage_confirmation_gate.lock() = Some(gate);
        platform
            .permission_confirmation_decisions
            .lock()
            .unwrap()
            .push_back(PermissionDecision::AllowAlways);
        let host = upload_host(platform, "upload.dot");
        let session = host.authority.current_session().unwrap();
        let approval = host.approve_preimage_upload(&session, 1);
        futures::pin_mut!(approval);
        assert!(futures::poll!(approval.as_mut()).is_pending());
        host.set_permission_authorization_status(
            permission(&session),
            PermissionAuthorizationStatus::NotDetermined,
        )
        .await
        .unwrap();
        release.send(()).unwrap();
        assert!(approval.await.is_err());
        assert_eq!(
            host.permission_authorization_status(permission(&session))
                .await
                .unwrap(),
            PermissionAuthorizationStatus::NotDetermined
        );
    });
}

#[test]
fn switching_accounts_while_the_consent_sheet_is_open_cannot_grant_either_account() {
    futures::executor::block_on(async {
        let (release, gate) = futures::channel::oneshot::channel();
        let platform = Arc::new(StubPlatform::default());
        *platform.preimage_confirmation_gate.lock() = Some(gate);
        platform
            .permission_confirmation_decisions
            .lock()
            .unwrap()
            .push_back(PermissionDecision::AllowAlways);
        let host = upload_host(platform, "upload.dot");
        let session = host.authority.current_session().unwrap();
        let approval = host.approve_preimage_upload(&session, 1);
        futures::pin_mut!(approval);
        assert!(futures::poll!(approval.as_mut()).is_pending());
        let mut other = session_info();
        other.public_key = [0x88; 32];
        install_pairing_session(&host, other);
        release.send(()).unwrap();
        assert!(approval.await.is_err());
        let current = host.authority.current_session().unwrap();
        assert_eq!(
            host.permission_authorization_status(permission(&current))
                .await
                .unwrap(),
            PermissionAuthorizationStatus::NotDetermined
        );
        install_pairing_session(&host, session_info());
        assert_eq!(
            host.permission_authorization_status(permission(&session))
                .await
                .unwrap(),
            PermissionAuthorizationStatus::NotDetermined
        );
    });
}

#[test]
fn cancelling_the_product_upload_dismisses_consent_without_a_grant_or_backend_call() {
    futures::executor::block_on(async {
        let (_release, gate) = futures::channel::oneshot::channel();
        let platform = Arc::new(StubPlatform::default());
        *platform.preimage_confirmation_gate.lock() = Some(gate);
        platform
            .permission_confirmation_decisions
            .lock()
            .unwrap()
            .push_back(PermissionDecision::AllowAlways);
        let host = upload_host(platform.clone(), "upload.dot");
        let session = host.authority.current_session().unwrap();
        let cancel = truapi::CancellationToken::default();
        let cx = CallContext::with_parts("cancel-upload".into(), cancel.clone());
        let upload = Preimage::submit(&host, &cx, RemotePreimageSubmitRequest::V1(vec![1]));
        futures::pin_mut!(upload);
        assert!(futures::poll!(upload.as_mut()).is_pending());
        cancel.cancel();
        assert!(upload.await.is_err());
        assert_eq!(
            host.permission_authorization_status(permission(&session))
                .await
                .unwrap(),
            PermissionAuthorizationStatus::NotDetermined
        );
        assert!(platform.sent_rpc.lock().unwrap().is_empty());
    });
}

#[test]
fn automatic_consent_never_bypasses_remote_upload_permission() {
    futures::executor::block_on(async {
        let platform = Arc::new(StubPlatform {
            remote_permission_denied: true,
            ..Default::default()
        });
        let host = upload_host(platform.clone(), "upload.dot");
        let session = host.authority.current_session().unwrap();
        host.set_permission_authorization_status(
            permission(&session),
            PermissionAuthorizationStatus::Authorized,
        )
        .await
        .unwrap();
        assert!(
            Preimage::submit(
                &host,
                &CallContext::default(),
                RemotePreimageSubmitRequest::V1(vec![1])
            )
            .await
            .is_err()
        );
        assert!(platform.sent_rpc.lock().unwrap().is_empty());
        assert!(platform.preimage_reviews.lock().is_empty());
        assert_eq!(
            host.permission_authorization_status(permission(&session))
                .await
                .unwrap(),
            PermissionAuthorizationStatus::Authorized
        );
    });
}

#[test]
fn unreadable_consent_and_failed_quota_writes_never_authorize_an_upload() {
    futures::executor::block_on(async {
        let platform = Arc::new(StubPlatform::default());
        let host = upload_host(platform.clone(), "upload.dot");
        let session = host.authority.current_session().unwrap();
        let key = CoreStorageKey::AutomaticPreimageUploads {
            product_id: host.product_id(),
            root_public_key: session.public_key,
            genesis_hash: host.services.bulletin.genesis_hash(),
        };
        host.set_permission_authorization_status(
            permission(&session),
            PermissionAuthorizationStatus::Authorized,
        )
        .await
        .unwrap();
        platform
            .core_write_failures
            .lock()
            .insert(core_storage_test_key(key.clone()));
        assert!(host.approve_preimage_upload(&session, 1).await.is_err());
        platform.core_write_failures.lock().clear();
        platform.write_core_storage(key, vec![0xff]).await.unwrap();
        assert!(host.approve_preimage_upload(&session, 1).await.is_err());
        assert!(platform.preimage_reviews.lock().is_empty());
    });
}
