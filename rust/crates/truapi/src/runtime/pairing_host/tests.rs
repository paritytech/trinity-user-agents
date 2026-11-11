//! Session, auto-signing and session-store tests driven through the pairing host.

use crate::{
	host_logic::statement_store::SUBMIT_STATEMENT_METHOD,
	platform::{AuthState, CoreStorageKey},
	runtime::{
		ProductRuntimeHost,
		allowances::AllowanceResource,
		authority::{Allowance, AutoSigningKey, StatementStoreAllowanceKey},
		tests::{
			auto_signing_test_platform, cache_test_product_subtree, install_pairing_session,
			request_auto_signing, withdrawn_requests,
		},
		*,
	},
	test_support::*,
};
use std::sync::Mutex;
use truapi::{
	api::{Account, Signing},
	versioned::{
		account::{
			HostAccountConnectionStatusSubscribeItem, HostAccountRingVrfSignError,
			HostAccountRingVrfSignRequest, HostAccountRingVrfSignResponse,
		},
		signing::HostSignRawRequest,
	},
};

/// The phone may already be prompting for a request this host has published.
/// Withdrawing the call has to reach it there, or the person is left
/// approving a request nobody is waiting for.
#[test]
fn a_withdrawn_request_already_published_is_cancelled_on_the_phone() {
	let session = sso_session_info();
	let platform = Arc::new(StubPlatform {
		sign_raw_confirmed: true,
		rpc_responses: vec![
			subscribe_ack_frame("truapi:1", "own-sub-withdrawn"),
			subscribe_ack_frame("truapi:2", "peer-sub-withdrawn"),
			r#"{"jsonrpc":"2.0","id":"truapi:3","result":{"status":"new"}}"#.to_string(),
		],
		..Default::default()
	});
	let (host_config, _) = runtime_config("myapp.dot");
	let product = ProductContext::new(ProductId::for_test("myapp.dot"));
	let services = RuntimeServices::new(
		platform.clone(),
		host_config.host.host_info.clone(),
		host_config.people_chain_genesis_hash,
		host_config.bulletin_chain_genesis_hash,
		host_config.asset_hub_chain_genesis_hash,
		Spawner::for_test(),
	);
	let pairing_host = PairingHost::new(services.clone(), host_config);
	let adapters = crate::host_core::ConnectionAdapters::from_services(&services);
	let host = ProductRuntimeHost::from_services(
		services,
		adapters,
		pairing_host.accounts().clone(),
		pairing_host.session().clone(),
		product,
	);
	install_pairing_session(&host, session.clone());
	let cancel = truapi::CancellationToken::default();
	let cx = CallContext::with_parts("sign-raw-withdrawn".to_string(), cancel.clone());
	let request = HostSignRawRequest::V1(v01::HostSignRawRequest {
		account: account_id("myapp.dot", 0),
		payload: raw_payload(),
	});
	let call = std::thread::spawn(move || {
		futures::executor::block_on(host.sign_raw(&cx, request)).unwrap_err()
	});
	let published = submitted_remote_message(&platform, &session).message_id;
	wait_until(
		|| pairing_host.session().newest_request_for_tests().as_deref() == Some(&published),
		"the request was not published",
	);

	cancel.cancel();
	call.join().expect("sign_raw thread panicked");

	let withdrawn = || withdrawn_requests(&platform, &session);
	wait_until(|| !withdrawn().is_empty(), "no request was withdrawn");
	assert_eq!(withdrawn(), vec![published]);
}

/// A host that times out has not been asked to stop, so the phone keeps the
/// request: a slow allocation it finishes stays available to the next call.
#[test]
fn a_request_that_times_out_is_not_withdrawn_from_the_phone() {
	let session = sso_session_info();
	let platform = Arc::new(StubPlatform {
		sign_raw_confirmed: true,
		rpc_responses: vec![
			subscribe_ack_frame("truapi:1", "own-sub-timeout"),
			subscribe_ack_frame("truapi:2", "peer-sub-timeout"),
			r#"{"jsonrpc":"2.0","id":"truapi:3","result":{"status":"new"}}"#.to_string(),
		],
		..Default::default()
	});
	let (host_config, _) = runtime_config("myapp.dot");
	let product = ProductContext::new(ProductId::for_test("myapp.dot"));
	let services = RuntimeServices::new(
		platform.clone(),
		host_config.host.host_info.clone(),
		host_config.people_chain_genesis_hash,
		host_config.bulletin_chain_genesis_hash,
		host_config.asset_hub_chain_genesis_hash,
		Spawner::for_test(),
	);
	let pairing_host = PairingHost::new(services.clone(), host_config);
	let adapters = crate::host_core::ConnectionAdapters::from_services(&services);
	let host = ProductRuntimeHost::from_services(
		services,
		adapters,
		pairing_host.accounts().clone(),
		pairing_host.session().clone(),
		product,
	);
	install_pairing_session(&host, session.clone());
	let mut cx = CallContext::with_request_id("sign-raw-timeout".to_string());
	cx.set_timeout(std::time::Duration::from_millis(50));
	let request = HostSignRawRequest::V1(v01::HostSignRawRequest {
		account: account_id("myapp.dot", 0),
		payload: raw_payload(),
	});

	futures::executor::block_on(host.sign_raw(&cx, request)).unwrap_err();

	let published = submitted_remote_message(&platform, &session).message_id;
	assert_eq!(pairing_host.session().newest_request_for_tests(), Some(published));
}

