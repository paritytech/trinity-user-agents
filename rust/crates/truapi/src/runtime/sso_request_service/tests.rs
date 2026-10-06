use super::*;
use crate::platform::SecretCoreStorageKey;
use crate::platform::{CoreStorage, SecretCoreStorage};
use crate::runtime::ProductRuntimeHost;
use crate::runtime::allowances;
use crate::runtime::authority::AutoSigningKey;
use crate::test_support::{StubPlatform, sso_session_info, test_spawner};
use futures::FutureExt;
use futures::executor::block_on;
use parity_scale_codec::Encode;
use std::collections::BTreeMap;
use truapi::latest::GenericError;

#[derive(Default)]
struct CleanupStorage {
    values: Mutex<BTreeMap<Vec<u8>, Vec<u8>>>,
    pause: Mutex<Option<(Vec<u8>, oneshot::Receiver<()>)>>,
    failure: Mutex<Option<Vec<u8>>>,
    write_pause: Mutex<Option<oneshot::Receiver<()>>>,
    write_failure: bool,
}

#[crate::platform::async_trait]
impl CoreStorage for CleanupStorage {
    async fn read_core_storage(
        &self,
        key: CoreStorageKey,
    ) -> Result<Option<Vec<u8>>, GenericError> {
        Ok(self.values.lock().unwrap().get(&key.encode()).cloned())
    }

    async fn write_core_storage(
        &self,
        key: CoreStorageKey,
        value: Vec<u8>,
    ) -> Result<(), GenericError> {
        self.values.lock().unwrap().insert(key.encode(), value);
        let pause = self.write_pause.lock().unwrap().take();
        if let Some(pause) = pause {
            let _ = pause.await;
        }
        if self.write_failure {
            return Err(GenericError {
                reason: "session write failed".to_string(),
            });
        }
        Ok(())
    }

    async fn clear_core_storage(&self, key: CoreStorageKey) -> Result<(), GenericError> {
        let pause = {
            let mut pause = self.pause.lock().unwrap();
            if pause
                .as_ref()
                .is_some_and(|(paused, _)| *paused == key.encode())
            {
                pause.take().map(|(_, receiver)| receiver)
            } else {
                None
            }
        };
        if let Some(pause) = pause {
            let _ = pause.await;
        }
        if self.failure.lock().unwrap().as_ref() == Some(&key.encode()) {
            return Err(GenericError {
                reason: "storage deletion failed".to_string(),
            });
        }
        self.values.lock().unwrap().remove(&key.encode());
        Ok(())
    }
}

#[crate::platform::async_trait]
impl SecretCoreStorage for CleanupStorage {
    async fn read_secret_core_storage(
        &self,
        key: SecretCoreStorageKey,
    ) -> Result<Option<Vec<u8>>, GenericError> {
        Ok(self
            .values
            .lock()
            .unwrap()
            .get(&key.storage_key().into_bytes())
            .cloned())
    }

    async fn write_secret_core_storage(
        &self,
        key: SecretCoreStorageKey,
        value: Vec<u8>,
    ) -> Result<(), GenericError> {
        self.values
            .lock()
            .unwrap()
            .insert(key.storage_key().into_bytes(), value);
        let pause = self.write_pause.lock().unwrap().take();
        if let Some(pause) = pause {
            let _ = pause.await;
        }
        if self.write_failure {
            return Err(GenericError {
                reason: "session write failed".to_string(),
            });
        }
        Ok(())
    }

    async fn clear_secret_core_storage(
        &self,
        key: SecretCoreStorageKey,
    ) -> Result<(), GenericError> {
        let pause = {
            let mut pause = self.pause.lock().unwrap();
            if pause
                .as_ref()
                .is_some_and(|(paused, _)| *paused == key.storage_key().into_bytes())
            {
                pause.take().map(|(_, receiver)| receiver)
            } else {
                None
            }
        };
        if let Some(pause) = pause {
            let _ = pause.await;
        }
        if self.failure.lock().unwrap().as_ref() == Some(&key.storage_key().into_bytes()) {
            return Err(GenericError {
                reason: "storage deletion failed".to_string(),
            });
        }
        self.values
            .lock()
            .unwrap()
            .remove(&key.storage_key().into_bytes());
        Ok(())
    }
}

