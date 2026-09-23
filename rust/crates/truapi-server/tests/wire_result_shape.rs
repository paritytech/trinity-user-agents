//! Result-wire-shape regression test.
//!
//! The TS host/client codec expects every request/response frame to be
//! `Result<{Method}Response, CallError<{Method}Error>>`, and every
//! subscription's `Interrupt` frame to be `Result<(), CallError<{Method}Error>>`
//! — both leg types already-versioned wrappers (their own `V<N>` tag is the
//! wire's only version signal), with which leg a frame carries named by the
//! outer wire's own `messageType` byte rather than anything inside these
//! payloads. This test stands up a `TrUApiCore::from_platform_with_config`
//! with a platform whose `Features` impl returns `Ok(supported = true)` and
//! asserts:
//!
//! - A `system_feature_supported` request produces a response frame with
//!   `messageType = Response` whose payload begins with `0x00` (Result::Ok),
//!   `0x00` (the response wrapper's own V1 tag), followed by the encoded
//!   `HostFeatureSupportedResponse`.
//! - A `local_storage_read` request whose stub returns
//!   `Err(HostLocalStorageReadError::Full)` produces a response frame with
//!   `messageType = Response` whose payload begins with `0x01`
//!   (Result::Err), `0x00` (CallError::Domain), `0x00` (the error wrapper's
//!   own V1 tag), followed by the encoded `HostLocalStorageReadError::Full`.
//!
//! Both halves prove the wire layout stays in lockstep with the TS
//! `S.Result(okCodec, S.CallError(errCodec))` composition the generated
//! client decodes a `Response` frame's payload through.

use std::sync::Arc;

use parity_scale_codec::{Decode, Encode};

use truapi::{CallError, v01, versioned::account};

use truapi_server::core::TrUApiCore;
use truapi_server::frame::{
    MESSAGE_TYPE_INTERRUPT, MESSAGE_TYPE_REQUEST, MESSAGE_TYPE_RESPONSE, MESSAGE_TYPE_START,
    PROTOCOL_ERROR_METHOD_ID, PROTOCOL_ERROR_TRAIT_ID, Payload, ProtocolErrorV1, ProtocolMessage,
    VersionedProtocolError, request_ids, subscription_ids,
};

mod common;
use common::{RecordingTransport, WireShapePlatform, test_runtime_config, test_spawner};

const PAYMENTS_NOT_IMPLEMENTED: &str = "Payments are not supported in dot.li";

fn dispatch(core: &TrUApiCore, frame: ProtocolMessage) -> ProtocolMessage {
    let encoded = frame.encode();
    let response_bytes = futures::executor::block_on(core.receive_from_product(&encoded))
        .expect("dispatcher emitted a response frame");
    ProtocolMessage::decode(&mut &response_bytes[..]).expect("decode response")
}

#[test]
fn feature_supported_ok_response_uses_ok_discriminant() {
    let core = make_core();
    let request = v01::HostFeatureSupportedRequest::Chain {
        genesis_hash: vec![0u8; 32],
    };
    let ids = request_ids("system_feature_supported").expect("known request method");
    let frame = ProtocolMessage {
        request_id: "p:1".into(),
        payload: Payload {
            trait_id: ids.trait_id,
            method_id: ids.method_id,
            message_type: MESSAGE_TYPE_REQUEST,
            value: truapi::versioned::system::HostFeatureSupportedRequest::V1(request).encode(),
        },
    };
    let response = dispatch(&core, frame);
    assert_eq!(response.request_id, "p:1");
    assert_eq!(response.payload.trait_id, ids.trait_id);
    assert_eq!(response.payload.method_id, ids.method_id);
    assert_eq!(response.payload.message_type, MESSAGE_TYPE_RESPONSE);

    let expected: Result<
        truapi::versioned::system::HostFeatureSupportedResponse,
        CallError<truapi::versioned::system::HostFeatureSupportedError>,
    > = Ok(truapi::versioned::system::HostFeatureSupportedResponse::V1(
        v01::HostFeatureSupportedResponse { supported: true },
    ));
    assert_eq!(response.payload.value, expected.encode());
    // [Result::Ok=0x00][response wrapper V1=0x00][encoded response body...].
    assert_eq!(response.payload.value.first(), Some(&0x00));
    assert_eq!(response.payload.value.get(1), Some(&0x00));
}

