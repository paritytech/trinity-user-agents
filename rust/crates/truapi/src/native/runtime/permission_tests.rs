use super::*;
use crate::native::tests::{EventCallbacks, native_execution_config, native_host_runtime_config};
use crate::platform::{CoreStorageKey, PermissionDecision};
use truapi::latest::{RemotePermission, RemotePermissionRequest};

fn block_on<T>(future: impl Future<Output = T>) -> T {
    // Native construction synchronously opens its database through the futures
    // executor; use the existing native Tokio test convention around that API.
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime")
        .block_on(future)
}

fn request() -> PermissionAuthorizationRequest {
    PermissionAuthorizationRequest::Remote(RemotePermissionRequest {
        permission: RemotePermission::ChainSubmit,
    })
}

fn host(callbacks: &Arc<EventCallbacks>) -> Arc<NativeTrUApiHostRuntime> {
    NativeTrUApiHostRuntime::with_runtime_config(callbacks.clone(), native_host_runtime_config())
        .unwrap()
}

fn open(
    host: &NativeTrUApiHostRuntime,
    callbacks: &Arc<EventCallbacks>,
    product: &str,
) -> Arc<NativeProductExecution> {
    host.open_product_execution(
        callbacks.clone(),
        None,
        None,
        None,
        native_execution_config(product, ProductExecutionKind::App),
    )
    .unwrap()
}

fn entry(status: PermissionAuthorizationStatus) -> PermissionAuthorizationEntry {
    PermissionAuthorizationEntry {
        request: request(),
        status,
    }
}

#[test]
fn native_permission_settings_enumerate_existing_grants_and_preserve_canonical_resets() {
    block_on(async {
        let callbacks = Arc::new(EventCallbacks::new());
        // Bytes written by the old native runtime, before settings integration.
        let key = CoreStorageKey::remote_permission_authorization(
            "settings.dot",
            &RemotePermissionRequest {
                permission: RemotePermission::ChainSubmit,
            },
        );
        callbacks
            .core_storage
            .lock()
            .expect("storage")
            .insert(key.encode(), vec![1]);
        callbacks.core_storage.lock().expect("storage").insert(
            CoreStorageKey::account_access_authorization("settings", "other").encode(),
            vec![0],
        );
        let runtime = host(&callbacks);
        let records = runtime
            .permission_authorizations("settings.dot".into())
            .await
            .unwrap();
        assert_eq!(records.len(), 2);
        assert!(records.iter().any(|record| record.request == request()
            && record.status == PermissionAuthorizationStatus::Denied));
        assert!(records.iter().any(|record| matches!(&record.request,
            PermissionAuthorizationRequest::AccountAccess { target_product_id } if target_product_id == "other")));
        let imported = runtime
            .import_permission_authorizations(
                "settings.dot".into(),
                vec![entry(PermissionAuthorizationStatus::Authorized)],
            )
            .await
            .unwrap();
        assert!(imported.iter().any(|record| record.request == request()
            && record.status == PermissionAuthorizationStatus::Denied));
        runtime
            .set_permission_authorization_status(
                "settings.dot".into(),
                request(),
                PermissionAuthorizationStatus::NotDetermined,
            )
            .await
            .unwrap();
        let restarted = host(&callbacks);
        let imported = restarted
            .import_permission_authorizations(
                "settings.dot".into(),
                vec![entry(PermissionAuthorizationStatus::Authorized)],
            )
            .await
            .unwrap();
        assert!(imported.iter().any(|record| record.request == request()
            && record.status == PermissionAuthorizationStatus::NotDetermined));
        assert!(
            restarted
                .permission_authorization_products()
                .await
                .unwrap()
                .contains(&"settings.dot".to_string())
        );
        assert!(
            callbacks
                .permission_changes
                .lock()
                .contains(&"settings.dot".to_string())
        );
    });
}

