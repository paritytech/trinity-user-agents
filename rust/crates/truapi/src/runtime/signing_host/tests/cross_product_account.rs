//! Signing with an account a publisher granted to another product.
//!
//! The staging name of a product asks for the product's own account, which is
//! what `trustedProducts` is for. These go through the signing role end to
//! end, so what they pin is the signature: whose key actually signed, not only
//! which door the request got through.

use super::*;

use crate::platform::CoreStorage;
use crate::test_support::account_id;
use futures::FutureExt as _;
use crate::runtime::product_manifest::{CachedManifest, manifest_cache_key};
use crate::unix_time::current_unix_secs;
use parity_scale_codec::Encode;
use truapi::versioned::signing::{
    HostCreateTransactionRequest, HostSignPayloadError, HostSignPayloadRequest,
    HostSignPayloadResponse, HostSignRawRequest,
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
    let platform = Arc::new(StubPlatform {
        sign_payload_confirmed: true,
        ..Default::default()
    });
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
        owner
            .public
            .verify_simple(b"substrate", &preimage, &signature)
            .is_ok(),
        "dim2.paseo's account signed the payload",
    );
}

#[test]
fn an_ungranted_product_cannot_sign_with_the_account() {
    let platform = Arc::new(StubPlatform {
        sign_payload_confirmed: true,
        ..Default::default()
    });
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
/// signature, which this role asks for before the first one.
#[test]
fn a_granted_signature_is_still_confirmed_by_the_user() {
    let platform = Arc::new(StubPlatform {
        sign_payload_confirmed: false,
        ..Default::default()
    });
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
        assert_eq!(
            (result, platform.sign_payload_reviews.lock().unwrap().len()),
            (expected, 0),
        );
    }
}

