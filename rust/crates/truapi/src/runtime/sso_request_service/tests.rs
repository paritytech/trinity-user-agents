use super::*;
use crate::{
	platform::{AuthState, CoreStorage, ProductId},
	runtime::{HostRole, ProductRuntimeHost, allowances},
	test_support::{
		DOTNS_NAMED_ACCOUNT, StubPlatform, dotns_usernames_frames, legacy_asset_hub_frames,
		paseo_test_stack, sso_session_info, unnamed_sso_session_info,
	},
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
	read_pause: Mutex<Option<ReadPause>>,
}

/// A read of `key` that reports it has started and answers once resumed.
struct ReadPause {
	key: CoreStorageKey,
	started: oneshot::Sender<()>,
	resume: oneshot::Receiver<()>,
}

impl CleanupStorage {
	/// Holds the next read of `key` until the returned sender fires, and
	/// reports through the receiver that the read has started.
	fn pause_read(&self, key: CoreStorageKey) -> (oneshot::Receiver<()>, oneshot::Sender<()>) {
		let (started, has_started) = oneshot::channel();
		let (resume, resumed) = oneshot::channel();
		*self.read_pause.lock().unwrap() = Some(ReadPause { key, started, resume: resumed });
		(has_started, resume)
	}
}

#[async_trait::async_trait]
impl CoreStorage for CleanupStorage {
	async fn read_core_storage(
		&self,
		key: CoreStorageKey,
	) -> Result<Option<Vec<u8>>, GenericError> {
		let pause = self.read_pause.lock().unwrap().take_if(|pause| pause.key == key);
		if let Some(pause) = pause {
			let _ = pause.started.send(());
			let _ = pause.resume.await;
		}
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
				// A login that is not installed records no run for the
				// session another writer stored.
				BTreeMap::from_iter(
					[(CoreStorageKey::AuthSession.encode(), encode_persisted_session(&stored))]
						.into_iter()
						.chain((!changed).then(|| {
							(
								CoreStorageKey::SessionUsernamesRun.encode(),
								usernames_record(
									&paseo_test_stack().run(),
									&encode_persisted_session(&stored),
								),
							)
						})),
				),
			),
		);
	}
}

/// A device that is offline cannot tell a username from a missing one. The
/// lookup after a session is installed must then leave the session, the
/// stored blob and the record as they were, so the username is not erased
/// and the next start looks again.
#[test]
fn a_failed_username_lookup_changes_nothing() {
	let storage = Arc::new(CleanupStorage::default());
	let session = sso_session_info();
	let host = host_with_session(&storage, offline(), &session);
	let blob = encode_persisted_session(&session);

	block_on(host.look_up_usernames(session.clone(), Some(blob.clone())));

	let read = |key| block_on(storage.read_core_storage(key)).unwrap();
	assert_eq!(
		(
			host.session_state.current(),
			read(CoreStorageKey::AuthSession),
			read(CoreStorageKey::SessionUsernamesRun),
		),
		(Some(session), Some(blob), None)
	);
}

/// A platform whose chains do not connect, so no username lookup answers.
fn offline() -> StubPlatform {
	StubPlatform { chain_connect_error: Some("offline"), ..Default::default() }
}

/// A pairing host on `chains` that keeps core storage in `storage`, with
/// `session` installed and stored in the `AuthSession` slot.
fn host_with_session(
	storage: &Arc<CleanupStorage>,
	chains: StubPlatform,
	session: &SessionInfo,
) -> Arc<SsoRequestService> {
	let platform =
		Arc::new(StubPlatform { core_storage_override: Some(storage.clone()), ..chains });
	let (_, pairing) = ProductRuntimeHost::new_compat_with_pairing(platform, Spawner::for_test());
	let host = pairing.session().clone();
	let blob = encode_persisted_session(session);
	block_on(storage.write_core_storage(CoreStorageKey::AuthSession, blob)).unwrap();
	block_on(host.set_connected_session_for_tests(session.clone()));
	host
}

/// `session` with a username a lookup found for it.
fn named(session: &SessionInfo) -> SessionInfo {
	SessionInfo { lite_username: Some("alice.01".to_string()), ..session.clone() }
}

