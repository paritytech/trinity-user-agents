use super::*;
use crate::runtime::signing_host::ring_vrf::{MemberCandidate, ResolvedRing, RingResolver};
use futures::channel::oneshot;
use std::sync::atomic::AtomicUsize;
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

struct LocalRingResolver {
    ring: ResolvedRing,
    pause: Mutex<Option<(oneshot::Sender<()>, oneshot::Receiver<()>)>>,
    lookups: AtomicUsize,
}

#[async_trait::async_trait]
impl RingResolver for LocalRingResolver {
    async fn members_pallet_index(&self, _chain_id: &[u8; 32]) -> Result<u8, RingVrfError> {
        Ok(42)
    }

    async fn validate(&self, _location: &v01::RingLocation) -> Result<[u8; 32], RingVrfError> {
        Ok(*b"pop:polkadot.network/people     ")
    }

    async fn resolve(
        &self,
        _location: &v01::RingLocation,
        candidates: &[MemberCandidate],
    ) -> Result<ResolvedRing, RingVrfError> {
        self.lookups.fetch_add(1, Ordering::SeqCst);
        assert_eq!(candidates, &[self.ring.selected]);
        let pause = self.pause.lock().unwrap().take();
        if let Some((entered, resume)) = pause {
            entered.send(()).unwrap();
            resume.await.unwrap();
        }
        Ok(self.ring.clone())
    }
}

fn local_fixture() -> (
    ProductRuntimeHost,
    Arc<StubPlatform>,
    Arc<LocalRingResolver>,
) {
    use crate::host_logic::product_account::{
        derive_ring_vrf_domain_entropy, derive_ring_vrf_entropy_from_domain,
    };

    let payload = request();
    let domain = derive_ring_vrf_domain_entropy(&[0xab; 16], "myapp.dot").unwrap();
    let entropy =
        derive_ring_vrf_entropy_from_domain(&domain, &payload.key_handle.derivation_index);
    let member = futures::executor::block_on(crate::runtime::vrf::load())
        .unwrap()
        .member(&entropy)
        .unwrap();
    let resolver = Arc::new(LocalRingResolver {
        ring: ResolvedRing {
            selected: MemberCandidate { member },
            ring_index: 7,
            ring_revision: 11,
            domain_size: crate::runtime::vrf::DOMAIN_2E11,
            members: vec![member],
        },
        pause: Mutex::new(None),
        lookups: AtomicUsize::new(0),
    });
    let platform = Arc::new(StubPlatform::default());
    let (config, product) = runtime_config("myapp.dot");
    let services = RuntimeServices::new(
        platform.clone(),
        config.host.host_info.clone(),
        config.people_chain_genesis_hash,
        config.bulletin_chain_genesis_hash,
        config.asset_hub_chain_genesis_hash,
        test_spawner(),
    );
    let authority = PairingHost::new_with_ring_resolver(services.clone(), config, resolver.clone());
    let adapters = crate::host_core::ConnectionAdapters::from_services(&services);
    let host = ProductRuntimeHost::from_services(services, adapters, authority.clone(), product);
    let session = sso_session_info();
    install_pairing_session(&host, session.clone());
    let root =
        crate::host_logic::product_account::derive_root_keypair_from_entropy(&[0xab; 16]).unwrap();
    let subtree =
        crate::host_logic::product_account::derive_product_subtree_keypair(&root, "myapp.dot")
            .unwrap();
    futures::executor::block_on(authority.remember_auto_signing_key_for_tests(
        &session,
        authority.current_session_lifecycle_epoch(),
        "myapp.dot",
        subtree.public.to_bytes(),
        subtree.secret.to_bytes(),
        domain,
    ))
    .unwrap();
    futures::executor::block_on(authority.register_ring_vrf_key_for_tests(
        &session,
        payload.key_handle,
        payload.ring_location,
        member,
    ))
    .unwrap();
    (host, platform, resolver)
}

