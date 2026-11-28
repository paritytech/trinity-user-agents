use std::{
	collections::BTreeMap,
	sync::{Arc, Mutex},
};

use futures::executor::block_on;
use parity_scale_codec::Encode;
use serde_json::Value;

use super::{super::tests::CleanupStorage, *};
use crate::{
	Spawner,
	host_internal::sso_messages::{RemoteMessage, RemoteMessageData, Response, v1},
	host_logic::{
		session_times::{KEY_ROTATION_INTERVAL_SECS, SESSION_LIFETIME_SECS},
		sso::pairing::SsoStatementData,
		statement_store::{SUBMIT_STATEMENT_METHOD, hex_topic},
	},
	platform::AuthState,
	runtime::ProductRuntimeHost,
	test_support::{
		RpcResponder, StubPlatform, new_statements_frame, sso_session_info, sso_statement,
		statement_submit_ack_frame, submitted_sso_request_from_submit, subscribe_ack_frame,
		wait_until,
	},
};

/// How the scripted signing host answers a session key change.
#[derive(Clone)]
enum Wallet {
	/// It serves the new keys, disconnects the old ones, and says so, in that
	/// order, so a resend finds the disconnect stored before the answer.
	Rotates,
	/// It answers with this refusal.
	Refuses(RotateSessionError),
	/// It predates the message and acknowledges the request as undecodable.
	CannotDecode,
	/// It never answers.
	Silent,
}

/// A signing host paired with `session` that answers each request it
/// receives as `wallet` says, on the topic the pairing host waits on.
fn signing_host(session: SessionInfo, wallet: Wallet) -> RpcResponder {
	let own_topic = hex_topic(&session.sso.as_ref().unwrap().session_id_own);
	let own_subscription = Arc::new(Mutex::new(None));
	Arc::new(move |request: &str| {
		let call: Value = serde_json::from_str(request).unwrap();
		let id = call["id"].as_str().unwrap_or_default();
		match call["method"].as_str() {
			Some("statement_subscribeStatement") => {
				let subscription = format!("sub-{id}");
				if call["params"][0]["matchAll"][0] == own_topic.as_str() {
					*own_subscription.lock().unwrap() = Some(subscription.clone());
				}
				vec![subscribe_ack_frame(id, &subscription)]
			},
			Some(SUBMIT_STATEMENT_METHOD) => {
				let (request_id, message) = submitted_sso_request_from_submit(request, &session);
				let subscription = own_subscription.lock().unwrap().clone().unwrap();
				let acknowledgement = |response_code| {
					sso_statement(
						&session,
						SsoStatementData::Response {
							request_id: request_id.clone(),
							response_code,
						},
						1,
					)
				};
				let message_from_wallet = |data: v1::RemoteMessage, nonce_seed| {
					let message = RemoteMessage {
						message_id: format!("wallet-{nonce_seed}-{}", message.message_id),
						data: RemoteMessageData::V1(data),
					};
					sso_statement(
						&session,
						SsoStatementData::Request {
							request_id: format!("wallet-{nonce_seed}-{request_id}"),
							data: vec![message.encode()],
						},
						nonce_seed,
					)
				};
				let answer = |payload| {
					v1::RemoteMessage::RotateSessionResponse(Response {
						responding_to: message.message_id.clone(),
						payload,
					})
				};
				let statements = match &wallet {
					Wallet::Rotates => vec![
						acknowledgement(0),
						message_from_wallet(v1::RemoteMessage::Disconnected, 2),
						message_from_wallet(answer(Ok(())), 3),
					],
					Wallet::Refuses(refusal) => {
						vec![
							acknowledgement(0),
							message_from_wallet(answer(Err(refusal.clone())), 2),
						]
					},
					Wallet::CannotDecode => vec![acknowledgement(2)],
					Wallet::Silent => Vec::new(),
				};
				let mut frames = vec![statement_submit_ack_frame(id)];
				if !statements.is_empty() {
					frames.push(new_statements_frame(&subscription, statements));
				}
				frames
			},
			_ => Vec::new(),
		}
	})
}

