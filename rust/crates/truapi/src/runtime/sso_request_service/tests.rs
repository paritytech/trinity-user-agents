use super::*;
use crate::{
	platform::{AuthState, CoreStorage, ProductId},
	runtime::{HostRole, ProductRuntimeHost, allowances},
	test_support::{
		DOTNS_NAMED_ACCOUNT, StubPlatform, dotns_usernames_frames, external_sso_session_info,
		legacy_asset_hub_frames, paseo_test_stack, sso_session_info, usernames_record,
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
									&paseo_test_stack(),
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
	let platform = Arc::new(StubPlatform {
		core_storage_override: Some(storage.clone()),
		chain_connect_error: Some("offline"),
		..Default::default()
	});
	let (_, pairing) = ProductRuntimeHost::new_compat_with_pairing(platform, Spawner::for_test());
	let host = pairing.session().clone();
	let session = sso_session_info();
	let blob = encode_persisted_session(&session);
	let stored = || block_on(storage.read_core_storage(CoreStorageKey::AuthSession)).unwrap();
	block_on(storage.write_core_storage(CoreStorageKey::AuthSession, blob.clone())).unwrap();
	block_on(host.set_connected_session_for_tests(session.clone()));

	block_on(host.look_up_usernames(session.clone(), Some(blob.clone())));

	let record = block_on(storage.read_core_storage(CoreStorageKey::SessionUsernamesRun)).unwrap();
	assert_eq!((host.session_state.current(), stored(), record), (Some(session), Some(blob), None));
}

/// A pairing host over `storage` whose chains do not connect, so no lookup of
/// its own answers, with no session selected.
fn offline_host(storage: &Arc<CleanupStorage>) -> Arc<SsoRequestService> {
	let platform = Arc::new(StubPlatform {
		core_storage_override: Some(storage.clone()),
		chain_connect_error: Some("offline"),
		..Default::default()
	});
	let (_, pairing) = ProductRuntimeHost::new_compat_with_pairing(platform, Spawner::for_test());
	pairing.session().clone()
}

/// An [`offline_host`] with `installed` restored from the `AuthSession` slot.
fn host_restored_from(
	storage: &Arc<CleanupStorage>,
	installed: &SessionInfo,
) -> Arc<SsoRequestService> {
	let host = offline_host(storage);
	let blob = encode_persisted_session(installed);
	block_on(storage.write_core_storage(CoreStorageKey::AuthSession, blob)).unwrap();
	block_on(host.activate_stored_session()).expect("the stored session activates");
	host
}

/// `session` with a username a lookup found for it.
fn named(session: &SessionInfo) -> SessionInfo {
	SessionInfo { lite_username: Some("alice.01".to_string()), ..session.clone() }
}

/// An activation reads the session slot twice and gives up on a change between
/// the reads. The usernames a lookup found are the core's own rewrite of that
/// slot, which nothing notifies the core of, so they wait for the activation,
/// and the session comes out of both installed, named and stored with its
/// record.
#[test]
fn naming_a_session_waits_for_an_activation_in_progress() {
	let storage = Arc::new(CleanupStorage::default());
	let installed = external_sso_session_info();
	let host = host_restored_from(&storage, &installed);
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
			Some(usernames_record(&paseo_test_stack(), &named_blob)),
		)
	);
}

