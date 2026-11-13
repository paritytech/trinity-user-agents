//! Grants a signing host keeps, seeded and inspected where its `HostAccounts` holds them.

use std::sync::Arc;

use futures::FutureExt;
use truapi::{
	CallContext, CallError,
	api::{ResourceAllocation, StatementStore},
	latest::{DerivationIndex, ProductAccountId},
	v01,
	versioned::{
		resource_allocation::HostRequestResourceAllocationRequest,
		statement_store::{RemoteStatementStoreSubmitError, RemoteStatementStoreSubmitRequest},
	},
};

use super::grant_revision;
use crate::{
	host_logic::{
		product_account::derive_root_keypair_from_entropy, statement_store::SUBMIT_STATEMENT_METHOD,
	},
	platform::{ProductContext, ProductId},
	runtime::{
		HostSession, LocalActivation, SigningHostRole,
		allowances::AllowanceResource,
		authority::{Allowance, AuthorityError, StatementStoreAllowanceKey},
		statement_allowance::slot,
		tests::{
			ENTROPY, active_signing_host, grant_auto_signing, product_runtime_for,
			signing_runtime_on_asset_hub,
		},
	},
	test_support::StubPlatform,
};

const PRODUCT_ID: &str = "myapp.dot";

/// Whether this host keeps an AutoSigning key for `product_id` in this activation.
fn keeps_auto_signing(authority: &SigningHostRole, product_id: &str) -> bool {
	let session = authority.session_state().current().expect("an active wallet");
	futures::executor::block_on(
		authority
			.accounts()
			.grants
			.auto_signing_key(&session, &ProductId::for_test(product_id)),
	)
	.expect("kept keys are readable")
	.is_some()
}

/// A product reset revokes that product's grant and fences grant work begun
/// before it, while the wallet session and other products' grants survive.
#[test]
fn product_clear_revokes_only_current_activation_grant_and_fences_stale_work() {
	let platform =
		Arc::new(StubPlatform { resource_allocation_confirmed: true, ..StubPlatform::default() });
	let (services, authority) = signing_runtime_on_asset_hub(platform, crate::test_support::UNSERVED_GENESIS);
	futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec())).unwrap();
	let authority_session = authority.accounts().current_session().unwrap();
	let revision = grant_revision(authority.accounts());
	for product in ["myapp.dot", "other.dot"] {
		grant_auto_signing(&product_runtime_for(services.clone(), authority.clone(), product));
	}
	let myapp = ProductId::for_test("myapp.dot");
	let wallet_session = authority.session_state().current().unwrap();
	let key = futures::executor::block_on(
		authority.accounts().grants.auto_signing_key(&wallet_session, &myapp),
	)
	.unwrap()
	.unwrap();
	let root = derive_root_keypair_from_entropy(&ENTROPY).unwrap();
	let subtree =
		crate::host_logic::product_account::derive_product_subtree_keypair(&root, &myapp).unwrap();
	authority.clear_product_state(&myapp);
	let current_session = authority.accounts().current_session().unwrap();
	let own = keeps_auto_signing(&authority, "myapp.dot");
	let other = keeps_auto_signing(&authority, "other.dot");
	let stale = futures::executor::block_on(authority.accounts().grants.remember_auto_signing_key(
		&wallet_session,
		revision,
		&myapp,
		subtree.public.to_bytes(),
		key,
	));
	assert_eq!(
		(current_session.validation_id, own, other, stale),
		(authority_session.validation_id, false, true, Err(AuthorityError::Disconnected)),
	);
}