#[test]
fn get_chain_info_ok_response_round_trips_over_the_wire() {
    let core = make_core();
    let request = v01::RemoteChainInfoRequest {
        chain: v01::ChainIdentifier::AssetHub,
    };
    let ids = request_ids("chain_get_chain_info").expect("known request method");
    let frame = ProtocolMessage {
        request_id: "p:9".into(),
        payload: Payload {
            trait_id: ids.trait_id,
            method_id: ids.method_id,
            message_type: MESSAGE_TYPE_REQUEST,
            value: truapi::versioned::chain::RemoteChainInfoRequest::V1(request).encode(),
        },
    };
    let response = dispatch(&core, frame);
    assert_eq!(response.request_id, "p:9");
    assert_eq!(response.payload.trait_id, ids.trait_id);
    assert_eq!(response.payload.method_id, ids.method_id);
    assert_eq!(response.payload.message_type, MESSAGE_TYPE_RESPONSE);

    let expected: Result<
        truapi::versioned::chain::RemoteChainInfoResponse,
        CallError<truapi::versioned::chain::RemoteChainInfoError>,
    > = Ok(truapi::versioned::chain::RemoteChainInfoResponse::V1(
        v01::RemoteChainInfoResponse {
            network: "paseo".to_string(),
            chain: v01::ChainIdentifier::AssetHub,
            genesis_hash: [0xaa; 32],
        },
    ));
    assert_eq!(response.payload.value, expected.encode());
}

#[test]
fn get_chain_info_unserved_chain_uses_err_discriminant() {
    let core = make_core();
    let request = v01::RemoteChainInfoRequest {
        chain: v01::ChainIdentifier::Bulletin,
    };
    let ids = request_ids("chain_get_chain_info").expect("known request method");
    let frame = ProtocolMessage {
        request_id: "p:10".into(),
        payload: Payload {
            trait_id: ids.trait_id,
            method_id: ids.method_id,
            message_type: MESSAGE_TYPE_REQUEST,
            value: truapi::versioned::chain::RemoteChainInfoRequest::V1(request).encode(),
        },
    };
    let response = dispatch(&core, frame);
    assert_eq!(response.payload.trait_id, ids.trait_id);
    assert_eq!(response.payload.method_id, ids.method_id);
    assert_eq!(response.payload.message_type, MESSAGE_TYPE_RESPONSE);

    assert_eq!(
        response.payload.value,
        versioned_result_err_payload(truapi::versioned::chain::RemoteChainInfoError::V1(
            v01::RemoteChainInfoError::NotSupported
        ))
    );
}

#[test]
fn local_storage_read_err_response_uses_err_discriminant() {
    let core = make_core();
    let request = v01::HostLocalStorageReadRequest {
        key: "missing".to_string(),
    };
    let ids = request_ids("local_storage_read").expect("known request method");
    let frame = ProtocolMessage {
        request_id: "p:2".into(),
        payload: Payload {
            trait_id: ids.trait_id,
            method_id: ids.method_id,
            message_type: MESSAGE_TYPE_REQUEST,
            value: truapi::versioned::local_storage::HostLocalStorageReadRequest::V1(request)
                .encode(),
        },
    };
    let response = dispatch(&core, frame);
    assert_eq!(response.request_id, "p:2");
    assert_eq!(response.payload.trait_id, ids.trait_id);
    assert_eq!(response.payload.method_id, ids.method_id);
    assert_eq!(response.payload.message_type, MESSAGE_TYPE_RESPONSE);

    let expected = versioned_result_err_payload(
        truapi::versioned::local_storage::HostLocalStorageReadError::V1(
            v01::HostLocalStorageReadError::Full,
        ),
    );
    assert_eq!(response.payload.value, expected);
    // [Result::Err=0x01][CallError::Domain=0x00][error wrapper V1=0x00]...
    assert_eq!(response.payload.value.first(), Some(&0x01));
    assert_eq!(response.payload.value.get(1), Some(&0x00));
}

