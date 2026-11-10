use super::*;
use crate::{
	host_rpc_client::HostRpcClient,
	platform::{AuthState, ChainProvider, CoreStorageKey},
	runtime::{
		ProductRuntimeHost, connected_session_ui_info, sso_request_service::SsoRequestService,
	},
	test_support::{
		StubPlatform, core_storage_test_key, pairing_device_from_deeplink, peer_statement_keypair,
		runtime_config, session_info, signed_test_statement, stub_platform, subscribe_ack_frame,
		wallet_device_encryption_public_key, wallet_handshake_statement,
	},
};
use truapi::{
	CallContext,
	api::Account,
	versioned::account::{HostAccountConnectionStatusSubscribeItem, HostRequestLoginRequest},
};
use x25519_dalek::{PublicKey as X25519PublicKey, StaticSecret as X25519SecretKey};

/// Cancel the login as soon as the host observes the `Pairing` state,
/// mimicking a user dismissing the pairing UI immediately.
fn cancel_on_pairing(platform: &StubPlatform, sso: Arc<SsoRequestService>) {
	*platform.on_auth_state.lock().expect("auth state hook mutex poisoned") =
		Some(Arc::new(move |state| {
			if matches!(state, AuthState::Pairing { .. }) {
				sso.cancel_login();
			}
		}));
}

#[test]
fn request_login_presents_pairing_and_rejects_when_cancelled() {
	let platform = stub_platform();
	let (host, pairing) =
		ProductRuntimeHost::new_compat_with_pairing(platform.clone(), Spawner::for_test());
	let sso = pairing.session().clone();
	let host = Arc::new(host);
	cancel_on_pairing(&platform, sso);
	let cx = CallContext::default();
	let request = HostRequestLoginRequest::V1(api::HostRequestLoginRequest { reason: None });
	let response = futures::executor::block_on(host.request_login(&cx, request)).unwrap();

	assert_eq!(response, HostRequestLoginResponse::V1(api::HostRequestLoginResponse::Rejected));
	let auth_states = platform.auth_states.lock().expect("auth state list mutex poisoned");
	assert_eq!(auth_states.len(), 2, "states: {auth_states:?}");
	match &auth_states[0] {
		AuthState::Pairing { deeplink } => {
			assert!(deeplink.starts_with("polkadotapp://pair?handshake="));
		},
		other => panic!("expected pairing state first, got {other:?}"),
	}
	assert_eq!(auth_states[1], AuthState::Disconnected);

	let sent_rpc = platform.sent_rpc.lock().expect("rpc list mutex poisoned");
	if let Some(sent) = sent_rpc.first() {
		let request: serde_json::Value = serde_json::from_str(sent).unwrap();
		assert_eq!(request["method"], "statement_subscribeStatement");
		assert_eq!(request["params"][0]["matchAll"][0].as_str().unwrap().len(), 66);
	}
}

/// A peer on a different SSO envelope publishes statements this host cannot
/// open, so the topic stays silent past the subscription exactly as it would
/// before the peer answered at all. The flow has to give up on its own and
/// say why.
#[test]
fn request_login_gives_up_when_nothing_decryptable_answers() {
	let platform =
		Arc::new(StubPlatform { pairing_silent_after_subscribe: true, ..Default::default() });
	let (host, _pairing) =
		ProductRuntimeHost::new_compat_with_pairing(platform.clone(), Spawner::for_test());
	let host = Arc::new(host);
	let cx = CallContext::default();
	let request = HostRequestLoginRequest::V1(api::HostRequestLoginRequest { reason: None });
	let outcome = futures::executor::block_on(host.request_login(&cx, request));

	let error = outcome.expect_err("a pairing nothing answers must not wait forever");
	let rendered = format!("{error:?}");
	assert!(
		rendered.contains("different SSO envelope"),
		"the failure should name the likely cause, got {rendered}"
	);
}

/// Nothing answers the subscription request itself, so the flow never gets
/// as far as watching the topic. The wait for a live subscription carries
/// the same deadline as the wait for a peer on it.
#[test]
fn request_login_gives_up_when_the_pairing_topic_never_acks() {
	let platform = stub_platform();
	let (host, _pairing) =
		ProductRuntimeHost::new_compat_with_pairing(platform.clone(), Spawner::for_test());
	let host = Arc::new(host);
	let cx = CallContext::default();
	let request = HostRequestLoginRequest::V1(api::HostRequestLoginRequest { reason: None });
	let outcome = futures::executor::block_on(host.request_login(&cx, request));

	let error = outcome.expect_err("a subscription nothing acknowledges must not wait forever");
	let rendered = format!("{error:?}");
	assert!(
		rendered.contains("pairing did not complete within"),
		"the failure should name the deadline, got {rendered}"
	);
}

