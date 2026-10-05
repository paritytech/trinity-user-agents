//! An AutoSigning grant or a blessed caller waives the per-call confirmation on
//! the signing role.

use super::*;
use crate::host_internal::sso_messages::{RemoteMessage, RemoteMessageData, v1};
use crate::runtime::signing_host::SsoAccountHolderService;
use crate::runtime::sso_service::Dispatch;
use truapi::versioned::account::HostAccountSignVrfRequest;
use truapi::versioned::signing::HostSignRawWithLegacyAccountRequest;

/// Allocate an AutoSigning grant for the runtime's own product.
fn grant_auto_signing(runtime: &ProductRuntimeHost<WalletAccountHolder>) {
    let allocation = futures::executor::block_on(ResourceAllocation::request(
        runtime,
        &CallContext::default(),
        HostRequestResourceAllocationRequest::V1(v01::HostRequestResourceAllocationRequest {
            resources: vec![v01::AllocatableResource::AutoSigning],
        }),
    ))
    .expect("approved AutoSigning allocation succeeds");
    let HostRequestResourceAllocationResponse::V1(allocation) = allocation;
    assert_eq!(allocation.outcomes, vec![v01::AllocationOutcome::Allocated]);
}

/// A platform that approves the allocation and declines every signing prompt,
/// so any call that succeeds did so without asking.
fn granting_platform() -> Arc<StubPlatform> {
    Arc::new(StubPlatform {
        resource_allocation_confirmed: true,
        sign_raw_confirmed: false,
        sign_payload_confirmed: false,
        create_transaction_confirmed: false,
        ..StubPlatform::default()
    })
}

fn raw_request(product_id: &str) -> HostSignRawRequest {
    HostSignRawRequest::V1(v01::HostSignRawRequest {
        account: v01::ProductAccountId {
            dot_ns_identifier: product_id.to_string(),
            derivation_index: v01::DerivationIndex::Index(0),
        },
        payload: v01::RawPayload::Bytes {
            bytes: b"hello world".to_vec(),
        },
    })
}

#[test]
fn a_granted_product_signs_raw_without_a_prompt() {
    let platform = granting_platform();
    let (services, activation) = signing_runtime_with_platform(platform.clone());
    futures::executor::block_on(activation.activate_local_session(ENTROPY.to_vec()))
        .expect("activation succeeds");
    let runtime = product_runtime(services, activation);
    grant_auto_signing(&runtime);

    let HostSignRawResponse::V1(response) = futures::executor::block_on(
        runtime.sign_raw(&CallContext::default(), raw_request("myapp.dot")),
    )
    .expect("a granted product signs without confirmation");

    assert!(
        platform
            .sign_raw_reviews
            .lock()
            .expect("raw signing review list mutex poisoned")
            .is_empty(),
        "the grant waives the prompt",
    );
    let root = derive_root_keypair_from_entropy(&ENTROPY).unwrap();
    let keypair = derive_product_keypair(&root, "myapp.dot", index_bytes(0)).unwrap();
    let signature =
        schnorrkel::Signature::from_bytes(&response.signature).expect("64-byte signature");
    assert!(
        keypair
            .public
            .verify_simple(b"substrate", b"<Bytes>hello world</Bytes>", &signature)
            .is_ok(),
        "the grant signs the same bytes the prompt would have shown",
    );
}

#[test]
fn an_ungranted_product_still_prompts_for_raw_signing() {
    let platform = granting_platform();
    let (services, activation) = signing_runtime_with_platform(platform.clone());
    futures::executor::block_on(activation.activate_local_session(ENTROPY.to_vec()))
        .expect("activation succeeds");
    let runtime = product_runtime(services, activation);

    let error = futures::executor::block_on(
        runtime.sign_raw(&CallContext::default(), raw_request("myapp.dot")),
    )
    .expect_err("the stub declines the confirmation");

    assert!(matches!(
        error,
        CallError::Domain(HostSignRawError::V1(v01::HostSignPayloadError::Rejected))
    ));
    assert_eq!(
        platform
            .sign_raw_reviews
            .lock()
            .expect("raw signing review list mutex poisoned")
            .len(),
        1,
        "without a grant the user is asked exactly once",
    );
}