/// Expected bytes for a request/response method's `Response`-leg payload
/// answering with a domain error: `[Result::Err=0x01][CallError::Domain=0x00]
/// [encoded, already-versioned error wrapper]`. `wrapped_error` is the
/// method's own `{Method}Error::V<N>(domain_error)` value.
fn versioned_result_err_payload<Wrapper: Encode>(wrapped_error: Wrapper) -> Vec<u8> {
    let mut expected = vec![0x01u8, 0x00u8];
    wrapped_error.encode_to(&mut expected);
    expected
}

/// Expected bytes for a subscription's `Interrupt`-leg payload ending with a
/// domain error: `[Result::Err=0x01][CallError::Domain=0x00][encoded,
/// already-versioned error wrapper]`.
fn versioned_interrupt_err_payload<Wrapper: Encode>(wrapped_error: Wrapper) -> Vec<u8> {
    let mut expected = vec![0x01u8, 0x00u8];
    wrapped_error.encode_to(&mut expected);
    expected
}

fn assert_request_returns_domain_error<Wrapper: Encode>(
    core: &TrUApiCore,
    request_id: &str,
    method: &str,
    value: Vec<u8>,
    wrapped_error: Wrapper,
) {
    let ids = request_ids(method).expect("known request method");
    let response = dispatch(
        core,
        ProtocolMessage {
            request_id: request_id.into(),
            payload: Payload {
                trait_id: ids.trait_id,
                method_id: ids.method_id,
                message_type: MESSAGE_TYPE_REQUEST,
                value,
            },
        },
    );
    assert_eq!(response.request_id, request_id);
    assert_eq!(response.payload.trait_id, ids.trait_id);
    assert_eq!(response.payload.method_id, ids.method_id);
    assert_eq!(response.payload.message_type, MESSAGE_TYPE_RESPONSE);
    assert_eq!(
        response.payload.value,
        versioned_result_err_payload(wrapped_error)
    );
}

fn assert_subscription_start_interrupts_error<Wrapper: Encode>(
    core: &TrUApiCore,
    request_id: &str,
    method: &str,
    value: Vec<u8>,
    wrapped_error: Wrapper,
) {
    let ids = subscription_ids(method).expect("known subscription method");
    let transport = Arc::new(RecordingTransport::default());
    futures::executor::block_on(core.dispatch(
        ProtocolMessage {
            request_id: request_id.into(),
            payload: Payload {
                trait_id: ids.trait_id,
                method_id: ids.method_id,
                message_type: MESSAGE_TYPE_START,
                value,
            },
        },
        transport.clone(),
    ));

    transport.wait_for(1, std::time::Duration::from_secs(5));

    let sent = transport.sent.lock().unwrap();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].request_id, request_id);
    assert_eq!(sent[0].payload.trait_id, ids.trait_id);
    assert_eq!(sent[0].payload.method_id, ids.method_id);
    assert_eq!(sent[0].payload.message_type, MESSAGE_TYPE_INTERRUPT);
    assert_eq!(
        sent[0].payload.value,
        versioned_interrupt_err_payload(wrapped_error)
    );
}