/// The statement-store connection never opens, so the flow parks before it
/// has a topic to watch at all. The deadline is armed ahead of that connect,
/// not after it.
/// The bound has to cover the entry point, not just the flow. These reads
/// are host callbacks, so a platform that never answers one would otherwise
/// hold `request_login` open before the deadline is armed, which is the
/// same silence from the same entry point this whole change exists to end.
#[test]
fn request_login_gives_up_when_core_storage_never_answers() {
	let platform = Arc::new(StubPlatform { core_storage_pending: true, ..Default::default() });
	let host = ProductRuntimeHost::new_compat(platform, Spawner::for_test());
	let cx = CallContext::default();
	let request = HostRequestLoginRequest::V1(api::HostRequestLoginRequest { reason: None });

	let error = futures::executor::block_on(host.request_login(&cx, request))
		.expect_err("storage that never answers must not hold the attempt open");

	let rendered = format!("{error:?}");
	assert!(
		rendered.contains("pairing did not complete within"),
		"the failure should name the deadline, got {rendered}"
	);
}

#[test]
fn request_login_gives_up_when_the_statement_store_never_connects() {
	let platform = Arc::new(StubPlatform { chain_connect_pending: true, ..Default::default() });
	let (host, _pairing) =
		ProductRuntimeHost::new_compat_with_pairing(platform.clone(), Spawner::for_test());
	let host = Arc::new(host);
	let cx = CallContext::default();
	let request = HostRequestLoginRequest::V1(api::HostRequestLoginRequest { reason: None });
	let outcome = futures::executor::block_on(host.request_login(&cx, request));

	let error = outcome.expect_err("a connect that never opens must not wait forever");
	let rendered = format!("{error:?}");
	assert!(
		rendered.contains("pairing did not complete within"),
		"the failure should name the deadline, got {rendered}"
	);
}

#[test]
fn request_login_regenerates_unmarked_pairing_device_identity_between_attempts() {
	let platform = stub_platform();
	let (host, pairing) =
		ProductRuntimeHost::new_compat_with_pairing(platform.clone(), Spawner::for_test());
	let sso = pairing.session().clone();
	let host = Arc::new(host);
	cancel_on_pairing(&platform, sso);
	let cx = CallContext::default();
	let request = HostRequestLoginRequest::V1(api::HostRequestLoginRequest { reason: None });

	let first = futures::executor::block_on(host.request_login(&cx, request.clone())).unwrap();
	let second = futures::executor::block_on(host.request_login(&cx, request)).unwrap();

	assert_eq!(first, HostRequestLoginResponse::V1(api::HostRequestLoginResponse::Rejected));
	assert_eq!(second, HostRequestLoginResponse::V1(api::HostRequestLoginResponse::Rejected));
	let deeplinks: Vec<String> = platform
		.auth_states
		.lock()
		.expect("auth state list mutex poisoned")
		.iter()
		.filter_map(|state| match state {
			AuthState::Pairing { deeplink } => Some(deeplink.clone()),
			_ => None,
		})
		.collect();
	assert_eq!(deeplinks.len(), 2);
	assert_ne!(
		pairing_device_from_deeplink(&deeplinks[0]),
		pairing_device_from_deeplink(&deeplinks[1])
	);
	assert!(
		platform
			.local_storage
			.lock()
			.expect("local storage mutex poisoned")
			.contains_key(&core_storage_test_key(CoreStorageKey::PairingDeviceIdentity)),
		"cancelled pairing keeps the latest identity; the next unmarked reuse regenerates it"
	);
}

