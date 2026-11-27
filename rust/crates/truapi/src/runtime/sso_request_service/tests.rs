use super::*;
use crate::{
	host_logic::session_times::{
		KEY_ROTATION_INTERVAL_SECS, SESSION_LIFETIME_SECS, SessionTimes, StoredSessionTimes,
	},
	platform::{AuthState, CoreStorage, ProductId},
	runtime::{HostRole, ProductRuntimeHost, allowances},
	test_support::{StubPlatform, sso_session_info},
};
use futures::{FutureExt, executor::block_on};
use parity_scale_codec::Encode;
use std::collections::BTreeMap;
use truapi::latest::GenericError;

#[derive(Default)]
pub struct CleanupStorage {
	pub values: Mutex<BTreeMap<Vec<u8>, Vec<u8>>>,
	pause: Mutex<Option<(CoreStorageKey, oneshot::Receiver<()>)>>,
	pub read_pause: Mutex<Option<(CoreStorageKey, oneshot::Receiver<()>)>>,
	failure: Mutex<Option<CoreStorageKey>>,
	pub write_failure: Mutex<Option<CoreStorageKey>>,
	write_pause: Mutex<Option<(CoreStorageKey, oneshot::Receiver<()>)>>,
}

#[async_trait::async_trait]
impl CoreStorage for CleanupStorage {
	async fn read_core_storage(
		&self,
		key: CoreStorageKey,
	) -> Result<Option<Vec<u8>>, GenericError> {
		let value = self.values.lock().unwrap().get(&key.encode()).cloned();
		if let Some(pause) = take_pause(&self.read_pause, &key) {
			let _ = pause.await;
		}
		Ok(value)
	}

	async fn write_core_storage(
		&self,
		key: CoreStorageKey,
		value: Vec<u8>,
	) -> Result<(), GenericError> {
		if self.write_failure.lock().unwrap().as_ref() == Some(&key) {
			return Err(GenericError { reason: "storage write failed".to_string() });
		}
		let pause = take_pause(&self.write_pause, &key);
		self.values.lock().unwrap().insert(key.encode(), value);
		if let Some(pause) = pause {
			let _ = pause.await;
		}
		Ok(())
	}

	async fn clear_core_storage(&self, key: CoreStorageKey) -> Result<(), GenericError> {
		if let Some(pause) = take_pause(&self.pause, &key) {
			let _ = pause.await;
		}
		if self.failure.lock().unwrap().as_ref() == Some(&key) {
			return Err(GenericError { reason: "storage deletion failed".to_string() });
		}
		self.values.lock().unwrap().remove(&key.encode());
		Ok(())
	}
}

/// The pause armed for `key`, taken so it applies to one call.
fn take_pause(
	pause: &Mutex<Option<(CoreStorageKey, oneshot::Receiver<()>)>>,
	key: &CoreStorageKey,
) -> Option<oneshot::Receiver<()>> {
	let mut pause = pause.lock().unwrap();
	if pause.as_ref().is_some_and(|(paused, _)| paused == key) {
		pause.take().map(|(_, receiver)| receiver)
	} else {
		None
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
			block_on(storage.write_core_storage(key, blob.clone())).unwrap();
		}
		pairing
			.accounts()
			.cache_product_subtree_for_test(&session, &product_id, [0xAB; 32]);
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
		block_on(host.activate_external_session(&blob)).unwrap();
		assert_eq!(
			(host.session_state.current(), storage.values.lock().unwrap().clone()),
			(
				Some(session.clone()),
				BTreeMap::from([(CoreStorageKey::AutoSigningKeys.encode(), blob.clone())])
			)
		);

		*storage.failure.lock().unwrap() = None;
		block_on(host.activate_external_session(&blob)).unwrap();
		assert_eq!(
			(host.session_state.current(), storage.values.lock().unwrap().clone()),
			(Some(session), BTreeMap::new())
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
			Some(AuthState::Connected(Box::new(SsoRequestService::session_ui_info(&next))))
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
		let paired_at = current_unix_secs();
		let generation = host.begin_login_attempt();
		let (release, pause) = oneshot::channel();
		*storage.write_pause.lock().unwrap() = Some((CoreStorageKey::AuthSession, pause));
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
		let login = block_on(login);
		let mut values = storage.values.lock().unwrap().clone();
		let times = values.remove(&CoreStorageKey::SessionTimes.encode());
		let keys_fresh = matches!(
			SessionTimes::from_stored(times.as_deref(), session.sso.as_ref().unwrap()),
			StoredSessionTimes::Current(times)
				if !times.is_rotation_due(paired_at + KEY_ROTATION_INTERVAL_SECS)
		);
		assert_eq!(
			(login, host.session_state.current(), values, keys_fresh),
			(
				Ok(!changed),
				(!changed).then_some(session),
				BTreeMap::from([(
					CoreStorageKey::AuthSession.encode(),
					encode_persisted_session(&stored)
				)]),
				true,
			),
		);
	}
}