#[test]
fn foreign_account_proof_encodes_a_domain_refusal() {
    let core = make_core();
    let request = v01::HostAccountCreateProofRequest {
        key_handle: v01::ProductAccountId {
            dot_ns_identifier: "peopl.dot".to_string(),
            derivation_index: v01::DerivationIndex::Index(0),
        },
        context: v01::ProductProofContext {
            product_id: "myapp.dot".to_string(),
            suffix: v01::DerivationIndex::Index(0),
        },
        ring_location: v01::RingLocation {
            chain_id: [0u8; 32],
            junctions: vec![v01::RingLocationJunction::PalletInstance(0)],
        },
        message: Vec::new(),
    };

    let ids = request_ids("account_create_account_proof").expect("known request method");
    let response = dispatch(
        &core,
        ProtocolMessage {
            request_id: "p:account-proof".into(),
            payload: Payload {
                trait_id: ids.trait_id,
                method_id: ids.method_id,
                message_type: MESSAGE_TYPE_REQUEST,
                value: truapi::versioned::account::HostAccountCreateProofRequest::V1(request)
                    .encode(),
            },
        },
    );
    assert_eq!(response.request_id, "p:account-proof");
    assert_eq!(response.payload.trait_id, ids.trait_id);
    assert_eq!(response.payload.method_id, ids.method_id);
    assert_eq!(response.payload.message_type, MESSAGE_TYPE_RESPONSE);
    // RFC-0024 forbids a prompt fallback for a bearer proof made with a foreign
    // key. What this pins is the wire shape of the refusal: an encoded domain
    // error rather than a transport failure or a success.
    //
    // It does NOT pin "without confirmation", which the old name claimed. The
    // stub platform answers `confirm_user_action` with `Ok(false)` and records
    // nothing, and the test holds no handle to it, so a confirmation could be
    // asked and denied and this would still pass. That property is asserted in
    // `runtime::signing_host::tests` against a recording platform.
    //
    // It does NOT pin which refusal. `create_account_proof` consults the session
    // before the grant (#655), so with no session this is the session guard's
    // answer and says nothing about allowlisting. Giving the core a session does
    // not fix that either: the authority picks one up asynchronously, so the
    // assertion would race the dispatch. That the grant is what refuses a
    // foreign handle is asserted in
    // `runtime::signing_host::tests::a_foreign_proof_is_refused_when_the_owner_granted_nothing`
    // and end to end by `make e2e-cross-product-ringvrf`.
    let expected =
        versioned_result_err_payload(truapi::versioned::account::HostAccountCreateProofError::V1(
            v01::HostAccountCreateProofError::Rejected,
        ));
    assert_eq!(response.payload.value, expected);
}

#[test]
fn deferred_payment_requests_return_dotli_not_implemented_errors() {
    let core = make_core();
    let request = v01::HostPaymentRequest {
        from: None,
        amount: 1,
        destination: [0u8; 32],
    };

    assert_request_returns_domain_error(
        &core,
        "p:payment",
        "payment_request",
        truapi::versioned::payment::HostPaymentRequest::V1(request).encode(),
        truapi::versioned::payment::HostPaymentError::V1(v01::HostPaymentError::Unknown {
            reason: PAYMENTS_NOT_IMPLEMENTED.to_string(),
        }),
    );
}

#[test]
fn deferred_payment_subscriptions_interrupt_dotli_not_implemented_errors() {
    let core = make_core();
    let balance = v01::HostPaymentBalanceSubscribeRequest { purse: None };
    assert_subscription_start_interrupts_error(
        &core,
        "p:balance",
        "payment_balance_subscribe",
        truapi::versioned::payment::HostPaymentBalanceSubscribeRequest::V1(balance).encode(),
        truapi::versioned::payment::HostPaymentBalanceSubscribeError::V1(
            v01::HostPaymentBalanceSubscribeError::PermissionDenied,
        ),
    );

    let status = v01::HostPaymentStatusSubscribeRequest {
        payment_id: "payment-id".to_string(),
    };
    assert_subscription_start_interrupts_error(
        &core,
        "p:status",
        "payment_status_subscribe",
        truapi::versioned::payment::HostPaymentStatusSubscribeRequest::V1(status).encode(),
        truapi::versioned::payment::HostPaymentStatusSubscribeError::V1(
            v01::HostPaymentStatusSubscribeError::Unknown {
                reason: PAYMENTS_NOT_IMPLEMENTED.to_string(),
            },
        ),
    );
}

#[test]
fn statement_store_subscribe_topic_limit_interrupts_with_typed_error() {
    let core = make_core();
    let request = v01::RemoteStatementStoreSubscribeRequest::MatchAny(vec![[7u8; 32]; 129]);

    assert_subscription_start_interrupts_error(
        &core,
        "p:ss-too-many",
        "statement_store_subscribe",
        truapi::versioned::statement_store::RemoteStatementStoreSubscribeRequest::V1(request)
            .encode(),
        truapi::versioned::statement_store::RemoteStatementStoreSubscribeError::V1(
            v01::GenericError {
                reason: "MatchAny has 129 topics, maximum is 128".to_string(),
            },
        ),
    );
}