#[test]
fn request_login_regenerates_marked_stored_pairing_device_identity() {
	let platform = stub_platform();
	let identity = generate_pairing_device_identity().unwrap();
	platform
		.local_storage
		.lock()
		.expect("local storage mutex poisoned")
		.insert(core_storage_test_key(CoreStorageKey::PairingDeviceIdentity), identity.encode());
	platform.local_storage.lock().expect("local storage mutex poisoned").insert(
		core_storage_test_key(CoreStorageKey::LastProcessedPairingStatement),
		vec![0xde, 0xad],
	);
	let (host, pairing) =
		ProductRuntimeHost::new_compat_with_pairing(platform.clone(), Spawner::for_test());
	let sso = pairing.session().clone();
	let host = Arc::new(host);
	cancel_on_pairing(&platform, sso);
	let cx = CallContext::default();
	let request = HostRequestLoginRequest::V1(api::HostRequestLoginRequest { reason: None });

	let response = futures::executor::block_on(host.request_login(&cx, request)).unwrap();

	assert_eq!(response, HostRequestLoginResponse::V1(api::HostRequestLoginResponse::Rejected));
	let deeplink = platform
		.auth_states
		.lock()
		.expect("auth state list mutex poisoned")
		.iter()
		.find_map(|state| match state {
			AuthState::Pairing { deeplink } => Some(deeplink.clone()),
			_ => None,
		})
		.expect("pairing state should be emitted");
	assert_ne!(
		pairing_device_from_deeplink(&deeplink),
		(identity.statement_store_public_key, identity.encryption_public_key)
	);
	assert!(
		platform
			.local_storage
			.lock()
			.expect("local storage mutex poisoned")
			.contains_key(&core_storage_test_key(CoreStorageKey::PairingDeviceIdentity)),
		"cancelled pairing keeps the rotated identity; the next login rotates again"
	);
}

#[test]
fn request_login_waits_for_pairing_statement() {
	let wallet_ephemeral_secret = X25519SecretKey::from([2; 32]);
	let wallet_ephemeral_public = X25519PublicKey::from(&wallet_ephemeral_secret).to_bytes();
	let handshake = VersionedHandshakeResponse::V2 {
		encrypted_message: vec![0xde, 0xad],
		public_key: wallet_ephemeral_public,
	};
	let statement = signed_test_statement(handshake.encode());
	let notification = format!(
		r#"{{"jsonrpc":"2.0","method":"statement_statement","params":{{"subscription":"remote-sub","result":{{"event":"newStatements","data":{{"statements":["0x{}"],"remaining":0}}}}}}}}"#,
		hex::encode(statement)
	);
	let platform = Arc::new(StubPlatform {
		rpc_responses: vec![
			r#"{"jsonrpc":"2.0","id":"truapi:1","result":"remote-sub"}"#.to_string(),
			notification,
		],
		..Default::default()
	});
	let host = ProductRuntimeHost::new_compat(platform.clone(), Spawner::for_test());
	let cx = CallContext::default();
	let request = HostRequestLoginRequest::V1(api::HostRequestLoginRequest { reason: None });
	let err = futures::executor::block_on(host.request_login(&cx, request)).unwrap_err();

	match err {
		CallError::HostFailure { reason } => {
			assert_eq!(reason, "encrypted SSO handshake answer is too short");
		},
		other => panic!("expected handshake decrypt failure, got {other:?}"),
	}
	let sent_rpc = platform.sent_rpc.lock().expect("rpc list mutex poisoned");
	let requests = sent_rpc
		.iter()
		.map(|request| serde_json::from_str::<serde_json::Value>(request).unwrap())
		.collect::<Vec<_>>();
	let methods = requests
		.iter()
		.map(|request| request["method"].as_str().unwrap())
		.collect::<Vec<_>>();
	assert_eq!(methods.first().copied(), Some("statement_subscribeStatement"));
	assert!(
		methods.contains(&"statement_unsubscribeStatement"),
		"pairing subscription should be cleaned up"
	);
	let unsubscribe = requests
		.iter()
		.find(|request| request["method"].as_str() == Some("statement_unsubscribeStatement"))
		.expect("pairing subscription should be cleaned up");
	assert_eq!(unsubscribe["params"][0], "remote-sub");
}