#[test]
fn corrupt_signing_grants_are_not_overwritten_by_another_grant() {
    let platform = Arc::new(StubPlatform::default());
    let (_, _, host) =
        ProductRuntimeHost::new_compat_with_pairing(platform.clone(), test_spawner());
    let session = sso_session_info();
    block_on(host.set_connected_session_for_tests(session.clone()));
    let root =
        crate::host_logic::product_account::derive_root_keypair_from_entropy(&[0xAB; 16]).unwrap();
    let subtree =
        crate::host_logic::product_account::derive_product_subtree_keypair(&root, "myapp.dot")
            .unwrap();
    block_on(platform.write_secret_core_storage(SecretCoreStorageKey::AutoSigningKeys, vec![0xff]))
        .unwrap();

    let revision = host.grants.lifecycle().revision();
    let result = block_on(host.grants.remember_auto_signing_key(
        &host.session_state(),
        &session,
        revision,
        "myapp.dot",
        subtree.public.to_bytes(),
        AutoSigningKey::from_parts(subtree.secret.to_bytes(), [0x42; 32]),
    ));
    assert_eq!(
        (
            result,
            block_on(platform.read_secret_core_storage(SecretCoreStorageKey::AutoSigningKeys))
                .unwrap()
        ),
        (
            Err(crate::runtime::AuthorityError::Unavailable {
                reason: "persisted AutoSigning capabilities are invalid".to_string()
            }),
            Some(vec![0xff])
        ),
    );
}

#[test]
fn replacement_waits_for_old_session_cleanup() {
    let storage = Arc::new(CleanupStorage::default());
    let platform = Arc::new(StubPlatform {
        core_storage_override: Some(storage.clone()),
        secret_core_storage_override: Some(storage.clone()),
        ..Default::default()
    });
    let (_, _, host) =
        ProductRuntimeHost::new_compat_with_pairing(platform.clone(), test_spawner());
    let session = sso_session_info();
    block_on(host.set_connected_session_for_tests(session.clone()));
    let root =
        crate::host_logic::product_account::derive_root_keypair_from_entropy(&[0xAB; 16]).unwrap();
    let subtree =
        crate::host_logic::product_account::derive_product_subtree_keypair(&root, "myapp.dot")
            .unwrap();
    let revision = host.grants.lifecycle().revision();
    block_on(host.grants.remember_auto_signing_key(
        &host.session_state(),
        &session,
        revision,
        "myapp.dot",
        subtree.public.to_bytes(),
        AutoSigningKey::from_parts(subtree.secret.to_bytes(), [0x42; 32]),
    ))
    .unwrap();
    let blob = encode_persisted_session(&session);
    block_on(storage.write_secret_core_storage(SecretCoreStorageKey::AuthSession, blob.clone()))
        .unwrap();
    let (release, pause) = oneshot::channel();
    *storage.pause.lock().unwrap() = Some((
        SecretCoreStorageKey::AuthSession.storage_key().into_bytes(),
        pause,
    ));

    let mut cleanup = Box::pin(host.clear_disconnected_session(true, None));
    assert!(cleanup.as_mut().now_or_never().is_none());
    assert_eq!(host.session_state.current(), None);
    let mut activation = Box::pin(host.activate_external_session(&blob));
    assert!(
        activation.as_mut().now_or_never().is_none(),
        "replacement must wait for durable revocation"
    );

    release.send(()).unwrap();
    block_on(cleanup);
    block_on(activation).unwrap();
    assert!(
        !block_on(host.grants.auto_signing_key(&session, "myapp.dot"))
            .unwrap()
            .is_some()
    );
    let revision = host.grants.lifecycle().revision();
    block_on(host.grants.remember_auto_signing_key(
        &host.session_state(),
        &session,
        revision,
        "myapp.dot",
        subtree.public.to_bytes(),
        AutoSigningKey::from_parts(subtree.secret.to_bytes(), [0x55; 32]),
    ))
    .unwrap();
    let expected = vec![(
        (
            session.public_key,
            session.sso.as_ref().map(|sso| sso.identity_account_id),
        ),
        "myapp.dot".to_string(),
        subtree.public.to_bytes(),
        subtree.secret.to_bytes(),
        [0x55_u8; 32],
    )]
    .encode();
    assert_eq!(
        (
            host.session_state.current(),
            storage.values.lock().unwrap().clone(),
            platform.auth_states.lock().unwrap().clone()
        ),
        (
            Some(session.clone()),
            BTreeMap::from([(
                SecretCoreStorageKey::AutoSigningKeys
                    .storage_key()
                    .into_bytes(),
                expected
            )]),
            vec![
                crate::platform::AuthState::Connected(connected_session_ui_info(&session)),
                crate::platform::AuthState::Disconnected,
                crate::platform::AuthState::Connected(connected_session_ui_info(&session)),
            ]
        ),
    );
    let key = block_on(host.grants.auto_signing_key(&session, "myapp.dot"))
        .unwrap()
        .unwrap();
    assert_eq!(
        (*key.as_secret_bytes(), *key.ring_vrf_domain_entropy()),
        (subtree.secret.to_bytes(), [0x55; 32])
    );
}