/// The store keeps one statement per channel, so a `Cancel` replaces the
/// newest request this host published. Sent for an older one it would
/// replace a request the product is still waiting on instead.
#[test]
fn a_withdrawn_request_with_a_newer_one_behind_it_sends_no_cancel() {
	let session = sso_session_info();
	let answers = |method: &'static str, result: &str, times: usize| {
		std::iter::repeat_n((method, result.to_string()), times)
	};
	let platform = Arc::new(StubPlatform {
		sign_raw_confirmed: true,
		rpc_method_responses: answers("statement_subscribeStatement", r#""sub""#, 4)
			.chain(answers(SUBMIT_STATEMENT_METHOD, r#"{"status":"new"}"#, 4))
			.chain(answers("statement_unsubscribeStatement", "true", 4))
			.collect(),
		..Default::default()
	});
	let (host_config, _) = runtime_config("myapp.dot");
	let product = ProductContext::new(ProductId::for_test("myapp.dot"));
	let services = RuntimeServices::new(
		platform.clone(),
		host_config.host.host_info.clone(),
		host_config.people_chain_genesis_hash,
		host_config.bulletin_chain_genesis_hash,
		host_config.asset_hub_chain_genesis_hash,
		Spawner::for_test(),
	);
	let pairing_host = PairingHost::new(services.clone(), host_config);
	let adapters = crate::host_core::ConnectionAdapters::from_services(&services);
	let host = Arc::new(ProductRuntimeHost::from_services(
		services,
		adapters,
		pairing_host.accounts().clone(),
		pairing_host.session().clone(),
		product,
	));
	install_pairing_session(&host, session.clone());
	let start = |request_id: &str| {
		let host = host.clone();
		let cancel = truapi::CancellationToken::default();
		let cx = CallContext::with_parts(request_id.to_string(), cancel.clone());
		let request = HostSignRawRequest::V1(v01::HostSignRawRequest {
			account: account_id("myapp.dot", 0),
			payload: raw_payload(),
		});
		let call = std::thread::spawn(move || {
			let _ = futures::executor::block_on(host.sign_raw(&cx, request));
		});
		(cancel, call)
	};
	let wait_published = |count: usize| {
		wait_until(
			|| {
				let published = submitted_remote_messages(&platform, &session);
				published.len() == count &&
					pairing_host.session().newest_request_for_tests().as_deref() ==
						published.last().map(|message| message.message_id.as_str())
			},
			"the request was not published",
		);
		submitted_remote_messages(&platform, &session)
			.last()
			.expect("a published request")
			.message_id
			.clone()
	};
	let (first_cancel, first_call) = start("sign-raw-first");
	wait_published(1);
	let (second_cancel, second_call) = start("sign-raw-second");
	let second = wait_published(2);

	first_cancel.cancel();
	first_call.join().expect("first sign_raw thread panicked");
	second_cancel.cancel();
	second_call.join().expect("second sign_raw thread panicked");

	let withdrawn = || withdrawn_requests(&platform, &session);
	wait_until(|| !withdrawn().is_empty(), "no request was withdrawn");
	assert_eq!(withdrawn(), vec![second]);
}

#[test]
fn idle_peer_disconnect_monitor_clears_session_store_and_broadcasts() {
	let session = sso_session_info();
	let platform = Arc::new(StubPlatform {
		rpc_responses: sso_peer_disconnect_monitor_responses(&session),
		..Default::default()
	});
	let (host_config, product) = runtime_config("myapp.dot");
	let (host, pairing_host) = ProductRuntimeHost::new_pairing_for_tests(
		platform.clone(),
		host_config,
		product,
		Spawner::for_test(),
	);
	install_pairing_session(&host, session.clone());
	let mut statuses = host.test_session_state().subscribe();
	assert_eq!(
		futures::executor::block_on(statuses.next()).unwrap(),
		HostAccountConnectionStatusSubscribeItem::V1(
			v01::HostAccountConnectionStatusSubscribeItem::Connected
		)
	);

	pairing_host.session().start_remote_monitor_for_current_session();

	let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
	let disconnected = loop {
		if let Some(item) = statuses.next().now_or_never() {
			break item.expect("status stream ended");
		}
		assert!(
			std::time::Instant::now() < deadline,
			"peer disconnect monitor did not emit Disconnected"
		);
		std::thread::sleep(std::time::Duration::from_millis(5));
	};

	assert!(host.test_session_state().current().is_none());
	assert_eq!(*platform.session_clears.lock().expect("session clear counter mutex poisoned"), 1);
	assert_eq!(
		disconnected,
		HostAccountConnectionStatusSubscribeItem::V1(
			v01::HostAccountConnectionStatusSubscribeItem::Disconnected
		)
	);
}

#[test]
fn auto_signing_ring_vrf_requires_registration_and_signs_locally() {
	use verifiable::{GenerateVerifiable, ring::bandersnatch::BandersnatchVrfVerifiable};

	let session = sso_session_info();
	let platform = Arc::new(StubPlatform::default());
	let (host, pairing_host) = ProductRuntimeHost::new_pairing_for_tests(
		platform.clone(),
		ProductRuntimeHost::compat_host_config(),
		ProductContext::new(ProductId::for_test("myapp.dot")),
		Spawner::for_test(),
	);
	install_pairing_session(&host, session.clone());

	let root =
		crate::host_logic::product_account::derive_root_keypair_from_entropy(&[0xAB; 16]).unwrap();
	let subtree = crate::host_logic::product_account::derive_product_subtree_keypair(
		&root,
		&ProductId::for_test("myapp.dot"),
	)
	.unwrap();
	let domain = crate::host_logic::product_account::derive_ring_vrf_domain_entropy(
		&[0xAB; 16],
		&ProductId::for_test("myapp.dot"),
	)
	.unwrap();
	futures::executor::block_on(remember_auto_key(
		&pairing_host,
		&session,
		pairing_host.accounts().grant_revision(),
		"myapp.dot",
		subtree.public.to_bytes(),
		subtree.secret.to_bytes(),
		domain,
	))
	.unwrap();

	let handle = account_id("myapp.dot", 7);
	let request = HostAccountRingVrfSignRequest::V1(v01::HostAccountRingVrfSignRequest {
		key_handle: handle.clone(),
		message: b"registered keys only".to_vec(),
	});
	let error = futures::executor::block_on(host.ring_vrf_sign(&CallContext::default(), request))
		.unwrap_err();
	assert!(matches!(
		error,
		CallError::Domain(HostAccountRingVrfSignError::V1(
			v01::HostAccountRingVrfSignError::KeyNotRegistered
		))
	));

	let ring = ring_location_fixture();
	let entropy = crate::host_logic::product_account::derive_ring_vrf_entropy_from_domain(
		&domain,
		&handle.derivation_index,
	);
	let public_key = futures::executor::block_on(crate::runtime::vrf::load())
		.expect("verifiable is linked")
		.member(&entropy)
		.unwrap();
	futures::executor::block_on(register_vrf_key(
		&pairing_host,
		&session,
		handle.clone(),
		ring,
		public_key,
	))
	.unwrap();

	let message = b"registered keys only".to_vec();
	let response = futures::executor::block_on(host.ring_vrf_sign(
		&CallContext::default(),
		HostAccountRingVrfSignRequest::V1(v01::HostAccountRingVrfSignRequest {
			key_handle: handle,
			message: message.clone(),
		}),
	))
	.unwrap();
	let HostAccountRingVrfSignResponse::V1(signature) = response;
	let signature: [u8; 64] = signature.try_into().expect("fixed-width ring-VRF signature");
	assert!(BandersnatchVrfVerifiable::verify_signature(&signature, &message, &public_key));
	assert!(
		platform.sent_rpc.lock().expect("sent RPC mutex poisoned").is_empty(),
		"registered AutoSigning ring-VRF use stays local"
	);

	let mismatched_handle = account_id("myapp.dot", 8);
	futures::executor::block_on(register_vrf_key(
		&pairing_host,
		&session,
		mismatched_handle.clone(),
		ring_location_fixture(),
		[0xFF; 32],
	))
	.unwrap();
	let error = futures::executor::block_on(host.ring_vrf_sign(
		&CallContext::default(),
		HostAccountRingVrfSignRequest::V1(v01::HostAccountRingVrfSignRequest {
			key_handle: mismatched_handle,
			message: b"reject mismatched registry state".to_vec(),
		}),
	))
	.unwrap_err();
	assert!(matches!(
		error,
		CallError::Domain(HostAccountRingVrfSignError::V1(
			v01::HostAccountRingVrfSignError::Unknown { reason }
		)) if reason.contains("does not match the AutoSigning capability")
	));
}

#[test]
fn auto_signing_logout_reset_clears_cached_and_persisted_capability() {
	let session = sso_session_info();
	let platform = auto_signing_test_platform(&session, "auto-logout");
	let (host_config, product) = runtime_config("myapp.dot");
	let (host, pairing_host) = ProductRuntimeHost::new_pairing_for_tests(
		platform.clone(),
		host_config,
		product,
		Spawner::for_test(),
	);
	install_pairing_session(&host, session.clone());
	request_auto_signing(&host, "auto-logout");
	assert!(
		platform
			.local_storage
			.lock()
			.expect("local storage mutex poisoned")
			.contains_key(&core_storage_test_key(CoreStorageKey::AutoSigningKeys))
	);

	futures::executor::block_on(pairing_host.logout_and_reset_pairing()).unwrap();

	assert!(
		!platform
			.local_storage
			.lock()
			.expect("local storage mutex poisoned")
			.contains_key(&core_storage_test_key(CoreStorageKey::AutoSigningKeys))
	);
	assert!(
		!futures::executor::block_on(has_auto_key(&pairing_host, &session, "myapp.dot"))
			.expect("AutoSigning storage remains readable"),
		"logout must evict the in-memory AutoSigning capability"
	);
}

#[test]
fn stale_secret_allocations_cannot_persist_after_reset_and_same_owner_reactivation() {
	let session = sso_session_info();
	let platform = Arc::new(StubPlatform::default());
	let (host_config, product) = runtime_config("myapp.dot");
	let (host, pairing_host) = ProductRuntimeHost::new_pairing_for_tests(
		platform.clone(),
		host_config,
		product,
		Spawner::for_test(),
	);
	install_pairing_session(&host, session.clone());
	let stale_revision = pairing_host.accounts().grant_revision();
	let root =
		crate::host_logic::product_account::derive_root_keypair_from_entropy(&[0xAB; 16]).unwrap();
	let subtree = crate::host_logic::product_account::derive_product_subtree_keypair(
		&root,
		&ProductId::for_test("myapp.dot"),
	)
	.unwrap();

	futures::executor::block_on(pairing_host.logout_and_reset_pairing()).unwrap();
	futures::executor::block_on(
		pairing_host.session().set_connected_session_for_tests(session.clone()),
	);
	let auto_signing_error = futures::executor::block_on(remember_auto_key(
		&pairing_host,
		&session,
		stale_revision,
		"myapp.dot",
		subtree.public.to_bytes(),
		subtree.secret.to_bytes(),
		[0x42; 32],
	))
	.expect_err("the old AutoSigning allocation completion must be rejected");
	let statement_store_result = futures::executor::block_on(retain_allowance_key(
		&pairing_host,
		AllowanceResource::StatementStore,
		&session,
		stale_revision,
		&ProductId::for_test("myapp.dot"),
		subtree.secret.to_bytes().to_vec(),
	));
	let Err(statement_store_error) = statement_store_result else {
		panic!("the old statement-store allocation completion must be rejected");
	};
	let bulletin_result = futures::executor::block_on(retain_allowance_key(
		&pairing_host,
		AllowanceResource::Bulletin,
		&session,
		stale_revision,
		&ProductId::for_test("myapp.dot"),
		subtree.secret.to_bytes().to_vec(),
	));
	let Err(bulletin_error) = bulletin_result else {
		panic!("the old Bulletin allocation completion must be rejected");
	};

	assert!(matches!(auto_signing_error, AuthorityError::Disconnected));
	assert!(matches!(statement_store_error, AuthorityError::Disconnected));
	assert!(matches!(bulletin_error, AuthorityError::Disconnected));
	assert!(
		!platform
			.local_storage
			.lock()
			.expect("local storage mutex poisoned")
			.contains_key(&core_storage_test_key(CoreStorageKey::AutoSigningKeys)),
		"the stale allocation must not restore durable AutoSigning authority"
	);
	assert!(
		platform.local_storage.lock().expect("local storage mutex poisoned").is_empty(),
		"stale allowance completions must not restore durable slot secrets"
	);
	assert!(
		!futures::executor::block_on(has_auto_key(&pairing_host, &session, "myapp.dot"))
			.expect("AutoSigning storage remains readable"),
		"the stale allocation must not restore cached AutoSigning authority"
	);
}

#[test]
fn product_clear_preserves_other_capabilities_and_fences_stale_work() {
	let session = sso_session_info();
	let platform = Arc::new(StubPlatform::default());
	let (host_config, product) = runtime_config("myapp.dot");
	let (host, pairing_host) = ProductRuntimeHost::new_pairing_for_tests(
		platform.clone(),
		host_config,
		product,
		Spawner::for_test(),
	);
	install_pairing_session(&host, session.clone());
	cache_test_product_subtree(&host, &session, "other.dot");
	let stale_revision = pairing_host.accounts().grant_revision();
	let root =
		crate::host_logic::product_account::derive_root_keypair_from_entropy(&[0xAB; 16]).unwrap();
	let first = crate::host_logic::product_account::derive_product_subtree_keypair(
		&root,
		&ProductId::for_test("myapp.dot"),
	)
	.unwrap();
	let other = crate::host_logic::product_account::derive_product_subtree_keypair(
		&root,
		&ProductId::for_test("other.dot"),
	)
	.unwrap();

	for (product_id, subtree) in
		[(ProductId::for_test("myapp.dot"), &first), (ProductId::for_test("other.dot"), &other)]
	{
		futures::executor::block_on(remember_auto_key(
			&pairing_host,
			&session,
			stale_revision,
			product_id.as_str(),
			subtree.public.to_bytes(),
			subtree.secret.to_bytes(),
			[0x42; 32],
		))
		.unwrap();
		futures::executor::block_on(retain_allowance_key(
			&pairing_host,
			AllowanceResource::StatementStore,
			&session,
			stale_revision,
			&product_id,
			subtree.secret.to_bytes().to_vec(),
		))
		.unwrap();
		futures::executor::block_on(retain_allowance_key(
			&pairing_host,
			AllowanceResource::Bulletin,
			&session,
			stale_revision,
			&product_id,
			subtree.secret.to_bytes().to_vec(),
		))
		.unwrap();
	}

	futures::executor::block_on(
		pairing_host.clear_product_state(&ProductId::for_test("myapp.dot")),
	)
	.unwrap();
	let current_revision = pairing_host.accounts().grant_revision();
	assert_ne!(current_revision, stale_revision);
	assert_eq!(cache_sizes(&pairing_host), (1, 1, 1, 1));
	assert!(
		!futures::executor::block_on(has_auto_key(&pairing_host, &session, "myapp.dot")).unwrap()
	);
	assert!(
		futures::executor::block_on(has_auto_key(&pairing_host, &session, "other.dot")).unwrap()
	);
	assert!(
		futures::executor::block_on(cached_allowance_key(
			&pairing_host,
			AllowanceResource::StatementStore,
			&session,
			current_revision,
			&ProductId::for_test("myapp.dot"),
		))
		.unwrap()
		.is_none()
	);
	assert!(
		futures::executor::block_on(cached_allowance_key(
			&pairing_host,
			AllowanceResource::StatementStore,
			&session,
			current_revision,
			&ProductId::for_test("other.dot"),
		))
		.unwrap()
		.is_some()
	);
	assert!(
		futures::executor::block_on(cached_allowance_key(
			&pairing_host,
			AllowanceResource::Bulletin,
			&session,
			current_revision,
			&ProductId::for_test("myapp.dot"),
		))
		.unwrap()
		.is_none()
	);
	assert!(
		futures::executor::block_on(cached_allowance_key(
			&pairing_host,
			AllowanceResource::Bulletin,
			&session,
			current_revision,
			&ProductId::for_test("other.dot"),
		))
		.unwrap()
		.is_some()
	);

	assert!(matches!(
		futures::executor::block_on(remember_auto_key(
			&pairing_host,
			&session,
			stale_revision,
			"myapp.dot",
			first.public.to_bytes(),
			first.secret.to_bytes(),
			[0x42; 32],
		)),
		Err(AuthorityError::Disconnected)
	));
	assert!(matches!(
		futures::executor::block_on(retain_allowance_key(
			&pairing_host,
			AllowanceResource::StatementStore,
			&session,
			stale_revision,
			&ProductId::for_test("myapp.dot"),
			first.secret.to_bytes().to_vec(),
		)),
		Err(AuthorityError::Disconnected)
	));
	assert_eq!(cache_sizes(&pairing_host), (1, 1, 1, 1));
	assert_eq!(
		platform.local_storage.lock().expect("local storage mutex poisoned").len(),
		2,
		"the aggregate AutoSigning and active-session allowance blobs remain for other.dot"
	);
}

#[test]
fn reset_session_state_clears_all_capabilities_without_peer_traffic() {
	let session = sso_session_info();
	let platform = Arc::new(StubPlatform::default());
	let (host_config, product) = runtime_config("myapp.dot");
	let (host, pairing_host) = ProductRuntimeHost::new_pairing_for_tests(
		platform.clone(),
		host_config,
		product,
		Spawner::for_test(),
	);
	install_pairing_session(&host, session.clone());
	let grant_revision = pairing_host.accounts().grant_revision();
	let root =
		crate::host_logic::product_account::derive_root_keypair_from_entropy(&[0xAB; 16]).unwrap();
	let subtree = crate::host_logic::product_account::derive_product_subtree_keypair(
		&root,
		&ProductId::for_test("myapp.dot"),
	)
	.unwrap();
	futures::executor::block_on(remember_auto_key(
		&pairing_host,
		&session,
		grant_revision,
		"myapp.dot",
		subtree.public.to_bytes(),
		subtree.secret.to_bytes(),
		[0x42; 32],
	))
	.unwrap();
	futures::executor::block_on(retain_allowance_key(
		&pairing_host,
		AllowanceResource::StatementStore,
		&session,
		grant_revision,
		&ProductId::for_test("myapp.dot"),
		subtree.secret.to_bytes().to_vec(),
	))
	.unwrap();
	futures::executor::block_on(retain_allowance_key(
		&pairing_host,
		AllowanceResource::Bulletin,
		&session,
		grant_revision,
		&ProductId::for_test("myapp.dot"),
		subtree.secret.to_bytes().to_vec(),
	))
	.unwrap();
	assert_eq!(cache_sizes(&pairing_host), (1, 1, 1, 1));

	futures::executor::block_on(pairing_host.session().reset_session_state());

	assert!(pairing_host.session().session_state().current().is_none());
	assert_eq!(cache_sizes(&pairing_host), (0, 0, 0, 0));
	assert!(platform.local_storage.lock().expect("local storage mutex poisoned").is_empty());
	assert!(
		platform.sent_rpc.lock().expect("sent RPC mutex poisoned").is_empty(),
		"canonical reset must not submit a duplicate peer-disconnect statement"
	);
}

#[test]
fn identity_replacement_clears_all_stale_wallet_capabilities() {
	let session = sso_session_info();
	let platform = auto_signing_test_platform(&session, "auto-replace");
	let (host_config, product) = runtime_config("myapp.dot");
	let (host, pairing_host) = ProductRuntimeHost::new_pairing_for_tests(
		platform.clone(),
		host_config,
		product,
		Spawner::for_test(),
	);
	install_pairing_session(&host, session.clone());
	request_auto_signing(&host, "auto-replace");
	let grant_revision = pairing_host.accounts().grant_revision();
	let root =
		crate::host_logic::product_account::derive_root_keypair_from_entropy(&[0xAB; 16]).unwrap();
	let subtree = crate::host_logic::product_account::derive_product_subtree_keypair(
		&root,
		&ProductId::for_test("myapp.dot"),
	)
	.unwrap();
	futures::executor::block_on(retain_allowance_key(
		&pairing_host,
		AllowanceResource::StatementStore,
		&session,
		grant_revision,
		&ProductId::for_test("myapp.dot"),
		subtree.secret.to_bytes().to_vec(),
	))
	.unwrap();
	futures::executor::block_on(retain_allowance_key(
		&pairing_host,
		AllowanceResource::Bulletin,
		&session,
		grant_revision,
		&ProductId::for_test("myapp.dot"),
		subtree.secret.to_bytes().to_vec(),
	))
	.unwrap();

	let mut replacement = session;
	replacement.public_key = [0x44; 32];
	replacement.sso.as_mut().expect("fixture has SSO identity").identity_account_id = [0x55; 32];
	futures::executor::block_on(
		pairing_host.session().set_connected_session_for_tests(replacement.clone()),
	);

	assert_eq!(host.test_session_state().current(), Some(replacement.clone()));
	assert!(
		!platform
			.local_storage
			.lock()
			.expect("local storage mutex poisoned")
			.contains_key(&core_storage_test_key(CoreStorageKey::AutoSigningKeys))
	);
	assert!(
		platform.local_storage.lock().expect("local storage mutex poisoned").is_empty(),
		"a replacement wallet must not leave the prior session's durable allowance secrets"
	);
	assert!(
		!futures::executor::block_on(has_auto_key(&pairing_host, &replacement, "myapp.dot"))
			.expect("AutoSigning storage remains readable"),
		"a newly paired wallet must not reuse the previous wallet's cached capability"
	);
}

#[test]
fn auto_signing_restored_different_wallet_rejects_persisted_capability() {
	let session = sso_session_info();
	let platform = auto_signing_test_platform(&session, "auto-other-wallet");
	let original =
		ProductRuntimeHost::new(platform.clone(), runtime_config("myapp.dot"), Spawner::for_test());
	install_pairing_session(&original, session.clone());
	request_auto_signing(&original, "auto-other-wallet");

	let mut replacement = session;
	replacement.public_key = [0x66; 32];
	replacement.sso.as_mut().expect("fixture has SSO identity").identity_account_id = [0x77; 32];
	let (host_config, product) = runtime_config("myapp.dot");
	let (restored, pairing_host) = ProductRuntimeHost::new_pairing_for_tests(
		platform.clone(),
		host_config,
		product,
		Spawner::for_test(),
	);
	install_pairing_session(&restored, replacement.clone());

	assert!(
		!futures::executor::block_on(has_auto_key(&pairing_host, &replacement, "myapp.dot"))
			.expect("AutoSigning storage remains readable"),
		"a different restored wallet must not use a prior wallet's capability"
	);
	assert!(
		!platform
			.local_storage
			.lock()
			.expect("local storage mutex poisoned")
			.contains_key(&core_storage_test_key(CoreStorageKey::AutoSigningKeys))
	);
}

#[test]
fn store_notification_during_external_activation_restores_persisted_session() {
	let persisted = sso_session_info();
	let platform = Arc::new(StubPlatform {
		session_blob: Some(crate::host_logic::session::encode_persisted_session(&persisted)),
		..Default::default()
	});
	let (host, pairing_host) =
		ProductRuntimeHost::new_compat_with_pairing(platform, Spawner::for_test());
	pairing_host.session().clone().start_session_store_sync(Spawner::for_test());
	wait_until(
		|| host.test_session_state().current() == Some(persisted.clone()),
		"initial persisted session was not restored",
	);

	let mut stale_external = persisted.clone();
	stale_external.public_key = [0x44; 32];
	let stale_blob = crate::host_logic::session::encode_persisted_session(&stale_external);
	let (activation_entered, resume_activation) =
		pairing_host.session().pause_external_session_activation_for_tests();
	let activation = std::thread::spawn({
		let pairing_host = pairing_host.clone();
		move || {
			futures::executor::block_on(
				pairing_host.session().activate_external_session(&stale_blob),
			)
		}
	});
	futures::executor::block_on(activation_entered)
		.expect("external activation reached the installation fence");

	pairing_host.session().notify_session_store_changed();
	resume_activation.send(()).expect("external activation remains in flight");
	activation
		.join()
		.expect("external activation thread panicked")
		.expect("superseded external activation completes");

	wait_until(
		|| host.test_session_state().current() == Some(persisted.clone()),
		"store reconciliation did not preserve the persisted replacement",
	);
	assert_eq!(host.test_session_state().current(), Some(persisted));
}

#[test]
fn disconnect_during_external_activation_prevents_stale_reinstallation() {
	let (host, pairing_host) = ProductRuntimeHost::new_compat_with_pairing(
		Arc::new(StubPlatform::default()),
		Spawner::for_test(),
	);
	let stale_blob = crate::host_logic::session::encode_persisted_session(&sso_session_info());
	let (activation_entered, resume_activation) =
		pairing_host.session().pause_external_session_activation_for_tests();
	let activation = std::thread::spawn({
		let pairing_host = pairing_host.clone();
		move || {
			futures::executor::block_on(
				pairing_host.session().activate_external_session(&stale_blob),
			)
		}
	});
	futures::executor::block_on(activation_entered)
		.expect("external activation reached the installation fence");

	futures::executor::block_on(host.disconnect());
	resume_activation.send(()).expect("external activation remains in flight");
	activation
		.join()
		.expect("external activation thread panicked")
		.expect("superseded external activation completes");

	assert!(
		host.test_session_state().current().is_none(),
		"disconnect must win over the earlier external activation"
	);
}

/// A product reset concerns that product's grants, not which wallet is
/// paired, so it keeps a session that is being activated.
#[test]
fn a_product_reset_during_activation_keeps_the_new_session() {
	let (host, pairing_host) = ProductRuntimeHost::new_compat_with_pairing(
		Arc::new(StubPlatform::default()),
		Spawner::for_test(),
	);
	let session = sso_session_info();
	let blob = crate::host_logic::session::encode_persisted_session(&session);
	let (activation_entered, resume_activation) =
		pairing_host.session().pause_external_session_activation_for_tests();
	let activation = std::thread::spawn({
		let pairing_host = pairing_host.clone();
		move || futures::executor::block_on(pairing_host.session().activate_external_session(&blob))
	});
	futures::executor::block_on(activation_entered)
		.expect("external activation reached the installation fence");

	futures::executor::block_on(
		pairing_host.clear_product_state(&ProductId::for_test("myapp.dot")),
	)
	.unwrap();
	resume_activation.send(()).expect("external activation remains in flight");
	activation
		.join()
		.expect("external activation thread panicked")
		.expect("external activation completes");

	assert_eq!(host.test_session_state().current(), Some(session));
}

#[test]
fn session_store_sync_restores_valid_blob_from_tick() {
	let stored = sso_session_info();
	let platform = Arc::new(StubPlatform {
		session_blob: Some(crate::host_logic::session::encode_persisted_session(&stored)),
		..Default::default()
	});
	let (host, pairing_host) =
		ProductRuntimeHost::new_compat_with_pairing(platform.clone(), Spawner::for_test());

	pairing_host.session().clone().start_session_store_sync(Spawner::for_test());
	wait_until(
		|| host.test_session_state().current() == Some(stored.clone()),
		"session store sync did not restore valid blob",
	);

	assert_eq!(host.test_session_state().current(), Some(stored.clone()));
	let expected_auth_states =
		vec![AuthState::Connected(Box::new(connected_session_ui_info(&stored)))];
	wait_until(
		|| {
			*platform.auth_states.lock().expect("auth state list mutex poisoned") ==
				expected_auth_states
		},
		"session store sync did not broadcast connected auth state",
	);
	assert_eq!(
		*platform.auth_states.lock().expect("auth state list mutex poisoned"),
		expected_auth_states
	);
}

#[test]
fn session_store_sync_announces_a_signed_out_boot() {
	let platform = Arc::new(StubPlatform::default());
	let (_host, pairing_host) =
		ProductRuntimeHost::new_compat_with_pairing(platform.clone(), Spawner::for_test());

	pairing_host.session().clone().start_session_store_sync(Spawner::for_test());

	wait_until(
		|| !platform.auth_states.lock().expect("auth state list mutex poisoned").is_empty(),
		"boot reconcile did not report the signed-out state",
	);
	assert_eq!(
		*platform.auth_states.lock().expect("auth state list mutex poisoned"),
		vec![AuthState::Disconnected]
	);
}

#[test]
fn session_store_sync_announces_a_restored_boot_once() {
	let stored = sso_session_info();
	let platform = Arc::new(StubPlatform {
		session_blob: Some(crate::host_logic::session::encode_persisted_session(&stored)),
		..Default::default()
	});
	let (_host, pairing_host) =
		ProductRuntimeHost::new_compat_with_pairing(platform.clone(), Spawner::for_test());

	pairing_host.session().clone().start_session_store_sync(Spawner::for_test());

	wait_until(
		|| !platform.auth_states.lock().expect("auth state list mutex poisoned").is_empty(),
		"boot reconcile did not report the restored session",
	);
	futures::executor::block_on(pairing_host.session().activate_stored_session())
		.expect("valid stored session activates");
	assert_eq!(
		*platform.auth_states.lock().expect("auth state list mutex poisoned"),
		vec![AuthState::Connected(Box::new(connected_session_ui_info(&stored)))]
	);
}

#[test]
fn session_store_sync_stays_silent_on_an_unchanged_tick() {
	let stored = sso_session_info();
	let platform = Arc::new(StubPlatform {
		session_blob: Some(crate::host_logic::session::encode_persisted_session(&stored)),
		..Default::default()
	});
	let (_host, pairing_host) =
		ProductRuntimeHost::new_compat_with_pairing(platform.clone(), Spawner::for_test());

	pairing_host.session().clone().start_session_store_sync(Spawner::for_test());
	wait_until(
		|| !platform.auth_states.lock().expect("auth state list mutex poisoned").is_empty(),
		"boot reconcile did not report the restored session",
	);

	pairing_host.session().notify_session_store_changed();
	wait_until(
		|| pairing_host.session().session_store_change_ticks_for_tests() == 1,
		"session store sync did not process the change tick",
	);

	// The store still holds the same session, so the tick is not a
	// transition and must not repeat the opening state.
	assert_eq!(
		*platform.auth_states.lock().expect("auth state list mutex poisoned"),
		vec![AuthState::Connected(Box::new(connected_session_ui_info(&stored)))]
	);
}

#[test]
fn session_store_sync_replaces_valid_blob_and_broadcasts_connected() {
	let mut replacement = sso_session_info();
	replacement.public_key = [0x44; 32];
	let (host, pairing_host) = ProductRuntimeHost::new_compat_with_pairing(
		Arc::new(StubPlatform {
			session_blob: Some(crate::host_logic::session::encode_persisted_session(&replacement)),
			..Default::default()
		}),
		Spawner::for_test(),
	);
	install_pairing_session(&host, sso_session_info());
	let mut statuses = host.test_session_state().subscribe();
	let _ = futures::executor::block_on(statuses.next());

	pairing_host.session().clone().start_session_store_sync(Spawner::for_test());

	assert_eq!(
		futures::executor::block_on(statuses.next()).unwrap(),
		HostAccountConnectionStatusSubscribeItem::V1(
			v01::HostAccountConnectionStatusSubscribeItem::Connected
		)
	);
	assert_eq!(host.test_session_state().current(), Some(replacement));
}

#[test]
fn session_store_sync_clears_invalid_blob() {
	let platform = Arc::new(StubPlatform { session_blob: Some(vec![0xff]), ..Default::default() });
	let (host, pairing_host) =
		ProductRuntimeHost::new_compat_with_pairing(platform.clone(), Spawner::for_test());
	install_pairing_session(&host, sso_session_info());

	pairing_host.session().clone().start_session_store_sync(Spawner::for_test());
	wait_until(
		|| host.test_session_state().current().is_none(),
		"session store sync did not clear invalid blob",
	);

	assert!(host.test_session_state().current().is_none());
	// `set_session` bypasses the auth state cell, so the clear is not a
	// transition; the boot tick's announcement is the only emission.
	wait_until(
		|| !platform.auth_states.lock().expect("auth state list mutex poisoned").is_empty(),
		"boot reconcile did not report the cleared session",
	);
	assert_eq!(
		*platform.auth_states.lock().expect("auth state list mutex poisoned"),
		vec![AuthState::Disconnected]
	);
}

#[test]
fn session_store_sync_clears_unreadable_blob() {
	let session_clears = Arc::new(Mutex::new(0));
	let (host, pairing_host) = ProductRuntimeHost::new_compat_with_pairing(
		Arc::new(StubPlatform {
			session_error: Some("storage unavailable"),
			session_clears: session_clears.clone(),
			..Default::default()
		}),
		Spawner::for_test(),
	);
	install_pairing_session(&host, sso_session_info());

	pairing_host.session().clone().start_session_store_sync(Spawner::for_test());
	wait_until(
		|| *session_clears.lock().unwrap() == 1,
		"session store sync did not clear unreadable blob",
	);

	assert!(host.test_session_state().current().is_none());
	assert_eq!(*session_clears.lock().unwrap(), 1);
}

/// A persistently failing read clears the backing store once at boot.
/// Further clears require explicit host notifications.
#[test]
fn session_store_sync_clears_once_on_initial_persistent_read_error() {
	let session_clears = Arc::new(Mutex::new(0));
	let (host, pairing_host) = ProductRuntimeHost::new_compat_with_pairing(
		Arc::new(StubPlatform {
			session_error: Some("storage unavailable"),
			session_clears: session_clears.clone(),
			..Default::default()
		}),
		Spawner::for_test(),
	);
	install_pairing_session(&host, sso_session_info());

	pairing_host.session().clone().start_session_store_sync(Spawner::for_test());

	wait_until(|| *session_clears.lock().unwrap() == 1, "clear_stored_session was never called");
	assert_eq!(*session_clears.lock().unwrap(), 1);
	assert!(host.test_session_state().current().is_none());
}

#[test]
fn disconnect_emits_disconnected_auth_state_after_store_sync_connected() {
	let stored = sso_session_info();
	let platform = Arc::new(StubPlatform {
		session_blob: Some(crate::host_logic::session::encode_persisted_session(&stored)),
		..Default::default()
	});
	let (host, pairing_host) =
		ProductRuntimeHost::new_compat_with_pairing(platform.clone(), Spawner::for_test());
	pairing_host.session().clone().start_session_store_sync(Spawner::for_test());
	wait_until(
		|| platform.auth_states.lock().expect("auth state list mutex poisoned").len() == 1,
		"session store sync did not emit connected auth state",
	);

	futures::executor::block_on(host.disconnect());

	assert_eq!(
		*platform.auth_states.lock().expect("auth state list mutex poisoned"),
		vec![
			AuthState::Connected(Box::new(connected_session_ui_info(&stored))),
			AuthState::Disconnected,
		]
	);
}

/// A pairing test host, whose wallet answered the Bulletin allowance in-page,
/// keeps the submission in the core once told to and serves it back, so a
/// product's submit-then-lookup round trip works without the chain.
#[cfg(feature = "test-host")]
#[test]
fn a_pairing_test_host_keeps_a_submitted_preimage_and_serves_it_back() {
	use crate::host_internal::bulletin::preimage_key;
	use truapi::{
		api::Preimage,
		versioned::preimage::{
			RemotePreimageLookupSubscribeItem, RemotePreimageLookupSubscribeRequest,
			RemotePreimageSubmitRequest, RemotePreimageSubmitResponse,
		},
	};

	let session = sso_session_info();
	let platform = Arc::new(StubPlatform::default());
	let (host_config, product) = runtime_config("myapp.dot");
	let (host, pairing_host) = ProductRuntimeHost::new_pairing_for_tests(
		platform,
		host_config,
		product,
		Spawner::for_test(),
	);
	install_pairing_session(&host, session.clone());
	let grant_revision = pairing_host.accounts().grant_revision();
	let allowance =
		crate::host_logic::product_account::derive_root_keypair_from_entropy(&[0x42; 32]).unwrap();
	futures::executor::block_on(retain_allowance_key(
		&pairing_host,
		AllowanceResource::Bulletin,
		&session,
		grant_revision,
		&ProductId::for_test("myapp.dot"),
		allowance.secret.to_bytes().to_vec(),
	))
	.expect("the wallet's allowance is cached");
	pairing_host.accounts().set_submit_preimages_locally(true);
	let value = b"pairing test host preimage".to_vec();
	let cx = CallContext::default();

	// No Bulletin client is configured, so reaching the chain would fail here.
	let response = futures::executor::block_on(Preimage::submit(
		&host,
		&cx,
		RemotePreimageSubmitRequest::V1(value.clone()),
	))
	.expect("the submission stays in the core");
	assert_eq!(response, RemotePreimageSubmitResponse::V1(preimage_key(&value).to_vec()));

	let mut lookup = futures::executor::block_on(host.lookup_subscribe(
		&cx,
		RemotePreimageLookupSubscribeRequest::V1(v01::RemotePreimageLookupSubscribeRequest {
			key: preimage_key(&value).to_vec(),
		}),
	));
	assert_eq!(
		futures::executor::block_on(lookup.next()).expect("a lookup item"),
		Ok(RemotePreimageLookupSubscribeItem::V1(v01::RemotePreimageLookupSubscribeItem {
			value: Some(value)
		}))
	);
}

/// The paired host's account holder keeps its copy of what the phone
/// registered, so the host's own provider list shows the phone's answer.
#[test]
fn a_paired_registration_records_the_phones_key_locally() {
	use crate::host_internal::sso_messages::{RemoteMessage, RemoteMessageData, Response, v1};

	let session = sso_session_info();
	let public_key = [7; 32];
	let platform = Arc::new(StubPlatform {
		sso_response_script: Some(sso_success_response_script(
			&session,
			RemoteMessage {
				message_id: "wallet-register".to_string(),
				data: RemoteMessageData::V1(v1::RemoteMessage::RegisterRingVrfKeyResponse(
					Response { responding_to: "register".to_string(), payload: Ok(public_key) },
				)),
			},
		)),
		..Default::default()
	});
	let (host_config, product) = runtime_config("myapp.dot");
	let (host, pairing) = ProductRuntimeHost::new_pairing_for_tests(
		platform,
		host_config,
		product,
		Spawner::for_test(),
	);
	install_pairing_session(&host, session);
	let authority_session = pairing.accounts().current_session().unwrap();

	let registered = futures::executor::block_on(pairing.accounts().register_ring_vrf_key(
		&authority_session,
		&CallContext::with_request_id("register".to_string()),
		&ProductContext::new(ProductId::for_test("myapp.dot")),
		v01::HostAccountRegisterRingVrfKeyRequest {
			index: v01::DerivationIndex::Index(0),
			ring: ring_location_fixture(),
		},
	));

	assert_eq!(
		(
			registered,
			futures::executor::block_on(
				pairing.accounts().ring_vrf_providers(&ring_location_fixture())
			),
		),
		(Ok(public_key), Ok(vec![account_id("myapp.dot", 0)])),
	);
}

async fn has_auto_key(
	pairing_host: &PairingHost,
	session: &SessionInfo,
	product_id: &str,
) -> Result<bool, AuthorityError> {
	pairing_host
		.accounts()
		.grants_for_tests()
		.auto_signing_key(session, &crate::platform::ProductId::for_test(product_id))
		.await
		.map(|key| key.is_some())
}

async fn remember_auto_key(
	pairing_host: &PairingHost,
	session: &SessionInfo,
	grant_revision: u64,
	product_id: &str,
	expected_product_subtree_public_key: [u8; 32],
	secret: [u8; 64],
	ring_vrf_domain_entropy: [u8; 32],
) -> Result<(), AuthorityError> {
	pairing_host
		.accounts()
		.grants_for_tests()
		.remember_auto_signing_key(
			session,
			grant_revision,
			&crate::platform::ProductId::for_test(product_id),
			expected_product_subtree_public_key,
			AutoSigningKey::from_parts(secret, ring_vrf_domain_entropy),
		)
		.await
}

async fn retain_allowance_key(
	pairing_host: &PairingHost,
	resource: AllowanceResource,
	session: &SessionInfo,
	grant_revision: u64,
	product_id: &ProductId,
	secret: Vec<u8>,
) -> Result<(), AuthorityError> {
	let grant = match resource {
		AllowanceResource::StatementStore => Allowance::StatementStore {
			key: StatementStoreAllowanceKey::from_secret_bytes(secret)?,
			period: None,
		},
		AllowanceResource::Bulletin => {
			Allowance::Bulletin(BulletinAllowanceKey::from_secret_bytes(secret)?)
		},
	};
	pairing_host
		.accounts()
		.grants_for_tests()
		.retain_allowance(session, grant_revision, product_id, &grant)
		.await
}

async fn cached_allowance_key(
	pairing_host: &PairingHost,
	resource: AllowanceResource,
	session: &SessionInfo,
	grant_revision: u64,
	product_id: &ProductId,
) -> Result<Option<Allowance>, AuthorityError> {
	pairing_host
		.accounts()
		.grants_for_tests()
		.cached_allowance(session, grant_revision, product_id, resource)
		.await
}

async fn register_vrf_key(
	pairing_host: &PairingHost,
	session: &SessionInfo,
	handle: latest::ProductAccountId,
	ring: latest::RingLocation,
	public_key: [u8; 32],
) -> Result<(), RingVrfError> {
	pairing_host
		.accounts()
		.register_ring_vrf_key_for_tests(session.public_key, handle, ring, public_key)
		.await
}

fn cache_sizes(pairing_host: &PairingHost) -> (usize, usize, usize, usize) {
	pairing_host.accounts().grants_for_tests().capability_cache_sizes_for_tests()
}