#[test]
fn request_login_accepts_valid_pairing_statement_and_persists_session() {
	let session_writes = Arc::new(Mutex::new(Vec::new()));
	let platform = Arc::new(StubPlatform {
		pairing_success_response: true,
		session_writes: session_writes.clone(),
		..Default::default()
	});
	let host =
		ProductRuntimeHost::new(platform.clone(), runtime_config("myapp.dot"), Spawner::for_test());
	let mut statuses = host.test_session_state().subscribe();
	assert_eq!(
		futures::executor::block_on(statuses.next()).unwrap(),
		HostAccountConnectionStatusSubscribeItem::V1(
			api::HostAccountConnectionStatusSubscribeItem::Disconnected
		)
	);

	let cx = CallContext::default();
	let request = HostRequestLoginRequest::V1(api::HostRequestLoginRequest { reason: None });
	let response = futures::executor::block_on(host.request_login(&cx, request)).unwrap();

	assert_eq!(response, HostRequestLoginResponse::V1(api::HostRequestLoginResponse::Success));
	assert_eq!(
		futures::executor::block_on(statuses.next()).unwrap(),
		HostAccountConnectionStatusSubscribeItem::V1(
			api::HostAccountConnectionStatusSubscribeItem::Connected
		)
	);

	let session = host.test_session_state().current().expect("paired session should be active");
	assert_eq!(session.public_key, session_info().public_key);
	assert_eq!(session.root_entropy_source, Some([0x66; 32]));
	assert_eq!(session.sso.as_ref().unwrap().identity_account_id, peer_statement_keypair().1);

	// The handshake shares both values once; nothing can recompute them
	// afterwards, so pairing must retain them verbatim.
	assert_eq!(session.identity_chat_private_key, Some([0x77; 32]));
	// The fixture's device key differs from its SSO channel key, so this
	// also pins that the two are not conflated.
	assert_eq!(session.device_enc_public_key, Some(wallet_device_encryption_public_key()));

	// Public values project to host UI; the chat secret deliberately does not.
	let ui_info = connected_session_ui_info(&session);
	assert_eq!(
		ui_info.chat_public_key,
		Some(X25519PublicKey::from(&X25519SecretKey::from([0x77; 32])).to_bytes()),
		"the projected chat public key must match the retained secret"
	);
	assert_eq!(ui_info.peer_statement_account_id, Some(peer_statement_keypair().1));
	assert_eq!(
		ui_info.device_statement_account_id,
		session.sso.as_ref().map(|sso| sso.ss_public_key)
	);

	let writes = session_writes.lock().expect("session write list mutex poisoned");
	assert_eq!(writes.len(), 1);
	assert_eq!(crate::host_logic::session::decode_persisted_session(&writes[0]).unwrap(), session);

	let auth_states = platform.auth_states.lock().expect("auth state list mutex poisoned");
	assert_eq!(auth_states.len(), 3, "states: {auth_states:?}");
	assert!(matches!(&auth_states[0], AuthState::Pairing { .. }));
	assert_eq!(auth_states[1], AuthState::Authenticating);
	assert_eq!(auth_states[2], AuthState::Connected(Box::new(connected_session_ui_info(&session))));
	drop(auth_states);

	let methods = platform
		.sent_rpc
		.lock()
		.expect("rpc list mutex poisoned")
		.iter()
		.map(|request| serde_json::from_str::<serde_json::Value>(request).unwrap())
		.map(|request| request["method"].as_str().unwrap().to_string())
		.collect::<Vec<_>>();
	assert_eq!(methods.first().map(String::as_str), Some("statement_subscribeStatement"));
	assert!(
		methods.iter().any(|method| method == "statement_unsubscribeStatement"),
		"pairing subscription should be cleaned up"
	);
}

#[test]
fn request_login_enters_authenticating_on_pending_wallet_response() {
	let platform = Arc::new(StubPlatform { pairing_pending_response: true, ..Default::default() });
	let (host, pairing) =
		ProductRuntimeHost::new_compat_with_pairing(platform.clone(), Spawner::for_test());
	let sso = pairing.session().clone();
	let cancel_host = sso.clone();
	*platform.on_auth_state.lock().expect("auth state hook mutex poisoned") =
		Some(Arc::new(move |state| {
			if matches!(state, AuthState::Authenticating) {
				cancel_host.cancel_login();
			}
		}));

	let cx = CallContext::default();
	let request = HostRequestLoginRequest::V1(api::HostRequestLoginRequest { reason: None });
	let response = futures::executor::block_on(host.request_login(&cx, request)).unwrap();

	assert_eq!(response, HostRequestLoginResponse::V1(api::HostRequestLoginResponse::Rejected));
	let auth_states = platform.auth_states.lock().expect("auth state list mutex poisoned");
	assert_eq!(auth_states.len(), 3, "states: {auth_states:?}");
	assert!(matches!(auth_states[0], AuthState::Pairing { .. }));
	assert_eq!(auth_states[1], AuthState::Authenticating);
	assert_eq!(auth_states[2], AuthState::Disconnected);
}

