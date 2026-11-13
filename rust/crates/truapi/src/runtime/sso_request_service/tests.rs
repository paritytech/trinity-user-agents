use super::*;
use crate::{
	platform::{AuthState, CoreStorage, ProductId, StackStorageKey},
	runtime::{ProductAuthority, ProductRuntimeHost, allowances},
	test_support::{StubPlatform, paseo_test_stack, sso_session_info, test_storage_key},
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
		key: StackStorageKey,
	) -> Result<Option<Vec<u8>>, GenericError> {
		Ok(self.values.lock().unwrap().get(&key.encode()).cloned())
	}

	async fn write_core_storage(
		&self,
		key: StackStorageKey,
		value: Vec<u8>,
	) -> Result<(), GenericError> {
		self.values.lock().unwrap().insert(key.encode(), value);
		let pause = self.write_pause.lock().unwrap().take();
		if let Some(pause) = pause {
			let _ = pause.await;
		}
		Ok(())
	}

	async fn clear_core_storage(&self, key: StackStorageKey) -> Result<(), GenericError> {
		let pause = {
			let mut pause = self.pause.lock().unwrap();
			if pause.as_ref().is_some_and(|(paused, _)| *paused == key.slot) {
				pause.take().map(|(_, receiver)| receiver)
			} else {
				None
			}
		};
		if let Some(pause) = pause {
			let _ = pause.await;
		}
		if self.failure.lock().unwrap().as_ref() == Some(&key.slot) {
			return Err(GenericError { reason: "storage deletion failed".to_string() });
		}
		self.values.lock().unwrap().remove(&key.encode());
		Ok(())
	}
}

/// The record of the chains' run, which the first installed session leaves
/// and no session change removes.
fn run_record() -> (Vec<u8>, Vec<u8>) {
	(test_storage_key(CoreStorageKey::ChainRun).encode(), paseo_test_stack().run().encode())
}

#[test]
fn interrupted_cleanup_retains_its_scope_and_later_auth_deletion() {
	for cancel in [false, true] {
		let storage = Arc::new(CleanupStorage::default());
		let platform = Arc::new(StubPlatform {
			core_storage_override: Some(storage.clone()),
			..Default::default()
		});
		let (_, pairing) =
			ProductRuntimeHost::new_compat_with_pairing(platform, Spawner::for_test());
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
			block_on(storage.write_core_storage(test_storage_key(key), blob.clone())).unwrap();
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
			BTreeMap::from([
				(test_storage_key(CoreStorageKey::AutoSigningKeys).encode(), blob.clone()),
				run_record(),
			])
		);
		block_on(host.activate_external_session(&blob)).unwrap();
		assert_eq!(
			(host.session_state.current(), storage.values.lock().unwrap().clone()),
			(
				Some(session.clone()),
				BTreeMap::from([
					(test_storage_key(CoreStorageKey::AutoSigningKeys).encode(), blob.clone()),
					run_record(),
				])
			)
		);

		*storage.failure.lock().unwrap() = None;
		block_on(host.activate_external_session(&blob)).unwrap();
		assert_eq!(
			(host.session_state.current(), storage.values.lock().unwrap().clone()),
			(Some(session), BTreeMap::from([run_record()]))
		);
	}
}

#[test]
fn clearing_the_old_session_during_login_keeps_the_new_one_connected() {
	let storage = Arc::new(CleanupStorage::default());
	let platform = Arc::new(StubPlatform {
		core_storage_override: Some(storage.clone()),
		..Default::default()
	});
	let states = Arc::new(Mutex::new(Vec::new()));
	let recorded = states.clone();
	*platform.on_auth_state.lock().unwrap() =
		Some(Arc::new(move |state: &AuthState| recorded.lock().unwrap().push(state.clone())));
	let (_, pairing) = ProductRuntimeHost::new_compat_with_pairing(platform, Spawner::for_test());
	let host = pairing.session().clone();
	let previous = sso_session_info();
	block_on(host.set_connected_session_for_tests(previous.clone()));
	let mut next = previous;
	next.sso.as_mut().unwrap().identity_account_id = [0x55; 32];
	let (release, pause) = oneshot::channel();
	*storage.pause.lock().unwrap() = Some((CoreStorageKey::AutoSigningKeys, pause));
	let generation = host.begin_login_attempt();
	let mut login = Box::pin(host.commit_login_session(&next, generation));
	assert!(login.as_mut().now_or_never().is_none());
	let mut clear = Box::pin(host.clear_disconnected_session(true, None));
	assert!(clear.as_mut().now_or_never().is_none());
	release.send(()).unwrap();
	assert_eq!(block_on(login), Ok(true));
	block_on(clear);
	assert_eq!(
		(host.session_state.current(), states.lock().unwrap().last().cloned()),
		(
			Some(next.clone()),
			Some(AuthState::Connected(Box::new(connected_session_ui_info(&next))))
		)
	);
}

#[test]
fn login_store_notifications_follow_the_persisted_value() {
	for changed in [false, true] {
		let storage = Arc::new(CleanupStorage::default());
		let platform = Arc::new(StubPlatform {
			core_storage_override: Some(storage.clone()),
			..Default::default()
		});
		let (_, pairing) =
			ProductRuntimeHost::new_compat_with_pairing(platform, Spawner::for_test());
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
				test_storage_key(CoreStorageKey::AuthSession),
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
				// A login that is not installed leaves the run unrecorded.
				BTreeMap::from_iter(
					[(
						test_storage_key(CoreStorageKey::AuthSession).encode(),
						encode_persisted_session(&stored),
					)]
					.into_iter()
					.chain((!changed).then(run_record)),
				),
			),
		);
	}
}