#[test]
fn honour_proof_uses_the_pairing_hosts_cached_local_key() {
    use verifiable::GenerateVerifiable;
    use verifiable::ring::ark_vrf::ring::SrsLookup;
    use verifiable::ring::bandersnatch::{BandersnatchSha512Ell2, BandersnatchVrfVerifiable};
    use verifiable::ring::{RingDomainSize, StaticChunk, ring_verifier_builder_params};

    let (host, platform, resolver) = local_fixture();
    let payload = request();
    let Answer::V1(response) = futures::executor::block_on(
        host.create_honour_proof(&CallContext::default(), Request::V1(payload.clone())),
    )
    .unwrap();
    assert_eq!(resolver.lookups.load(Ordering::SeqCst), 1);
    assert!(platform.sent_rpc.lock().unwrap().is_empty());
    let domain = RingDomainSize::try_from(resolver.ring.domain_size).unwrap();
    let builder = ring_verifier_builder_params::<BandersnatchSha512Ell2>(domain);
    let lookup = |range: core::ops::Range<usize>| {
        (&builder)
            .lookup(range)
            .map(|chunks| chunks.into_iter().map(StaticChunk).collect())
            .ok_or(())
    };
    let mut members = BandersnatchVrfVerifiable::start_members(domain);
    BandersnatchVrfVerifiable::push_members(
        &mut members,
        resolver.ring.members.iter().copied(),
        lookup,
    )
    .unwrap();
    let root = BandersnatchVrfVerifiable::finish_members(members);
    let contexts = response
        .contextual_aliases
        .iter()
        .map(|alias| alias.context.as_slice())
        .collect::<Vec<_>>();
    assert_eq!(contexts.len(), 2);
    let aliases = BandersnatchVrfVerifiable::validate_multi_context(
        domain,
        &response.proof.try_into().unwrap(),
        &root,
        &contexts,
        &payload.message,
    )
    .unwrap();
    assert_eq!(
        aliases
            .into_iter()
            .map(|alias| alias.to_vec())
            .collect::<Vec<_>>(),
        response
            .contextual_aliases
            .iter()
            .map(|alias| alias.alias.clone())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        (response.ring_index, response.ring_revision),
        (resolver.ring.ring_index, resolver.ring.ring_revision)
    );
}

#[test]
fn honour_proof_rejects_pairing_disconnect_during_local_ring_lookup() {
    let (host, _, resolver) = local_fixture();
    let (entered, entered_rx) = oneshot::channel();
    let (resume, resume_rx) = oneshot::channel();
    *resolver.pause.lock().unwrap() = Some((entered, resume_rx));
    let cx = CallContext::default();
    let (result, ()) = futures::executor::block_on(futures::future::join(
        host.create_honour_proof(&cx, Request::V1(request())),
        async {
            entered_rx.await.unwrap();
            host.disconnect().await;
            resume.send(()).unwrap();
        },
    ));
    assert_eq!(resolver.lookups.load(Ordering::SeqCst), 1);
    assert_eq!(
        result,
        Err(CallError::Domain(HostAccountCreateProofError::V1(
            v01::HostAccountCreateProofError::Unknown {
                reason: "Disconnected".to_string(),
            }
        )))
    );
}

#[test]
fn honour_proof_rejects_pairing_replacement_during_local_ring_lookup() {
    let (host, _, resolver) = local_fixture();
    let (entered, entered_rx) = oneshot::channel();
    let (resume, resume_rx) = oneshot::channel();
    *resolver.pause.lock().unwrap() = Some((entered, resume_rx));
    let cx = CallContext::default();
    let (result, ()) = futures::executor::block_on(futures::future::join(
        host.create_honour_proof(&cx, Request::V1(request())),
        async {
            entered_rx.await.unwrap();
            let mut replacement = sso_session_info();
            replacement.sso.as_mut().unwrap().session_id_own = [0x88; 32];
            install_pairing_session(&host, replacement);
            resume.send(()).unwrap();
        },
    ));
    assert_eq!(resolver.lookups.load(Ordering::SeqCst), 1);
    assert_eq!(
        result,
        Err(CallError::Domain(HostAccountCreateProofError::V1(
            v01::HostAccountCreateProofError::Unknown {
                reason: "Disconnected".to_string(),
            }
        )))
    );
}