#[test]
fn request_login_surfaces_wallet_failure_status() {
	let session_writes = Arc::new(Mutex::new(Vec::new()));
	let platform = Arc::new(StubPlatform {
		pairing_failure_response: true,
		session_writes: session_writes.clone(),
		..Default::default()
	});
	let host =
		ProductRuntimeHost::new(platform.clone(), runtime_config("myapp.dot"), Spawner::for_test());
	let cx = CallContext::default();
	let request = HostRequestLoginRequest::V1(api::HostRequestLoginRequest { reason: None });
	let err = futures::executor::block_on(host.request_login(&cx, request)).unwrap_err();

	let expected_reason =
		"The operation couldn't be completed. (SubstrateSdk.JSONRPCError error 1.)";
	assert_eq!(err, CallError::HostFailure { reason: expected_reason.to_string() });
	assert!(session_writes.lock().expect("session writes mutex poisoned").is_empty());
	let auth_states = platform.auth_states.lock().expect("auth state list mutex poisoned");
	assert!(
		auth_states.iter().any(
			|state| matches!(state, AuthState::LoginFailed { reason, .. } if reason == expected_reason)
		),
		"wallet failure should be surfaced to the modal: {auth_states:?}"
	);
}

#[test]
fn request_login_clears_auth_session_when_cancelled_after_persist() {
	let session_writes = Arc::new(Mutex::new(Vec::new()));
	let session_clears = Arc::new(Mutex::new(0));
	let platform = Arc::new(StubPlatform {
		pairing_success_response: true,
		session_writes: session_writes.clone(),
		session_clears: session_clears.clone(),
		..Default::default()
	});
	let (host, pairing) =
		ProductRuntimeHost::new_compat_with_pairing(platform.clone(), Spawner::for_test());
	let sso = pairing.session().clone();
	let cancel_host = sso.clone();
	*platform
		.on_auth_session_write
		.lock()
		.expect("auth session write hook mutex poisoned") = Some(Arc::new(move || {
		cancel_host.cancel_login();
	}));

	let cx = CallContext::default();
	let request = HostRequestLoginRequest::V1(api::HostRequestLoginRequest { reason: None });
	let response = futures::executor::block_on(host.request_login(&cx, request)).unwrap();

	assert_eq!(response, HostRequestLoginResponse::V1(api::HostRequestLoginResponse::Rejected));
	assert!(host.test_session_state().current().is_none());
	assert_eq!(session_writes.lock().expect("session write list mutex poisoned").len(), 1);
	assert_eq!(*session_clears.lock().expect("session clear counter mutex poisoned"), 1);
}

#[test]
fn request_login_connected_callback_can_clear_session_without_reinstalling_it() {
	let platform = Arc::new(StubPlatform { pairing_success_response: true, ..Default::default() });
	let host = Arc::new(ProductRuntimeHost::new(
		platform.clone(),
		runtime_config("myapp.dot"),
		Spawner::for_test(),
	));
	let disconnect_host = host.clone();
	*platform.on_auth_state.lock().expect("auth state hook mutex poisoned") =
		Some(Arc::new(move |state| {
			if matches!(state, AuthState::Connected(_)) {
				disconnect_host.test_session_state().clear_session();
			}
		}));

	let cx = CallContext::default();
	let request = HostRequestLoginRequest::V1(api::HostRequestLoginRequest { reason: None });
	let response = futures::executor::block_on(host.request_login(&cx, request)).unwrap();

	assert_eq!(response, HostRequestLoginResponse::V1(api::HostRequestLoginResponse::Success));
	assert!(host.test_session_state().current().is_none());

	let auth_states = platform.auth_states.lock().expect("auth state list mutex poisoned");
	assert!(matches!(&auth_states[0], AuthState::Pairing { .. }));
	assert_eq!(auth_states[1], AuthState::Authenticating);
	assert!(matches!(&auth_states[2], AuthState::Connected(_)));
}