/// A product reset fails only that product's kept-key signatures: one another
/// product began before the reset still completes.
#[test]
fn a_product_reset_leaves_another_products_kept_key_signing() {
	let platform =
		Arc::new(StubPlatform { resource_allocation_confirmed: true, ..StubPlatform::default() });
	let (services, authority) = signing_runtime_on_asset_hub(platform, crate::test_support::UNSERVED_GENESIS);
	futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec())).unwrap();
	let products =
		["myapp.dot", "other.dot"].map(|product| ProductContext::new(ProductId::for_test(product)));
	for product in &products {
		grant_auto_signing(&product_runtime_for(
			services.clone(),
			authority.clone(),
			product.product_id.as_str(),
		));
	}
	let accounts = authority.accounts();
	let session = accounts.current_session().unwrap();
	let [reset_account, other_account] = products.each_ref().map(|product| ProductAccountId {
		dot_ns_identifier: product.product_id.clone(),
		derivation_index: DerivationIndex::Index(0),
	});
	let [reset, other] = &products;
	let kept_before_reset = |product, account| {
		futures::executor::block_on(accounts.kept_signing_key(&session, product, account))
			.unwrap()
			.expect("the product keeps an AutoSigning key")
	};
	let reset_key = kept_before_reset(reset, &reset_account);
	let other_key = kept_before_reset(other, &other_account);
	authority.clear_product_state(&reset.product_id);
	assert_eq!(
		(
			reset_key.sign_with(|_| Ok::<_, AuthorityError>(())),
			other_key.sign_with(|_| Ok::<_, AuthorityError>(())),
		),
		(Err(AuthorityError::Disconnected), Ok(())),
	);
}

#[test]
fn product_reset_during_allocation_review_cannot_restore_native_grants() {
	let (release, gate) = futures::channel::oneshot::channel();
	let platform = Arc::new(StubPlatform {
		resource_allocation_confirmed: true,
		resource_allocation_confirmation_gate: std::sync::Mutex::new(Some(gate)),
		..StubPlatform::default()
	});
	let (services, authority) = signing_runtime_on_asset_hub(platform.clone(), crate::test_support::UNSERVED_GENESIS);
	futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec())).unwrap();
	let runtime = product_runtime_for(services, authority.clone(), PRODUCT_ID);
	let cx = CallContext::default();
	let allocation = ResourceAllocation::request(
		&runtime,
		&cx,
		HostRequestResourceAllocationRequest::V1(v01::HostRequestResourceAllocationRequest {
			resources: vec![v01::AllocatableResource::AutoSigning],
		}),
	);
	futures::pin_mut!(allocation);
	assert!(allocation.as_mut().now_or_never().is_none());
	authority.clear_product_state(&ProductId::for_test("myapp.dot"));
	release.send(()).unwrap();
	let result = futures::executor::block_on(allocation);
	let status = keeps_auto_signing(&authority, "myapp.dot");
	assert_eq!(
		(result.map(|_| ()), status, platform.resource_allocation_reviews.lock().unwrap().len()),
		(
			Err(CallError::Domain(
				truapi::versioned::resource_allocation::HostRequestResourceAllocationError::V1(
					v01::ResourceAllocationError::Unknown {
						reason: AuthorityError::Disconnected.to_string()
					}
				)
			)),
			false,
			1
		),
	);
}

const PERIOD: u32 = 7;
const SECRET: [u8; 64] = [0x42; 64];

fn secret_key() -> StatementStoreAllowanceKey {
	StatementStoreAllowanceKey::from_secret_bytes(SECRET.to_vec()).expect("64-byte secret")
}

fn remember(signing_host: &SigningHostRole, product_id: &str, period: u32) {
	let state = signing_host.session_state();
	let session = state.current().unwrap();
	let revision = grant_revision(signing_host.accounts());
	futures::executor::block_on(signing_host.accounts().grants.retain_allowance(
		&session,
		revision,
		&ProductId::for_test(product_id),
		&Allowance::StatementStore { key: secret_key(), period: Some(period) },
	))
	.unwrap();
}

