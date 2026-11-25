use super::*;
use crate::{
	platform::{ProductContext, ResourceAllocationReview},
	test_support::{StubPlatform, dot_test_stack, paseo_test_stack},
};
use PermissionAuthorizationStatus::{Authorized, Denied};

fn consent_answering(
	decisions: impl IntoIterator<Item = PermissionDecision>,
) -> (Arc<StubPlatform>, ProductConsent) {
	let platform = Arc::new(StubPlatform {
		permission_confirmation_decisions: Mutex::new(decisions.into_iter().collect()),
		..StubPlatform::default()
	});
	(platform.clone(), ProductConsent::new(platform, dot_test_stack()))
}

fn access(consent: &ProductConsent, requester: &str) -> PermissionAuthorizationStatus {
	futures::executor::block_on(consent.account_access(
		&ProductId::for_test(requester),
		&ProductId::for_test("peopl.dot"),
		consent.platform.as_ref(),
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
	let consent = ProductConsent::new(platform.clone(), dot_test_stack());
	let (requester, target) = (ProductId::for_test("myapp.dot"), ProductId::for_test("peopl.dot"));
	let (first, second, ()) = futures::executor::block_on(async {
		futures::join!(
			consent.account_access(&requester, &target, platform.as_ref()),
			consent.account_access(&requester, &target, platform.as_ref()),
			async { release.send(()).unwrap() },
		)
	});
	assert_eq!((first.unwrap(), second.unwrap(), prompts(&platform)), (Authorized, Authorized, 1));
}

/// A trusted label is trusted only under a TLD the stack accepts, so
/// `dim2.testnet` on a paseo stack is asked like any other product.
#[test]
fn a_trusted_label_off_the_stack_is_asked_for_account_access() {
	let platform = Arc::new(StubPlatform {
		permission_confirmation_decisions: Mutex::new([PermissionDecision::Deny].into()),
		..StubPlatform::default()
	});
	let consent = ProductConsent::new(platform.clone(), paseo_test_stack());
	assert_eq!(
		(access(&consent, "dim2.paseo"), access(&consent, "dim2.testnet"), prompts(&platform)),
		(Authorized, Denied, 1)
	);
}

/// A trusted product skips the review of its own operations, but only under a
/// TLD the stack accepts.
#[test]
fn a_trusted_label_off_the_stack_does_not_skip_a_review() {
	let stack = paseo_test_stack();
	let skips = |product_id: &str| {
		let product = ProductContext::new(ProductId::for_test(product_id));
		let review = UserConfirmationReview::ResourceAllocation(ResourceAllocationReview {
			calling_product_id: product.product_id.clone(),
			resources: vec![],
		});
		skips_review(&stack, AccountCaller::Local { product: &product }, &review)
	};
	assert_eq!([skips("dim2.paseo"), skips("dim2.testnet")], [true, false]);
}
