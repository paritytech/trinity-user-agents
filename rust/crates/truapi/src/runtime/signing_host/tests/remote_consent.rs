use super::*;
use crate::host_internal::permissions::set_account_access_status;
use crate::host_internal::sso_messages::{RemoteMessage, RemoteMessageData, v1};
use crate::platform::PermissionAuthorizationStatus;
use crate::runtime::signing_host::SigningHostSsoService;
use crate::runtime::sso_service::Dispatch;
use truapi::latest::{HostAccountListRingVrfKeysRequest, RingVrfKeyDisclosure};

#[test]
fn remote_vrf_cannot_reuse_a_native_auto_signing_grant() {
    let platform = Arc::new(StubPlatform::default());
    let (_, authority) = signing_runtime_with_platform(platform.clone());
    futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec())).unwrap();
    let session = authority.current_session().unwrap();
    authority
        .grant_auto_signing(&authority.current_operation().unwrap(), "myapp.dot")
        .unwrap();
    let local = futures::executor::block_on(AccountHolder::sign_vrf(
        authority.as_ref(),
        AccountInvocation {
            call: &CallContext::default(),
            session: &session,
            caller: AccountCaller::Local(&ProductContext::new("myapp.dot".to_string()).unwrap()),
        },
        vrf_request("myapp.dot"),
    ));
    let Dispatch::Response(answer) = futures::executor::block_on(
        SigningHostSsoService::new(authority).answer(RemoteMessage::request(
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
            local.map(|_| ()),
            response.payload.map(|_| ()),
            platform.sign_vrf_reviews.lock().unwrap().len(),
        ),
        (Ok(()), Err(v01::HostAccountSignVrfError::Rejected), 1),
    );
}

#[test]
fn remote_account_access_neither_reuses_nor_changes_native_permissions() {
    for operation in ["alias", "list"] {
        for (stored, confirmed, storage_error) in [
            (Some(PermissionAuthorizationStatus::Authorized), false, None),
            (Some(PermissionAuthorizationStatus::Denied), true, None),
            (None, true, None),
            (None, true, Some("permission storage unavailable")),
        ] {
            let platform = Arc::new(StubPlatform {
                account_access_confirmed: confirmed,
                permission_storage_error: storage_error,
                ..StubPlatform::default()
            });
            cache_grant(&platform, "peopl.dot", "{}");
            let (_, authority) =
                signing_runtime_with_ring_resolver(platform.clone(), full_person_ring_resolver());
            futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
                .unwrap();
            let session = authority.current_session().unwrap();
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
            let storage_before = platform.local_storage.lock().unwrap().clone();
            let service = SigningHostSsoService::new(authority);
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
                let Dispatch::Response(answer) =
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
            let expected = if confirmed {
                Ok(())
            } else {
                Err(RingVrfError::Rejected)
            };
            assert_eq!(
                (
                    outcomes,
                    platform.account_access_reviews.lock().unwrap().len(),
                    platform.local_storage.lock().unwrap().clone(),
                ),
                (vec![expected.clone(), expected], 2, storage_before),
                "{operation}, native decision {stored:?}",
            );
        }
    }
}

#[test]
fn remote_published_access_is_independent_of_native_refusals() {
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
        let session = authority.current_session().unwrap();
        register_full_person_key(&authority, &session, &full_person_ring_location());
        let request = ProductRequest {
            calling_product_id: "myapp.dot".to_string(),
            payload: HostAccountRingVrfSignRequest {
                key_handle: full_person_key_handle(),
                message: b"published access".to_vec(),
            },
        };
        let local = futures::executor::block_on(authority.ring_vrf_sign(
            AccountInvocation {
                call: &CallContext::default(),
                session: &session,
                caller: AccountCaller::Local(
                    &ProductContext::new(request.calling_product_id.clone()).unwrap(),
                ),
            },
            request.payload.clone(),
        ));
        let Dispatch::Response(answer) =
            futures::executor::block_on(SigningHostSsoService::new(authority).answer(
                RemoteMessage::request("remote-published-access".to_string(), request),
            ))
        else {
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
            (Err(RingVrfError::NotAllowlisted), Ok(()), 0),
            "{storage_error:?}",
        );
    }
}