/// Pairing success must also be decoded from a snapshot query page, not only
/// from the live pairing subscription.
#[test]
fn request_login_accepts_pairing_statement_from_snapshot_query_page() {
	let (host_config, _) = runtime_config("myapp.dot");
	let pairing_identity = generate_pairing_device_identity().unwrap();
	let bootstrap = create_pairing_bootstrap_from_identity(&host_config, pairing_identity).unwrap();
	let statement = wallet_handshake_statement(&bootstrap.deeplink);
	let platform = Arc::new(StubPlatform {
		rpc_responses: vec![
			subscribe_ack_frame("truapi:1", "query-sub"),
			crate::test_support::new_statements_frame("query-sub", vec![statement]),
		],
		..Default::default()
	});
	let connection =
		futures::executor::block_on(platform.connect(host_config.people_chain_genesis_hash))
			.unwrap();
	let rpc_client = RpcClient::new(HostRpcClient::new(Arc::from(connection), Spawner::for_test()));
	let progress = futures::executor::block_on(run_pairing_snapshot_query(
		rpc_client,
		bootstrap.topic,
		bootstrap.encryption_secret_key,
		None,
	))
	.unwrap()
	.expect("snapshot query should return pairing success");
	let PairingProgress::Success(success) = progress else {
		panic!("snapshot query should return final pairing success");
	};

	assert_eq!(success.peer_statement_account_id, peer_statement_keypair().1);
	assert_eq!(success.success.root_account_id, session_info().public_key);

	let methods = platform
		.sent_rpc
		.lock()
		.expect("rpc list mutex poisoned")
		.iter()
		.map(|request| serde_json::from_str::<serde_json::Value>(request).unwrap())
		.map(|request| request["method"].as_str().unwrap().to_string())
		.collect::<Vec<_>>();
	assert_eq!(methods.first().map(String::as_str), Some("statement_subscribeStatement"));
}

#[test]
fn pairing_result_skips_last_processed_statement() {
	let (host_config, _) = runtime_config("myapp.dot");
	let pairing_identity = generate_pairing_device_identity().unwrap();
	let bootstrap = create_pairing_bootstrap_from_identity(&host_config, pairing_identity).unwrap();
	let statement = wallet_handshake_statement(&bootstrap.deeplink);
	let page = serde_json::json!({
		"event": "newStatements",
		"data": {
			"statements": [format!("0x{}", hex::encode(&statement))],
			"remaining": 0,
		},
	});

	let ignored = handle_v2_pairing_result(
		&page,
		bootstrap.encryption_secret_key,
		Some(statement.as_slice()),
	)
	.unwrap();
	assert!(ignored.is_none());

	let accepted = handle_v2_pairing_result(&page, bootstrap.encryption_secret_key, None)
		.unwrap()
		.expect("unmarked statement should be accepted");
	let PairingProgress::Success(accepted) = accepted else {
		panic!("unmarked statement should contain final pairing success");
	};
	assert_eq!(accepted.peer_statement_account_id, peer_statement_keypair().1);
}

#[test]
fn request_login_emits_login_failed_for_pre_pairing_errors() {
	let platform = Arc::new(StubPlatform {
		local_storage_error: Some("identity storage unavailable"),
		..Default::default()
	});
	let host = ProductRuntimeHost::new_compat(platform.clone(), Spawner::for_test());
	let cx = CallContext::default();
	let request = HostRequestLoginRequest::V1(api::HostRequestLoginRequest { reason: None });
	let err = futures::executor::block_on(host.request_login(&cx, request)).unwrap_err();

	assert!(matches!(err, CallError::Domain(_)));
	let auth_states = platform.auth_states.lock().expect("auth state list mutex poisoned");
	assert_eq!(auth_states.len(), 1, "states: {auth_states:?}");
	assert!(matches!(&auth_states[0], AuthState::LoginFailed { reason, .. }
        if reason.contains("identity storage unavailable")));
}