#[test]
fn malformed_result_subscription_start_interrupts_with_malformed_frame() {
    let core = make_core();
    let method = "payment_balance_subscribe";
    let ids = subscription_ids(method).expect("known subscription method");
    let transport = Arc::new(RecordingTransport::default());

    futures::executor::block_on(core.dispatch(
        ProtocolMessage {
            request_id: "p:malformed-sub".into(),
            payload: Payload {
                trait_id: ids.trait_id,
                method_id: ids.method_id,
                message_type: MESSAGE_TYPE_START,
                value: vec![0xff],
            },
        },
        transport.clone(),
    ));

    let sent = transport.sent.lock().unwrap();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].request_id, "p:malformed-sub");
    assert_eq!(sent[0].payload.trait_id, ids.trait_id);
    assert_eq!(sent[0].payload.method_id, ids.method_id);
    assert_eq!(sent[0].payload.message_type, MESSAGE_TYPE_INTERRUPT);
    assert_eq!(sent[0].payload.value.first(), Some(&0x01), "Result::Err");
    assert_eq!(
        sent[0].payload.value.get(1),
        Some(&0x03),
        "CallError::MalformedFrame"
    );

    let mut payload = &sent[0].payload.value[..];
    let interrupt = Result::<
        (),
        CallError<truapi::versioned::payment::HostPaymentBalanceSubscribeError>,
    >::decode(&mut payload)
    .expect("decode malformed interrupt error");
    assert!(payload.is_empty());
    match interrupt {
        Err(CallError::MalformedFrame { reason }) => assert!(!reason.is_empty()),
        other => panic!("expected MalformedFrame interrupt, got {other:?}"),
    }
}

#[test]
fn product_device_chat_initialize_reaches_authorization_at_method_twelve() {
    let core = make_core();
    let request = account::HostProductDeviceChatRequest::V2(
        truapi::latest::HostProductDeviceChatRequest::Initialize,
    );
    let response = dispatch(
        &core,
        ProtocolMessage {
            request_id: "p:product-device-chat".into(),
            payload: Payload {
                trait_id: 2,
                method_id: 12,
                message_type: MESSAGE_TYPE_REQUEST,
                value: request.encode(),
            },
        },
    );
    assert_eq!(response.request_id, "p:product-device-chat");
    assert_eq!(response.payload.trait_id, 2);
    assert_eq!(response.payload.method_id, 12);
    assert_eq!(response.payload.message_type, MESSAGE_TYPE_RESPONSE);
    assert_eq!(
        response.payload.value,
        versioned_result_err_payload(account::HostProductDeviceChatError::V1(
            truapi::v02::HostProductDeviceChatError::NotConnected,
        )),
    );
}

#[test]
fn retired_high_level_chat_cannot_dispatch_at_method_twelve() {
    let core = make_core();
    let request = account::HostProductDeviceChatRequest::V1(
        truapi::v02::HostProductDeviceChatRequest::Initialize,
    );
    let response = dispatch(
        &core,
        ProtocolMessage {
            request_id: "p:retired-high-level-chat".into(),
            payload: Payload {
                trait_id: 2,
                method_id: 12,
                message_type: MESSAGE_TYPE_REQUEST,
                value: request.encode(),
            },
        },
    );
    let result = Result::<
        account::HostProductDeviceChatResponse,
        CallError<account::HostProductDeviceChatError>,
    >::decode(&mut response.payload.value.as_slice())
    .expect("retired request remains decodable");
    assert!(matches!(result, Err(CallError::HostFailure { .. })));
}

#[test]
fn retired_raw_chat_method_cannot_dispatch() {
    let core = make_core();
    // The retired V1 Identity request was [V1, Identity, ProductAccountId].
    // Keep a valid historical payload so rejection cannot be a decode failure.
    let mut raw_identity = vec![0, 4];
    v01::ProductAccountId {
        dot_ns_identifier: "chat.dot".into(),
        derivation_index: v01::DerivationIndex::Index(0),
    }
    .encode_to(&mut raw_identity);
    let response = dispatch(
        &core,
        ProtocolMessage {
            request_id: "p:retired-chat".into(),
            payload: Payload {
                trait_id: 2,
                method_id: 11,
                message_type: MESSAGE_TYPE_REQUEST,
                value: raw_identity,
            },
        },
    );
    assert_eq!(
        response,
        ProtocolMessage {
            request_id: "p:retired-chat".into(),
            payload: Payload {
                trait_id: PROTOCOL_ERROR_TRAIT_ID,
                method_id: PROTOCOL_ERROR_METHOD_ID,
                message_type: MESSAGE_TYPE_RESPONSE,
                value: VersionedProtocolError::V1(ProtocolErrorV1::UnsupportedMessage {
                    trait_id: 2,
                    method_id: 11,
                })
                .encode(),
            },
        },
    );
}

