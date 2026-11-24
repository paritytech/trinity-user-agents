use super::*;
use crate::{
	platform::{AuthState, CoreStorage, ProductId},
	runtime::{HostRole, ProductRuntimeHost, allowances},
	test_support::{
		DOTNS_NAMED_ACCOUNT, StubPlatform, dotns_usernames_frames, dotns_usernames_responder,
		legacy_asset_hub_frames, paseo_test_stack, sso_session_info, unnamed_sso_session_info,
		wait_until,
	},
};
use futures::{FutureExt, executor::block_on, future::BoxFuture};
use parity_scale_codec::Encode;
use std::{collections::BTreeMap, sync::mpsc, time::Duration};
use truapi::latest::GenericError;

#[derive(Default)]
struct CleanupStorage {
	values: Mutex<BTreeMap<Vec<u8>, Vec<u8>>>,
	pause: Mutex<Option<(CoreStorageKey, oneshot::Receiver<()>)>>,
	failure: Mutex<Option<CoreStorageKey>>,
	read_failure: Mutex<Option<CoreStorageKey>>,
	write_failure: Mutex<Option<CoreStorageKey>>,
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
		if self.read_failure.lock().unwrap().as_ref() == Some(&key) {
			return Err(GenericError { reason: "storage read failed".to_string() });
		}
		Ok(self.values.lock().unwrap().get(&key.encode()).cloned())
	}

	async fn write_core_storage(
		&self,
		key: CoreStorageKey,
		value: Vec<u8>,
	) -> Result<(), GenericError> {
		if self.write_failure.lock().unwrap().as_ref() == Some(&key) {
			return Err(GenericError { reason: "storage write failed".to_string() });
		}
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

/// A pairing host on `chains` that keeps core storage in `storage`, with no
/// session selected.
fn pairing_host(storage: &Arc<CleanupStorage>, chains: StubPlatform) -> Arc<SsoRequestService> {
	let platform =
		Arc::new(StubPlatform { core_storage_override: Some(storage.clone()), ..chains });
	let (_, pairing) = ProductRuntimeHost::new_compat_with_pairing(platform, Spawner::for_test());
	pairing.session().clone()
}

/// A [`pairing_host`] with `session` installed and stored in the `AuthSession`
/// slot.
fn host_with_session(
	storage: &Arc<CleanupStorage>,
	chains: StubPlatform,
	session: &SessionInfo,
) -> Arc<SsoRequestService> {
	let host = pairing_host(storage, chains);
	let blob = encode_persisted_session(session);
	block_on(storage.write_core_storage(CoreStorageKey::AuthSession, blob)).unwrap();
	block_on(host.set_connected_session_for_tests(session.clone()));
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
	let reading = std::thread::spawn(move || block_on(between_the_reads));
	finished(reading, "the activation reads the usernames record").expect("it is paused");
	let (named_it, naming) = mpsc::channel();
	let rename = std::thread::spawn({
		let (host, installed, named) = (host.clone(), installed.clone(), named.clone());
		move || {
			block_on(host.name_session(&installed, named, Some(blob)));
			let _ = named_it.send(());
		}
	});
	// A rewrite that did not wait lands here, between the activation's reads.
	let _ = naming.recv_timeout(Duration::from_millis(200));
	resume.send(()).expect("the activation is still reading");
	let activated = finished(activation, "the activation ends");
	finished(rename, "the rename ends");

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

/// After a wipe the stored blob still carries the usernames read from the old
/// chains, and the session comes up without them. When a product then asks who
/// the user is and the lookup answers, what it found is stored over that blob
/// with its run, so the next start has the username whether or not it is
/// online.
#[test]
fn usernames_found_for_a_product_replace_the_blob_from_before_a_wipe() {
	let storage = Arc::new(CleanupStorage::default());
	let installed = SessionInfo {
		identity_account_id: Some(DOTNS_NAMED_ACCOUNT),
		..unnamed_sso_session_info()
	};
	let chains = StubPlatform {
		rpc_responder: Some(Arc::new(dotns_usernames_frames)),
		..Default::default()
	};
	let host = host_with_session(&storage, chains, &installed);
	let before_the_wipe = encode_persisted_session(&SessionInfo {
		lite_username: Some("stale.07".to_string()),
		..installed
	});
	block_on(storage.write_core_storage(CoreStorageKey::AuthSession, before_the_wipe)).unwrap();

	block_on(host.refresh_current_session_identity());

	let session = host.session_state.current().expect("the session stays installed");
	let named_blob = encode_persisted_session(&session);
	let read = |key| block_on(storage.read_core_storage(key)).unwrap();
	assert_eq!(
		(
			session.lite_username,
			read(CoreStorageKey::AuthSession),
			read(CoreStorageKey::SessionUsernamesRun),
		),
		(
			Some("alice.01".to_string()),
			Some(named_blob.clone()),
			Some(usernames_record(&paseo_test_stack().run(), &named_blob)),
		)
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
	let installed = unnamed_sso_session_info();
	let named = named(&installed);
	let renamed_elsewhere =
		SessionInfo { lite_username: Some("alice.02".to_string()), ..installed.clone() };
	let sso = installed.sso.clone().expect("the fixture has an SSO channel");
	let allowance_keys =
		CoreStorageKey::AllowanceKeys { session_id: allowances::session_storage_id(&sso) };
	let reinstalled = |reinstall: &dyn Fn(&SsoRequestService, &CleanupStorage)| {
		let storage = Arc::new(CleanupStorage::default());
		let host = host_with_session(&storage, offline(), &installed);
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
				let another_blobs = usernames_record(&paseo_test_stack().run(), &vouched);
				block_on(
					storage.write_core_storage(CoreStorageKey::SessionUsernamesRun, another_blobs),
				)
				.unwrap();
				block_on(host.activate_external_session(&blob)).unwrap();
			}),
			reinstalled(&|host, _| block_on(host.activate_stored_session()).unwrap()),
			reinstalled(&|host, storage| {
				let record = usernames_record(&paseo_test_stack().run(), &vouched);
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
	let installed = unnamed_sso_session_info();
	let other_account = SessionInfo { public_key: [0x56; 32], ..installed.clone() };
	let sso = installed.sso.clone().expect("the fixture has an SSO channel");
	let allowance_keys =
		CoreStorageKey::AllowanceKeys { session_id: allowances::session_storage_id(&sso) };
	let blob = |session: &SessionInfo| Some(encode_persisted_session(session));
	let raced = |selected: Option<&SessionInfo>, first: &SessionInfo, then: Option<Vec<u8>>| {
		let storage = Arc::new(CleanupStorage::default());
		let host = match selected {
			Some(selected) => host_with_session(&storage, offline(), selected),
			None => pairing_host(&storage, offline()),
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

/// The lookup after an external activation of the session names it without
/// storing it, and the lookup started when the session was installed from the
/// slot, which knows the blob there, can finish after it. In either order the
/// slot is rewritten with the usernames and recorded.
#[test]
fn a_lookup_that_finds_its_session_already_named_still_stores_it() {
	let stored_after = |knows_the_slot_in_turn: [bool; 2]| {
		let storage = Arc::new(CleanupStorage::default());
		let installed = unnamed_sso_session_info();
		let host = host_with_session(&storage, offline(), &installed);
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
				Some(usernames_record(&paseo_test_stack().run(), &named_blob)),
		)
	};

	assert_eq!([stored_after([false, true]), stored_after([true, false])], [(true, true, true); 2]);
}

/// A product asking for the username of a session that has none makes the
/// core look it up. The usernames go into the session, but a session the host
/// installed with `activateExternalSession` is the host's to keep, so the
/// `AuthSession` slot stays empty.
#[test]
fn a_username_refresh_does_not_store_an_external_session() {
	let storage = Arc::new(CleanupStorage::default());
	let dotns = StubPlatform {
		rpc_responder: Some(Arc::new(dotns_usernames_frames)),
		..Default::default()
	};
	let host = pairing_host(&storage, dotns);
	let unnamed = SessionInfo {
		identity_account_id: Some(DOTNS_NAMED_ACCOUNT),
		..unnamed_sso_session_info()
	};
	let epoch = host.advance_session_lifecycle();
	assert!(host.install_session_if_current(unnamed, epoch, true, None), "the session installs");

	block_on(host.refresh_current_session_identity());

	let session = host.session_state.current().expect("the session stays selected");
	let stored = block_on(storage.read_core_storage(CoreStorageKey::AuthSession)).unwrap();
	assert_eq!(
		((session.lite_username, session.full_username), stored),
		((Some("alice.01".to_string()), Some("myproject".to_string())), None)
	);
}

/// A selected session with no username, whose lookup answers.
fn session_to_look_up() -> SessionInfo {
	SessionInfo { identity_account_id: Some(DOTNS_NAMED_ACCOUNT), ..unnamed_sso_session_info() }
}

/// What an activation and a product's username request leave behind when
/// [`session_to_look_up`] is selected and the slot holds `slot`, recorded as
/// read in this run when `recorded`. The request starts while the activation
/// is between its two reads of the slot.
fn refresh_while_an_activation_reads(
	slot: &SessionInfo,
	recorded: bool,
) -> (Result<(), String>, Option<SessionInfo>, Option<Vec<u8>>) {
	let storage = Arc::new(CleanupStorage::default());
	let dotns =
		StubPlatform { rpc_responder: Some(dotns_usernames_responder()), ..Default::default() };
	let host = host_with_session(&storage, dotns, &session_to_look_up());
	let blob = encode_persisted_session(slot);
	block_on(storage.write_core_storage(CoreStorageKey::AuthSession, blob.clone())).unwrap();
	if recorded {
		let record = usernames_record(&paseo_test_stack().run(), &blob);
		block_on(storage.write_core_storage(CoreStorageKey::SessionUsernamesRun, record)).unwrap();
	}

	let (between_the_reads, resume) = storage.pause_read(CoreStorageKey::SessionUsernamesRun);
	let activation = std::thread::spawn({
		let host = host.clone();
		move || block_on(host.activate_stored_session())
	});
	let reading = std::thread::spawn(move || block_on(between_the_reads));
	finished(reading, "the activation reads the usernames record").expect("it is paused");
	let (refreshed, refreshing) = std::sync::mpsc::channel();
	let refresh = std::thread::spawn({
		let host = host.clone();
		move || {
			block_on(host.refresh_current_session_identity());
			let _ = refreshed.send(());
		}
	});
	// A refresh that did not wait would land here, between the activation's reads.
	let _ = refreshing.recv_timeout(std::time::Duration::from_millis(200));
	resume.send(()).expect("the activation is still reading");
	let activated = finished(activation, "the activation ends");
	finished(refresh, "the refresh ends");

	let stored = block_on(storage.read_core_storage(CoreStorageKey::AuthSession)).unwrap();
	(activated, host.session_state.current(), stored)
}

/// A product's username request renames the selected session and stores it,
/// which is a change to the slot. Made between an activation's two reads, it
/// would make that activation reject, and nothing tells the core of its own
/// writes, so the request waits for the activation and stores after it.
#[test]
fn a_username_refresh_waits_for_an_activation_in_progress() {
	let selected = session_to_look_up();
	let named = SessionInfo {
		lite_username: Some("alice.01".to_string()),
		full_username: Some("myproject".to_string()),
		..selected.clone()
	};

	let (activated, current, stored) = refresh_while_an_activation_reads(&selected, false);

	assert_eq!(
		(activated, current, stored),
		(Ok(()), Some(named.clone()), Some(encode_persisted_session(&named)))
	);
}

/// Another origin can sign in to another account and store it while a product
/// here asks for the username of the selected session. The request neither
/// lands between the reads of the activation installing that sign-in nor
/// stores over another account's blob, so the old session does not come back
/// in the slot. The new account is installed and keeps its blob.
#[test]
fn a_username_refresh_does_not_overwrite_a_sign_in_an_activation_is_reading() {
	let signed_in = SessionInfo {
		public_key: [0x56; 32],
		identity_account_id: None,
		lite_username: Some("bob.01".to_string()),
		..unnamed_sso_session_info()
	};

	let (activated, current, stored) = refresh_while_an_activation_reads(&signed_in, true);

	assert_eq!(
		(activated, current, stored),
		(Ok(()), Some(signed_in.clone()), Some(encode_persisted_session(&signed_in)))
	);
}

/// A lookup can answer after its session was replaced, by a sign-in to
/// another account here, while the slot still holds the old blob. What it
/// found belongs to a session no longer selected, so it is not stored.
#[test]
fn a_lookup_for_a_session_no_longer_selected_stores_nothing() {
	let storage = Arc::new(CleanupStorage::default());
	let installed = unnamed_sso_session_info();
	let host = host_with_session(&storage, offline(), &installed);
	let blob = encode_persisted_session(&installed);
	let other_account = SessionInfo { public_key: [0x56; 32], ..installed.clone() };
	block_on(host.set_connected_session_for_tests(other_account.clone()));

	block_on(host.name_session(&installed, named(&installed), Some(blob.clone())));

	let read = |key| block_on(storage.read_core_storage(key)).unwrap();
	assert_eq!(
		(
			host.session_state.current(),
			read(CoreStorageKey::AuthSession),
			read(CoreStorageKey::SessionUsernamesRun),
		),
		(Some(other_account), Some(blob), None)
	);
}

/// Tasks spawned on a [`Spawner`] that runs nothing by itself, in the order
/// they were spawned, so a test polls each one when it chooses.
type HandPolled = Arc<Mutex<Vec<BoxFuture<'static, ()>>>>;

/// A spawner that queues its tasks in [`HandPolled`].
fn polled_by_hand() -> (Spawner, HandPolled) {
	let tasks = HandPolled::default();
	let queue = tasks.clone();
	(Spawner::with_test_scheduler(move |task| queue.lock().unwrap().push(task)), tasks)
}

/// `task` polled once, as an executor would: `None` once it has finished,
/// which drops it, otherwise the task to poll again.
fn poll_once(mut task: BoxFuture<'static, ()>) -> Option<BoxFuture<'static, ()>> {
	task.as_mut().now_or_never().is_none().then_some(task)
}

/// A host can drop its runtime while the username lookup of an activation
/// waits on dotNS. The service owns the lookup, so dropping the service ends
/// it, and the dotNS read it started stops with it, as a read does once its
/// last waiter has gone. Nothing else stops them: here the read has not even
/// started. The lookup does not keep the service alive meanwhile.
#[test]
fn a_lookup_stops_when_its_service_is_dropped() {
	let (spawner, tasks) = polled_by_hand();
	let storage = Arc::new(CleanupStorage::default());
	let platform = Arc::new(StubPlatform {
		core_storage_override: Some(storage.clone()),
		..Default::default()
	});
	let host = ProductRuntimeHost::new_compat_with_pairing(platform, spawner)
		.1
		.session()
		.clone();
	let installed = SessionInfo { sso: None, ..session_to_look_up() };
	let blob = encode_persisted_session(&installed);
	block_on(storage.write_core_storage(CoreStorageKey::AuthSession, blob)).unwrap();
	block_on(host.activate_stored_session()).expect("the stored session activates");
	let lookup = tasks.lock().unwrap().pop().expect("the activation starts a lookup");
	let lookup = poll_once(lookup).expect("the lookup waits on dotNS");
	let read = tasks.lock().unwrap().pop().expect("the lookup starts a dotNS read");

	let service = Arc::downgrade(&host);
	drop(host);
	let dropped = service.upgrade().is_none();
	let lookup = poll_once(lookup);
	let read = poll_once(read);

	assert_eq!((dropped, lookup.is_none(), read.is_none()), (true, true, true));
}

/// The service holds every lookup it started until it learns that one has
/// ended, so a host that activates its session again and again does not pile
/// up the lookups of earlier activations.
#[test]
fn a_lookup_that_ended_is_let_go() {
	let storage = Arc::new(CleanupStorage::default());
	let host = host_with_session(&storage, offline(), &unnamed_sso_session_info());
	let ended = || {
		host.username_lookups
			.lock()
			.unwrap()
			.iter_mut()
			.all(|lookup| !lookup.is_running())
	};

	for _ in 0..3 {
		block_on(host.activate_stored_session()).expect("the stored session activates");
		wait_until(ended, "the lookup of the activation ends");
	}

	assert_eq!(host.username_lookups.lock().unwrap().len(), 1);
}

/// Storing the usernames a lookup found takes two writes, the blob and then
/// its record, and either can fail. The session stays selected under the
/// usernames found. A failed write of the blob leaves the slot and the record
/// as they were. A failed write of the record leaves it naming the blob
/// before, so the next start takes the usernames in the new blob for stale and
/// looks them up again rather than vouching for them.
#[test]
fn a_failed_write_of_the_named_session_keeps_it_selected() {
	let installed = unnamed_sso_session_info();
	let named = named(&installed);
	let (blob, named_blob) =
		(encode_persisted_session(&installed), encode_persisted_session(&named));
	let record = usernames_record(&paseo_test_stack().run(), &blob);
	let stored_after = |failing: CoreStorageKey| {
		let storage = Arc::new(CleanupStorage::default());
		let host = host_with_session(&storage, offline(), &installed);
		block_on(storage.write_core_storage(CoreStorageKey::SessionUsernamesRun, record.clone()))
			.unwrap();
		*storage.write_failure.lock().unwrap() = Some(failing);

		block_on(host.name_session(&installed, named.clone(), Some(blob.clone())));

		let read = |key| block_on(storage.read_core_storage(key)).unwrap();
		(
			host.session_state.current(),
			read(CoreStorageKey::AuthSession),
			read(CoreStorageKey::SessionUsernamesRun),
		)
	};

	assert_eq!(
		[
			stored_after(CoreStorageKey::AuthSession),
			stored_after(CoreStorageKey::SessionUsernamesRun),
		],
		[
			(Some(named.clone()), Some(blob.clone()), Some(record.clone())),
			(Some(named), Some(named_blob), Some(record)),
		]
	);
}

/// A usernames record that cannot be read does not vouch for the usernames
/// stored with the session, which may name accounts on chains that are gone.
/// The session comes up without them, as for a record of another run, and
/// keeps none while the lookup fails.
#[test]
fn a_usernames_record_that_cannot_be_read_drops_the_usernames() {
	let storage = Arc::new(CleanupStorage::default());
	let host = pairing_host(&storage, offline());
	let stored = named(&unnamed_sso_session_info());
	let blob = encode_persisted_session(&stored);
	let record = usernames_record(&paseo_test_stack().run(), &blob);
	block_on(storage.write_core_storage(CoreStorageKey::AuthSession, blob)).unwrap();
	block_on(storage.write_core_storage(CoreStorageKey::SessionUsernamesRun, record)).unwrap();
	*storage.read_failure.lock().unwrap() = Some(CoreStorageKey::SessionUsernamesRun);

	block_on(host.activate_stored_session()).expect("the stored session activates");

	assert_eq!(
		host.session_state.current(),
		Some(SessionInfo { lite_username: None, full_username: None, ..stored })
	);
}