#[test]
fn native_permission_revocation_cancels_pending_prompt_and_all_sibling_executions() {
    block_on(async {
        let callbacks = Arc::new(EventCallbacks {
            remote_permission_result: Ok(PermissionDecision::AllowAlways),
            ..EventCallbacks::new()
        });
        let runtime = host(&callbacks);
        let execution = open(&runtime, &callbacks, "settings.dot");
        let sibling = open(&runtime, &callbacks, "settings.dot");
        let unrelated = open(&runtime, &callbacks, "other.dot");
        let other_network = open(&runtime, &callbacks, "settings.paseo");
        let other_executable = open(&runtime, &callbacks, "app.settings.dot");
        let (release, reply) = futures::channel::oneshot::channel();
        *callbacks.remote_permission_reply.lock().expect("reply") = Some(reply);
        let pending = execution.authorize_remote_permission(RemotePermissionRequest {
            permission: RemotePermission::ChainSubmit,
        });
        futures::pin_mut!(pending);
        assert!(futures::poll!(&mut pending).is_pending());
        runtime
            .set_permission_authorization_status(
                "settings.dot".into(),
                request(),
                PermissionAuthorizationStatus::Denied,
            )
            .await
            .unwrap();
        assert!(execution.is_closed());
        assert!(sibling.is_closed());
        assert!(!unrelated.is_closed());
        assert!(!other_network.is_closed());
        assert!(!other_executable.is_closed());
        assert_eq!(
            runtime
                .permission_authorization_revision("settings.paseo".into())
                .unwrap(),
            0
        );
        assert_eq!(
            runtime
                .permission_authorization_revision("app.settings.dot".into())
                .unwrap(),
            0
        );
        // Completes without dismissing/releasing the old native consent UI.
        assert!(pending.await.is_err());
        assert!(release.send(Ok(PermissionDecision::AllowAlways)).is_err());
        let fresh = open(&runtime, &callbacks, "settings.dot");
        assert!(!fresh.is_closed());
        assert!(
            !fresh
                .authorize_remote_permission(RemotePermissionRequest {
                    permission: RemotePermission::ChainSubmit
                })
                .await
                .unwrap()
        );
        assert!(
            unrelated
                .authorize_remote_permission(RemotePermissionRequest {
                    permission: RemotePermission::ChainSubmit
                })
                .await
                .unwrap()
        );
        assert_eq!(
            runtime
                .permission_authorizations("settings.dot".into())
                .await
                .unwrap()[0]
                .status,
            PermissionAuthorizationStatus::Denied
        );
    });
}

#[test]
fn native_permission_revocation_clears_sibling_one_use_grants() {
    use truapi::api::Permissions;
    block_on(async {
        let callbacks = Arc::new(EventCallbacks {
            remote_permission_result: Ok(PermissionDecision::AllowOnce),
            ..EventCallbacks::new()
        });
        let runtime = host(&callbacks);
        let first = open(&runtime, &callbacks, "settings.dot");
        let second = open(&runtime, &callbacks, "settings.dot");
        let first_admin = first.admin();
        let second_admin = second.admin();
        for admin in [&first_admin, &second_admin] {
            admin
                .product_runtime()
                .request_remote_permission(
                    &truapi::CallContext::default(),
                    truapi::versioned::permissions::RemotePermissionRequest::V1(
                        RemotePermissionRequest {
                            permission: RemotePermission::ChainSubmit,
                        },
                    ),
                )
                .await
                .unwrap();
            assert_eq!(
                admin
                    .permission_authorization_status(request())
                    .await
                    .unwrap(),
                PermissionAuthorizationStatus::Authorized
            );
        }
        runtime
            .set_permission_authorization_status(
                "settings.dot".into(),
                request(),
                PermissionAuthorizationStatus::NotDetermined,
            )
            .await
            .unwrap();
        for admin in [&first_admin, &second_admin] {
            assert_eq!(
                admin
                    .permission_authorization_status(request())
                    .await
                    .unwrap(),
                PermissionAuthorizationStatus::NotDetermined
            );
        }
    });
}

#[test]
fn native_permission_legacy_commit_is_fenced_without_breaking_grant_bundles() {
    block_on(async {
        let callbacks = Arc::new(EventCallbacks::new());
        let runtime = host(&callbacks);
        let revision = runtime
            .permission_authorization_revision("settings.dot".into())
            .unwrap();
        assert!(
            runtime
                .set_permission_authorization_status_if_current(
                    "settings.dot".into(),
                    request(),
                    PermissionAuthorizationStatus::Authorized,
                    revision
                )
                .await
                .unwrap()
        );
        assert_eq!(
            runtime
                .permission_authorization_revision("settings.dot".into())
                .unwrap(),
            revision
        );
        assert!(
            runtime
                .set_permission_authorization_status_if_current(
                    "settings.dot".into(),
                    PermissionAuthorizationRequest::IdentityDisclosure,
                    PermissionAuthorizationStatus::Authorized,
                    revision
                )
                .await
                .unwrap()
        );
        runtime
            .set_permission_authorization_status(
                "settings.dot".into(),
                request(),
                PermissionAuthorizationStatus::Denied,
            )
            .await
            .unwrap();
        assert!(
            !runtime
                .set_permission_authorization_status_if_current(
                    "settings.dot".into(),
                    request(),
                    PermissionAuthorizationStatus::Authorized,
                    revision
                )
                .await
                .unwrap()
        );
        let next = runtime
            .permission_authorization_revision("settings.dot".into())
            .unwrap();
        assert_eq!(next, revision + 1);
        assert!(
            runtime
                .set_permission_authorization_status_if_current(
                    "settings.dot".into(),
                    request(),
                    PermissionAuthorizationStatus::NotDetermined,
                    next
                )
                .await
                .unwrap()
        );
        assert_eq!(
            runtime
                .permission_authorization_revision("settings.dot".into())
                .unwrap(),
            next + 1
        );
    });
}

