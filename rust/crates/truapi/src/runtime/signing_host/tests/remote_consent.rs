use super::*;
use crate::host_internal::permissions::set_account_access_status;
use crate::host_internal::sso_messages::{RemoteMessage, RemoteMessageData, v1};
use crate::platform::{PermissionAuthorizationStatus, PermissionDecision, SignRawReview};
use crate::runtime::SsoAccountHolderService;
use crate::runtime::sso_service::Dispatch;
use truapi::latest::{HostAccountListRingVrfKeysRequest, RingVrfKeyDisclosure};

#[test]
fn wallet_signing_requires_the_callers_authorization() {
    for remote in [false, true] {
        let platform = Arc::new(StubPlatform {
            resource_allocation_confirmed: true,
            ..StubPlatform::default()
        });
        let (services, authority) = signing_runtime_with_platform(platform.clone());
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec())).unwrap();
        let session = authority.account_holder().current_session().unwrap();
        auto_signing::grant_auto_signing(&product_runtime(services, authority.clone()));
        let product = ProductContext::new("myapp.dot".to_string()).unwrap();
        let call = CallContext::default();
        let invocation = || AccountInvocation {
            call: &call,
            session: &session,
            caller: if remote {
                AccountCaller::Remote {
                    product_id: Some(&product.product_id),
                }
            } else {
                AccountCaller::Local {
                    product: &product,
                }
            },
        };
        let request = truapi::latest::HostSignRawRequest {
            account: product_account(0),
            payload: truapi::latest::RawPayload::Bytes {
                bytes: b"approval".to_vec(),
            },
        };
        let signed = futures::executor::block_on(authority.account_holder().sign_raw(
            invocation(),
            SignRawAuthorityRequest::Product(request.clone()),
            true,
        ));
        assert_eq!(
            (signed, platform.sign_raw_reviews.lock().unwrap().clone()),
            (
                Err(AuthorityError::Rejected),
                vec![SignRawReview::Product {
                    calling_product_id: Some(product.product_id.clone()),
                    request,
                    watermarked: true,
                }]
            ),
            "remote: {remote}",
        );
        if remote {
            let statement = futures::executor::block_on(
                authority
                    .account_holder()
                    .sign_statement_store_product_payload(
                        invocation(),
                        product_account(0),
                        b"statement".to_vec(),
                    ),
            );
            assert_eq!(
                (
                    statement,
                    platform
                        .statement_store_product_sign_reviews
                        .lock()
                        .unwrap()
                        .clone()
                ),
                (
                    Err(AuthorityError::Rejected),
                    vec![crate::platform::StatementStoreProductSignReview {
                        calling_product_id: Some(product.product_id),
                        account: product_account(0),
                        payload: b"statement".to_vec(),
                    }]
                ),
            );
        }
    }
}

