//! A test host that answers allowances unchecked keeps preimage submissions in
//! the core, since the Bulletin allowance it hands out was never authorized on
//! chain.

use super::*;
use crate::host_internal::bulletin::preimage_key;
use futures::StreamExt;
use truapi::api::Preimage;
use truapi::versioned::preimage::{
    RemotePreimageLookupSubscribeItem, RemotePreimageLookupSubscribeRequest,
    RemotePreimageSubmitRequest, RemotePreimageSubmitResponse,
};

/// A platform that approves the submission and the allowance behind it.
fn approving_platform() -> Arc<StubPlatform> {
    Arc::new(StubPlatform {
        resource_allocation_confirmed: true,
        ..StubPlatform::default()
    })
}

/// A product runtime on a signing host with a local session.
fn activated(unchecked: bool, withheld: &[&str]) -> (ProductRuntimeHost, Arc<SigningHostRole>) {
    let (services, activation) = signing_runtime_with_platform(approving_platform());
    futures::executor::block_on(activation.activate_local_session(ENTROPY.to_vec()))
        .expect("activation succeeds");
    activation.set_grant_allowances_unchecked(unchecked);
    activation.set_withheld_resources(withheld.iter().map(|tag| tag.to_string()).collect());
    (product_runtime(services, activation.clone()), activation)
}

fn submit(
    runtime: &ProductRuntimeHost,
    value: &[u8],
) -> Result<RemotePreimageSubmitResponse, CallError<truapi::versioned::preimage::RemotePreimageSubmitError>> {
    futures::executor::block_on(Preimage::submit(
        runtime,
        &CallContext::default(),
        RemotePreimageSubmitRequest::V1(value.to_vec()),
    ))
}

#[test]
fn an_unchecked_test_host_answers_a_submit_with_the_content_key_and_serves_it_back() {
    let (runtime, _) = activated(true, &[]);
    let value = b"host-playground preimage".to_vec();

    // No Bulletin client is configured, so reaching the chain would fail here.
    let response = submit(&runtime, &value).expect("the submission stays local");
    assert_eq!(
        response,
        RemotePreimageSubmitResponse::V1(preimage_key(&value).to_vec())
    );

    let mut lookup = futures::executor::block_on(runtime.lookup_subscribe(
        &CallContext::default(),
        RemotePreimageLookupSubscribeRequest::V1(v01::RemotePreimageLookupSubscribeRequest {
            key: preimage_key(&value).to_vec(),
        }),
    ));
    assert_eq!(
        futures::executor::block_on(lookup.next()).expect("a lookup item"),
        Ok(RemotePreimageLookupSubscribeItem::V1(
            v01::RemotePreimageLookupSubscribeItem { value: Some(value) }
        ))
    );
}

/// Keeping the value local must not skip the allowance: a suite that withholds
/// it is proving its product survives the refusal.
#[test]
fn a_withheld_bulletin_allowance_still_refuses_the_submit() {
    let (runtime, _) = activated(true, &["BulletinAllowance"]);

    let error = submit(&runtime, b"refused").expect_err("the withheld allowance refuses it");
    assert_eq!(
        error,
        CallError::Domain(truapi::versioned::preimage::RemotePreimageSubmitError::V1(
            v01::PreimageSubmitError::Unknown {
                reason: "Bulletin allowance allocation was rejected by the signing host".to_string(),
            }
        ))
    );
}

#[test]
fn a_signing_host_sends_preimages_to_the_chain_unless_told_or_unchecked() {
    let (_, checked) = activated(false, &[]);
    let (_, unchecked) = activated(true, &[]);
    let (_, told) = activated(false, &[]);
    told.set_submit_preimages_locally(true);

    assert!(!ProductAuthority::submits_preimages_locally(&*checked));
    assert!(ProductAuthority::submits_preimages_locally(&*unchecked));
    assert!(ProductAuthority::submits_preimages_locally(&*told));
}