/// A `dim2next.paseo` execution that `dim2.paseo` granted its accounts, on a
/// local signing session. One runtime is one execution, so what the user
/// approved carries from one call to the next.
fn dim2next_runtime(platform: Arc<StubPlatform>) -> ProductRuntimeHost {
    cache_grant(&platform, "dim2.paseo", r#"{"dim2next":["context"]}"#);
    let (services, activation) = signing_runtime_with_platform(platform);
    futures::executor::block_on(activation.activate_local_session(ENTROPY.to_vec()))
        .expect("the local session activates");
    product_runtime_for(services, activation, "dim2next.paseo")
}

fn sign_payload_with(runtime: &ProductRuntimeHost, account: v01::ProductAccountId) {
    futures::executor::block_on(runtime.sign_payload(
        &CallContext::default(),
        HostSignPayloadRequest::V1(v01::HostSignPayloadRequest {
            account,
            payload: crate::test_support::sign_payload_data(),
        }),
    ))
    .expect("the grant admits the account and the user approves");
}

fn sign_raw_with(runtime: &ProductRuntimeHost, account: v01::ProductAccountId) {
    futures::executor::block_on(runtime.sign_raw(
        &CallContext::default(),
        HostSignRawRequest::V1(v01::HostSignRawRequest {
            account,
            payload: crate::test_support::raw_payload(),
        }),
    ))
    .expect("the grant admits the account and the user approves");
}

/// Starts a transaction and leaves it once it has been confirmed and handed to
/// the authority, which would then wait on a chain these tests do not run.
fn create_transaction_with(runtime: &ProductRuntimeHost, account: v01::ProductAccountId) {
    let mut payload = crate::test_support::product_tx_payload("dim2.paseo");
    payload.signer = account;
    let cx = CallContext::default();
    let call = runtime.create_transaction(&cx, HostCreateTransactionRequest::V1(payload));
    assert!(
        call.now_or_never().is_none(),
        "the grant admits the account and the user approves",
    );
}

/// How many prompts of each kind the user has seen: payload, raw, transaction.
fn prompts(platform: &StubPlatform) -> (usize, usize, usize) {
    (
        platform.sign_payload_reviews.lock().unwrap().len(),
        platform.sign_raw_reviews.lock().unwrap().len(),
        platform.create_transaction_reviews.lock().unwrap().len(),
    )
}

/// A product that signs repeatedly with another product's account, such as a
/// game registering a player and then signalling, would otherwise put a prompt
/// in front of every signature. The user approves each kind once per product
/// whose accounts sign, as the grant is filed, and approving one kind does not
/// let another through unasked.
#[test]
fn a_cross_product_signature_is_confirmed_once_per_kind_and_product() {
    let platform = Arc::new(StubPlatform {
        sign_payload_confirmed: true,
        sign_raw_confirmed: true,
        create_transaction_confirmed: true,
        ..Default::default()
    });
    let runtime = dim2next_runtime(platform.clone());

    for _ in 0..3 {
        sign_payload_with(&runtime, account_id("dim2.paseo", 0));
    }
    assert_eq!(prompts(&platform), (1, 0, 0));

    sign_payload_with(&runtime, account_id("dim2.paseo", 1));
    assert_eq!(
        prompts(&platform),
        (1, 0, 0),
        "another account of the same product does not ask again",
    );

    cache_grant(&platform, "stash.paseo", r#"{"dim2next":["context"]}"#);
    sign_payload_with(&runtime, account_id("stash.paseo", 0));
    assert_eq!(prompts(&platform), (2, 0, 0), "another product asks again");

    for _ in 0..2 {
        sign_raw_with(&runtime, account_id("dim2.paseo", 0));
        create_transaction_with(&runtime, account_id("dim2.paseo", 0));
    }
    assert_eq!(prompts(&platform), (2, 1, 1), "each kind asks once");
}

/// A refusal is not an answer for later: one mistaken tap must not leave the
/// product unable to sign until it is reopened.
#[test]
fn a_refused_cross_product_signature_asks_again() {
    let platform = Arc::new(StubPlatform {
        sign_payload_confirmed: false,
        ..Default::default()
    });
    let runtime = dim2next_runtime(platform.clone());

    for _ in 0..2 {
        futures::executor::block_on(runtime.sign_payload(
            &CallContext::default(),
            HostSignPayloadRequest::V1(v01::HostSignPayloadRequest {
                account: account_id("dim2.paseo", 0),
                payload: crate::test_support::sign_payload_data(),
            }),
        ))
        .expect_err("the user refused");
    }

    assert_eq!(prompts(&platform), (2, 0, 0));
}

/// Unwatermarked bytes cannot be told apart from a transaction, so an earlier
/// approval must not let them through, just as an AutoSigning grant does not.
#[test]
fn unwatermarked_raw_bytes_are_confirmed_every_time() {
    let platform = Arc::new(StubPlatform {
        sign_raw_confirmed: true,
        ..Default::default()
    });
    let runtime = dim2next_runtime(platform.clone());

    for _ in 0..2 {
        #[allow(deprecated)]
        futures::executor::block_on(runtime.sign_raw_unwatermarked_deprecated(
            &CallContext::default(),
            HostSignRawRequest::V1(v01::HostSignRawRequest {
                account: account_id("dim2.paseo", 0),
                payload: crate::test_support::raw_payload(),
            }),
        ))
        .expect("the grant admits the account and the user approves");
    }

    assert_eq!(prompts(&platform), (0, 2, 0));
}

/// A signature the user already approved goes through while a prompt about
/// another one is still open, rather than queueing behind a question it does
/// not need answered.
#[test]
fn an_approved_signature_does_not_wait_for_another_prompt() {
    let (_release, gate) = futures::channel::oneshot::channel();
    let platform = Arc::new(StubPlatform {
        sign_payload_confirmed: true,
        create_transaction_confirmed: true,
        create_transaction_confirmation_gate: std::sync::Mutex::new(Some(gate)),
        ..Default::default()
    });
    let runtime = dim2next_runtime(platform.clone());
    sign_payload_with(&runtime, account_id("dim2.paseo", 0));

    let cx = CallContext::default();
    let mut payload = crate::test_support::product_tx_payload("dim2.paseo");
    payload.signer = account_id("dim2.paseo", 0);
    let mut transaction = Box::pin(
        runtime.create_transaction(&cx, HostCreateTransactionRequest::V1(payload)),
    );
    assert!(transaction.as_mut().now_or_never().is_none());
    assert_eq!(prompts(&platform), (1, 0, 1), "the transaction prompt is open");

    let signed = runtime
        .sign_payload(
            &cx,
            HostSignPayloadRequest::V1(v01::HostSignPayloadRequest {
                account: account_id("dim2.paseo", 0),
                payload: crate::test_support::sign_payload_data(),
            }),
        )
        .now_or_never();
    assert!(
        matches!(signed, Some(Ok(_))),
        "the approved payload signature does not wait: {signed:?}",
    );
}