#[test]
fn remote_vrf_cannot_reuse_a_native_auto_signing_grant() {
    let platform = Arc::new(StubPlatform {
        resource_allocation_confirmed: true,
        ..StubPlatform::default()
    });
    let (services, authority) = signing_runtime_with_platform(platform.clone());
    futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec())).unwrap();
    auto_signing::grant_auto_signing(&product_runtime(services, authority.clone()));
    let Ok(Dispatch::Response(answer)) = futures::executor::block_on(
        SsoAccountHolderService::new(
            authority.account_holder().clone(),
            authority.account_holder().current_session().unwrap(),
        )
        .answer(RemoteMessage::request(
            "remote-vrf".to_string(),
            ProductRequest {
                calling_product_id: "myapp.dot".to_string(),
                payload: vrf_request("myapp.dot"),
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
            response.payload.map(|_| ()),
            platform.sign_vrf_reviews.lock().unwrap().len(),
        ),
        (Err(v01::HostAccountSignVrfError::Rejected), 1),
    );
}

#[test]
fn remote_account_access_reuses_shared_decisions_and_preserves_their_lifetime() {
    for operation in ["alias", "list"] {
        for (stored, decision, expected_status, prompts) in [
            (
                Some(PermissionAuthorizationStatus::Authorized),
                PermissionDecision::Deny,
                PermissionAuthorizationStatus::Authorized,
                0,
            ),
            (
                Some(PermissionAuthorizationStatus::Denied),
                PermissionDecision::AllowAlways,
                PermissionAuthorizationStatus::Denied,
                0,
            ),
            (
                None,
                PermissionDecision::AllowAlways,
                PermissionAuthorizationStatus::Authorized,
                1,
            ),
            (
                None,
                PermissionDecision::AllowOnce,
                PermissionAuthorizationStatus::NotDetermined,
                1,
            ),
            (
                None,
                PermissionDecision::Deny,
                PermissionAuthorizationStatus::Denied,
                1,
            ),
        ] {
            let platform = Arc::new(StubPlatform {
                permission_confirmation_decisions: std::sync::Mutex::new(
                    [decision, decision].into(),
                ),
                ..StubPlatform::default()
            });
            cache_grant(&platform, "peopl.dot", "{}");
            let (_, authority) =
                signing_runtime_with_ring_resolver(platform.clone(), full_person_ring_resolver());
            futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
                .unwrap();
            let session = authority.account_holder().current_session().unwrap();
            register_full_person_key(&authority, &session, &full_person_ring_location());
            let owner = if operation == "alias" {
                "peopl"
            } else {
                "foreign"
            };
            if let Some(status) = stored {
                futures::executor::block_on(set_account_access_status(
                    platform.as_ref(),
                    "myapp",
                    owner,
                    status,
                ))
                .unwrap();
            }
            let service = SsoAccountHolderService::new(
                authority.account_holder().clone(),
                authority.account_holder().current_session().unwrap(),
            );
            let mut outcomes = Vec::new();
            for request_id in ["first", "second"] {
                let request = if operation == "alias" {
                    RemoteMessage::request(
                        request_id.to_string(),
                        ProductRequest {
                            calling_product_id: "myapp.dot".to_string(),
                            payload: HostAccountGetAliasRequest {
                                key_handle: full_person_key_handle(),
                                context: v01::ProductProofContext {
                                    product_id: "myapp.dot".to_string(),
                                    suffix: v01::DerivationIndex::Index(0),
                                },
                                ring_location: full_person_ring_location(),
                            },
                        },
                    )
                } else {
                    RemoteMessage::request(
                        request_id.to_string(),
                        ProductRequest {
                            calling_product_id: "myapp.dot".to_string(),
                            payload: HostAccountListRingVrfKeysRequest {
                                owner: "foreign.dot".to_string(),
                                disclosure: RingVrfKeyDisclosure::PublicKey,
                            },
                        },
                    )
                };
                let Ok(Dispatch::Response(answer)) =
                    futures::executor::block_on(service.answer(request))
                else {
                    panic!("expected an account response")
                };
                outcomes.push(match answer.message.data {
                    RemoteMessageData::V1(v1::RemoteMessage::GetAccountAliasResponse(response)) => {
                        response.payload.map(|_| ())
                    }
                    RemoteMessageData::V1(v1::RemoteMessage::ListRingVrfKeysResponse(response)) => {
                        response.payload.map(|_| ())
                    }
                    _ => panic!("unexpected account response"),
                });
            }
            let expected = if expected_status == PermissionAuthorizationStatus::Denied {
                Err(RingVrfError::Rejected)
            } else {
                Ok(())
            };
            assert_eq!(
                (
                    outcomes,
                    platform.account_access_reviews.lock().unwrap().len(),
                    futures::executor::block_on(
                        crate::host_internal::permissions::account_access_status(
                            platform.as_ref(),
                            "myapp",
                            owner
                        )
                    )
                    .unwrap(),
                ),
                (vec![expected.clone(), expected], prompts, expected_status),
                "{operation}, stored decision {stored:?}, answer {decision:?}",
            );
        }
    }
}

#[test]
fn shared_denials_override_published_access_for_local_and_remote_callers() {
    for storage_error in [None, Some("permission storage unavailable")] {
        let platform = Arc::new(StubPlatform {
            permission_storage_error: storage_error,
            ..StubPlatform::default()
        });
        cache_grant(&platform, "peopl.dot", r#"{"myapp":["context"]}"#);
        deny_account_access(&platform, "myapp.dot", "peopl.dot");
        let (_, authority) =
            signing_runtime_with_ring_resolver(platform.clone(), full_person_ring_resolver());
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec())).unwrap();
        let session = authority.account_holder().current_session().unwrap();
        register_full_person_key(&authority, &session, &full_person_ring_location());
        let request = ProductRequest {
            calling_product_id: "myapp.dot".to_string(),
            payload: HostAccountRingVrfSignRequest {
                key_handle: full_person_key_handle(),
                message: b"published access".to_vec(),
            },
        };
        let local = futures::executor::block_on(authority.account_holder().ring_vrf_sign(
            AccountInvocation {
                call: &CallContext::default(),
                session: &session,
                caller: AccountCaller::Local {
                    product: &ProductContext::new(request.calling_product_id.clone()).unwrap(),
                },
            },
            request.payload.clone(),
        ));
        let Ok(Dispatch::Response(answer)) = futures::executor::block_on(
            SsoAccountHolderService::new(
                authority.account_holder().clone(),
                authority.account_holder().current_session().unwrap(),
            )
            .answer(RemoteMessage::request(
                "remote-published-access".to_string(),
                request,
            )),
        ) else {
            panic!("expected a ring-VRF response")
        };
        let RemoteMessageData::V1(v1::RemoteMessage::RingVrfSignResponse(response)) =
            answer.message.data
        else {
            panic!("expected a ring-VRF signing response")
        };
        assert_eq!(
            (
                local.map(|_| ()),
                response.payload.map(|_| ()),
                platform.account_access_reviews.lock().unwrap().len(),
            ),
            (
                Err(RingVrfError::NotAllowlisted),
                Err(RingVrfError::NotAllowlisted),
                0
            ),
            "{storage_error:?}",
        );
    }
}
