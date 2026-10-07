use super::*;
use truapi::latest::{HostAccountCreateHonourProofRequest, HostAccountCreateHonourProofResponse};
use truapi::versioned::account::{
    HostAccountCreateHonourProofRequest as Request, HostAccountCreateHonourProofResponse as Answer,
};

fn request() -> HostAccountCreateHonourProofRequest {
    HostAccountCreateHonourProofRequest {
        key_handle: v01::ProductAccountId {
            dot_ns_identifier: "myapp.dot".to_string(),
            derivation_index: v01::DerivationIndex::Index(7),
        },
        ring_location: v01::RingLocation {
            chain_id: [0x22; 32],
            junctions: vec![v01::RingLocationJunction::CollectionId(
                b"pop:polkadot.network/people     ".to_vec(),
            )],
        },
        subject: [1; 32],
        point: 255,
        message: [2; 32],
    }
}

fn fixture(
    response: Result<HostAccountCreateHonourProofResponse, RingVrfError>,
) -> (ProductRuntimeHost, Arc<StubPlatform>, SessionInfo) {
    let session = sso_session_info();
    let platform = Arc::new(StubPlatform {
        sso_response_script: Some(sso_success_response_script(
            &session,
            RemoteMessage {
                message_id: "wallet-honour-1".to_string(),
                data: RemoteMessageData::V1(v1::RemoteMessage::CreateHonourProofResponse(
                    Response {
                        responding_to: "honour-1".to_string(),
                        payload: response,
                    },
                )),
            },
        )),
        ..Default::default()
    });
    let host = ProductRuntimeHost::new(
        platform.clone(),
        runtime_config("myapp.dot"),
        test_spawner(),
    );
    install_pairing_session(&host, session.clone());
    (host, platform, session)
}

#[test]
fn honour_proof_forwards_the_vote_and_returns_both_aliases() {
    let expected = HostAccountCreateHonourProofResponse {
        proof: vec![1, 2, 3],
        contextual_aliases: vec![
            v01::ContextualAlias {
                context: [3; 32],
                alias: vec![4; 32],
            },
            v01::ContextualAlias {
                context: [5; 32],
                alias: vec![6; 32],
            },
        ],
        ring_index: 7,
        ring_revision: 11,
    };
    let (host, platform, session) = fixture(Ok(expected.clone()));
    let payload = request();
    let response = futures::executor::block_on(host.create_honour_proof(
        &CallContext::with_request_id("honour-1".to_string()),
        Request::V1(payload.clone()),
    ))
    .unwrap();
    assert_eq!(response, Answer::V1(expected));
    let message = submitted_remote_message(&platform, &session);
    let RemoteMessageData::V1(v1::RemoteMessage::CreateHonourProofRequest(request)) = message.data
    else {
        panic!("expected an Honour proof request");
    };
    assert_eq!(request.calling_product_id, "myapp.dot");
    assert_eq!(request.payload, payload);
    assert!(platform.account_access_reviews.lock().unwrap().is_empty());
}

#[test]
fn honour_proof_maps_a_signing_host_membership_refusal() {
    let (host, _, _) = fixture(Err(RingVrfError::NotMember));
    assert_eq!(
        futures::executor::block_on(host.create_honour_proof(
            &CallContext::with_request_id("honour-1".to_string()),
            Request::V1(request()),
        )),
        Err(CallError::Domain(HostAccountCreateProofError::V1(
            v01::HostAccountCreateProofError::NotMember
        )))
    );
}

#[test]
fn honour_proof_requires_a_pairing_session() {
    let host =
        ProductRuntimeHost::new(stub_platform(), runtime_config("myapp.dot"), test_spawner());
    assert_eq!(
        futures::executor::block_on(
            host.create_honour_proof(&CallContext::default(), Request::V1(request()),)
        ),
        Err(CallError::Domain(HostAccountCreateProofError::V1(
            v01::HostAccountCreateProofError::Rejected
        )))
    );
}

#[test]
fn honour_proof_bounds_an_unresponsive_peer() {
    let platform = Arc::new(StubPlatform::default());
    let host = ProductRuntimeHost::new(
        platform.clone(),
        runtime_config("myapp.dot"),
        test_spawner(),
    );
    install_pairing_session(&host, sso_session_info());
    let mut cx = CallContext::default();
    cx.set_timeout(Duration::from_millis(10));
    let started = std::time::Instant::now();
    assert!(
        futures::executor::block_on(host.create_honour_proof(&cx, Request::V1(request()))).is_err()
    );
    assert!(matches!(
        cx.cancel().reason(),
        Some(CancellationReason::TimedOut { .. })
    ));
    assert!(started.elapsed() < AUTHORITY_CANCEL_UNWIND_GRACE + Duration::from_secs(1));
}

#[test]
fn honour_proof_reports_a_peer_that_cannot_decode_the_request() {
    let session = sso_session_info();
    let platform = Arc::new(StubPlatform {
        sso_response_script: Some(SsoResponseScript::DecodeRejected {
            session: session.clone(),
        }),
        ..Default::default()
    });
    let host = ProductRuntimeHost::new(platform, runtime_config("myapp.dot"), test_spawner());
    install_pairing_session(&host, session);
    let cx = CallContext::with_request_id("honour-1".to_string());
    let result = futures::executor::block_on(host.create_honour_proof(&cx, Request::V1(request())));
    assert!(
        matches!(result, Err(CallError::Domain(HostAccountCreateProofError::V1(
        v01::HostAccountCreateProofError::Unknown { reason }
    ))) if reason.contains("decodingFailed"))
    );
}