/// A chain follow that cannot reach its provider must end with the failure,
/// not with `Ok(())`. A clean end reaches the product as `complete`, which
/// reads as a chain that simply stopped having blocks to report.
#[test]
fn a_chain_follow_that_cannot_start_interrupts_with_the_failure() {
    let core = make_core();
    let ids = subscription_ids("chain_follow_head_subscribe").expect("known subscription method");
    let transport = Arc::new(RecordingTransport::default());
    let value = truapi::versioned::chain::RemoteChainHeadFollowRequest::V1(
        v01::RemoteChainHeadFollowRequest {
            genesis_hash: vec![0u8; 32],
            with_runtime: false,
        },
    )
    .encode();

    futures::executor::block_on(core.dispatch(
        ProtocolMessage {
            request_id: "p:chain-follow".into(),
            payload: Payload {
                trait_id: ids.trait_id,
                method_id: ids.method_id,
                message_type: MESSAGE_TYPE_START,
                value,
            },
        },
        transport.clone(),
    ));

    transport.wait_for(1, std::time::Duration::from_secs(5));

    let sent = transport.sent.lock().unwrap();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].payload.message_type, MESSAGE_TYPE_INTERRUPT);

    let mut payload = &sent[0].payload.value[..];
    let interrupt = Result::<(), CallError<v01::GenericError>>::decode(&mut payload)
        .expect("decode the follow interrupt");
    assert!(payload.is_empty());
    match interrupt {
        Err(CallError::HostFailure { reason }) => {
            assert!(
                reason.contains("remote_chain_head_follow"),
                "unexpected reason: {reason}"
            );
        }
        other => panic!("expected the follow failure, got {other:?}"),
    }
}

fn make_core() -> TrUApiCore {
    let (host_config, product) = test_runtime_config();
    TrUApiCore::from_platform_with_config(
        Arc::new(WireShapePlatform),
        host_config,
        product,
        test_spawner(),
    )
}

/// Untrusted product input that is not a decodable frame must be dropped
/// (return `None`), never panic. Exercises the decode-failure boundary in
/// `receive_from_product` that the happy-path tests above bypass.
#[test]
fn malformed_frames_are_dropped_without_panic() {
    let core = make_core();

    // Empty input and arbitrary garbage.
    assert!(futures::executor::block_on(core.receive_from_product(&[])).is_none());
    assert!(
        futures::executor::block_on(core.receive_from_product(&[0xff, 0xff, 0xff, 0xff])).is_none()
    );

    // A truncated SCALE string header (claims length but no body).
    assert!(
        futures::executor::block_on(core.receive_from_product(&[200u8 << 2, 0x61, 0x62])).is_none()
    );
}

#[test]
fn unknown_wire_discriminant_returns_correlated_protocol_error() {
    let core = make_core();
    let request = ProtocolMessage {
        request_id: "p:unknown".into(),
        payload: Payload {
            trait_id: 250,
            method_id: 249,
            message_type: MESSAGE_TYPE_REQUEST,
            value: vec![0, 0, 0, 0],
        },
    };
    let response_bytes = futures::executor::block_on(core.receive_from_product(&request.encode()))
        .expect("unknown message receives a protocol error");
    let response = ProtocolMessage::decode(&mut &response_bytes[..]).expect("decode response");
    assert_eq!(
        response,
        ProtocolMessage {
            request_id: "p:unknown".into(),
            payload: Payload {
                trait_id: PROTOCOL_ERROR_TRAIT_ID,
                method_id: PROTOCOL_ERROR_METHOD_ID,
                message_type: MESSAGE_TYPE_RESPONSE,
                value: VersionedProtocolError::V1(ProtocolErrorV1::UnsupportedMessage {
                    // echoed in arrival order; 250 != 249 so a transposed
                    // pair cannot pass this assertion
                    trait_id: 250,
                    method_id: 249,
                })
                .encode(),
            },
        }
    );
}