fn stored_session_host(
	storage: Arc<CleanupStorage>,
) -> (Arc<StubPlatform>, Arc<SsoRequestService>) {
	let platform =
		Arc::new(StubPlatform { core_storage_override: Some(storage), ..Default::default() });
	let (_, pairing) =
		ProductRuntimeHost::new_compat_with_pairing(platform.clone(), Spawner::for_test());
	(platform, pairing.session().clone())
}

/// The fixture session under a pairing of its own.
fn other_pairing() -> SessionInfo {
	let mut session = sso_session_info();
	session.sso.as_mut().unwrap().ss_public_key = [0xEE; 32];
	session
}

#[test]
fn a_stored_session_past_its_lifetime_is_cleared_with_its_grants() {
	for (age, kept) in [(SESSION_LIFETIME_SECS - 60, true), (SESSION_LIFETIME_SECS, false)] {
		let storage = Arc::new(CleanupStorage::default());
		let (platform, host) = stored_session_host(storage.clone());
		let session = sso_session_info();
		let sso = session.sso.as_ref().unwrap();
		let times = SessionTimes::paired(sso, current_unix_secs() - age).to_stored();
		let stored = BTreeMap::from([
			(CoreStorageKey::AuthSession.encode(), encode_persisted_session(&session)),
			(CoreStorageKey::SessionTimes.encode(), times),
			(CoreStorageKey::AutoSigningKeys.encode(), Vec::<()>::new().encode()),
			(
				CoreStorageKey::AllowanceKeys { session_id: allowances::session_storage_id(sso) }
					.encode(),
				vec![0x02],
			),
		]);
		*storage.values.lock().unwrap() = stored.clone();

		let restored = block_on(host.activate_stored_session());

		let expected = if kept {
			(
				Ok(()),
				Some(session.clone()),
				stored,
				Some(AuthState::Connected(Box::new(SsoRequestService::session_ui_info(&session)))),
			)
		} else {
			(
				Err("stored auth session expired".to_string()),
				None,
				BTreeMap::new(),
				Some(AuthState::Disconnected),
			)
		};
		assert_eq!(
			(
				restored,
				host.session_state.current(),
				storage.values.lock().unwrap().clone(),
				platform.auth_states.lock().unwrap().last().cloned(),
			),
			expected
		);
	}
}

#[test]
fn a_session_stored_before_times_were_kept_stays_signed_in_and_is_due_for_a_key_change() {
	let storage = Arc::new(CleanupStorage::default());
	let (_, host) = stored_session_host(storage.clone());
	let session = sso_session_info();
	let sso = session.sso.as_ref().unwrap();
	let blob = encode_persisted_session(&session);
	storage
		.values
		.lock()
		.unwrap()
		.insert(CoreStorageKey::AuthSession.encode(), blob.clone());
	let first_seen = current_unix_secs();

	block_on(host.activate_stored_session()).unwrap();

	let values = storage.values.lock().unwrap().clone();
	let StoredSessionTimes::Current(times) = SessionTimes::from_stored(
		values.get(&CoreStorageKey::SessionTimes.encode()).map(Vec::as_slice),
		sso,
	) else {
		panic!("restoring a session without times records them");
	};
	assert_eq!(
		(
			host.session_state.current(),
			values.get(&CoreStorageKey::AuthSession.encode()),
			times.is_rotation_due(first_seen),
			times.is_expired(first_seen + SESSION_LIFETIME_SECS - 60),
		),
		(Some(session.clone()), Some(&blob), true, false),
		"the restore leaves the session blob as it was, and the lifetime starts when the session is first seen"
	);
}

