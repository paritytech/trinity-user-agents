use super::*;
use crate::runtime::{allowances::AllowanceResource, authority::AccountGrant};

use futures::FutureExt;
use parity_scale_codec::Encode;
use truapi::{
	api::StatementStore,
	versioned::statement_store::{
		RemoteStatementStoreSubmitError, RemoteStatementStoreSubmitRequest,
	},
};

use crate::{
	host_internal::sso_messages::OnExistingAllowancePolicy,
	host_logic::{
		product_account::{derive_full_person_ring_vrf_entropy, derive_sr25519_hard_path},
		statement_store::SUBMIT_STATEMENT_METHOD,
	},
	runtime::statement_allowance::slot,
};

const PRODUCT_ID: &str = "myapp.dot";
const PERIOD: u32 = 7;
const SECRET: [u8; 64] = [0x42; 64];

const PEOPLE_METADATA: &[u8] =
	include_bytes!("../../../../tests/fixtures/paseo-next-v2-metadata-v16.scale");

/// Slot 0 already holds the product's allowance.
fn chain_with_allocated_slot() -> Arc<StubPlatform> {
	let allowance =
		derive_sr25519_hard_path(&ENTROPY, &["allowance", "statement-store", PRODUCT_ID])
			.expect("allowance derivation succeeds");
	let slot_entry = (allowance.public.to_bytes(), 0u32, 0u64).encode();
	let people_row = slot::testing::slot_row(
		derive_full_person_ring_vrf_entropy(&ENTROPY, TEST_NETWORK_SUFFIX),
		TEST_NETWORK_SUFFIX.as_bytes(),
		slot::current_period(crate::unix_time::current_unix_secs()),
		&[Some(format!(r#""0x{}""#, hex::encode(&slot_entry)))],
	);
	Arc::new(StubPlatform {
		rpc_method_responses: vec![
			(
				"state_getRuntimeVersion",
				r#"{"specVersion":1000000,"transactionVersion":1}"#.to_string(),
			),
			("chain_getBlockHash", format!(r#""0x{}""#, hex::encode([0u8; 32]))),
			(
				"Metadata_metadata_at_version",
				format!(r#""0x{}""#, hex::encode(Some(PEOPLE_METADATA.to_vec()).encode()),),
			),
			(
				"RuntimeViewFunction_execute_view_function",
				format!(r#""0x{}""#, hex::encode(Ok::<Vec<u8>, ()>(20u32.encode()).encode()),),
			),
			(
				"RuntimeViewFunction_execute_view_function",
				format!(r#""0x{}""#, hex::encode(Ok::<Vec<u8>, ()>(10u32.encode()).encode()),),
			),
			("state_getStorage", format!(r#""0x{}""#, hex::encode(TEST_NETWORK_SUFFIX.encode()))),
			("state_queryStorageAt", people_row),
			// The LitePeople row, read alongside People's, is empty.
			("state_queryStorageAt", r#"[{"block":"0xb10c","changes":[]}]"#.to_string()),
		],
		..Default::default()
	})
}

fn active_signing_host(platform: Arc<StubPlatform>) -> Arc<SigningHostRole> {
	let (_services, signing_host) = signing_runtime_with_platform(platform);
	futures::executor::block_on(signing_host.activate_local_session(ENTROPY.to_vec()))
		.expect("activation succeeds");
	signing_host
}

/// Bounded so a regression waiting on an unanswered chain read fails instead
/// of hanging.
fn allowance_key(signing_host: &SigningHostRole) -> StatementStoreAllowanceKey {
	futures::executor::block_on(async {
		let session = signing_host.current_session().expect("a session is active");
		let cx = CallContext::default();
		futures::select! {
			result = signing_host
				.statement_store_allowance_key(&cx, &session, PRODUCT_ID.to_string())
				.fuse() => result,
			_ = futures_timer::Delay::new(std::time::Duration::from_secs(30)).fuse() => {
				panic!("the proof blocked on a chain read")
			}
		}
	})
	.expect("the proof is served")
}

fn sent_rpc_count(platform: &StubPlatform) -> usize {
	platform.sent_rpc.lock().expect("rpc list mutex poisoned").len()
}

fn secret_key() -> StatementStoreAllowanceKey {
	StatementStoreAllowanceKey::from_secret_bytes(SECRET.to_vec()).expect("64-byte secret")
}

fn remember(signing_host: &SigningHostRole, product_id: &str, period: u32) {
	let state = signing_host.session_state();
	let session = state.current().unwrap();
	let revision = signing_host.grants_for_tests().lifecycle().revision();
	futures::executor::block_on(signing_host.grants_for_tests().retain_allowance(
		&state,
		&session,
		revision,
		product_id,
		&AccountGrant::StatementStore { key: secret_key(), period: Some(period) },
	))
	.unwrap();
}

fn remembered(signing_host: &SigningHostRole, product_id: &str) -> Option<[u8; 64]> {
	let state = signing_host.session_state();
	let session = state.current().unwrap();
	let revision = signing_host.grants_for_tests().lifecycle().revision();
	futures::executor::block_on(signing_host.grants_for_tests().cached_allowance(
		&state,
		&session,
		revision,
		product_id,
		AllowanceResource::StatementStore,
	))
	.unwrap()
	.map(|grant| match grant {
		AccountGrant::StatementStore { key, .. } => *key.as_secret_bytes(),
		_ => panic!("expected statement-store grant"),
	})
}

/// An existing statement-store allowance must be served without resolving a
/// ring or submitting anything. The cache and the scan are covered on their
/// own; this pins the composition, so removing the early return fails here
/// rather than passing quietly.
#[test]
fn an_existing_allowance_is_served_without_touching_the_ring() {
	let allowance =
		derive_sr25519_hard_path(&ENTROPY, &["allowance", "statement-store", PRODUCT_ID])
			.expect("allowance derivation succeeds");
	let platform = chain_with_allocated_slot();
	let signing_host = active_signing_host(platform.clone());

	// Bounded, because the failure mode of losing the early return is a
	// wait on a chain read the stub deliberately does not answer — an
	// unbounded test would hang instead of reporting. The bound is generous
	// because it is catching a hang, not asserting latency.
	let allocation = futures::executor::block_on(async {
		let session = signing_host.wallet_for_tests().current_session().unwrap();
		futures::select! {
			result = signing_host.wallet_for_tests().allocate_statement_store_allowance(
				&session,
				PRODUCT_ID,
				OnExistingAllowancePolicy::Ignore,
			)
			.fuse() => result,
			_ = futures_timer::Delay::new(std::time::Duration::from_secs(30)).fuse() => {
				panic!("allocation blocked on a chain read it should not have made")
			}
		}
	})
	.expect("an existing allowance is returned");

	assert_eq!(allocation.secret, allowance.secret.to_bytes().to_vec());

	let sent = platform.sent_rpc.lock().expect("rpc list mutex poisoned");
	let methods: Vec<String> = sent
		.iter()
		.filter_map(|request| {
			let value: serde_json::Value = serde_json::from_str(request).ok()?;
			value.get("method")?.as_str().map(ToString::to_string)
		})
		.collect();

	// `find_including_rings` opens with `chain_getFinalizedHead`, so none of
	// these means no ring was resolved.
	assert_eq!(
		methods.iter().filter(|method| *method == "chain_getFinalizedHead").count(),
		0,
		"a ring was resolved for an allowance already in place: {methods:?}"
	);
	assert!(
		!methods.iter().any(|method| method.starts_with("author_submit")),
		"an extrinsic was submitted for an allowance already in place: {methods:?}"
	);
}

#[test]
fn a_second_proof_in_the_same_session_sends_nothing_to_the_chain() {
	let platform = chain_with_allocated_slot();
	let signing_host = active_signing_host(platform.clone());

	let first = allowance_key(&signing_host);
	let sent_after_first = sent_rpc_count(&platform);
	let second = allowance_key(&signing_host);

	assert_eq!(
		(sent_after_first > 0, second.public_key, sent_rpc_count(&platform)),
		(true, first.public_key, sent_after_first),
		"the first proof must reach the chain, and a proof after it must not"
	);
}

#[test]
fn a_new_period_keeps_the_sponsorship_key_until_submission() {
	let platform = chain_with_allocated_slot();
	let signing_host = active_signing_host(platform.clone());
	let period = slot::current_period(crate::unix_time::current_unix_secs());
	remember(&signing_host, PRODUCT_ID, period.checked_sub(1).unwrap());

	let signer = allowance_key(&signing_host).public_key;
	assert_eq!(
		(signer == secret_key().public_key, sent_rpc_count(&platform) > 0),
		(true, false),
		"proof creation reuses the key; submission renews its sponsorship",
	);
}

#[test]
fn a_new_session_looks_the_allowance_up_again() {
	let signing_host = active_signing_host(Arc::new(StubPlatform::default()));
	remember(&signing_host, PRODUCT_ID, PERIOD);

	futures::executor::block_on(signing_host.disconnect());
	futures::executor::block_on(signing_host.activate_local_session(ENTROPY.to_vec()))
		.expect("re-activation succeeds");

	assert_eq!(
		remembered(&signing_host, PRODUCT_ID),
		None,
		"the second session served the first session's key"
	);
}

#[test]
fn clearing_a_product_forgets_only_its_key() {
	let signing_host = active_signing_host(Arc::new(StubPlatform::default()));
	remember(&signing_host, PRODUCT_ID, PERIOD);
	remember(&signing_host, "other.dot", PERIOD);

	futures::executor::block_on(signing_host.clear_product_state(PRODUCT_ID))
		.expect("the product id is valid");

	assert_eq!(
		(remembered(&signing_host, PRODUCT_ID), remembered(&signing_host, "other.dot"),),
		(None, Some(SECRET)),
		"clearing a product's state must forget its key and keep the others"
	);
}

#[test]
fn a_replaced_session_is_not_served_the_new_sessions_key() {
	let signing_host = active_signing_host(Arc::new(StubPlatform::default()));
	let authority_session = signing_host.current_session().unwrap();
	futures::executor::block_on(signing_host.activate_local_session(ENTROPY.to_vec()))
		.expect("re-activation succeeds");
	remember(
		&signing_host,
		PRODUCT_ID,
		slot::current_period(crate::unix_time::current_unix_secs()),
	);

	assert!(
		matches!(
			futures::executor::block_on(signing_host.statement_store_allowance_key(
				&CallContext::default(),
				&authority_session,
				PRODUCT_ID.to_string(),
			)),
			Err(AuthorityError::Disconnected)
		),
		"a request validated under the replaced session was served a key"
	);
}

#[test]
fn a_key_allocated_under_a_replaced_session_is_not_remembered() {
	let signing_host = active_signing_host(Arc::new(StubPlatform::default()));
	let state = signing_host.session_state();
	let session = state.current().unwrap();
	let revision = signing_host.grants_for_tests().lifecycle().revision();
	futures::executor::block_on(signing_host.activate_local_session(ENTROPY.to_vec()))
		.expect("re-activation succeeds");

	let remembered_stale =
		futures::executor::block_on(signing_host.grants_for_tests().retain_allowance(
			&state,
			&session,
			revision,
			PRODUCT_ID,
			&AccountGrant::StatementStore { key: secret_key(), period: Some(PERIOD) },
		));
	assert_eq!(
		(remembered_stale.map(|_| ()), remembered(&signing_host, PRODUCT_ID)),
		(Err(AuthorityError::Disconnected), None),
		"a key allocated for a replaced session was handed back or remembered",
	);
}

/// Every attempt the submit retry makes: the first plus its retries.
const SUBMIT_ATTEMPTS: usize = 11;

fn rejected(reason: &str) -> String {
	format!(r#"{{"status":"rejected","reason":"{reason}"}}"#)
}

fn submit_answered(
	answers: Vec<String>,
	signer: [u8; 32],
) -> (Result<(), CallError<RemoteStatementStoreSubmitError>>, Arc<SigningHostRole>) {
	let platform = Arc::new(StubPlatform {
		rpc_method_responses: answers
			.into_iter()
			.map(|answer| (SUBMIT_STATEMENT_METHOD, answer))
			.collect(),
		..Default::default()
	});
	let (services, signing_host) = signing_runtime_with_platform(platform);
	futures::executor::block_on(signing_host.activate_local_session(ENTROPY.to_vec()))
		.expect("activation succeeds");
	remember(&signing_host, PRODUCT_ID, PERIOD);
	signing_host.set_grant_allowances_unchecked(true);
	let runtime = product_runtime_for(services, signing_host.clone(), PRODUCT_ID);

	let submitted = futures::executor::block_on(runtime.submit(
		&CallContext::default(),
		RemoteStatementStoreSubmitRequest::V1(truapi::latest::SignedStatement {
			proof: truapi::latest::StatementProof::Sr25519 { signature: [0; 64], signer },
			decryption_key: None,
			expiry: None,
			channel: None,
			topics: Vec::new(),
			data: None,
		}),
	))
	.map(|_| ());
	(submitted, signing_host)
}

fn submit_rejected(reason: &str, attempts: usize, signer: [u8; 32]) -> Arc<SigningHostRole> {
	let (submitted, signing_host) = submit_answered(vec![rejected(reason); attempts], signer);
	let Err(CallError::Domain(RemoteStatementStoreSubmitError::V1(error))) = submitted else {
		panic!("the store rejection must surface as a domain error: {submitted:?}");
	};
	assert!(
		error.reason.contains(reason),
		"the submit failed before the store answered: {}",
		error.reason
	);
	signing_host
}

#[test]
fn a_rejected_submission_keeps_the_key_for_sponsorship_renewal() {
	let signing_host = submit_rejected("noAllowance", SUBMIT_ATTEMPTS, secret_key().public_key);

	assert_eq!(
		remembered(&signing_host, PRODUCT_ID),
		Some(SECRET),
		"submission failure must not discard an issued sponsorship key"
	);
}

#[test]
fn a_no_allowance_rejection_that_clears_on_retry_keeps_the_key() {
	let (submitted, signing_host) = submit_answered(
		vec![rejected("noAllowance"), r#"{"status":"new"}"#.to_string()],
		secret_key().public_key,
	);

	assert_eq!(
		(submitted.is_ok(), remembered(&signing_host, PRODUCT_ID)),
		(true, Some(SECRET)),
		"credit that is only late to reach the store must not cost the product its key"
	);
}

#[test]
fn a_no_allowance_rejection_for_another_signer_keeps_the_key() {
	let signing_host = submit_rejected("noAllowance", SUBMIT_ATTEMPTS, [0x11; 32]);

	assert_eq!(
		remembered(&signing_host, PRODUCT_ID),
		Some(SECRET),
		"a rejection for a key the host did not issue must not evict the cached one"
	);
}

#[test]
fn another_rejection_keeps_the_key() {
	let signing_host = submit_rejected("badProof", 1, secret_key().public_key);

	assert_eq!(
		remembered(&signing_host, PRODUCT_ID),
		Some(SECRET),
		"a rejection that says nothing about the allowance must not evict the key"
	);
}

#[test]
fn only_a_retained_sponsorship_renews_without_another_approval() {
	let platform = chain_with_allocated_slot();
	let host = active_signing_host(platform.clone());
	let key =
		derive_sr25519_hard_path(&ENTROPY, &["allowance", "statement-store", PRODUCT_ID]).unwrap();
	let state = host.session_state();
	let session = state.current().unwrap();
	let revision = host.grants_for_tests().lifecycle().revision();
	let product = crate::platform::ProductContext::new(PRODUCT_ID.to_string()).unwrap();
	futures::executor::block_on(async {
		host.grants_for_tests()
			.retain_allowance(
				&state,
				&session,
				revision,
				PRODUCT_ID,
				&AccountGrant::StatementStore {
					key: StatementStoreAllowanceKey::from_secret_bytes(
						key.secret.to_bytes().to_vec(),
					)
					.unwrap(),
					period: Some(PERIOD),
				},
			)
			.await
			.unwrap();
		host.renew_statement_sponsorship(&CallContext::default(), &product, [0x11; 32])
			.await
			.unwrap();
		assert_eq!(sent_rpc_count(&platform), 0);
		host.renew_statement_sponsorship(&CallContext::default(), &product, key.public.to_bytes())
			.await
			.unwrap();
	});
	assert_eq!(
		(
			sent_rpc_count(&platform) > 0,
			platform.resource_allocation_reviews.lock().unwrap().len(),
			remembered(&host, PRODUCT_ID)
		),
		(true, 0, Some(key.secret.to_bytes()))
	);
}

fn sent_methods(platform: &StubPlatform) -> Vec<String> {
	platform
		.sent_rpc
		.lock()
		.expect("rpc list mutex poisoned")
		.iter()
		.map(|request| {
			let request: serde_json::Value = serde_json::from_str(request).unwrap();
			request["method"].as_str().unwrap().to_string()
		})
		.collect()
}

/// A Bulletin `Authorizations` entry for one transaction with `remaining` bytes left.
fn bulletin_authorization(remaining: u64) -> String {
	let entry = (0u32, 1u32, 0u64, 0u64, remaining, 100u32).encode();
	format!(r#""0x{}""#, hex::encode(entry))
}

/// A payload the authorization cannot hold is extended before it is signed,
/// rather than signed, rejected by the chain and signed again.
#[test]
fn a_preimage_larger_than_the_bulletin_authorization_extends_it_before_submitting() {
	let header = r#"{"number":"0x1"}"#.to_string();
	let platform = Arc::new(StubPlatform {
		remote_permission_decisions: std::sync::Mutex::new(
			[crate::platform::PermissionDecision::AllowAlways].into(),
		),
		rpc_method_responses: vec![
			("state_getStorage", bulletin_authorization(4)),
			("state_getStorage", bulletin_authorization(4)),
			("state_getStorage", bulletin_authorization(4)),
			("chain_getHeader", header.clone()),
			("chain_getHeader", header.clone()),
			("chain_getHeader", header),
		],
		// The People chain is unreachable, so the extension itself fails.
		unreachable_genesis: Some([0; 32]),
		..Default::default()
	});
	let (services, signing_host) = signing_runtime_with_platform(platform.clone());
	futures::executor::block_on(signing_host.activate_local_session(ENTROPY.to_vec()))
		.expect("activation succeeds");
	let runtime = product_runtime(services, signing_host);

	let cx = CallContext::default();
	let submitted = futures::executor::block_on(async {
		futures::select! {
			result = truapi::api::Preimage::submit(
				&runtime,
				&cx,
				truapi::versioned::preimage::RemotePreimageSubmitRequest::V1(
					b"longer than four bytes".to_vec(),
				),
			).fuse() => result.is_ok(),
			() = futures_timer::Delay::new(std::time::Duration::from_secs(5)).fuse() => {
				panic!("the submission waited on an unscripted chain read")
			},
		}
	});
	assert_eq!(
		(submitted, sent_methods(&platform)),
		(
			false,
			["state_getStorage", "chain_getHeader"]
				.repeat(3)
				.iter()
				.map(|method| method.to_string())
				.collect()
		),
	);
}

/// A payload the authorization already holds goes straight to the submit,
/// without asking for more.
#[test]
fn a_preimage_the_bulletin_authorization_holds_is_submitted_without_extending_it() {
	let header = r#"{"number":"0x1"}"#.to_string();
	let platform = Arc::new(StubPlatform {
		remote_permission_decisions: std::sync::Mutex::new(
			[crate::platform::PermissionDecision::AllowAlways].into(),
		),
		rpc_method_responses: vec![
			("state_getStorage", bulletin_authorization(64)),
			("state_getStorage", bulletin_authorization(64)),
			("chain_getHeader", header.clone()),
			("chain_getHeader", header),
			("chainHead_v1_follow", "null".to_string()),
		],
		unreachable_genesis: Some([0; 32]),
		..Default::default()
	});
	let (services, signing_host) = signing_runtime_with_platform(platform.clone());
	futures::executor::block_on(signing_host.activate_local_session(ENTROPY.to_vec()))
		.expect("activation succeeds");
	let runtime = product_runtime(services, signing_host);

	let cx = CallContext::default();
	let submitted = futures::executor::block_on(async {
		futures::select! {
			result = truapi::api::Preimage::submit(
				&runtime,
				&cx,
				truapi::versioned::preimage::RemotePreimageSubmitRequest::V1(b"fits".to_vec()),
			).fuse() => result.is_ok(),
			() = futures_timer::Delay::new(std::time::Duration::from_secs(5)).fuse() => {
				panic!("the submission waited on an unscripted chain read")
			},
		}
	});

	// The scripted Bulletin node drops the follow subscription, so the submit
	// itself fails; the two authorization reads before it are what matter.
	assert_eq!(
		(submitted, sent_methods(&platform)),
		(
			false,
			[
				"state_getStorage",
				"chain_getHeader",
				"state_getStorage",
				"chain_getHeader",
				"chainHead_v1_follow",
			]
			.map(String::from)
			.to_vec()
		),
	);
}