fn remembered(signing_host: &SigningHostRole, product_id: &str) -> Option<[u8; 64]> {
	let state = signing_host.session_state();
	let session = state.current().unwrap();
	let revision = grant_revision(signing_host.accounts());
	futures::executor::block_on(signing_host.accounts().grants.cached_allowance(
		&session,
		revision,
		&ProductId::for_test(product_id),
		AllowanceResource::StatementStore,
	))
	.unwrap()
	.map(|allowance| *allowance.as_secret_bytes())
}

/// The key the host serves for the product, bounded so a lookup that falls
/// through to an unanswered chain read fails instead of hanging.
fn served(signing_host: &SigningHostRole) -> Option<[u8; 64]> {
	futures::executor::block_on(async {
		let session = signing_host.accounts().current_session().expect("a session is active");
		let cx = CallContext::default();
		let product = ProductContext::new(ProductId::for_test(PRODUCT_ID));
		futures::select! {
			result = signing_host
				.accounts()
				.statement_store_allowance_key(&cx, &session, &product)
				.fuse() => result.ok().map(|key| *key.as_secret_bytes()),
			_ = futures_timer::Delay::new(std::time::Duration::from_secs(30)).fuse() => {
				panic!("the lookup blocked on a chain read")
			}
		}
	})
}

#[test]
fn a_new_period_looks_the_allowance_up_again() {
	let signing_host = active_signing_host(Arc::new(StubPlatform {
		chain_connect_error: Some("the chain is unreachable"),
		..StubPlatform::default()
	}));
	let period = slot::current_period(crate::unix_time::current_unix_secs());
	remember(&signing_host, PRODUCT_ID, period);
	let this_period = served(&signing_host);
	remember(&signing_host, PRODUCT_ID, period.checked_sub(1).unwrap());

	assert_eq!(
		(this_period, served(&signing_host)),
		(Some(SECRET), None),
		"the next period served the previous period's key"
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

	signing_host.clear_product_state(&ProductId::for_test(PRODUCT_ID));

	assert_eq!(
		(remembered(&signing_host, PRODUCT_ID), remembered(&signing_host, "other.dot")),
		(None, Some(SECRET)),
		"clearing a product's state must forget its key and keep the others"
	);
}

#[test]
fn a_replaced_session_is_not_served_the_new_sessions_key() {
	let signing_host = active_signing_host(Arc::new(StubPlatform::default()));
	let authority_session = signing_host.accounts().current_session().unwrap();
	futures::executor::block_on(signing_host.activate_local_session(ENTROPY.to_vec()))
		.expect("re-activation succeeds");
	remember(
		&signing_host,
		PRODUCT_ID,
		slot::current_period(crate::unix_time::current_unix_secs()),
	);

	assert!(
		matches!(
			futures::executor::block_on(signing_host.accounts().statement_store_allowance_key(
				&CallContext::default(),
				&authority_session,
				&ProductContext::new(ProductId::for_test(PRODUCT_ID)),
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
	let revision = grant_revision(signing_host.accounts());
	futures::executor::block_on(signing_host.activate_local_session(ENTROPY.to_vec()))
		.expect("re-activation succeeds");

	let remembered_stale =
		futures::executor::block_on(signing_host.accounts().grants.retain_allowance(
			&session,
			revision,
			&ProductId::for_test(PRODUCT_ID),
			&Allowance::StatementStore { key: secret_key(), period: Some(PERIOD) },
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
	let (services, signing_host) = signing_runtime_on_asset_hub(platform, crate::test_support::UNSERVED_GENESIS);
	futures::executor::block_on(signing_host.activate_local_session(ENTROPY.to_vec()))
		.expect("activation succeeds");
	remember(&signing_host, PRODUCT_ID, PERIOD);
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
fn a_lasting_no_allowance_rejection_forgets_the_rejected_key() {
	let signing_host = submit_rejected("noAllowance", SUBMIT_ATTEMPTS, secret_key().public_key);

	assert_eq!(
		remembered(&signing_host, PRODUCT_ID),
		None,
		"the next proof would reuse a key the store no longer accepts"
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