#[test]
fn native_permission_import_cannot_race_an_inflight_canonical_write() {
    block_on(async {
        let callbacks = Arc::new(EventCallbacks::new());
        let runtime = host(&callbacks);
        let (release, wait) = futures::channel::oneshot::channel();
        *callbacks.core_storage_write_release.lock() = Some(wait);
        let canonical = runtime.set_permission_authorization_status(
            "settings.dot".into(),
            request(),
            PermissionAuthorizationStatus::Denied,
        );
        let import = runtime.import_permission_authorizations(
            "settings.dot".into(),
            vec![entry(PermissionAuthorizationStatus::Authorized)],
        );
        futures::pin_mut!(canonical, import);
        assert!(futures::poll!(&mut canonical).is_pending());
        assert!(futures::poll!(&mut import).is_pending());
        release.send(()).unwrap();
        let (written, imported) = futures::join!(canonical, import);
        written.unwrap();
        assert_eq!(
            imported.unwrap()[0].status,
            PermissionAuthorizationStatus::Denied
        );
    });
}

#[test]
fn native_permission_revoke_waits_out_prior_persistence_before_acknowledging() {
    block_on(async {
        let callbacks = Arc::new(EventCallbacks {
            remote_permission_result: Ok(PermissionDecision::AllowAlways),
            ..EventCallbacks::new()
        });
        let runtime = host(&callbacks);
        let execution = open(&runtime, &callbacks, "settings.dot");
        let (release, wait) = futures::channel::oneshot::channel();
        *callbacks.core_storage_write_release.lock() = Some(wait);
        let grant = execution.authorize_remote_permission(RemotePermissionRequest {
            permission: RemotePermission::ChainSubmit,
        });
        let revoke = runtime.set_permission_authorization_status(
            "settings.dot".into(),
            request(),
            PermissionAuthorizationStatus::Denied,
        );
        futures::pin_mut!(grant, revoke);
        assert!(futures::poll!(&mut grant).is_pending());
        assert!(futures::poll!(&mut revoke).is_pending());
        release.send(()).unwrap();
        let (_, revoked) = futures::join!(grant, revoke);
        revoked.unwrap();
        assert_eq!(
            runtime
                .permission_authorizations("settings.dot".into())
                .await
                .unwrap()[0]
                .status,
            PermissionAuthorizationStatus::Denied
        );
        assert!(execution.is_closed());
    });
}

#[test]
fn native_permission_failures_are_errors_not_empty_lists_or_successful_revokes() {
    block_on(async {
        let callbacks = Arc::new(EventCallbacks::new());
        let runtime = host(&callbacks);
        runtime
            .set_permission_authorization_status(
                "settings.dot".into(),
                request(),
                PermissionAuthorizationStatus::Authorized,
            )
            .await
            .unwrap();
        let execution = open(&runtime, &callbacks, "settings.dot");
        callbacks
            .core_storage_write_failure
            .store(true, Ordering::SeqCst);
        assert!(
            runtime
                .set_permission_authorization_status(
                    "settings.dot".into(),
                    request(),
                    PermissionAuthorizationStatus::Denied
                )
                .await
                .is_err()
        );
        assert!(execution.is_closed());
        assert_eq!(
            runtime
                .permission_authorizations("settings.dot".into())
                .await
                .unwrap()[0]
                .status,
            PermissionAuthorizationStatus::Authorized
        );
        callbacks
            .core_storage_keys_failure
            .store(true, Ordering::SeqCst);
        assert!(
            runtime
                .permission_authorizations("settings.dot".into())
                .await
                .is_err()
        );
        assert!(runtime.permission_authorization_products().await.is_err());
    });
}

#[test]
fn native_permission_account_aliases_never_display_phantom_authority() {
    block_on(async {
        let callbacks = Arc::new(EventCallbacks::new());
        callbacks.core_storage.lock().expect("storage").insert(
            CoreStorageKey::account_access_authorization("settings.dot", "other.dot").encode(),
            vec![0],
        );
        let runtime = host(&callbacks);
        let records = runtime
            .permission_authorizations("settings.dot".into())
            .await
            .unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(
            records[0].status,
            PermissionAuthorizationStatus::NotDetermined
        );
        callbacks.core_storage.lock().expect("storage").insert(
            CoreStorageKey::account_access_authorization("settings", "other").encode(),
            vec![1],
        );
        let records = runtime
            .permission_authorizations("settings.dot".into())
            .await
            .unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].status, PermissionAuthorizationStatus::Denied);
        assert_eq!(
            records[0].request,
            PermissionAuthorizationRequest::AccountAccess {
                target_product_id: "other".into()
            }
        );
    });
}