/// A pairing host over `storage`, paired with a signing host that answers as `wallet`.
fn pairing_host(
	storage: Arc<CleanupStorage>,
	wallet: Wallet,
) -> (Arc<StubPlatform>, Arc<SsoRequestService>) {
	let platform = Arc::new(StubPlatform {
		core_storage_override: Some(storage),
		rpc_responder: Some(signing_host(sso_session_info(), wallet)),
		..Default::default()
	});
	let (_, pairing) =
		ProductRuntimeHost::new_compat_with_pairing(platform.clone(), Spawner::for_test());
	(platform, pairing.session().clone())
}

/// Every session key change the pairing host sent, oldest first.
fn sent_changes(platform: &StubPlatform) -> Vec<RemoteMessage> {
	let session = sso_session_info();
	platform
		.sent_rpc
		.lock()
		.unwrap()
		.iter()
		.filter(|request| request.contains(SUBMIT_STATEMENT_METHOD))
		.map(|submit| submitted_sso_request_from_submit(submit, &session).1)
		.filter(|message| {
			matches!(
				message.data,
				RemoteMessageData::V1(v1::RemoteMessage::RotateSessionRequest(_))
			)
		})
		.collect()
}

/// Storage holding the fixture session, paired `paired_ago` seconds ago and
/// last changed `changed_ago` seconds ago.
fn stored_session(paired_ago: u64, changed_ago: u64) -> BTreeMap<Vec<u8>, Vec<u8>> {
	let session = sso_session_info();
	let sso = session.sso.as_ref().unwrap();
	let now = current_unix_secs();
	let times = SessionTimes::paired(sso, now - paired_ago)
		.rotated(sso, now - changed_ago)
		.to_stored();
	BTreeMap::from([
		(CoreStorageKey::AuthSession.encode(), encode_persisted_session(&session)),
		(CoreStorageKey::SessionTimes.encode(), times),
	])
}

/// The fixture session due for a key change, with that change pending.
fn stored_pending_change() -> (PendingSessionRotation, BTreeMap<Vec<u8>, Vec<u8>>) {
	let sso = sso_session_info().sso.unwrap();
	let pending = PendingSessionRotation::start(&sso, "rotate-1".to_string()).unwrap();
	let mut stored = stored_session(3 * KEY_ROTATION_INTERVAL_SECS, 2 * KEY_ROTATION_INTERVAL_SECS);
	stored.insert(CoreStorageKey::PendingSessionRotation.encode(), pending.to_stored());
	(pending, stored)
}

/// Storage holding `values`.
fn storage_with(values: BTreeMap<Vec<u8>, Vec<u8>>) -> Arc<CleanupStorage> {
	let storage = Arc::new(CleanupStorage::default());
	*storage.values.lock().unwrap() = values;
	storage
}

/// The fixture session on the keys `pending` changes it to.
fn rotated_session(pending: &PendingSessionRotation) -> SessionInfo {
	let session = sso_session_info();
	SessionInfo { sso: Some(pending.rotated(session.sso.as_ref().unwrap()).unwrap()), ..session }
}

fn fixture_key() -> SsoSessionKey {
	SsoSessionKey::from_session(sso_session_info().sso.as_ref().unwrap())
}

