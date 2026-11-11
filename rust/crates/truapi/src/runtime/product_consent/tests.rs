use super::*;
use crate::test_support::StubPlatform;
use PermissionAuthorizationStatus::{Authorized, Denied};

fn consent_answering(
	decisions: impl IntoIterator<Item = PermissionDecision>,
) -> (Arc<StubPlatform>, ProductConsent) {
	let platform = Arc::new(StubPlatform {
		permission_confirmation_decisions: Mutex::new(decisions.into_iter().collect()),
		..StubPlatform::default()
	});
	(platform.clone(), ProductConsent::new(platform, Arc::default()))
}

fn access(consent: &ProductConsent, requester: &str) -> PermissionAuthorizationStatus {
	futures::executor::block_on(consent.account_access(
		&ProductId::for_test(requester),
		&ProductId::for_test("peopl.dot"),
		consent.platform.as_ref(),
		|| true,
	))
	.expect("the stub answers every prompt")
}

fn prompts(platform: &StubPlatform) -> usize {
	platform.account_access_reviews.lock().unwrap().len()
}

/// Resetting a product drops what it was allowed once, not what others were.
#[test]
fn a_product_reset_forgets_only_that_products_allow_once() {
	let (platform, consent) = consent_answering([
		PermissionDecision::AllowOnce,
		PermissionDecision::AllowOnce,
		PermissionDecision::Deny,
	]);
	access(&consent, "myapp.dot");
	access(&consent, "other.dot");
	consent.forget_allowed_once_for(&ProductId::for_test("myapp.dot"));
	assert_eq!(
		(access(&consent, "other.dot"), access(&consent, "myapp.dot"), prompts(&platform)),
		(Authorized, Denied, 3)
	);
}

/// A decision changed in settings applies at once, even over Allow once.
#[test]
fn a_stored_decision_overrides_allow_once() {
	let (platform, consent) = consent_answering([PermissionDecision::AllowOnce]);
	access(&consent, "myapp.dot");
	futures::executor::block_on(set_authorization_status(
		platform.as_ref(),
		CoreStorageKey::product_account_access_authorization(
			&ProductId::for_test("myapp.dot"),
			&ProductId::for_test("peopl.dot"),
		),
		Denied,
	))
	.unwrap();
	assert_eq!((access(&consent, "myapp.dot"), prompts(&platform)), (Denied, 1));
}

/// Two calls racing for the same decision raise one prompt, and the
/// second reuses its answer instead of asking again.
#[test]
fn a_request_waiting_for_the_prompt_reuses_its_answer() {
	let (release, gate) = futures::channel::oneshot::channel();
	let platform = Arc::new(StubPlatform {
		permission_confirmation_decisions: Mutex::new(
			[PermissionDecision::AllowOnce, PermissionDecision::Deny].into(),
		),
		account_access_confirmation_gate: Mutex::new(Some(gate)),
		..StubPlatform::default()
	});
	let consent = ProductConsent::new(platform.clone(), Arc::default());
	let (requester, target) = (ProductId::for_test("myapp.dot"), ProductId::for_test("peopl.dot"));
	let (first, second, ()) = futures::executor::block_on(async {
		futures::join!(
			consent.account_access(&requester, &target, platform.as_ref(), || true),
			consent.account_access(&requester, &target, platform.as_ref(), || true),
			async { release.send(()).unwrap() },
		)
	});
	assert_eq!((first.unwrap(), second.unwrap(), prompts(&platform)), (Authorized, Authorized, 1));
}

#[test]
fn a_stored_denial_overrides_a_trusted_products_account_access() {
	let (platform, consent) = consent_answering([]);
	futures::executor::block_on(set_authorization_status(
		platform.as_ref(),
		CoreStorageKey::product_account_access_authorization(
			&ProductId::for_test("dim2.dot"),
			&ProductId::for_test("peopl.dot"),
		),
		Denied,
	))
	.unwrap();
	assert_eq!((access(&consent, "dim2.dot"), prompts(&platform)), (Denied, 0));
}

#[test]
fn administration_cancels_an_account_access_prompt_before_it_can_commit() {
	use crate::{
		host_internal::permissions::PermissionsService,
		platform::{PermissionAuthorizationRequest, ProductContext},
	};
	let (release, gate) = futures::channel::oneshot::channel();
	let platform = Arc::new(StubPlatform {
		permission_confirmation_decisions: Mutex::new([PermissionDecision::AllowAlways].into()),
		account_access_confirmation_gate: Mutex::new(Some(gate)),
		..StubPlatform::default()
	});
	let authority = Arc::new(PermissionAuthority::default());
	let consent = ProductConsent::new(platform.clone(), authority.clone());
	let requester = ProductId::for_test("myapp.dot");
	let target = ProductId::for_test("peopl.dot");
	let product = ProductContext::new(requester.clone());
	let admin = PermissionsService::new(platform.as_ref(), platform.as_ref(), &product)
		.with_authority(authority);
	futures::executor::block_on(async {
		let mut pending =
			Box::pin(consent.account_access(&requester, &target, platform.as_ref(), || true));
		assert!(futures::poll!(&mut pending).is_pending());
		admin
			.set_authorization_status(
				&PermissionAuthorizationRequest::AccountAccess {
					target_product_id: target.clone(),
				},
				Denied,
			)
			.await
			.unwrap();
		assert!(matches!(pending.await, Err(AccountAccessError::Superseded)));
		let _ = release.send(());
		assert_eq!(
			consent
				.account_access(&requester, &target, platform.as_ref(), || true)
				.await
				.unwrap(),
			Denied,
		);
	});
}

#[test]
fn genuine_account_access_failures_keep_their_classification() {
	for storage in [false, true] {
		let platform = Arc::new(StubPlatform {
			// The text is deliberately identical to PermissionScope's cancellation
			// reason: classification must depend on authority, not error wording.
			account_access_error: (!storage).then_some("permission decision was superseded"),
			permission_storage_error: storage.then_some("permission decision was superseded"),
			..StubPlatform::default()
		});
		let consent = ProductConsent::new(platform.clone(), Arc::default());
		let error = futures::executor::block_on(consent.account_disclosure(
			&ProductId::for_test("myapp.dot"),
			&ProductId::for_test("peopl.dot"),
			platform.as_ref(),
			|| true,
		))
		.unwrap_err();
		assert!(matches!(
			(storage, error),
			(true, AccountAccessError::PermissionStorage(_)) |
				(false, AccountAccessError::Confirmation(_))
		),);
	}
}
