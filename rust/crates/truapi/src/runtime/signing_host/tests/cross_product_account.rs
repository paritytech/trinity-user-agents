//! Signing with an account a publisher granted to another product.
//!
//! The staging name of a product asks for the product's own account, which is
//! what `trustedProducts` is for. These go through the signing role end to
//! end, so what they pin is the signature: whose key actually signed, not only
//! which door the request got through.

use super::*;

use crate::{
	platform::CoreStorage,
	runtime::product_manifest::{CachedManifest, manifest_cache_key},
	unix_time::current_unix_secs,
};
use parity_scale_codec::Encode;
use truapi::versioned::signing::{
	HostSignPayloadError, HostSignPayloadRequest, HostSignPayloadResponse,
};

/// Seed `owner`'s manifest cache, so the grant resolves without a chain.
fn cache_grant(platform: &StubPlatform, owner: &str, trusted: &str) {
	let entry = CachedManifest {
		fetched_at_secs: current_unix_secs(),
		json: Some(format!(r#"{{"$v":1,"trustedProducts":{trusted}}}"#)),
	};
	futures::executor::block_on(
		platform.write_core_storage(manifest_cache_key(owner), entry.encode()),
	)
	.expect("stub core storage accepts the entry");
}

fn dim2_account() -> v01::ProductAccountId {
	v01::ProductAccountId {
		dot_ns_identifier: "dim2.paseo".to_string(),
		derivation_index: v01::DerivationIndex::Index(0),
	}
}

/// `caller` signs a payload with `dim2.paseo`'s account.
fn sign_as(
	platform: Arc<StubPlatform>,
	caller: &str,
) -> Result<v01::HostSignPayloadResponse, CallError<HostSignPayloadError>> {
	let (services, activation) = signing_runtime_with_platform(platform);
	futures::executor::block_on(activation.activate_local_session(ENTROPY.to_vec()))
		.expect("the local session activates");
	let runtime = product_runtime_for(services, activation, caller);

	futures::executor::block_on(runtime.sign_payload(
		&CallContext::default(),
		HostSignPayloadRequest::V1(v01::HostSignPayloadRequest {
			account: dim2_account(),
			payload: crate::test_support::sign_payload_data(),
		}),
	))
	.map(|HostSignPayloadResponse::V1(response)| response)
}

#[test]
fn a_context_grant_signs_with_the_granting_products_account() {
	let platform = Arc::new(StubPlatform { sign_payload_confirmed: true, ..Default::default() });
	cache_grant(&platform, "dim2.paseo", r#"{"dim2next":["context"]}"#);

	let response = sign_as(platform, "dim2next.paseo").expect("the grant admits the account");

	// The account that signed is the one the grant was published for, not the
	// caller's own: a product acting for another does not sign as itself.
	let root = derive_root_keypair_from_entropy(&ENTROPY).expect("root derives");
	let owner =
		derive_product_keypair(&root, "dim2.paseo", index_bytes(0)).expect("owner key derives");
	let preimage = crate::host_internal::transaction::extrinsic_payload_preimage(
		&crate::test_support::sign_payload_data(),
	)
	.expect("preimage builds");
	let signature =
		schnorrkel::Signature::from_bytes(&response.signature[1..]).expect("64-byte signature");
	assert!(
		owner.public.verify_simple(b"substrate", &preimage, &signature).is_ok(),
		"dim2.paseo's account signed the payload",
	);
}

#[test]
fn an_ungranted_product_cannot_sign_with_the_account() {
	let platform = Arc::new(StubPlatform { sign_payload_confirmed: true, ..Default::default() });
	cache_grant(&platform, "dim2.paseo", r#"{}"#);

	let error = sign_as(platform, "dim2next.paseo").expect_err("no grant names this caller");

	assert!(
		matches!(
			error,
			CallError::Domain(HostSignPayloadError::V1(
				v01::HostSignPayloadError::PermissionDenied
			))
		),
		"expected PermissionDenied, got {error:?}",
	);
}

/// The user still sees what they are approving. A grant is the publisher's
/// answer about which product may act; it is not the user's answer about a
/// signature, which this role asks for every time.
#[test]
fn a_granted_signature_is_still_confirmed_by_the_user() {
	let platform = Arc::new(StubPlatform { sign_payload_confirmed: false, ..Default::default() });
	cache_grant(&platform, "dim2.paseo", r#"{"dim2next":["context"]}"#);

	sign_as(platform.clone(), "dim2next.paseo")
		.expect_err("a refused confirmation refuses the signature");

	assert_eq!(
		platform
			.sign_payload_reviews
			.lock()
			.expect("sign payload review list mutex poisoned")
			.len(),
		1,
		"the grant does not waive the per-signature confirmation",
	);
}

/// A blessed product skips the confirmation, never the owner's grant.
#[test]
fn a_blessed_product_signs_with_another_products_account_only_when_granted() {
	for (trusted, expected) in [
		(
			"{}",
			Err(CallError::Domain(HostSignPayloadError::V1(
				v01::HostSignPayloadError::PermissionDenied,
			))),
		),
		(r#"{"stash":["context"]}"#, Ok(())),
	] {
		let platform = Arc::new(StubPlatform::default());
		cache_grant(&platform, "dim2.paseo", trusted);
		let result = sign_as(platform.clone(), "stash.paseo").map(|_| ());
		assert_eq!((result, platform.sign_payload_reviews.lock().unwrap().len()), (expected, 0),);
	}
}
