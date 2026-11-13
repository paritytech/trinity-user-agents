//! Grants a pairing host keeps, seeded and inspected where its `HostAccounts` holds them.

use super::grant_revision;
use crate::{
	platform::CoreStorageKey,
	runtime::{
		ProductRuntimeHost,
		allowances::AllowanceResource,
		authority::{Allowance, AutoSigningKey, StatementStoreAllowanceKey},
		tests::{
			auto_signing_test_platform, cache_test_product_subtree, install_pairing_session,
			request_auto_signing,
		},
		*,
	},
	test_support::*,
};
use truapi::{
	api::Account,
	versioned::account::{
		HostAccountRingVrfSignError, HostAccountRingVrfSignRequest, HostAccountRingVrfSignResponse,
	},
};

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
		grant_revision(pairing_host.accounts()),
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
	let stale_revision = grant_revision(pairing_host.accounts());
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
	let stale_revision = grant_revision(pairing_host.accounts());
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
	let current_revision = grant_revision(pairing_host.accounts());
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
	let grant_revision = grant_revision(pairing_host.accounts());
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
	let grant_revision = grant_revision(pairing_host.accounts());
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
	let grant_revision = grant_revision(pairing_host.accounts());
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
	pairing_host.accounts().resource_controls().set_submit_preimages_locally(true);
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

async fn has_auto_key(
	pairing_host: &PairingHost,
	session: &SessionInfo,
	product_id: &str,
) -> Result<bool, AuthorityError> {
	pairing_host
		.accounts()
		.grants
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
		.grants
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
		.grants
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
		.grants
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
		.ring_vrf_registry
		.register(session.public_key, handle, ring, public_key)
		.await
}

fn cache_sizes(pairing_host: &PairingHost) -> (usize, usize, usize, usize) {
	pairing_host.accounts().grants.capability_cache_sizes_for_tests()
}