/// The signing host disconnects the old keys right after answering, so a
/// resend finds that disconnect stored ahead of the answer and still has to
/// wait for the answer.
#[test]
fn a_session_due_for_a_key_change_moves_to_new_keys_with_its_lifetime_and_grants() {
	let session = sso_session_info();
	let sso = session.sso.as_ref().unwrap();
	let paired_at = current_unix_secs() - 3 * KEY_ROTATION_INTERVAL_SECS;
	let mut stored = stored_session(3 * KEY_ROTATION_INTERVAL_SECS, 2 * KEY_ROTATION_INTERVAL_SECS);
	stored.insert(
		CoreStorageKey::AllowanceKeys { session_id: session_storage_id(sso) }.encode(),
		vec![0x02],
	);
	let storage = storage_with(stored);
	let (platform, host) = pairing_host(storage.clone(), Wallet::Rotates);

	host.clone().start_session_store_sync(Spawner::for_test());

	wait_until(
		|| host.session_state.current().is_some_and(|current| current.sso != session.sso),
		"the restored session moves to its new keys",
	);
	let rotated = host.session_state.current().unwrap();
	let rotated_sso = rotated.sso.clone().unwrap();
	let values = storage.values.lock().unwrap().clone();
	let StoredSessionTimes::Current(times) = SessionTimes::from_stored(
		values.get(&CoreStorageKey::SessionTimes.encode()).map(Vec::as_slice),
		&rotated_sso,
	) else {
		panic!("the new keys have their own times");
	};
	let RemoteMessageData::V1(v1::RemoteMessage::RotateSessionRequest(request)) =
		sent_changes(&platform).remove(0).data
	else {
		panic!("the pairing host asked for the change");
	};
	assert_eq!(
		(
			values,
			SessionInfo { sso: session.sso.clone(), ..rotated.clone() },
			rotated_sso.ss_public_key,
			[
				times.is_expired(paired_at + SESSION_LIFETIME_SECS - 60),
				times.is_expired(paired_at + SESSION_LIFETIME_SECS),
				times.is_rotation_due(current_unix_secs()),
			],
		),
		(
			BTreeMap::from([
				(CoreStorageKey::AuthSession.encode(), encode_persisted_session(&rotated)),
				(CoreStorageKey::SessionTimes.encode(), times.to_stored()),
				(
					CoreStorageKey::AllowanceKeys { session_id: session_storage_id(&rotated_sso) }
						.encode(),
					vec![0x02],
				),
			]),
			session.clone(),
			request.statement_account_id,
			[false, true, false],
		),
		"the session keeps everything but its own keys, its lifetime runs from the first pairing, \
		 and its allowance grants follow it to the new session id"
	);
}

#[test]
fn keys_changed_within_a_day_are_left_alone() {
	let stored = stored_session(3 * KEY_ROTATION_INTERVAL_SECS, KEY_ROTATION_INTERVAL_SECS - 60);
	let storage = storage_with(stored.clone());
	let (platform, host) = pairing_host(storage.clone(), Wallet::Rotates);
	block_on(host.set_connected_session_for_tests(sso_session_info()));

	block_on(host.rotate_due_keys());

	assert_eq!(
		(sent_changes(&platform), storage.values.lock().unwrap().clone()),
		(Vec::new(), stored)
	);
}

/// An old wallet cannot decode the request, and the core's own signing host
/// refuses it; either way the user stays signed in on keys that stay due.
#[test]
fn a_signing_host_that_keeps_the_keys_leaves_the_session_on_them() {
	for wallet in [Wallet::CannotDecode, Wallet::Refuses(RotateSessionError::Unsupported)] {
		let stored = stored_session(3 * KEY_ROTATION_INTERVAL_SECS, 2 * KEY_ROTATION_INTERVAL_SECS);
		let storage = storage_with(stored.clone());
		let (platform, host) = pairing_host(storage.clone(), wallet);
		block_on(host.set_connected_session_for_tests(sso_session_info()));

		block_on(host.rotate_due_keys());

		assert_eq!(
			(
				sent_changes(&platform).len(),
				storage.values.lock().unwrap().clone(),
				host.session_state.current(),
			),
			(1, stored, Some(sso_session_info()))
		);
	}
}

/// The signing host served another change of these keys first, as it does
/// for a copy of the session that changed them before this host did.
#[test]
fn keys_another_change_replaced_end_the_session() {
	let storage = storage_with(stored_session(
		3 * KEY_ROTATION_INTERVAL_SECS,
		2 * KEY_ROTATION_INTERVAL_SECS,
	));
	let (platform, host) =
		pairing_host(storage.clone(), Wallet::Refuses(RotateSessionError::Replaced));
	block_on(host.set_connected_session_for_tests(sso_session_info()));

	block_on(host.rotate_due_keys());

	assert_eq!(
		(
			host.session_state.current(),
			storage.values.lock().unwrap().clone(),
			platform.auth_states.lock().unwrap().last().cloned(),
		),
		(None, BTreeMap::new(), Some(AuthState::Disconnected))
	);
}