#[test]
fn a_grant_does_not_cover_the_unwatermarked_raw_signing_api() {
    // The deprecated API's signatures are not domain-separated from
    // transaction signatures, so a standing grant must not waive its prompt.
    let platform = granting_platform();
    let (services, activation) = signing_runtime_with_platform(platform.clone());
    futures::executor::block_on(activation.activate_local_session(ENTROPY.to_vec()))
        .expect("activation succeeds");
    let runtime = product_runtime(services, activation);
    grant_auto_signing(&runtime);

    #[allow(deprecated)]
    let error = futures::executor::block_on(
        runtime
            .sign_raw_unwatermarked_deprecated(&CallContext::default(), raw_request("myapp.dot")),
    )
    .expect_err("the stub declines the confirmation");

    assert!(matches!(
        error,
        CallError::Domain(HostSignRawError::V1(v01::HostSignPayloadError::Rejected))
    ));
    assert_eq!(
        platform
            .sign_raw_reviews
            .lock()
            .expect("raw signing review list mutex poisoned")
            .len(),
        1,
        "the unwatermarked API prompts whatever the grant says",
    );
}

#[test]
fn a_blessed_product_signs_its_own_account_without_a_grant() {
    for (product_id, blessed) in [("dim2.paseo", true), ("app.dim2.paseo", false)] {
        let platform = granting_platform();
        let (services, activation) = signing_runtime_with_platform(platform.clone());
        futures::executor::block_on(activation.activate_local_session(ENTROPY.to_vec()))
            .expect("activation succeeds");
        let runtime = product_runtime_for(services, activation, product_id);

        let signed = futures::executor::block_on(
            runtime.sign_raw(&CallContext::default(), raw_request(product_id)),
        )
        .is_ok();

        assert_eq!(
            (signed, platform.sign_raw_reviews.lock().unwrap().len()),
            (blessed, usize::from(!blessed)),
            "{product_id}",
        );
    }
}

/// Only the local runtime vouches for its caller, and only for its own account;
/// a relayed request can claim any product id.
#[test]
fn a_blessed_vrf_signature_skips_the_prompt_only_locally_for_its_own_account() {
    let platform = granting_platform();
    let (services, activation) = signing_runtime_with_platform(platform.clone());
    futures::executor::block_on(activation.activate_local_session(ENTROPY.to_vec()))
        .expect("activation succeeds");
    let runtime = product_runtime_for(services, activation.clone(), "dim2.paseo");
    let request = vrf_request("dim2.paseo");

    let sign_locally = |request| {
        futures::executor::block_on(
            runtime.sign_vrf(&CallContext::default(), HostAccountSignVrfRequest::V1(request)),
        )
        .is_ok()
    };
    let own_signed = sign_locally(request.clone());
    let foreign_signed = sign_locally(vrf_request("other.paseo"));
    let Dispatch::Response(answer) = futures::executor::block_on(
        SsoAccountHolderService::new(activation).answer(RemoteMessage::request(
            "relayed-vrf".to_string(),
            ProductRequest {
                calling_product_id: "dim2.paseo".to_string(),
                payload: request,
            },
        )),
    ) else {
        panic!("expected a VRF response")
    };
    let RemoteMessageData::V1(v1::RemoteMessage::SignVrfResponse(response)) = answer.message.data
    else {
        panic!("expected a VRF signing response")
    };

    assert_eq!(
        (
            own_signed,
            foreign_signed,
            response.payload.map(|_| ()),
            platform.sign_vrf_reviews.lock().unwrap().len(),
        ),
        (true, false, Err(v01::HostAccountSignVrfError::Rejected), 2),
    );
}

/// Legacy accounts sign with the user's own keys, not a product's.
#[test]
fn a_blessed_product_still_confirms_legacy_account_signing() {
    let platform = granting_platform();
    let (services, activation) = signing_runtime_with_platform(platform.clone());
    futures::executor::block_on(activation.activate_local_session(ENTROPY.to_vec()))
        .expect("activation succeeds");
    let runtime = product_runtime_for(services, activation, "dim2.paseo");
    let identity = derive_identity_keypair(&ENTROPY, TEST_NETWORK_SUFFIX).unwrap();
    let request =
        HostSignRawWithLegacyAccountRequest::V1(v01::HostSignRawWithLegacyAccountRequest {
            signer: subxt::utils::AccountId32(identity.public.to_bytes()).to_string(),
            payload: v01::RawPayload::Bytes {
                bytes: b"hello world".to_vec(),
            },
        });

    let signed = futures::executor::block_on(
        runtime.sign_raw_with_legacy_account(&CallContext::default(), request),
    )
    .is_ok();

    assert_eq!(
        (signed, platform.sign_raw_reviews.lock().unwrap().len()),
        (false, 1),
    );
}