/// By the time a lookup answers, another writer may have replaced the stored
/// session. The usernames still belong to the installed session, but the slot
/// is the other writer's and is left as it is, with no record.
#[test]
fn naming_a_session_leaves_a_slot_another_writer_replaced() {
	let storage = Arc::new(CleanupStorage::default());
	let installed = external_sso_session_info();
	let host = host_restored_from(&storage, &installed);
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

/// A lookup renames the selected session in place, and it stays the same
/// session. Installing it again must not end it, which would delete its
/// allowance keys and wake the SSO requests waiting on its channel. A blob
/// whose usernames the core does not vouch for, such as the host's own copy
/// with no record or a record of another blob, leaves the usernames the lookup
/// found. One it vouches for, as after another writer's lookup, brings its
/// own.
#[test]
fn installing_the_selected_session_again_keeps_it() {
	let installed = external_sso_session_info();
	let named = named(&installed);
	let renamed_elsewhere =
		SessionInfo { lite_username: Some("alice.02".to_string()), ..installed.clone() };
	let sso = installed.sso.clone().expect("the fixture has an SSO channel");
	let allowance_keys =
		CoreStorageKey::AllowanceKeys { session_id: allowances::session_storage_id(&sso) };
	let reinstalled = |reinstall: &dyn Fn(&SsoRequestService, &CleanupStorage)| {
		let storage = Arc::new(CleanupStorage::default());
		let host = host_restored_from(&storage, &installed);
		block_on(host.name_session(&installed, named.clone(), None));
		block_on(storage.write_core_storage(allowance_keys.clone(), vec![0xAA])).unwrap();
		let (_waiting, disconnected) = host.session_disconnects.subscribe(&sso);

		reinstall(&host, &storage);

		let kept = block_on(storage.read_core_storage(allowance_keys.clone())).unwrap();
		(host.session_state.current(), kept, disconnected.now_or_never())
	};
	let blob = encode_persisted_session(&installed);
	let vouched = encode_persisted_session(&renamed_elsewhere);

	assert_eq!(
		[
			reinstalled(&|host, _| block_on(host.activate_external_session(&blob)).unwrap()),
			reinstalled(&|host, storage| {
				let another_blobs = usernames_record(&paseo_test_stack(), &vouched);
				block_on(
					storage.write_core_storage(CoreStorageKey::SessionUsernamesRun, another_blobs),
				)
				.unwrap();
				block_on(host.activate_external_session(&blob)).unwrap();
			}),
			reinstalled(&|host, _| block_on(host.activate_stored_session()).unwrap()),
			reinstalled(&|host, storage| {
				let record = usernames_record(&paseo_test_stack(), &vouched);
				block_on(storage.write_core_storage(CoreStorageKey::AuthSession, vouched.clone()))
					.unwrap();
				block_on(storage.write_core_storage(CoreStorageKey::SessionUsernamesRun, record))
					.unwrap();
				block_on(host.activate_stored_session()).unwrap();
			}),
		],
		[
			(Some(named.clone()), Some(vec![0xAA]), None),
			(Some(named.clone()), Some(vec![0xAA]), None),
			(Some(named), Some(vec![0xAA]), None),
			(Some(renamed_elsewhere), Some(vec![0xAA]), None),
		]
	);
}

/// A lookup that answers puts what it found in the session and in the stored
/// blob, and records the run for that blob, so the next start takes the
/// usernames as they are. One that finds a username names the session. One
/// that finds none, as on chains where the account never registered, clears a
/// username stored from other chains.
#[test]
fn a_lookup_that_answers_names_the_session_and_the_stored_blob() {
	let unnamed = SessionInfo {
		identity_account_id: Some(DOTNS_NAMED_ACCOUNT),
		..external_sso_session_info()
	};
	let stale =
		SessionInfo { lite_username: Some("stale.07".to_string()), ..external_sso_session_info() };
	let no_dotns: fn(&str) -> Vec<String> = |request| {
		legacy_asset_hub_frames(request, |_| None, |_, _| panic!("no contract is deployed"))
	};
	let looked_up = |asset_hub: fn(&str) -> Vec<String>, installed: SessionInfo| {
		let storage = Arc::new(CleanupStorage::default());
		let platform = Arc::new(StubPlatform {
			core_storage_override: Some(storage.clone()),
			rpc_responder: Some(Arc::new(asset_hub)),
			..Default::default()
		});
		let (_, pairing) =
			ProductRuntimeHost::new_compat_with_pairing(platform, Spawner::for_test());
		let host = pairing.session().clone();
		let blob = encode_persisted_session(&installed);
		block_on(storage.write_core_storage(CoreStorageKey::AuthSession, blob.clone())).unwrap();
		block_on(host.set_connected_session_for_tests(installed.clone()));

		block_on(host.look_up_usernames(installed, Some(blob)));

		let session = host.session_state.current().expect("the session stays installed");
		let read = |key| block_on(storage.read_core_storage(key)).unwrap();
		let stored = read(CoreStorageKey::AuthSession).expect("the session stays stored");
		(
			(session.lite_username.clone(), session.full_username.clone()),
			stored == encode_persisted_session(&session),
			read(CoreStorageKey::SessionUsernamesRun) ==
				Some(usernames_record(&paseo_test_stack(), &stored)),
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

/// The result of `thread`, which has to finish within `wait_until`'s deadline,
/// so a regression fails the test instead of hanging it.
fn finished<T>(thread: std::thread::JoinHandle<T>, what: &str) -> T {
	crate::test_support::wait_until(|| thread.is_finished(), what);
	thread.join().expect("the thread does not panic")
}

/// While an activation is between its two reads of the slot, another origin
/// sharing it can write it. A write that leaves the selected session there,
/// under the usernames that origin's lookup found, is not another session:
/// the activation ends nothing, whichever blob it read first, and the session
/// keeps its keys. Another account, an emptied slot or one that no longer
/// reads, as after a sign-in or sign-out elsewhere, still ends the selected
/// session. With none selected, as at start, nothing is ended, so the stored
/// auto-signing keys stay for the session the change notification installs.
#[test]
fn a_slot_written_during_an_activation_ends_the_selected_session_only_for_another_one() {
	let installed = external_sso_session_info();
	let other_account = SessionInfo { public_key: [0x56; 32], ..installed.clone() };
	let sso = installed.sso.clone().expect("the fixture has an SSO channel");
	let allowance_keys =
		CoreStorageKey::AllowanceKeys { session_id: allowances::session_storage_id(&sso) };
	let blob = |session: &SessionInfo| Some(encode_persisted_session(session));
	let raced = |selected: Option<&SessionInfo>, first: &SessionInfo, then: Option<Vec<u8>>| {
		let storage = Arc::new(CleanupStorage::default());
		let host = match selected {
			Some(selected) => host_restored_from(&storage, selected),
			None => offline_host(&storage),
		};
		let write = |key, value| block_on(storage.write_core_storage(key, value)).unwrap();
		write(allowance_keys.clone(), vec![0xAA]);
		write(CoreStorageKey::AutoSigningKeys, vec![0xBB]);
		write(CoreStorageKey::AuthSession, encode_persisted_session(first));

		let (between_the_reads, resume) = storage.pause_read(CoreStorageKey::SessionUsernamesRun);
		let activation = std::thread::spawn({
			let host = host.clone();
			move || block_on(host.activate_stored_session())
		});
		let reading = std::thread::spawn(move || block_on(between_the_reads));
		finished(reading, "the activation reads the usernames record")
			.expect("the activation is paused");
		match then {
			Some(value) => write(CoreStorageKey::AuthSession, value),
			None => block_on(storage.clear_core_storage(CoreStorageKey::AuthSession)).unwrap(),
		}
		resume.send(()).expect("the activation is still reading");
		let activated = finished(activation, "the activation ends");

		let kept = |key| block_on(storage.read_core_storage(key)).unwrap().is_some();
		(
			activated.is_err(),
			host.session_state.current(),
			kept(allowance_keys.clone()),
			kept(CoreStorageKey::AutoSigningKeys),
		)
	};
	let selected = Some(&installed);

	assert_eq!(
		[
			raced(selected, &installed, blob(&named(&installed))),
			raced(selected, &other_account, blob(&named(&installed))),
			raced(selected, &other_account, blob(&named(&other_account))),
			raced(selected, &installed, None),
			raced(selected, &installed, Some(b"no longer a session".to_vec())),
			raced(None, &installed, blob(&other_account)),
		],
		[
			(true, Some(installed.clone()), true, true),
			(true, Some(installed), true, true),
			(true, None, false, false),
			(true, None, false, false),
			(true, None, false, false),
			(true, None, true, true),
		]
	);
}

/// An external activation of the session, or a username request over a slot
/// that does not hold the session's own encoding, names the session without
/// storing it, and the lookup started when the session was installed, which
/// knows the blob in the slot, can finish after it. In either order the slot
/// is rewritten with the usernames and recorded.
#[test]
fn a_lookup_that_finds_its_session_already_named_still_stores_it() {
	let stored_after = |knows_the_slot_in_turn: [bool; 2]| {
		let storage = Arc::new(CleanupStorage::default());
		let installed = external_sso_session_info();
		let host = host_restored_from(&storage, &installed);
		let (blob, named) = (encode_persisted_session(&installed), named(&installed));
		for knows_the_slot in knows_the_slot_in_turn {
			let stored = knows_the_slot.then(|| blob.clone());
			block_on(host.name_session(&installed, named.clone(), stored));
		}
		let read = |key| block_on(storage.read_core_storage(key)).unwrap();
		let named_blob = encode_persisted_session(&named);
		(
			host.session_state.current() == Some(named),
			read(CoreStorageKey::AuthSession) == Some(named_blob.clone()),
			read(CoreStorageKey::SessionUsernamesRun) ==
				Some(usernames_record(&paseo_test_stack(), &named_blob)),
		)
	};

	assert_eq!([stored_after([false, true]), stored_after([true, false])], [(true, true, true); 2]);
}

/// A product asking for the username of a session that has none makes the
/// core look it up. The usernames go into the session, but an external
/// session is the host's to store, so the `AuthSession` slot stays empty.
#[test]
fn a_username_refresh_does_not_store_an_external_session() {
	let storage = Arc::new(CleanupStorage::default());
	let platform = Arc::new(StubPlatform {
		core_storage_override: Some(storage.clone()),
		rpc_responder: Some(Arc::new(dotns_usernames_frames)),
		..Default::default()
	});
	let (_, pairing) = ProductRuntimeHost::new_compat_with_pairing(platform, Spawner::for_test());
	let host = pairing.session().clone();
	let unnamed = SessionInfo {
		identity_account_id: Some(DOTNS_NAMED_ACCOUNT),
		..external_sso_session_info()
	};
	block_on(host.set_connected_session_for_tests(unnamed));

	block_on(host.refresh_current_session_identity());

	let session = host.session_state.current().expect("the session stays selected");
	let stored = block_on(storage.read_core_storage(CoreStorageKey::AuthSession)).unwrap();
	assert_eq!(
		((session.lite_username, session.full_username), stored),
		((Some("alice.01".to_string()), Some("myproject".to_string())), None)
	);
}