/// An activation reads the session slot twice and takes a change between the
/// reads for another writer's session, which ends the installed session and
/// revokes its grants. The usernames a lookup found are the core's own
/// rewrite of that slot, so they wait for the activation, and the session
/// comes out of both installed, named and stored with its record.
///
/// The activation is held at its read of the usernames record, the one read
/// it makes between the two of the slot.
#[test]
fn naming_a_session_waits_for_an_activation_in_progress() {
	let storage = Arc::new(CleanupStorage::default());
	let installed = unnamed_sso_session_info();
	let host = host_with_session(&storage, offline(), &installed);
	let (blob, named) = (encode_persisted_session(&installed), named(&installed));

	let (between_the_reads, resume) = storage.pause_read(CoreStorageKey::SessionUsernamesRun);
	let activation = std::thread::spawn({
		let host = host.clone();
		move || block_on(host.activate_stored_session())
	});
	block_on(between_the_reads).expect("the activation reads the usernames record");
	let (finished, naming) = std::sync::mpsc::channel();
	std::thread::spawn({
		let (host, installed, named) = (host.clone(), installed.clone(), named.clone());
		move || {
			block_on(host.name_session(&installed, named, Some(blob)));
			let _ = finished.send(());
		}
	});
	// A rewrite that did not wait lands here, between the activation's reads.
	let _ = naming.recv_timeout(std::time::Duration::from_millis(200));
	resume.send(()).expect("the activation is still reading");
	let activated = activation.join().expect("the activation does not panic");
	let _ = naming.recv_timeout(std::time::Duration::from_secs(5));

	let named_blob = encode_persisted_session(&named);
	let read = |key| block_on(storage.read_core_storage(key)).unwrap();
	assert_eq!(
		(
			activated,
			host.session_state.current(),
			read(CoreStorageKey::AuthSession),
			read(CoreStorageKey::SessionUsernamesRun),
		),
		(
			Ok(()),
			Some(named),
			Some(named_blob.clone()),
			Some(usernames_record(&paseo_test_stack().run(), &named_blob)),
		)
	);
}

/// By the time a lookup answers, another writer may have replaced the stored
/// session. The usernames still belong to the installed session, but the slot
/// is the other writer's and is left as it is, with no record.
#[test]
fn naming_a_session_leaves_a_slot_another_writer_replaced() {
	let storage = Arc::new(CleanupStorage::default());
	let installed = unnamed_sso_session_info();
	let host = host_with_session(&storage, offline(), &installed);
	let (blob, named) = (encode_persisted_session(&installed), named(&installed));
	let replaced = b"another writer's session".to_vec();
	block_on(storage.write_core_storage(CoreStorageKey::AuthSession, replaced.clone())).unwrap();

	block_on(host.name_session(&installed, named.clone(), Some(blob)));

	let read = |key| block_on(storage.read_core_storage(key)).unwrap();
	assert_eq!(
		(
			host.session_state.current(),
			read(CoreStorageKey::AuthSession),
			read(CoreStorageKey::SessionUsernamesRun),
		),
		(Some(named), Some(replaced), None)
	);
}

/// A lookup that answers puts what it found in the session and in the stored
/// blob, and records the run for that blob, so the next start takes the
/// usernames as they are. One that finds a username names the session. One
/// that finds none, as on an Asset Hub with no dotNS contract, clears a
/// username stored from other chains.
#[test]
fn a_lookup_that_answers_names_the_session_and_the_stored_blob() {
	let unnamed = SessionInfo {
		identity_account_id: Some(DOTNS_NAMED_ACCOUNT),
		..unnamed_sso_session_info()
	};
	let stale =
		SessionInfo { lite_username: Some("stale.07".to_string()), ..unnamed_sso_session_info() };
	let no_dotns: fn(&str) -> Vec<String> = |request| {
		legacy_asset_hub_frames(request, |_| None, |_, _| panic!("no contract is deployed"))
	};
	let looked_up = |asset_hub: fn(&str) -> Vec<String>, installed: SessionInfo| {
		let storage = Arc::new(CleanupStorage::default());
		let chains =
			StubPlatform { rpc_responder: Some(Arc::new(asset_hub)), ..Default::default() };
		let host = host_with_session(&storage, chains, &installed);
		let blob = encode_persisted_session(&installed);

		block_on(host.look_up_usernames(installed, Some(blob)));

		let session = host.session_state.current().expect("the session stays installed");
		let read = |key| block_on(storage.read_core_storage(key)).unwrap();
		let stored = read(CoreStorageKey::AuthSession).expect("the session stays stored");
		(
			(session.lite_username.clone(), session.full_username.clone()),
			stored == encode_persisted_session(&session),
			read(CoreStorageKey::SessionUsernamesRun) ==
				Some(usernames_record(&paseo_test_stack().run(), &stored)),
		)
	};

	assert_eq!(
		[looked_up(dotns_usernames_frames, unnamed), looked_up(no_dotns, stale)],
		[
			((Some("alice.01".to_string()), Some("myproject".to_string())), true, true),
			((None, None), true, true),
		]
	);
}
