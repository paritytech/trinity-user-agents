use super::*;
use crate::{
	platform::{CoreStorage, ProductId},
	runtime::{ProductAuthority, ProductRuntimeHost, allowances},
	test_support::{StubPlatform, sso_session_info, test_spawner},
};
use futures::{FutureExt, executor::block_on};
use parity_scale_codec::Encode;
use std::collections::BTreeMap;
use truapi::latest::GenericError;

#[derive(Default)]
struct CleanupStorage {
	values: Mutex<BTreeMap<Vec<u8>, Vec<u8>>>,
	pause: Mutex<Option<(CoreStorageKey, oneshot::Receiver<()>)>>,
	failure: Mutex<Option<CoreStorageKey>>,
	write_pause: Mutex<Option<oneshot::Receiver<()>>>,
}

#[async_trait::async_trait]
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
		Ok(())
	}

	async fn clear_core_storage(&self, key: CoreStorageKey) -> Result<(), GenericError> {
		let pause = {
			let mut pause = self.pause.lock().unwrap();
			if pause.as_ref().is_some_and(|(paused, _)| *paused == key) {
				pause.take().map(|(_, receiver)| receiver)
			} else {
				None
			}
		};
		if let Some(pause) = pause {
			let _ = pause.await;
		}
		if self.failure.lock().unwrap().as_ref() == Some(&key) {
			return Err(GenericError { reason: "storage deletion failed".to_string() });
		}
		self.values.lock().unwrap().remove(&key.encode());
		Ok(())
	}
}

#[test]
fn interrupted_cleanup_retains_its_scope_and_later_auth_deletion() {
	for cancel in [false, true] {
		let storage = Arc::new(CleanupStorage::default());
		let platform = Arc::new(StubPlatform {
			core_storage_override: Some(storage.clone()),
			..Default::default()
		});
		let (_, pairing) = ProductRuntimeHost::new_compat_with_pairing(platform, test_spawner());
		let host = pairing.session().clone();
		let session = sso_session_info();
		block_on(host.set_connected_session_for_tests(session.clone()));
		let blob = encode_persisted_session(&session);
		let session_id = allowances::session_storage_id(session.sso.as_ref().unwrap());
		let product_id = ProductId::for_test("myapp.dot");
		for key in [
			CoreStorageKey::AuthSession,
			CoreStorageKey::AutoSigningKeys,
			CoreStorageKey::AllowanceKeys { session_id: session_id.clone() },
			CoreStorageKey::ProductSubtree { session_id, product_id: product_id.clone() },
		] {
			block_on(storage.write_core_storage(key, blob.clone())).unwrap();
		}
		pairing.cache_product_subtree_for_test(&session, &product_id, [0xAB; 32]);
		let (release, pause) = oneshot::channel();
		*storage.pause.lock().unwrap() = Some((CoreStorageKey::AutoSigningKeys, pause));
		*storage.failure.lock().unwrap() = Some(CoreStorageKey::AutoSigningKeys);
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
			BTreeMap::from([(CoreStorageKey::AutoSigningKeys.encode(), blob.clone())])
		);
		assert_eq!(
			block_on(host.activate_external_session(&blob)),
			Err("storage deletion failed".to_string())
		);
		assert_eq!(host.session_state.current(), None);

		*storage.failure.lock().unwrap() = None;
		block_on(host.activate_external_session(&blob)).unwrap();
		assert_eq!(
			(host.session_state.current(), storage.values.lock().unwrap().clone()),
			(Some(session), BTreeMap::new())
		);
	}
}

#[test]
fn login_store_notifications_follow_the_persisted_value() {
	for changed in [false, true] {
		let storage = Arc::new(CleanupStorage::default());
		let platform = Arc::new(StubPlatform {
			core_storage_override: Some(storage.clone()),
			..Default::default()
		});
		let (_, pairing) = ProductRuntimeHost::new_compat_with_pairing(platform, test_spawner());
		let host = pairing.session().clone();
		let session = sso_session_info();
		let generation = host.begin_login_attempt();
		let (release, pause) = oneshot::channel();
		*storage.write_pause.lock().unwrap() = Some(pause);
		let mut login = Box::pin(host.commit_login_session(&session, generation));
		assert!(login.as_mut().now_or_never().is_none());
		let mut stored = session.clone();
		if changed {
			stored.public_key = [0x56; 32];
			block_on(storage.write_core_storage(
				CoreStorageKey::AuthSession,
				encode_persisted_session(&stored),
			))
			.unwrap();
		}
		host.notify_session_store_changed();
		release.send(()).unwrap();
		assert_eq!(
			(block_on(login), host.session_state.current(), storage.values.lock().unwrap().clone(),),
			(
				Ok(!changed),
				(!changed).then_some(session),
				BTreeMap::from([(
					CoreStorageKey::AuthSession.encode(),
					encode_persisted_session(&stored)
				)]),
			),
		);
	}
}