#[test]
fn dropped_request_login_clears_single_flight_for_next_attempt() {
	use std::{
		future::Future,
		task::{Context, Poll},
	};

	let platform = Arc::new(StubPlatform { chain_connect_pending: true, ..Default::default() });
	let (host, pairing) =
		ProductRuntimeHost::new_compat_with_pairing(platform.clone(), Spawner::for_test());
	let sso = pairing.session().clone();
	let host = Arc::new(host);
	let request = HostRequestLoginRequest::V1(api::HostRequestLoginRequest { reason: None });
	let cx = CallContext::default();
	let mut first_login = Box::pin(host.request_login(&cx, request.clone()));
	let waker = futures::task::noop_waker();
	let mut task_cx = Context::from_waker(&waker);

	match first_login.as_mut().poll(&mut task_cx) {
		Poll::Pending => {},
		Poll::Ready(result) => panic!("first login should be pending, got {result:?}"),
	}
	assert!(
		platform
			.auth_states
			.lock()
			.expect("auth state list mutex poisoned")
			.iter()
			.any(|state| matches!(state, AuthState::Pairing { .. })),
		"first login did not enter pairing state"
	);

	drop(first_login);

	assert!(
		platform.pending_connect_dropped.load(std::sync::atomic::Ordering::SeqCst),
		"dropping the login future should drop the pending statement-store connect"
	);

	cancel_on_pairing(&platform, sso);
	let second_cx = CallContext::default();
	let mut second_login = Box::pin(host.request_login(&second_cx, request));
	let second = match second_login.as_mut().poll(&mut task_cx) {
		Poll::Ready(result) => result.expect("second login should complete after cancellation"),
		Poll::Pending => panic!("second login stayed pending behind stale single-flight state"),
	};
	assert_eq!(second, HostRequestLoginResponse::V1(api::HostRequestLoginResponse::Rejected));
}

#[test]
fn request_login_does_not_restore_persisted_session_before_pairing() {
	let stored = session_info();
	let platform = Arc::new(StubPlatform {
		session_blob: Some(crate::host_logic::session::encode_persisted_session(&stored)),
		..Default::default()
	});
	let (host, pairing) =
		ProductRuntimeHost::new_compat_with_pairing(platform.clone(), Spawner::for_test());
	let sso = pairing.session().clone();
	let host = Arc::new(host);
	cancel_on_pairing(&platform, sso);
	let cx = CallContext::default();
	let request = HostRequestLoginRequest::V1(api::HostRequestLoginRequest { reason: None });
	let response = futures::executor::block_on(host.request_login(&cx, request)).unwrap();

	assert_eq!(response, HostRequestLoginResponse::V1(api::HostRequestLoginResponse::Rejected));
	assert!(host.test_session_state().current().is_none());
}

#[test]
fn request_login_ignores_corrupt_persisted_session_before_pairing() {
	let session_clears = Arc::new(Mutex::new(0));
	let platform = Arc::new(StubPlatform {
		session_blob: Some(vec![0xff]),
		session_clears: session_clears.clone(),
		..Default::default()
	});
	let (host, pairing) =
		ProductRuntimeHost::new_compat_with_pairing(platform.clone(), Spawner::for_test());
	let sso = pairing.session().clone();
	let host = Arc::new(host);
	cancel_on_pairing(&platform, sso);
	let cx = CallContext::default();
	let request = HostRequestLoginRequest::V1(api::HostRequestLoginRequest { reason: None });
	let response = futures::executor::block_on(host.request_login(&cx, request)).unwrap();

	assert_eq!(response, HostRequestLoginResponse::V1(api::HostRequestLoginResponse::Rejected));
	assert!(host.test_session_state().current().is_none());
	assert_eq!(*session_clears.lock().unwrap(), 0);
}

#[test]
fn request_login_ignores_session_store_failure_before_pairing() {
	let platform =
		Arc::new(StubPlatform { session_error: Some("storage failed"), ..Default::default() });
	let (host, pairing) =
		ProductRuntimeHost::new_compat_with_pairing(platform.clone(), Spawner::for_test());
	let sso = pairing.session().clone();
	let host = Arc::new(host);
	cancel_on_pairing(&platform, sso);
	let cx = CallContext::default();
	let request = HostRequestLoginRequest::V1(api::HostRequestLoginRequest { reason: None });
	let response = futures::executor::block_on(host.request_login(&cx, request)).unwrap();

	assert_eq!(response, HostRequestLoginResponse::V1(api::HostRequestLoginResponse::Rejected));
	assert!(host.test_session_state().current().is_none());
}

#[test]
fn request_login_returns_already_connected_when_session_exists() {
	let host = ProductRuntimeHost::new_compat(stub_platform(), Spawner::for_test());
	host.test_session_state().set_session(session_info());
	let cx = CallContext::default();
	let request = HostRequestLoginRequest::V1(api::HostRequestLoginRequest { reason: None });
	let response = futures::executor::block_on(host.request_login(&cx, request)).unwrap();
	assert_eq!(
		response,
		HostRequestLoginResponse::V1(api::HostRequestLoginResponse::AlreadyConnected)
	);
}
