use super::*;
use crate::runtime::WalletSecretProvider;

struct WalletRoot {
    entropy: Result<Vec<u8>, truapi::latest::GenericError>,
    gate: std::sync::Mutex<Option<futures::channel::oneshot::Receiver<()>>>,
}

#[async_trait::async_trait]
impl WalletSecretProvider for WalletRoot {
    async fn read_wallet_root_entropy(
        &self,
        wallet_id: String,
    ) -> Result<Vec<u8>, truapi::latest::GenericError> {
        assert_eq!(wallet_id, "selected-wallet");
        let gate = self.gate.lock().unwrap().take();
        if let Some(gate) = gate {
            gate.await.unwrap();
        }
        self.entropy.clone()
    }
}

#[test]
fn invalid_activation_preserves_the_active_wallet_and_its_grants() {
    let platform = Arc::new(StubPlatform {
        sign_raw_confirmed: false,
        resource_allocation_confirmed: true,
        ..StubPlatform::default()
    });
    let (services, authority) = signing_runtime_with_platform(platform.clone());
    let provider = WalletRoot {
        entropy: Ok(ENTROPY.to_vec()),
        gate: Default::default(),
    };
    futures::executor::block_on(authority.activate_wallet(
        &provider,
        "selected-wallet".to_string(),
        Some("alice".to_string()),
    ))
    .expect("initial activation succeeds");
    let session = authority
        .account_holder()
        .current_session()
        .expect("active wallet");
    let runtime = product_runtime(services, authority.clone());
    auto_signing::grant_auto_signing(&runtime);

    for entropy in [
        Err(truapi::latest::GenericError {
            reason: "protected wallet unavailable".to_string(),
        }),
        Ok(vec![0xCD; 17]),
    ] {
        let provider = WalletRoot {
            entropy,
            gate: Default::default(),
        };
        let error = futures::executor::block_on(authority.activate_wallet(
            &provider,
            "selected-wallet".to_string(),
            Some("bob".to_string()),
        ))
        .expect_err("failed protected reads and invalid entropy cannot replace the wallet");
        assert!(matches!(error, AuthorityError::Unavailable { .. }));
    }

    let HostSignRawResponse::V1(response) = futures::executor::block_on(runtime.sign_raw(
        &CallContext::default(),
        HostSignRawRequest::V1(v01::HostSignRawRequest {
            account: v01::ProductAccountId {
                dot_ns_identifier: "myapp.dot".to_string(),
                derivation_index: v01::DerivationIndex::Index(0),
            },
            payload: v01::RawPayload::Bytes {
                bytes: b"still active".to_vec(),
            },
        }),
    ))
    .expect("the active wallet still signs under its existing grant");
    let root = derive_root_keypair_from_entropy(&ENTROPY).expect("original root derives");
    let keypair =
        derive_product_keypair(&root, "myapp.dot", index_bytes(0)).expect("product key derives");
    let signature =
        schnorrkel::Signature::from_bytes(&response.signature).expect("64-byte signature");

    assert_eq!(
        (
            authority.account_holder().current_session(),
            keypair
                .public
                .verify_simple(b"substrate", b"<Bytes>still active</Bytes>", &signature)
                .is_ok(),
            platform
                .sign_raw_reviews
                .lock()
                .expect("raw signing review list mutex poisoned")
                .is_empty(),
        ),
        (Some(session), true, true),
        "a failed activation must preserve the session token, signing key and grant",
    );
}

#[test]
fn a_late_protected_read_cannot_restore_a_locked_or_reactivated_wallet() {
    use futures::FutureExt;

    for reactivate in [false, true] {
        let (_, authority) = signing_runtime();
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec())).unwrap();
        let (release, gate) = futures::channel::oneshot::channel();
        let provider = WalletRoot {
            entropy: Ok(vec![0xCD; 16]),
            gate: std::sync::Mutex::new(Some(gate)),
        };
        let activation = authority.activate_wallet(
            &provider,
            "selected-wallet".to_string(),
            Some("late".to_string()),
        );
        futures::pin_mut!(activation);
        assert!(activation.as_mut().now_or_never().is_none());
        if reactivate {
            futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
                .unwrap();
        } else {
            authority.lock_wallet();
        }
        let expected = authority.account_holder().current_session();
        let revision = authority.grants.lifecycle().revision();
        release.send(()).unwrap();
        assert_eq!(
            (
                futures::executor::block_on(activation),
                authority.account_holder().current_session(),
                authority.grants.lifecycle().revision()
            ),
            (Err(AuthorityError::Disconnected), expected, revision),
        );
    }
}