#[test]
fn an_unanswered_change_is_kept_and_sent_again_under_the_same_message_id() {
	let storage = storage_with(stored_session(
		3 * KEY_ROTATION_INTERVAL_SECS,
		2 * KEY_ROTATION_INTERVAL_SECS,
	));
	let (platform, host) = pairing_host(storage.clone(), Wallet::Silent);
	block_on(host.set_connected_session_for_tests(sso_session_info()));
	let pending = || {
		storage
			.values
			.lock()
			.unwrap()
			.get(&CoreStorageKey::PendingSessionRotation.encode())
			.cloned()
	};

	block_on(host.rotate_due_keys());
	let first = pending().expect("the change is kept until it is answered");
	block_on(host.rotate_due_keys());

	let sent = sent_changes(&platform);
	assert_eq!(
		(sent.len(), sent.first(), pending()),
		(2, sent.get(1), Some(first)),
		"a signing host that served the first request answers the second with the same answer"
	);
}

/// A pairing host whose answer was lost hears only the signing host's
/// disconnect of the old keys, and must finish its change, not sign out.
#[test]
fn a_disconnect_while_a_change_is_pending_completes_the_change() {
	let (pending, stored) = stored_pending_change();
	let storage = storage_with(stored);
	let (platform, host) = pairing_host(storage.clone(), Wallet::Rotates);
	block_on(host.set_connected_session_for_tests(sso_session_info()));

	block_on(host.handle_signing_host_disconnected(fixture_key()));

	let rotated = encode_persisted_session(&rotated_session(&pending));
	wait_until(
		|| {
			storage.values.lock().unwrap().get(&CoreStorageKey::AuthSession.encode()) ==
				Some(&rotated)
		},
		"the pending change is completed",
	);
	assert_eq!(
		(
			sent_changes(&platform)
				.iter()
				.map(|sent| sent.message_id.clone())
				.collect::<Vec<_>>(),
			storage
				.values
				.lock()
				.unwrap()
				.contains_key(&CoreStorageKey::PendingSessionRotation.encode()),
			platform.auth_states.lock().unwrap().contains(&AuthState::Disconnected),
		),
		(vec!["rotate-1".to_string()], false, false)
	);
}

/// A copy of the session that changed the keys first gets the signing host's
/// disconnect sent to the real host, whose own change is then refused.
#[test]
fn a_disconnect_the_pending_change_does_not_explain_ends_the_session() {
	let (_, stored) = stored_pending_change();
	let storage = storage_with(stored);
	let (platform, host) = pairing_host(storage.clone(), Wallet::CannotDecode);
	block_on(host.set_connected_session_for_tests(sso_session_info()));

	block_on(host.handle_signing_host_disconnected(fixture_key()));

	wait_until(
		|| host.session_state.current().is_none() && storage.values.lock().unwrap().is_empty(),
		"the session and everything stored for it end",
	);
	assert_eq!(platform.auth_states.lock().unwrap().last().cloned(), Some(AuthState::Disconnected));
}

/// A disconnect that arrives while the change waits for an answer that never
/// comes still ends the session once the wait is over.
#[test]
fn a_disconnect_during_an_unanswered_change_ends_the_session_after_the_wait() {
	let storage = storage_with(stored_session(
		3 * KEY_ROTATION_INTERVAL_SECS,
		2 * KEY_ROTATION_INTERVAL_SECS,
	));
	let (platform, host) = pairing_host(storage.clone(), Wallet::Silent);
	block_on(host.set_connected_session_for_tests(sso_session_info()));

	host.rotate_keys_when_due();
	wait_until(|| sent_changes(&platform).len() == 1, "the change is asked for");
	block_on(host.handle_signing_host_disconnected(fixture_key()));

	wait_until(
		|| host.session_state.current().is_none() && storage.values.lock().unwrap().is_empty(),
		"the session and everything stored for it end after the wait",
	);
}

/// Two instances that started different changes together: the signing host
/// served the other one, which is already stored. The refused instance must
/// not sign that session out.
#[test]
fn a_refused_change_leaves_the_session_another_instance_stored() {
	let session = sso_session_info();
	let sso = session.sso.as_ref().unwrap();
	let ours = PendingSessionRotation::start(sso, "rotate-ours".to_string()).unwrap();
	let theirs = PendingSessionRotation::start(sso, "rotate-theirs".to_string()).unwrap();
	let stored = BTreeMap::from([(
		CoreStorageKey::AuthSession.encode(),
		encode_persisted_session(&rotated_session(&theirs)),
	)]);
	let storage = storage_with(stored.clone());
	storage
		.values
		.lock()
		.unwrap()
		.insert(CoreStorageKey::PendingSessionRotation.encode(), ours.to_stored());
	let (platform, host) = pairing_host(storage.clone(), Wallet::CannotDecode);
	block_on(host.set_connected_session_for_tests(session.clone()));

	block_on(host.handle_signing_host_disconnected(fixture_key()));

	wait_until(|| sent_changes(&platform).len() == 1, "the change is asked for again");
	wait_until(
		|| *storage.values.lock().unwrap() == stored,
		"the refused change is dropped and the other session stays",
	);
	assert!(!platform.auth_states.lock().unwrap().contains(&AuthState::Disconnected));
}