#[test]
fn interrupted_cleanup_retains_its_scope_and_later_auth_deletion() {
    for cancel in [false, true] {
        let storage = Arc::new(CleanupStorage::default());
        let platform = Arc::new(StubPlatform {
            core_storage_override: Some(storage.clone()),
            secret_core_storage_override: Some(storage.clone()),
            ..Default::default()
        });
        let (_, _, host) = ProductRuntimeHost::new_compat_with_pairing(platform, test_spawner());
        let session = sso_session_info();
        block_on(host.set_connected_session_for_tests(session.clone()));
        let blob = encode_persisted_session(&session);
        let session_id = allowances::session_storage_id(session.sso.as_ref().unwrap());
        for key in [
            SecretCoreStorageKey::AuthSession,
            SecretCoreStorageKey::AutoSigningKeys,
            SecretCoreStorageKey::AllowanceKeys {
                session_id: session_id.clone(),
            },
        ] {
            block_on(storage.write_secret_core_storage(key, blob.clone())).unwrap();
        }
        let cache_key = (
            allowances::GrantScope::from_session(&session),
            "myapp.dot".to_string(),
        );
        let revision = host.grants.lifecycle().revision();
        assert!(block_on(host.grants.persist_product_subtree_if_current(
            &host.session_state(),
            &session,
            revision,
            cache_key,
            [0xAB; 32],
        )));
        let (release, pause) = oneshot::channel();
        *storage.pause.lock().unwrap() = Some((
            SecretCoreStorageKey::AutoSigningKeys
                .storage_key()
                .into_bytes(),
            pause,
        ));
        *storage.failure.lock().unwrap() = Some(
            SecretCoreStorageKey::AutoSigningKeys
                .storage_key()
                .into_bytes(),
        );
        let mut initial = Box::pin(host.clear_disconnected_session(false, None));
        assert!(initial.as_mut().now_or_never().is_none());
        let mut repeated = Box::pin(host.clear_disconnected_session(true, None));
        assert!(repeated.as_mut().now_or_never().is_none());
        if cancel {
            drop(initial);
        } else {
            release.send(()).unwrap();
            block_on(initial);
        }
        block_on(repeated);
        assert_eq!(
            storage.values.lock().unwrap().clone(),
            BTreeMap::from([(
                SecretCoreStorageKey::AutoSigningKeys
                    .storage_key()
                    .into_bytes(),
                blob.clone()
            )])
        );
        assert_eq!(
            block_on(host.activate_external_session(&blob)),
            Err("storage deletion failed".to_string())
        );
        assert_eq!(host.session_state.current(), None);

        *storage.failure.lock().unwrap() = None;
        block_on(host.activate_external_session(&blob)).unwrap();
        assert_eq!(
            (
                host.session_state.current(),
                storage.values.lock().unwrap().clone()
            ),
            (Some(session), BTreeMap::new())
        );
    }
}