#[test]
fn pending_vrf_approval_distinguishes_wallet_and_host_reset() {
    use crate::host_internal::sso_messages::{RemoteMessage, RemoteMessageData, v1};
    use crate::runtime::SsoAccountHolderService;
    use crate::runtime::sso_service::Dispatch;
    use futures::FutureExt;
    use truapi::versioned::account::{HostAccountSignVrfError, HostAccountSignVrfRequest};

    for remote in [false, true] {
        for change in ["lock", "reactivate", "reset"] {
            let (release, gate) = futures::channel::oneshot::channel();
            let platform = Arc::new(StubPlatform {
                sign_vrf_confirmed: true,
                sign_vrf_confirmation_gate: std::sync::Mutex::new(Some(gate)),
                ..StubPlatform::default()
            });
            let (services, authority) = signing_runtime_with_platform(platform.clone());
            futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
                .unwrap();
            let session = authority.account_holder().current_session().unwrap();
            let runtime = product_runtime(services, authority.clone());
            let service = SsoAccountHolderService::new(authority.account_holder().clone(), authority.account_holder().current_session().unwrap());
            let answer = async {
                if remote {
                    let Dispatch::Response(answer) = service
                        .answer(RemoteMessage::request(
                            "pending-vrf".to_string(),
                            ProductRequest {
                                calling_product_id: "myapp.dot".to_string(),
                                payload: vrf_request("myapp.dot"),
                            },
                        ))
                        .await.map_err(v01::HostAccountSignVrfError::from)?
                    else {
                        panic!("expected a VRF response")
                    };
                    let RemoteMessageData::V1(v1::RemoteMessage::SignVrfResponse(response)) =
                        answer.message.data
                    else {
                        panic!("expected a VRF signing response")
                    };
                    response.payload.map(|_| ())
                } else {
                    runtime
                        .sign_vrf(
                            &CallContext::default(),
                            HostAccountSignVrfRequest::V1(vrf_request("myapp.dot")),
                        )
                        .await
                        .map(|_| ())
                        .map_err(|error| match error {
                            CallError::Domain(HostAccountSignVrfError::V1(error)) => error,
                            other => panic!("unexpected signing failure: {other:?}"),
                        })
                }
            };
            futures::pin_mut!(answer);
            assert!(answer.as_mut().now_or_never().is_none());
            match change {
                "lock" => futures::executor::block_on(authority.disconnect()),
                "reactivate" => {
                    futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
                        .unwrap();
                }
                "reset" => authority.clear_product_state("myapp.dot").unwrap(),
                _ => unreachable!(),
            }
            release.send(()).unwrap();
            let expected = if change == "reset" && remote {
                Ok(())
            } else {
                Err(v01::HostAccountSignVrfError::NotConnected)
            };
            assert_eq!(
                (
                    futures::executor::block_on(answer),
                    authority.account_holder().current_session() == Some(session),
                    platform.sign_vrf_reviews.lock().unwrap().len()
                ),
                (expected, change == "reset", 1),
                "{change}, remote: {remote}",
            );
        }
    }
}

#[test]
fn product_reset_during_allocation_review_cannot_restore_native_grants() {
    use futures::FutureExt;

    let (release, gate) = futures::channel::oneshot::channel();
    let platform = Arc::new(StubPlatform {
        resource_allocation_confirmed: true,
        resource_allocation_confirmation_gate: std::sync::Mutex::new(Some(gate)),
        ..StubPlatform::default()
    });
    let (services, authority) = signing_runtime_with_platform(platform.clone());
    futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec())).unwrap();
    let runtime = product_runtime(services, authority.clone());
    let cx = CallContext::default();
    let allocation = ResourceAllocation::request(
        &runtime,
        &cx,
        HostRequestResourceAllocationRequest::V1(v01::HostRequestResourceAllocationRequest {
            resources: vec![v01::AllocatableResource::AutoSigning],
        }),
    );
    futures::pin_mut!(allocation);
    assert!(allocation.as_mut().now_or_never().is_none());
    authority.clear_product_state("myapp.dot").unwrap();
    release.send(()).unwrap();
    let result = futures::executor::block_on(allocation);
    let session = authority.account_holder().current_session().unwrap();
    let status = authority.account_holder().auto_signing_status(
        &session,
        "myapp.dot",
        &vrf_request("myapp.dot").account,
        authority
            .accounts()
            .wallet_authorization(
                &authority.accounts().current_operation().unwrap(),
                &ProductContext::new("myapp.dot".to_string()).unwrap(),
            )
            .unwrap()
            .as_ref(),
    );
    assert_eq!(
        (
            result.map(|_| ()),
            status,
            platform.resource_allocation_reviews.lock().unwrap().len()
        ),
        (
            Err(CallError::Domain(
                truapi::versioned::resource_allocation::HostRequestResourceAllocationError::V1(
                    v01::ResourceAllocationError::Unknown {
                        reason: AuthorityError::Disconnected.to_string()
                    }
                )
            )),
            Ok(crate::runtime::authority::AutoSigningGrant::Absent),
            1
        ),
    );
}

#[test]
fn wallet_change_during_ring_preparation_rejects_the_alias() {
    use futures::FutureExt;

    for lock in [false, true] {
        let resolver = full_person_ring_resolver();
        let (_, authority) =
            signing_runtime_with_ring_resolver(Arc::new(StubPlatform::default()), resolver.clone());
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec())).unwrap();
        let session = authority.account_holder().current_session().unwrap();
        let ring = full_person_ring_location();
        register_full_person_key(&authority, &session, &ring);
        let (release, gate) = futures::channel::oneshot::channel();
        *resolver.validation_gate.lock().unwrap() = Some(gate);
        let product = ProductContext::new("peopl.dot".to_string()).unwrap();
        let cx = CallContext::default();
        let alias = authority.account_holder().account_alias(
            AccountInvocation {
                call: &cx,
                session: &session,
                caller: AccountCaller::Local {
                    product: &product,
                    authorization: None,
                    outbound_review: None,
                },
            },
            HostAccountGetAliasRequest {
                key_handle: full_person_key_handle(),
                context: v01::ProductProofContext {
                    product_id: "peopl.dot".to_string(),
                    suffix: v01::DerivationIndex::Index(0),
                },
                ring_location: ring,
            },
        );
        futures::pin_mut!(alias);
        assert!(alias.as_mut().now_or_never().is_none());
        if lock {
            futures::executor::block_on(authority.disconnect());
        } else {
            futures::executor::block_on(authority.activate_local_session([0xCD; 16].to_vec()))
                .unwrap();
        }
        release.send(()).unwrap();
        assert_eq!(
            futures::executor::block_on(alias),
            Err(RingVrfError::from(AuthorityError::Disconnected))
        );
    }
}