#[test]
fn native_authorization_does_not_approve_an_incoming_sso_operation() {
    let platform = granting_platform();
    let (services, holder) = signing_runtime_with_platform(platform.clone());
    futures::executor::block_on(holder.activate_local_session(ENTROPY.to_vec())).unwrap();
    let runtime = product_runtime(services, holder.clone());
    grant_auto_signing(&runtime);
    let service = SsoAccountHolderService::new(holder);
    let Dispatch::Response(answer) = futures::executor::block_on(service.answer(RemoteMessage::request(
        "remote-after-native-grant".to_string(),
        ProductRequest { calling_product_id: "myapp.dot".to_string(), payload: vrf_request("myapp.dot") },
    ))) else { panic!("expected response"); };
    let RemoteMessageData::V1(v1::RemoteMessage::SignVrfResponse(response)) = answer.message.data else { panic!("expected VRF response"); };
    let native = futures::executor::block_on(runtime.sign_vrf(&CallContext::default(), HostAccountSignVrfRequest::V1(vrf_request("myapp.dot"))));
    assert_eq!((response.payload.map(|_| ()), native.is_ok(), platform.sign_vrf_reviews.lock().unwrap().len()), (Err(v01::HostAccountSignVrfError::Rejected), true, 1));
}

#[test]
fn an_exported_sso_grant_does_not_populate_native_authorization() {
    use crate::host_internal::sso_messages::{ResourceAllocationRequest, OnExistingAllowancePolicy, SsoAllocationOutcome, SsoAllocatedResource};
    let platform = granting_platform();
    let (services, holder) = signing_runtime_with_platform(platform.clone());
    futures::executor::block_on(holder.activate_local_session(ENTROPY.to_vec())).unwrap();
    let runtime = product_runtime(services, holder.clone());
    let service = SsoAccountHolderService::new(holder);
    let Dispatch::Response(answer) = futures::executor::block_on(service.answer(RemoteMessage::request(
        "remote-allocation".to_string(), ResourceAllocationRequest { calling_product_id: "myapp.dot".to_string(), resources: vec![v01::AllocatableResource::AutoSigning], on_existing: OnExistingAllowancePolicy::Increase },
    ))) else { panic!("expected response"); };
    let RemoteMessageData::V1(v1::RemoteMessage::ResourceAllocationResponse(response)) = answer.message.data else { panic!("expected allocation response"); };
    assert!(matches!(response.payload.unwrap().as_slice(), [SsoAllocationOutcome::Allocated(SsoAllocatedResource::AutoSigning { .. })]));
    let native = futures::executor::block_on(runtime.sign_raw(&CallContext::default(), raw_request("myapp.dot")));
    assert_eq!((native.is_ok(), platform.sign_raw_reviews.lock().unwrap().len()), (false, 1));
}

#[test]
fn an_sso_service_cannot_follow_a_wallet_reactivation() {
    use crate::host_internal::sso_messages::ProductSubtreeRequest;
    let (services, holder) = signing_runtime_with_platform(granting_platform());
    futures::executor::block_on(holder.activate_local_session(ENTROPY.to_vec())).unwrap();
    let service = SsoAccountHolderService::new(holder.clone());
    let runtime = product_runtime(services, holder.clone());
    futures::executor::block_on(runtime.authority.disconnect()).unwrap();
    futures::executor::block_on(holder.activate_local_session(ENTROPY.to_vec())).unwrap();
    let Dispatch::Response(answer) = futures::executor::block_on(service.answer(RemoteMessage::request("old-channel".to_string(), ProductSubtreeRequest { product_id: "myapp.dot".to_string() }))) else { panic!("expected response"); };
    let RemoteMessageData::V1(v1::RemoteMessage::ProductSubtreeResponse(response)) = answer.message.data else { panic!("expected subtree response"); };
    assert!(response.payload.is_err());
}