/// Another instance sharing the store finished the change, so the disconnect
/// of the old keys is the signing host confirming it.
#[test]
fn a_disconnect_after_another_instance_finished_the_change_keeps_the_stored_session() {
	let (pending, _) = stored_pending_change();
	let stored = BTreeMap::from([(
		CoreStorageKey::AuthSession.encode(),
		encode_persisted_session(&rotated_session(&pending)),
	)]);
	let storage = storage_with(stored.clone());
	let (platform, host) = pairing_host(storage.clone(), Wallet::Silent);
	block_on(host.set_connected_session_for_tests(sso_session_info()));

	block_on(host.handle_signing_host_disconnected(fixture_key()));

	assert_eq!(
		(
			storage.values.lock().unwrap().clone(),
			platform.auth_states.lock().unwrap().contains(&AuthState::Disconnected),
		),
		(stored, false)
	);
}

/// A commit that cannot store the session puts everything else back, so the
/// old keys keep their times and no copy of the grants is left behind.
#[test]
fn a_change_whose_session_cannot_be_stored_is_rolled_back_and_kept() {
	let sso = sso_session_info().sso.unwrap();
	let (_, mut stored) = stored_pending_change();
	stored.insert(
		CoreStorageKey::AllowanceKeys { session_id: session_storage_id(&sso) }.encode(),
		vec![0x02],
	);
	let storage = storage_with(stored.clone());
	*storage.write_failure.lock().unwrap() = Some(CoreStorageKey::AuthSession);
	let (_, host) = pairing_host(storage.clone(), Wallet::Rotates);
	block_on(host.set_connected_session_for_tests(sso_session_info()));

	block_on(host.rotate_due_keys());

	assert_eq!(storage.values.lock().unwrap().clone(), stored);
}

/// A sign-out while the change waits for its answer deletes the new keys, and
/// the change writes nothing back.
#[test]
fn signing_out_during_a_change_leaves_nothing_behind() {
	let storage = storage_with(stored_session(
		3 * KEY_ROTATION_INTERVAL_SECS,
		2 * KEY_ROTATION_INTERVAL_SECS,
	));
	let (platform, host) = pairing_host(storage.clone(), Wallet::Silent);
	block_on(host.set_connected_session_for_tests(sso_session_info()));

	host.rotate_keys_when_due();
	wait_until(|| sent_changes(&platform).len() == 1, "the change is asked for");
	block_on(host.clear_disconnected_session(true, None));

	wait_until(|| host.rotation.try_lock().is_some(), "the change stops");
	assert_eq!(storage.values.lock().unwrap().clone(), BTreeMap::new());
}

#[test]
fn signing_out_deletes_a_pending_change_with_its_keys() {
	let (_, stored) = stored_pending_change();
	let storage = storage_with(stored);
	let (_, host) = pairing_host(storage.clone(), Wallet::Silent);
	block_on(host.set_connected_session_for_tests(sso_session_info()));

	block_on(host.clear_disconnected_session(true, None));

	assert_eq!(storage.values.lock().unwrap().clone(), BTreeMap::new());
}

/// A long-running instance asks again between restores for a change that
/// went unanswered.
#[test]
fn an_unanswered_change_is_asked_again_without_a_restore() {
	let storage = storage_with(stored_session(
		3 * KEY_ROTATION_INTERVAL_SECS,
		2 * KEY_ROTATION_INTERVAL_SECS,
	));
	let (platform, host) = pairing_host(storage, Wallet::Silent);

	host.clone().start_session_store_sync(Spawner::for_test());

	wait_until(|| sent_changes(&platform).len() >= 2, "the keys are checked again");
	assert_eq!(host.session_store_change_ticks_for_tests(), 0);
}

