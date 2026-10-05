use super::*;

#[test]
#[allow(deprecated)] // Exercise the temporary API's paired-host wire routing.
fn unwatermarked_signing_routes_product_and_legacy_accounts_without_downgrading() {
    use crate::host_internal::sso_messages::SignRequest;

    for legacy in [false, true] {
        let session = sso_session_info();
        let identity = session.identity_account_id.unwrap();
        let platform = Arc::new(StubPlatform {
            sign_raw_confirmed: true,
            sso_response_script: Some(sso_success_response_script(
                &session,
                sign_response_message("unwatermarked", vec![7, 7], None),
            )),
            ..Default::default()
        });
        let host = ProductRuntimeHost::new(
            platform.clone(),
            runtime_config("myapp.dot"),
            test_spawner(),
        );
        install_pairing_session(&host, session.clone());
        let cx = CallContext::with_request_id("unwatermarked".to_string());
        let payload = v01::RawPayload::Bytes {
            bytes: vec![0x11; 32],
        };
        let signature = futures::executor::block_on(async {
            if !legacy {
                let HostSignRawResponse::V1(response) = host
                    .sign_raw_unwatermarked_deprecated(
                        &cx,
                        HostSignRawRequest::V1(v01::HostSignRawRequest {
                            account: account_id("myapp.dot", 0),
                            payload: payload.clone(),
                        }),
                    )
                    .await
                    .unwrap();
                response.signature
            } else {
                let signer = subxt::utils::AccountId32(identity).to_string();
                let HostSignRawWithLegacyAccountResponse::V1(response) = host
                    .sign_raw_unwatermarked_deprecated_with_legacy_account(
                        &cx,
                        HostSignRawWithLegacyAccountRequest::V1(
                            v01::HostSignRawWithLegacyAccountRequest {
                                signer,
                                payload: payload.clone(),
                            },
                        ),
                    )
                    .await
                    .unwrap();
                response.signature
            }
        });
        assert_eq!(signature, vec![7, 7]);
        let RemoteMessageData::V1(v1::RemoteMessage::SignRequest(request)) =
            submitted_remote_message(&platform, &session).data
        else {
            panic!("expected an explicit unwatermarked SignRequest");
        };
        match request {
            SignRequest::RawUnwatermarkedDeprecated(request) => {
                assert!(!legacy);
                assert_eq!(request.account, account_id("myapp.dot", 0));
                assert_eq!(request.payload, payload);
            }
            SignRequest::RawWithLegacyAccountUnwatermarkedDeprecated(request) => {
                assert!(legacy);
                assert_eq!(request.account, identity);
                assert_eq!(request.data, payload);
            }
            request => panic!("unwatermarked signing was downgraded: {request:?}"),
        }
    }
}