/// Drive a subscription through the encoded-frame boundary: `_start` yields
/// the initial `_receive`, then `_stop` tears it down so a later session
/// change produces no further frames. Covers the wire layer the in-crate
/// `subscription.rs` unit tests bypass.
#[test]
fn subscription_start_receive_stop_through_wire_boundary() {
    use std::time::{Duration, Instant};
    use truapi_server::frame::MESSAGE_TYPE_STOP;
    use truapi_server::transport::Transport;

    let core = make_core();
    let transport = Arc::new(RecordingTransport::default());
    let dyn_transport: Arc<dyn Transport> = transport.clone();

    let method = "account_connection_status_subscribe";
    let ids = subscription_ids(method).expect("known subscription method");
    let start = ProtocolMessage {
        request_id: "p:1".into(),
        payload: Payload {
            trait_id: ids.trait_id,
            method_id: ids.method_id,
            message_type: MESSAGE_TYPE_START,
            value: truapi::versioned::account::HostAccountConnectionStatusSubscribeRequest::V1
                .encode(),
        },
    };
    futures::executor::block_on(core.dispatch(start, dyn_transport.clone()));

    // Wait for the initial `_receive` item (Disconnected).
    let deadline = Instant::now() + Duration::from_secs(2);
    while transport.sent.lock().unwrap().is_empty() {
        assert!(Instant::now() < deadline, "no initial _receive frame");
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        transport.sent.lock().unwrap()[0].payload.trait_id,
        ids.trait_id
    );
    assert_eq!(
        transport.sent.lock().unwrap()[0].payload.method_id,
        ids.method_id
    );

    // Stop the subscription, then push a session change. A live subscription
    // would emit a Connected `_receive`; a stopped one must stay silent.
    let stop = ProtocolMessage {
        request_id: "p:1".into(),
        payload: Payload {
            trait_id: ids.trait_id,
            method_id: ids.method_id,
            message_type: MESSAGE_TYPE_STOP,
            value: Vec::new(),
        },
    };
    futures::executor::block_on(core.dispatch(stop, dyn_transport));
    std::thread::sleep(Duration::from_millis(50));

    core.session_state()
        .set_session(truapi_server::host_logic::session::SessionInfo {
            public_key: [7u8; 32],
            sso: None,
            root_entropy_source: None,
            identity_account_id: None,
            identity_chat_private_key: None,
            device_enc_public_key: None,
            lite_username: None,
            full_username: None,
        });
    std::thread::sleep(Duration::from_millis(50));

    assert_eq!(
        transport.sent.lock().unwrap().len(),
        1,
        "stopped subscription must emit no further frames"
    );
}

/// Coin Payment answers `Unsupported` rather than the trait default's
/// `HostFailure`, which a product's retry logic reads as transient.
#[test]
fn coin_payment_request_reports_unsupported_on_the_wire() {
    let core = make_core();
    let request = v01::HostCoinPaymentQueryPurseRequest {
        purse: v01::MAIN_PURSE,
    };
    let ids = request_ids("coin_payment_query_purse").expect("known request method");
    let frame = ProtocolMessage {
        request_id: "p:coin".into(),
        payload: Payload {
            trait_id: ids.trait_id,
            method_id: ids.method_id,
            message_type: MESSAGE_TYPE_REQUEST,
            value: truapi::versioned::coin_payment::HostCoinPaymentQueryPurseRequest::V1(request)
                .encode(),
        },
    };
    let response = dispatch(&core, frame);
    assert_eq!(response.payload.trait_id, ids.trait_id);
    assert_eq!(response.payload.method_id, ids.method_id);
    assert_eq!(response.payload.message_type, MESSAGE_TYPE_RESPONSE);
    // [Result::Err=0x01][CallError::Unsupported=0x02], and nothing more:
    // `Unsupported` carries no domain payload, so no wrapper tag follows.
    assert_eq!(response.payload.value, vec![0x01u8, 0x02u8]);
}