/// Every deployed signing host refuses the change, so the periodic check
/// leaves a refused change to the next restore instead of asking each hour.
#[test]
fn a_refused_change_waits_for_the_next_restore() {
	let storage = storage_with(stored_session(
		3 * KEY_ROTATION_INTERVAL_SECS,
		2 * KEY_ROTATION_INTERVAL_SECS,
	));
	let (platform, host) = pairing_host(storage, Wallet::CannotDecode);
	host.clone().start_session_store_sync(Spawner::for_test());
	wait_until(|| sent_changes(&platform).len() == 1, "the restore asks for the change");

	std::thread::sleep(3 * ROTATION_CHECK_INTERVAL);
	let between_restores = sent_changes(&platform).len();
	block_on(host.activate_stored_session()).unwrap();

	wait_until(|| sent_changes(&platform).len() == 2, "the next restore asks again");
	assert_eq!(between_restores, 1);
}

/// Another session replaced this one while its change waited, so the new keys
/// of a session nobody uses any more are deleted.
#[test]
fn a_change_of_a_session_replaced_meanwhile_is_deleted() {
	let storage = storage_with(stored_session(
		3 * KEY_ROTATION_INTERVAL_SECS,
		2 * KEY_ROTATION_INTERVAL_SECS,
	));
	let (platform, host) = pairing_host(storage.clone(), Wallet::Silent);
	block_on(host.set_connected_session_for_tests(sso_session_info()));

	host.rotate_keys_when_due();
	wait_until(|| sent_changes(&platform).len() == 1, "the change is asked for");
	let mut other = sso_session_info();
	other.sso.as_mut().unwrap().ss_public_key = [0xEE; 32];
	block_on(host.set_connected_session_for_tests(other));

	wait_until(|| host.rotation.try_lock().is_some(), "the change stops");
	assert!(
		!storage
			.values
			.lock()
			.unwrap()
			.contains_key(&CoreStorageKey::PendingSessionRotation.encode())
	);
}

/// A sign-out that lands while a change is being started leaves no new keys
/// in a store it cleared.
#[test]
fn a_change_started_as_the_user_signs_out_is_not_kept() {
	let storage = storage_with(stored_session(
		3 * KEY_ROTATION_INTERVAL_SECS,
		2 * KEY_ROTATION_INTERVAL_SECS,
	));
	let (release, pause) = futures::channel::oneshot::channel();
	*storage.read_pause.lock().unwrap() = Some((CoreStorageKey::PendingSessionRotation, pause));
	let (_, host) = pairing_host(storage.clone(), Wallet::Silent);
	block_on(host.set_connected_session_for_tests(sso_session_info()));

	host.rotate_keys_when_due();
	wait_until(|| storage.read_pause.lock().unwrap().is_none(), "the change is being started");
	block_on(host.clear_disconnected_session(true, None));
	release.send(()).unwrap();

	wait_until(|| host.rotation.try_lock().is_some(), "the change stops");
	assert_eq!(storage.values.lock().unwrap().clone(), BTreeMap::new());
}

/// The signing host serves only the new keys once it answers, so a change
/// it served is stored even when this core cannot read the old keys' times,
/// which stay as a newer core wrote them.
#[test]
fn a_served_change_is_stored_when_the_old_times_cannot_be_read() {
	let (pending, mut stored) = stored_pending_change();
	let newer_times = vec![1, 2, 3];
	stored.insert(CoreStorageKey::SessionTimes.encode(), newer_times.clone());
	let storage = storage_with(stored);
	let (_, host) = pairing_host(storage.clone(), Wallet::Rotates);
	block_on(host.set_connected_session_for_tests(sso_session_info()));

	block_on(host.handle_signing_host_disconnected(fixture_key()));

	let rotated = encode_persisted_session(&rotated_session(&pending));
	wait_until(
		|| {
			storage.values.lock().unwrap().get(&CoreStorageKey::AuthSession.encode()) ==
				Some(&rotated)
		},
		"the served change is stored",
	);
	assert_eq!(
		storage.values.lock().unwrap().clone(),
		BTreeMap::from([
			(CoreStorageKey::AuthSession.encode(), rotated),
			(CoreStorageKey::SessionTimes.encode(), newer_times),
		])
	);
}