#[test]
fn sign_vrf_forwards_cross_product_mobile_sso_request_and_response() {
    let session = sso_session_info();
    let signature = v01::VrfSignature {
        pre_output: [0x11; 32],
        proof: [0x22; 64],
    };
    let platform = Arc::new(StubPlatform {
        sign_vrf_confirmed: true,
        sso_response_script: Some(sso_success_response_script(
            &session,
            RemoteMessage {
                message_id: "wallet-vrf-1".to_string(),
                data: RemoteMessageData::V1(v1::RemoteMessage::SignVrfResponse(
                    crate::host_internal::sso_messages::Response {
                        responding_to: "vrf-1".to_string(),
                        payload: Ok(signature.clone()),
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
    let request = v01::HostAccountSignVrfRequest {
        account: account_id("other-product.dot", 0),
        transcript_label: b"ctx".to_vec(),
        items: vec![v01::VrfTranscriptItem {
            label: b"domain".to_vec(),
            value: vec![1, 2],
        }],
    };

    let response = futures::executor::block_on(host.sign_vrf(
        &CallContext::with_request_id("vrf-1".to_string()),
        HostAccountSignVrfRequest::V1(request.clone()),
    ))
    .unwrap();

    assert_eq!(response, HostAccountSignVrfResponse::V1(signature));
    assert_eq!(
        *platform
            .sign_vrf_reviews
            .lock()
            .expect("VRF signing review list mutex poisoned"),
        vec![crate::platform::SignVrfReview {
            calling_product_id: "myapp.dot".to_string(),
            request: request.clone(),
        }]
    );
    let message = submitted_remote_message(&platform, &session);
    let RemoteMessageData::V1(v1::RemoteMessage::SignVrfRequest(request_message)) = message.data
    else {
        panic!("expected VRF signing request");
    };
    assert_eq!(request_message.calling_product_id, "myapp.dot");
    assert_eq!(request_message.payload, request);
}

#[test]
fn sign_vrf_rejects_declined_pairing_host_confirmation_before_mobile_sso() {
    let session = sso_session_info();
    let platform = Arc::new(StubPlatform {
        sso_response_script: Some(sso_success_response_script(
            &session,
            RemoteMessage {
                message_id: "wallet-vrf-declined".to_string(),
                data: RemoteMessageData::V1(v1::RemoteMessage::SignVrfResponse(
                    crate::host_internal::sso_messages::Response {
                        responding_to: "vrf-declined".to_string(),
                        payload: Ok(v01::VrfSignature {
                            pre_output: [0x11; 32],
                            proof: [0x22; 64],
                        }),
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
    install_pairing_session(&host, session);
    let request = v01::HostAccountSignVrfRequest {
        account: account_id("other-product.dot", 0),
        transcript_label: b"ctx".to_vec(),
        items: vec![v01::VrfTranscriptItem {
            label: b"domain".to_vec(),
            value: vec![1, 2],
        }],
    };

    let err = futures::executor::block_on(host.sign_vrf(
        &CallContext::with_request_id("vrf-declined".to_string()),
        HostAccountSignVrfRequest::V1(request.clone()),
    ))
    .unwrap_err();

    assert!(matches!(
        err,
        CallError::Domain(HostAccountSignVrfError::V1(
            v01::HostAccountSignVrfError::Rejected
        ))
    ));
    assert_eq!(
        *platform
            .sign_vrf_reviews
            .lock()
            .expect("VRF signing review list mutex poisoned"),
        vec![crate::platform::SignVrfReview {
            calling_product_id: "myapp.dot".to_string(),
            request,
        }]
    );
    assert!(
        platform
            .sent_rpc
            .lock()
            .expect("RPC request list mutex poisoned")
            .is_empty()
    );
}

#[test]
fn sign_vrf_rejects_oversized_transcript_before_sso() {
    let host =
        ProductRuntimeHost::new(stub_platform(), runtime_config("myapp.dot"), test_spawner());
    install_pairing_session(&host, sso_session_info());
    let request = v01::HostAccountSignVrfRequest {
        account: account_id("myapp.dot", 0),
        transcript_label: vec![],
        items: vec![
            v01::VrfTranscriptItem {
                label: vec![],
                value: vec![],
            };
            MAX_VRF_TRANSCRIPT_ITEMS + 1
        ],
    };

    let err = futures::executor::block_on(host.sign_vrf(
        &CallContext::default(),
        HostAccountSignVrfRequest::V1(request),
    ))
    .unwrap_err();

    assert!(matches!(
        err,
        CallError::Domain(HostAccountSignVrfError::V1(
            v01::HostAccountSignVrfError::Unknown { reason }
        )) if reason.contains("at most 32")
    ));
}

#[test]
fn sign_raw_rejects_invalid_product_account() {
    let host =
        ProductRuntimeHost::new(stub_platform(), runtime_config("myapp.dot"), test_spawner());
    install_pairing_session(&host, session_info());
    let cx = CallContext::default();
    let request = HostSignRawRequest::V1(v01::HostSignRawRequest {
        account: account_id("other.dot", 0),
        payload: raw_payload(),
    });
    let err = futures::executor::block_on(host.sign_raw(&cx, request)).unwrap_err();
    assert!(matches!(
        err,
        CallError::Domain(HostSignRawError::V1(
            v01::HostSignPayloadError::PermissionDenied
        ))
    ));
}

#[test]
fn sign_raw_rejects_without_session_after_valid_account() {
    let host =
        ProductRuntimeHost::new(stub_platform(), runtime_config("myapp.dot"), test_spawner());
    let cx = CallContext::default();
    let request = HostSignRawRequest::V1(v01::HostSignRawRequest {
        account: account_id("myapp.dot", 0),
        payload: raw_payload(),
    });
    let err = futures::executor::block_on(host.sign_raw(&cx, request)).unwrap_err();
    assert!(matches!(
        err,
        CallError::Domain(HostSignRawError::V1(v01::HostSignPayloadError::Rejected))
    ));
}

#[test]
fn sign_raw_denies_when_chain_submit_denied() {
    let host = ProductRuntimeHost::new(
        Arc::new(StubPlatform {
            remote_permission_denied: true,
            ..Default::default()
        }),
        runtime_config("myapp.dot"),
        test_spawner(),
    );
    install_pairing_session(&host, session_info());
    let cx = CallContext::default();
    let request = HostSignRawRequest::V1(v01::HostSignRawRequest {
        account: account_id("myapp.dot", 0),
        payload: raw_payload(),
    });
    let err = futures::executor::block_on(host.sign_raw(&cx, request)).unwrap_err();
    assert!(matches!(
        err,
        CallError::Domain(HostSignRawError::V1(
            v01::HostSignPayloadError::PermissionDenied
        ))
    ));
}

#[test]
fn sign_raw_rejects_when_user_declines_confirmation() {
    let host =
        ProductRuntimeHost::new(stub_platform(), runtime_config("myapp.dot"), test_spawner());
    install_pairing_session(&host, session_info());
    let cx = CallContext::default();
    let request = HostSignRawRequest::V1(v01::HostSignRawRequest {
        account: account_id("myapp.dot", 0),
        payload: raw_payload(),
    });
    let err = futures::executor::block_on(host.sign_raw(&cx, request)).unwrap_err();
    assert!(matches!(
        err,
        CallError::Domain(HostSignRawError::V1(v01::HostSignPayloadError::Rejected))
    ));
}

#[test]
fn sign_raw_accepts_confirmation_then_returns_sso_response() {
    let session = sso_session_info();
    let platform = Arc::new(StubPlatform {
        sign_raw_confirmed: true,
        sso_response_script: Some(sso_success_response_script(
            &session,
            sign_response_message("sign-raw-1", vec![7, 7], None),
        )),
        ..Default::default()
    });
    let host = ProductRuntimeHost::new(
        platform.clone(),
        runtime_config("myapp.dot"),
        test_spawner(),
    );
    install_pairing_session(&host, session.clone());
    let cx = CallContext::with_request_id("sign-raw-1".to_string());
    let request = HostSignRawRequest::V1(v01::HostSignRawRequest {
        account: account_id("myapp.dot", 0),
        payload: raw_payload(),
    });
    let response = futures::executor::block_on(host.sign_raw(&cx, request)).unwrap();
    let HostSignRawResponse::V1(inner) = response;
    assert_eq!(inner.signature, vec![7, 7]);
    assert_eq!(inner.signed_transaction, None);
    let message = submitted_remote_message(&platform, &session);
    assert!(matches!(
        &message.data,
        crate::host_internal::sso_messages::RemoteMessageData::V1(
            crate::host_internal::sso_messages::v1::RemoteMessage::SignRequest(
                crate::host_internal::sso_messages::SignRequest::Raw(_)
            )
        )
    ));
    let sent = platform.sent_rpc.lock().expect("rpc list mutex poisoned");
    let methods = sent
        .iter()
        .map(|request| {
            serde_json::from_str::<serde_json::Value>(request).unwrap()["method"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        methods,
        vec![
            "statement_subscribeStatement",
            "statement_subscribeStatement",
            "statement_submit",
            "statement_unsubscribeStatement",
            "statement_unsubscribeStatement",
        ]
    );
    let mut unsubscribe_ids = sent[3..]
        .iter()
        .map(|request| serde_json::from_str::<serde_json::Value>(request).unwrap())
        .map(|request| request["params"][0].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    unsubscribe_ids.sort();
    assert_eq!(unsubscribe_ids, vec!["own-sub", "peer-sub"]);
}

#[test]
fn sign_raw_uses_call_context_timeout_for_sso_response_wait() {
    let session = sso_session_info();
    let message_id = "sign-raw-timeout";
    let mut rpc_responses = sso_success_responses(
        &session,
        message_id,
        sign_response_message(message_id, vec![], None),
    );
    rpc_responses.truncate(3);
    let platform = Arc::new(StubPlatform {
        sign_raw_confirmed: true,
        rpc_responses,
        ..Default::default()
    });
    let host = ProductRuntimeHost::new(
        platform.clone(),
        runtime_config("myapp.dot"),
        test_spawner(),
    );
    install_pairing_session(&host, session);
    let mut cx = CallContext::with_request_id(message_id.to_string());
    cx.set_timeout(std::time::Duration::from_millis(1));
    let request = HostSignRawRequest::V1(v01::HostSignRawRequest {
        account: account_id("myapp.dot", 0),
        payload: raw_payload(),
    });
    let err = futures::executor::block_on(host.sign_raw(&cx, request)).unwrap_err();

    match err {
        CallError::Domain(HostSignRawError::V1(v01::HostSignPayloadError::Unknown { reason })) => {
            assert_eq!(
                reason,
                "Account authority request timed out after 1ms for sign-raw-timeout"
            )
        }
        other => panic!("expected SSO response timeout, got {other:?}"),
    }

    wait_until(
        || recorded_rpc_method_count(&platform.sent_rpc, "statement_unsubscribeStatement") == 2,
        "timed-out SSO request did not unsubscribe statement streams",
    );
}

#[test]
fn sign_raw_cancellation_unsubscribes_sso_subscriptions() {
    let session = sso_session_info();
    let message_id = "sign-raw-cancel";
    let platform = Arc::new(StubPlatform {
        sign_raw_confirmed: true,
        rpc_responses: vec![
            subscribe_ack_frame("truapi:1", "own-sub-sign-raw-cancel"),
            subscribe_ack_frame("truapi:2", "peer-sub-sign-raw-cancel"),
        ],
        ..Default::default()
    });
    let host = ProductRuntimeHost::new(
        platform.clone(),
        runtime_config("myapp.dot"),
        test_spawner(),
    );
    install_pairing_session(&host, session);
    let cancel = truapi::CancellationToken::default();
    let cx = CallContext::with_parts(message_id.to_string(), cancel.clone());
    let request = HostSignRawRequest::V1(v01::HostSignRawRequest {
        account: account_id("myapp.dot", 0),
        payload: raw_payload(),
    });
    let handle = std::thread::spawn(move || {
        futures::executor::block_on(host.sign_raw(&cx, request)).unwrap_err()
    });

    wait_until(
        || recorded_rpc_method_count(&platform.sent_rpc, "statement_subscribeStatement") == 2,
        "SSO subscriptions were not established before cancellation",
    );
    cancel.cancel();

    let err = handle
        .join()
        .expect("sign_raw cancellation thread panicked");
    match err {
        CallError::Domain(HostSignRawError::V1(v01::HostSignPayloadError::Unknown { reason })) => {
            assert!(
                reason.contains("cancelled"),
                "expected cancellation, got {reason}"
            );
            assert!(
                reason.contains(message_id),
                "expected request id in cancellation reason, got {reason}"
            );
        }
        other => panic!("expected SSO cancellation, got {other:?}"),
    }

    wait_until(
        || recorded_rpc_method_count(&platform.sent_rpc, "statement_unsubscribeStatement") == 2,
        "cancelled SSO request did not unsubscribe statement streams",
    );
}

#[test]
fn sign_raw_peer_disconnect_clears_session_store_and_broadcasts() {
    let session = sso_session_info();
    let platform = Arc::new(StubPlatform {
        sign_raw_confirmed: true,
        sso_response_script: Some(sso_peer_disconnect_response_script(&session)),
        ..Default::default()
    });
    let host = ProductRuntimeHost::new(
        platform.clone(),
        runtime_config("myapp.dot"),
        test_spawner(),
    );
    install_pairing_session(&host, session);
    let mut statuses = host.test_session_state().subscribe();
    assert_eq!(
        futures::executor::block_on(statuses.next()).unwrap(),
        HostAccountConnectionStatusSubscribeItem::V1(
            v01::HostAccountConnectionStatusSubscribeItem::Connected
        )
    );

    let cx = CallContext::with_request_id("sign-raw-disconnect".to_string());
    let request = HostSignRawRequest::V1(v01::HostSignRawRequest {
        account: account_id("myapp.dot", 0),
        payload: raw_payload(),
    });
    let err = futures::executor::block_on(host.sign_raw(&cx, request)).unwrap_err();

    assert!(matches!(
        err,
        CallError::Domain(HostSignRawError::V1(v01::HostSignPayloadError::Rejected))
    ));
    assert!(host.test_session_state().current().is_none());
    assert_eq!(
        *platform
            .session_clears
            .lock()
            .expect("session clear counter mutex poisoned"),
        1
    );
    assert_eq!(
        futures::executor::block_on(statuses.next()).unwrap(),
        HostAccountConnectionStatusSubscribeItem::V1(
            v01::HostAccountConnectionStatusSubscribeItem::Disconnected
        )
    );
}

#[test]
fn sign_payload_denies_when_chain_submit_denied() {
    let host = ProductRuntimeHost::new(
        Arc::new(StubPlatform {
            remote_permission_denied: true,
            ..Default::default()
        }),
        runtime_config("myapp.dot"),
        test_spawner(),
    );
    install_pairing_session(&host, session_info());
    let cx = CallContext::default();
    let request = HostSignPayloadRequest::V1(v01::HostSignPayloadRequest {
        account: account_id("myapp.dot", 0),
        payload: sign_payload_data(),
    });
    let err = futures::executor::block_on(host.sign_payload(&cx, request)).unwrap_err();
    assert!(matches!(
        err,
        CallError::Domain(HostSignPayloadError::V1(
            v01::HostSignPayloadError::PermissionDenied
        ))
    ));
}

#[test]
fn sign_payload_maps_confirmation_failure_to_host_failure() {
    let host = ProductRuntimeHost::new(
        Arc::new(StubPlatform {
            sign_payload_error: Some("modal failed"),
            ..Default::default()
        }),
        runtime_config("myapp.dot"),
        test_spawner(),
    );
    install_pairing_session(&host, session_info());
    let cx = CallContext::default();
    let request = HostSignPayloadRequest::V1(v01::HostSignPayloadRequest {
        account: account_id("myapp.dot", 0),
        payload: sign_payload_data(),
    });
    let err = futures::executor::block_on(host.sign_payload(&cx, request)).unwrap_err();
    assert!(matches!(err, CallError::HostFailure { reason } if reason.contains("modal failed")));
}

#[test]
fn sign_payload_accepts_confirmation_then_returns_sso_response() {
    let session = sso_session_info();
    let platform = Arc::new(StubPlatform {
        sign_payload_confirmed: true,
        sso_response_script: Some(sso_success_response_script(
            &session,
            sign_response_message("sign-payload-1", vec![8, 8], Some(vec![0xab, 0xcd])),
        )),
        ..Default::default()
    });
    let host = ProductRuntimeHost::new(
        platform.clone(),
        runtime_config("myapp.dot"),
        test_spawner(),
    );
    install_pairing_session(&host, session.clone());
    let cx = CallContext::with_request_id("sign-payload-1".to_string());
    let request = HostSignPayloadRequest::V1(v01::HostSignPayloadRequest {
        account: account_id("myapp.dot", 0),
        payload: sign_payload_data(),
    });

    let response = futures::executor::block_on(host.sign_payload(&cx, request)).unwrap();

    let HostSignPayloadResponse::V1(inner) = response;
    assert_eq!(inner.signature, vec![8, 8]);
    assert_eq!(inner.signed_transaction, Some(vec![0xab, 0xcd]));
    let message = submitted_remote_message(&platform, &session);
    assert!(matches!(
        &message.data,
        crate::host_internal::sso_messages::RemoteMessageData::V1(
            crate::host_internal::sso_messages::v1::RemoteMessage::SignRequest(
                crate::host_internal::sso_messages::SignRequest::Payload(_)
            )
        )
    ));
}

#[test]
fn create_transaction_accepts_confirmation_then_returns_sso_response() {
    let session = sso_session_info();
    let platform = Arc::new(StubPlatform {
        create_transaction_confirmed: true,
        sso_response_script: Some(sso_success_response_script(
            &session,
            crate::host_internal::sso_messages::RemoteMessage {
                message_id: "wallet-create-tx-1".to_string(),
                data: crate::host_internal::sso_messages::RemoteMessageData::V1(
                    crate::host_internal::sso_messages::v1::RemoteMessage::CreateTransactionResponse(
                        crate::host_internal::sso_messages::Response {
                            responding_to: "create-tx-1".to_string(),
                            payload: Ok(vec![0xca, 0xfe]),
                        },
                    ),
                ),
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
    let cx = CallContext::with_request_id("create-tx-1".to_string());
    let request = HostCreateTransactionRequest::V1(product_tx_payload("myapp.dot"));
    let response = futures::executor::block_on(host.create_transaction(&cx, request)).unwrap();
    let HostCreateTransactionResponse::V1(inner) = response;
    assert_eq!(inner.transaction, vec![0xca, 0xfe]);
    let message = submitted_remote_message(&platform, &session);
    assert!(matches!(
        message.data,
        crate::host_internal::sso_messages::RemoteMessageData::V1(
            crate::host_internal::sso_messages::v1::RemoteMessage::CreateTransactionRequest(_)
        )
    ));
}

/// A `createTransaction` on a paired session whose confirmation prompt stays
/// open until the returned sender releases it.
fn gated_create_transaction(
    request_id: &str,
) -> (
    Arc<StubPlatform>,
    ProductRuntimeHost,
    CallContext,
    futures::channel::oneshot::Sender<()>,
) {
    let (release, gate) = futures::channel::oneshot::channel();
    let platform = Arc::new(StubPlatform {
        create_transaction_confirmed: true,
        create_transaction_confirmation_gate: Mutex::new(Some(gate)),
        ..Default::default()
    });
    let host = ProductRuntimeHost::new(
        platform.clone(),
        runtime_config("myapp.dot"),
        test_spawner(),
    );
    install_pairing_session(&host, sso_session_info());
    let cx = CallContext::with_parts(request_id.to_string(), truapi::CancellationToken::default());
    (platform, host, cx, release)
}

/// A person who has not answered yet must not be able to authorize a
/// transaction the product has already withdrawn, and the product must not
/// wait on that person to learn the call is over.
#[test]
fn a_create_transaction_withdrawn_at_the_prompt_never_reaches_the_phone() {
    let (platform, host, cx, _release) = gated_create_transaction("create-tx-withdrawn");
    let request = HostCreateTransactionRequest::V1(product_tx_payload("myapp.dot"));
    let mut call = Box::pin(host.create_transaction(&cx, request));
    assert!(call.as_mut().now_or_never().is_none());
    assert_eq!(platform.create_transaction_reviews.lock().unwrap().len(), 1);

    cx.cancel().cancel();

    let err = call
        .as_mut()
        .now_or_never()
        .expect("a withdrawn call stops waiting on the prompt")
        .unwrap_err();
    assert_eq!(
        err,
        CallError::Domain(HostCreateTransactionError::V1(
            v01::HostCreateTransactionError::Unknown {
                reason: "Account authority request cancelled for create-tx-withdrawn".to_string(),
            }
        ))
    );
    assert_eq!(
        recorded_rpc_method_count(&platform.sent_rpc, "statement_subscribeStatement"),
        0
    );
}

/// An approval that lands after the withdrawal is an answer to a call that no
/// longer exists, so it must authorize nothing. Either the prompt race or the
/// authority call's own check is enough to hold this; the test pins that at
/// least one of them does.
#[test]
fn a_create_transaction_approved_after_its_withdrawal_never_reaches_the_phone() {
    let (platform, host, cx, release) = gated_create_transaction("create-tx-approved-late");
    let request = HostCreateTransactionRequest::V1(product_tx_payload("myapp.dot"));
    let mut call = Box::pin(host.create_transaction(&cx, request));
    assert!(call.as_mut().now_or_never().is_none());

    cx.cancel().cancel();
    release.send(()).unwrap();

    let err = call
        .as_mut()
        .now_or_never()
        .expect("a withdrawn call settles without waiting")
        .unwrap_err();
    assert_eq!(
        err,
        CallError::Domain(HostCreateTransactionError::V1(
            v01::HostCreateTransactionError::Unknown {
                reason: "Account authority request cancelled for create-tx-approved-late"
                    .to_string(),
            }
        ))
    );
    assert_eq!(
        recorded_rpc_method_count(&platform.sent_rpc, "statement_subscribeStatement"),
        0
    );
}

/// A withdrawal that lands while the paired-host request is still subscribing
/// must stop it before the request itself is published.
#[test]
fn a_signature_withdrawn_during_sso_setup_is_never_submitted() {
    let (release, gate) = futures::channel::oneshot::channel();
    let platform = Arc::new(StubPlatform {
        sign_raw_confirmed: true,
        rpc_method_responses: vec![
            ("statement_subscribeStatement", r#""own-sub""#.to_string()),
            ("statement_subscribeStatement", r#""peer-sub""#.to_string()),
            ("statement_unsubscribeStatement", "true".to_string()),
            ("statement_unsubscribeStatement", "true".to_string()),
            ("statement_submit", r#"{"status":"new"}"#.to_string()),
        ],
        rpc_method_responses_gate: Arc::new(Mutex::new(Some(gate))),
        ..Default::default()
    });
    let host = ProductRuntimeHost::new(
        platform.clone(),
        runtime_config("myapp.dot"),
        test_spawner(),
    );
    install_pairing_session(&host, sso_session_info());
    let cancel = truapi::CancellationToken::default();
    let cx = CallContext::with_parts("sign-raw-setup".to_string(), cancel.clone());
    let request = HostSignRawRequest::V1(v01::HostSignRawRequest {
        account: account_id("myapp.dot", 0),
        payload: raw_payload(),
    });
    let call = std::thread::spawn(move || {
        futures::executor::block_on(host.sign_raw(&cx, request)).unwrap_err()
    });
    wait_until(
        || recorded_rpc_method_count(&platform.sent_rpc, "statement_subscribeStatement") == 1,
        "the paired-host request did not start subscribing",
    );

    cancel.cancel();
    release.send(()).unwrap();
    call.join().expect("sign_raw thread panicked");

    assert_eq!(
        recorded_rpc_method_count(&platform.sent_rpc, "statement_submit"),
        0
    );
}

#[test]
fn legacy_sign_payload_rejects_identity_account() {
    let session = session_info();
    let identity = session.identity_account_id.unwrap();
    let host =
        ProductRuntimeHost::new(stub_platform(), runtime_config("myapp.dot"), test_spawner());
    install_pairing_session(&host, session);
    let cx = CallContext::default();
    let request =
        HostSignPayloadWithLegacyAccountRequest::V1(v01::HostSignPayloadWithLegacyAccountRequest {
            signer: subxt::utils::AccountId32(identity).to_string(),
            payload: sign_payload_data(),
        });

    let err = futures::executor::block_on(host.sign_payload_with_legacy_account(&cx, request))
        .unwrap_err();

    match err {
        CallError::Domain(HostSignPayloadWithLegacyAccountError::V1(
            v01::HostSignPayloadError::Unknown { reason },
        )) => assert_eq!(reason, LEGACY_PRODUCT_ACCOUNT_MISMATCH_REASON),
        other => panic!("expected identity account rejection, got {other:?}"),
    }
}

#[test]
fn legacy_sign_raw_rejects_signer_mismatch() {
    let host =
        ProductRuntimeHost::new(stub_platform(), runtime_config("myapp.dot"), test_spawner());
    install_pairing_session(&host, sso_session_info());
    let cx = CallContext::default();
    let request =
        HostSignRawWithLegacyAccountRequest::V1(v01::HostSignRawWithLegacyAccountRequest {
            signer: "5Ci5sCERp3MFEDpF2jVkQDJoBevpRosB7toYRqKWShewhdhq".to_string(),
            payload: raw_payload(),
        });
    let err =
        futures::executor::block_on(host.sign_raw_with_legacy_account(&cx, request)).unwrap_err();
    match err {
        CallError::Domain(HostSignRawWithLegacyAccountError::V1(
            v01::HostSignPayloadError::Unknown { reason },
        )) => assert_eq!(reason, "Account is not available in the active session"),
        other => panic!("expected legacy signer mismatch, got {other:?}"),
    }
}

#[test]
fn legacy_sign_raw_denies_when_chain_submit_denied() {
    let host = ProductRuntimeHost::new(
        Arc::new(StubPlatform {
            remote_permission_denied: true,
            ..Default::default()
        }),
        runtime_config("myapp.dot"),
        test_spawner(),
    );
    install_pairing_session(&host, sso_session_info());
    let cx = CallContext::default();
    let request =
        HostSignRawWithLegacyAccountRequest::V1(v01::HostSignRawWithLegacyAccountRequest {
            signer: subxt::utils::AccountId32(test_product_account_public("myapp.dot", 0))
                .to_string(),
            payload: raw_payload(),
        });
    let err =
        futures::executor::block_on(host.sign_raw_with_legacy_account(&cx, request)).unwrap_err();
    assert!(matches!(
        err,
        CallError::Domain(HostSignRawWithLegacyAccountError::V1(
            v01::HostSignPayloadError::PermissionDenied
        ))
    ));
}

#[test]
fn legacy_sign_raw_accepts_derived_ss58_then_returns_sso_response() {
    let session = sso_session_info();
    let platform = Arc::new(StubPlatform {
        sign_raw_confirmed: true,
        sso_response_script: Some(sso_success_response_script(
            &session,
            sign_response_message("legacy-sign-raw-1", vec![9, 9], None),
        )),
        ..Default::default()
    });
    let host = ProductRuntimeHost::new(
        platform.clone(),
        runtime_config("myapp.dot"),
        test_spawner(),
    );
    install_pairing_session(&host, session.clone());
    let cx = CallContext::with_request_id("legacy-sign-raw-1".to_string());
    let request =
        HostSignRawWithLegacyAccountRequest::V1(v01::HostSignRawWithLegacyAccountRequest {
            signer: subxt::utils::AccountId32(test_product_account_public("myapp.dot", 0))
                .to_string(),
            payload: raw_payload(),
        });
    let response =
        futures::executor::block_on(host.sign_raw_with_legacy_account(&cx, request)).unwrap();
    let HostSignRawWithLegacyAccountResponse::V1(inner) = response;
    assert_eq!(inner.signature, vec![9, 9]);
    assert_eq!(inner.signed_transaction, None);
    let message = submitted_remote_message(&platform, &session);
    let crate::host_internal::sso_messages::RemoteMessageData::V1(
        crate::host_internal::sso_messages::v1::RemoteMessage::SignRequest(request),
    ) = message.data
    else {
        panic!("expected product raw signing request");
    };
    let crate::host_internal::sso_messages::SignRequest::Raw(request) = request else {
        panic!("expected raw signing payload");
    };
    assert_eq!(
        request.account,
        v01::ProductAccountId {
            dot_ns_identifier: "myapp.dot".to_string(),
            derivation_index: v01::DerivationIndex::Index(0),
        }
    );
    assert!(matches!(
        &request.payload,
        truapi::latest::RawPayload::Bytes { bytes }
            if bytes == b"hello"
    ));
}

#[test]
fn legacy_sign_raw_accepts_derived_hex_then_returns_sso_response() {
    let session = sso_session_info();
    let signer = test_product_account_public("myapp.dot", 0);
    let platform = Arc::new(StubPlatform {
        sign_raw_confirmed: true,
        sso_response_script: Some(sso_success_response_script(
            &session,
            sign_response_message("legacy-sign-raw-hex-1", vec![8, 8], None),
        )),
        ..Default::default()
    });
    let host = ProductRuntimeHost::new(
        platform.clone(),
        runtime_config("myapp.dot"),
        test_spawner(),
    );
    install_pairing_session(&host, session.clone());
    let cx = CallContext::with_request_id("legacy-sign-raw-hex-1".to_string());
    let request =
        HostSignRawWithLegacyAccountRequest::V1(v01::HostSignRawWithLegacyAccountRequest {
            signer: format!("0x{}", hex::encode(signer)),
            payload: raw_payload(),
        });
    let response =
        futures::executor::block_on(host.sign_raw_with_legacy_account(&cx, request)).unwrap();
    let HostSignRawWithLegacyAccountResponse::V1(inner) = response;
    assert_eq!(inner.signature, vec![8, 8]);

    let message = submitted_remote_message(&platform, &session);
    let crate::host_internal::sso_messages::RemoteMessageData::V1(
        crate::host_internal::sso_messages::v1::RemoteMessage::SignRequest(request),
    ) = message.data
    else {
        panic!("expected product raw signing request");
    };
    let crate::host_internal::sso_messages::SignRequest::Raw(request) = request else {
        panic!("expected raw signing payload");
    };
    assert_eq!(
        request.account,
        v01::ProductAccountId {
            dot_ns_identifier: "myapp.dot".to_string(),
            derivation_index: v01::DerivationIndex::Index(0),
        }
    );
}

#[test]
fn legacy_sign_raw_accepts_identity_ss58_then_routes_legacy_request() {
    let session = sso_session_info();
    let identity = session.identity_account_id.unwrap();
    let platform = Arc::new(StubPlatform {
        sign_raw_confirmed: true,
        sso_response_script: Some(sso_success_response_script(
            &session,
            sign_raw_legacy_response_message("identity-sign-raw-1", vec![7, 7]),
        )),
        ..Default::default()
    });
    let host = ProductRuntimeHost::new(
        platform.clone(),
        runtime_config("myapp.dot"),
        test_spawner(),
    );
    install_pairing_session(&host, session.clone());
    let cx = CallContext::with_request_id("identity-sign-raw-1".to_string());
    let request =
        HostSignRawWithLegacyAccountRequest::V1(v01::HostSignRawWithLegacyAccountRequest {
            signer: subxt::utils::AccountId32(identity).to_string(),
            payload: raw_payload(),
        });

    let response =
        futures::executor::block_on(host.sign_raw_with_legacy_account(&cx, request)).unwrap();

    let HostSignRawWithLegacyAccountResponse::V1(response) = response;
    assert_eq!(response.signature, vec![7, 7]);
    let message = submitted_remote_message(&platform, &session);
    let RemoteMessageData::V1(v1::RemoteMessage::SignRawWithLegacyAccountRequest(request)) =
        message.data
    else {
        panic!("expected legacy raw signing request");
    };
    assert_eq!(request.account, identity);
}

#[test]
fn create_transaction_rejects_invalid_product_account() {
    let host =
        ProductRuntimeHost::new(stub_platform(), runtime_config("myapp.dot"), test_spawner());
    install_pairing_session(&host, session_info());
    let cx = CallContext::default();
    let request = HostCreateTransactionRequest::V1(product_tx_payload("other.dot"));
    let err = futures::executor::block_on(host.create_transaction(&cx, request)).unwrap_err();
    assert!(matches!(
        err,
        CallError::Domain(HostCreateTransactionError::V1(
            v01::HostCreateTransactionError::PermissionDenied
        ))
    ));
}