#[test]
fn native_permission_account_revocation_preserves_existing_bare_grantee_scope() {
    block_on(async {
        let callbacks = Arc::new(EventCallbacks::new());
        let runtime = host(&callbacks);
        let root = open(&runtime, &callbacks, "settings.dot");
        let app = open(&runtime, &callbacks, "app.settings.dot");
        let network = open(&runtime, &callbacks, "settings.paseo");
        let unrelated = open(&runtime, &callbacks, "other.dot");
        let revision = runtime
            .permission_authorization_revision("app.settings.dot".into())
            .unwrap();
        let request = PermissionAuthorizationRequest::AccountAccess {
            target_product_id: "target.dot".into(),
        };
        runtime
            .set_permission_authorization_status(
                "settings.dot".into(),
                request.clone(),
                PermissionAuthorizationStatus::Denied,
            )
            .await
            .unwrap();
        assert!(root.is_closed() && app.is_closed() && network.is_closed());
        assert!(!unrelated.is_closed());
        assert!(
            !runtime
                .set_permission_authorization_status_if_current(
                    "app.settings.dot".into(),
                    request.clone(),
                    PermissionAuthorizationStatus::Authorized,
                    revision
                )
                .await
                .unwrap()
        );
        let fresh = open(&runtime, &callbacks, "app.settings.dot");
        assert_eq!(
            fresh
                .admin()
                .permission_authorization_status(request)
                .await
                .unwrap(),
            PermissionAuthorizationStatus::Denied
        );
    });
}

#[test]
fn native_permission_bundle_import_and_edits_do_not_rewrite_singleton_authority() {
    block_on(async {
        let callbacks = Arc::new(EventCallbacks::new());
        let runtime = host(&callbacks);
        let domain_request = |domains: &[&str]| {
            PermissionAuthorizationRequest::Remote(RemotePermissionRequest {
                permission: RemotePermission::Remote {
                    domains: domains.iter().map(|domain| (*domain).to_owned()).collect(),
                },
            })
        };
        let bundle = domain_request(&["a.example", "b.example"]);
        let singleton = domain_request(&["a.example"]);
        runtime
            .import_permission_authorizations(
                "settings.dot".into(),
                vec![PermissionAuthorizationEntry {
                    request: bundle.clone(),
                    status: PermissionAuthorizationStatus::Denied,
                }],
            )
            .await
            .unwrap();
        let execution = open(&runtime, &callbacks, "settings.dot");
        assert_eq!(
            execution
                .admin()
                .permission_authorization_status(bundle.clone())
                .await
                .unwrap(),
            PermissionAuthorizationStatus::Denied
        );
        assert_eq!(
            execution
                .admin()
                .permission_authorization_status(singleton.clone())
                .await
                .unwrap(),
            PermissionAuthorizationStatus::NotDetermined
        );
        runtime
            .set_permission_authorization_status(
                "settings.dot".into(),
                singleton.clone(),
                PermissionAuthorizationStatus::Authorized,
            )
            .await
            .unwrap();
        runtime
            .set_permission_authorization_status(
                "settings.dot".into(),
                bundle,
                PermissionAuthorizationStatus::NotDetermined,
            )
            .await
            .unwrap();
        let fresh = open(&runtime, &callbacks, "settings.dot");
        assert_eq!(
            fresh
                .admin()
                .permission_authorization_status(singleton)
                .await
                .unwrap(),
            PermissionAuthorizationStatus::Authorized
        );
        assert_eq!(
            runtime
                .permission_authorizations("settings.dot".into())
                .await
                .unwrap()
                .len(),
            2
        );
    });
}

#[test]
fn native_permission_explicit_bundle_denial_overrides_trusted_default_only_for_that_bundle() {
    block_on(async {
        let callbacks = Arc::new(EventCallbacks::new());
        let runtime = host(&callbacks);
        let bundle = RemotePermissionRequest {
            permission: RemotePermission::Remote {
                domains: vec!["a.example".into(), "b.example".into()],
            },
        };
        runtime
            .set_permission_authorization_status(
                "peopl.dot".into(),
                PermissionAuthorizationRequest::Remote(bundle.clone()),
                PermissionAuthorizationStatus::Denied,
            )
            .await
            .unwrap();
        let execution = open(&runtime, &callbacks, "peopl.dot");
        assert!(!execution.authorize_remote_permission(bundle).await.unwrap());
        assert!(
            execution
                .authorize_remote_permission(RemotePermissionRequest {
                    permission: RemotePermission::Remote {
                        domains: vec!["a.example".into()]
                    },
                })
                .await
                .unwrap()
        );
    });
}