#[test]
fn superseded_login_cannot_leave_its_session_in_storage() {
    for outcome in ["success", "cancelled", "failed"] {
        let storage = Arc::new(CleanupStorage {
            write_failure: outcome == "failed",
            ..Default::default()
        });
        let platform = Arc::new(StubPlatform {
            core_storage_override: Some(storage.clone()),
            secret_core_storage_override: Some(storage.clone()),
            ..Default::default()
        });
        let (_, _, host) = ProductRuntimeHost::new_compat_with_pairing(platform, test_spawner());
        let session = sso_session_info();
        let generation = host.begin_login_attempt();
        let (release, pause) = oneshot::channel();
        *storage.write_pause.lock().unwrap() = Some(pause);
        let mut login = Box::pin(host.commit_login_session(&session, generation));
        assert!(login.as_mut().now_or_never().is_none());
        assert_eq!(
            storage.values.lock().unwrap().clone(),
            BTreeMap::from([(
                SecretCoreStorageKey::AuthSession.storage_key().into_bytes(),
                encode_persisted_session(&session)
            )]),
        );

        let mut replacement = session.clone();
        replacement.public_key = [0x56; 32];
        let replacement_blob = encode_persisted_session(&replacement);
        let mut activation = Box::pin(host.activate_external_session(&replacement_blob));
        assert!(activation.as_mut().now_or_never().is_none());
        if outcome == "cancelled" {
            drop(login);
        } else {
            release.send(()).unwrap();
            let expected = if outcome == "failed" {
                Err(GenericError {
                    reason: "session write failed".to_string(),
                })
            } else {
                Ok(false)
            };
            assert_eq!(block_on(login), expected);
        }
        block_on(activation).unwrap();
        assert_eq!(
            (
                host.session_state.current(),
                storage.values.lock().unwrap().clone()
            ),
            (Some(replacement), BTreeMap::new()),
        );
    }
}

#[test]
fn login_store_notifications_follow_the_persisted_value() {
    for changed in [false, true] {
        let storage = Arc::new(CleanupStorage::default());
        let platform = Arc::new(StubPlatform {
            core_storage_override: Some(storage.clone()),
            secret_core_storage_override: Some(storage.clone()),
            ..Default::default()
        });
        let (_, _, host) = ProductRuntimeHost::new_compat_with_pairing(platform, test_spawner());
        let session = sso_session_info();
        let generation = host.begin_login_attempt();
        let (release, pause) = oneshot::channel();
        *storage.write_pause.lock().unwrap() = Some(pause);
        let mut login = Box::pin(host.commit_login_session(&session, generation));
        assert!(login.as_mut().now_or_never().is_none());
        let mut stored = session.clone();
        if changed {
            stored.public_key = [0x56; 32];
            block_on(storage.write_secret_core_storage(
                SecretCoreStorageKey::AuthSession,
                encode_persisted_session(&stored),
            ))
            .unwrap();
        }
        host.notify_session_store_changed();
        release.send(()).unwrap();
        assert_eq!(
            (
                block_on(login),
                host.session_state.current(),
                storage.values.lock().unwrap().clone(),
            ),
            (
                Ok(!changed),
                (!changed).then_some(session),
                BTreeMap::from([(
                    SecretCoreStorageKey::AuthSession.storage_key().into_bytes(),
                    encode_persisted_session(&stored)
                )]),
            ),
        );
    }
}