#[test]
fn times_a_newer_core_wrote_are_left_alone_and_never_end_the_session() {
	let storage = Arc::new(CleanupStorage::default());
	let (_, host) = stored_session_host(storage.clone());
	let session = sso_session_info();
	let mut newer = SessionTimes::paired(
		session.sso.as_ref().unwrap(),
		current_unix_secs() - SESSION_LIFETIME_SECS,
	)
	.to_stored();
	newer[0] = 1;
	let stored = BTreeMap::from([
		(CoreStorageKey::AuthSession.encode(), encode_persisted_session(&session)),
		(CoreStorageKey::SessionTimes.encode(), newer),
	]);
	*storage.values.lock().unwrap() = stored.clone();

	let restored = block_on(host.activate_stored_session());

	assert_eq!(
		(restored, host.session_state.current(), storage.values.lock().unwrap().clone()),
		(Ok(()), Some(session), stored)
	);
}

#[test]
fn an_expired_restore_leaves_a_session_another_tab_committed_meanwhile() {
	for other_tab_wrote_times in [false, true] {
		let storage = Arc::new(CleanupStorage::default());
		let (_, host) = stored_session_host(storage.clone());
		let expired = sso_session_info();
		let committed = other_pairing();
		*storage.values.lock().unwrap() = BTreeMap::from([
			(CoreStorageKey::AuthSession.encode(), encode_persisted_session(&expired)),
			(
				CoreStorageKey::SessionTimes.encode(),
				SessionTimes::paired(
					expired.sso.as_ref().unwrap(),
					current_unix_secs() - SESSION_LIFETIME_SECS,
				)
				.to_stored(),
			),
		]);
		let (release, pause) = oneshot::channel();
		*storage.read_pause.lock().unwrap() = Some((CoreStorageKey::AuthSession, pause));
		let mut restore = Box::pin(host.activate_stored_session());
		assert!(restore.as_mut().now_or_never().is_none());
		storage
			.values
			.lock()
			.unwrap()
			.insert(CoreStorageKey::AuthSession.encode(), encode_persisted_session(&committed));
		if other_tab_wrote_times {
			storage.values.lock().unwrap().insert(
				CoreStorageKey::SessionTimes.encode(),
				SessionTimes::paired(committed.sso.as_ref().unwrap(), current_unix_secs())
					.to_stored(),
			);
		}
		let expected = storage.values.lock().unwrap().clone();
		release.send(()).unwrap();

		assert_eq!(
			(block_on(restore), storage.values.lock().unwrap().clone()),
			(Err("stored auth session changed during activation".to_string()), expected),
			"an expired restore must not delete or rewrite a session another tab committed meanwhile"
		);
	}
}

#[test]
fn a_failed_times_delete_does_not_hold_back_the_session_deletion() {
	let storage = Arc::new(CleanupStorage::default());
	let (_, host) = stored_session_host(storage.clone());
	let signed_out = sso_session_info();
	block_on(host.set_connected_session_for_tests(signed_out.clone()));
	*storage.values.lock().unwrap() = BTreeMap::from([
		(CoreStorageKey::AuthSession.encode(), encode_persisted_session(&signed_out)),
		(
			CoreStorageKey::SessionTimes.encode(),
			SessionTimes::paired(signed_out.sso.as_ref().unwrap(), current_unix_secs()).to_stored(),
		),
	]);
	*storage.failure.lock().unwrap() = Some(CoreStorageKey::SessionTimes);
	block_on(host.clear_disconnected_session(true, None));
	let committed = other_pairing();
	storage
		.values
		.lock()
		.unwrap()
		.insert(CoreStorageKey::AuthSession.encode(), encode_persisted_session(&committed));

	let restored = block_on(host.activate_stored_session());

	assert_eq!(
		(
			restored,
			host.session_state.current(),
			storage
				.values
				.lock()
				.unwrap()
				.get(&CoreStorageKey::AuthSession.encode())
				.cloned(),
		),
		(Ok(()), Some(committed.clone()), Some(encode_persisted_session(&committed))),
		"a session committed after sign-out must survive the next reconcile"
	);
}

#[test]
fn restoring_a_session_deletes_the_pairing_device_identity_left_beside_it() {
	let storage = Arc::new(CleanupStorage::default());
	let (_, host) = stored_session_host(storage.clone());
	let session = sso_session_info();
	for (key, value) in [
		(CoreStorageKey::AuthSession, encode_persisted_session(&session)),
		(CoreStorageKey::PairingDeviceIdentity, vec![0x03]),
	] {
		storage.values.lock().unwrap().insert(key.encode(), value);
	}

	block_on(host.activate_stored_session()).unwrap();

	assert!(
		!storage
			.values
			.lock()
			.unwrap()
			.contains_key(&CoreStorageKey::PairingDeviceIdentity.encode()),
		"the pairing identity is a second copy of the session keys"
	);
}
