use super::*;
use crate::host_internal::sso_messages::{RemoteMessage, RemoteMessageData};
use crate::host_internal::sso_wire::SsoRequest;
use crate::runtime::signing_host::sso_service::SigningHostSsoService;
use crate::runtime::sso_service::Dispatch;
use parity_scale_codec::{DecodeAll, Encode};
use truapi::latest::{HostAccountCreateHonourProofRequest, HostAccountCreateHonourProofResponse};
use truapi::versioned::account::{
    HostAccountCreateHonourProofRequest as Request, HostAccountCreateProofError,
};
use verifiable::GenerateVerifiable;
use verifiable::ring::ark_vrf::ring::SrsLookup;
use verifiable::ring::bandersnatch::{BandersnatchSha512Ell2, BandersnatchVrfVerifiable as Crypto};
use verifiable::ring::{RingDomainSize, StaticChunk, ring_verifier_builder_params};

fn request() -> HostAccountCreateHonourProofRequest {
    HostAccountCreateHonourProofRequest {
        key_handle: full_person_key_handle(),
        ring_location: full_person_ring_location(),
        subject: [1; 32],
        point: 0,
        message: [2; 32],
    }
}

fn fixture(grant: bool) -> (Arc<RuntimeServices>, Arc<SigningHostRole>, AuthoritySession) {
    let platform = Arc::new(StubPlatform::default());
    cache_grant(
        &platform,
        "peopl.dot",
        if grant {
            r#"{"dim2":["context"]}"#
        } else {
            "{}"
        },
    );
    let (services, authority) =
        signing_runtime_with_ring_resolver(platform, full_person_ring_resolver());
    futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec())).unwrap();
    let session = authority.current_session().unwrap();
    register_full_person_key(&authority, &session, &full_person_ring_location());
    (services, authority, session)
}

fn verify(
    response: &HostAccountCreateHonourProofResponse,
    request: &HostAccountCreateHonourProofRequest,
    ring: &ResolvedRing,
) {
    let contexts = response
        .contextual_aliases
        .iter()
        .map(|alias| alias.context.as_slice())
        .collect::<Vec<_>>();
    assert_eq!(response.contextual_aliases.len(), 2);
    assert_eq!(
        hex::encode(contexts[0]),
        "eb5adb96e9dda844f7585424f518ea5156ff2f1598d22e80c4b4a6225eddc05a"
    );
    assert_eq!(
        hex::encode(contexts[1]),
        "28f39e1fba0afb804589dc68b2b7d71e65df25de1f41f099da9bf98c2282e72e"
    );
    let domain = RingDomainSize::try_from(ring.domain_size).unwrap();
    let builder = ring_verifier_builder_params::<BandersnatchSha512Ell2>(domain);
    let lookup = |range: core::ops::Range<usize>| {
        (&builder)
            .lookup(range)
            .map(|chunks| chunks.into_iter().map(StaticChunk).collect())
            .ok_or(())
    };
    let mut members = Crypto::start_members(domain);
    Crypto::push_members(&mut members, ring.members.iter().copied(), lookup).unwrap();
    let root = Crypto::finish_members(members);
    let proof = response.proof.clone().try_into().unwrap();
    let aliases =
        Crypto::validate_multi_context(domain, &proof, &root, &contexts, &request.message).unwrap();
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
        (ring.ring_index, ring.ring_revision)
    );
    let entropy = derive_ring_vrf_entropy(
        &ENTROPY,
        &request.key_handle.dot_ns_identifier,
        &request.key_handle.derivation_index,
    )
    .unwrap();
    let vrf = futures::executor::block_on(crate::runtime::vrf::load()).unwrap();
    assert_eq!(
        response
            .contextual_aliases
            .iter()
            .map(|alias| alias.alias.clone())
            .collect::<Vec<_>>(),
        contexts
            .iter()
            .map(|context| vrf.alias(&entropy, context).unwrap().to_vec())
            .collect::<Vec<_>>()
    );
    let changed = [3; 32];
    assert!(Crypto::validate_multi_context(domain, &proof, &root, &contexts, &changed).is_err());
    assert!(
        Crypto::validate_multi_context(
            domain,
            &proof,
            &root,
            &[contexts[1], contexts[0]],
            &request.message
        )
        .is_err()
    );
    let other_subject = [0; 32];
    assert!(
        Crypto::validate_multi_context(
            domain,
            &proof,
            &root,
            &[&other_subject, contexts[1]],
            &request.message
        )
        .is_err()
    );
    let other_point = [0; 32];
    assert!(
        Crypto::validate_multi_context(
            domain,
            &proof,
            &root,
            &[contexts[0], &other_point],
            &request.message
        )
        .is_err()
    );
}

#[test]
fn honour_proof_preserves_aliases_and_binds_the_vote() {
    let (services, authority, _) = fixture(true);
    let host = product_runtime_for(services, authority, "dim2.dot");
    let request = request();
    let response = futures::executor::block_on(
        host.create_honour_proof(&CallContext::default(), Request::V1(request.clone())),
    )
    .unwrap();
    let truapi::versioned::account::HostAccountCreateHonourProofResponse::V1(response) = response;
    verify(&response, &request, &full_person_ring_resolver().ring);
}

#[test]
fn honour_proof_requires_a_grant_on_the_product_and_signing_paths() {
    let (services, authority, session) = fixture(false);
    let host = product_runtime_for(services, authority.clone(), "dim2.dot");
    assert_eq!(
        futures::executor::block_on(
            host.create_honour_proof(&CallContext::default(), Request::V1(request()))
        ),
        Err(CallError::Domain(HostAccountCreateProofError::V1(
            v01::HostAccountCreateProofError::NotAllowlisted
        )))
    );
    assert_eq!(
        futures::executor::block_on(authority.create_honour_proof(
            &CallContext::default(),
            &session,
            ProductRequest {
                calling_product_id: "dim2.dot".to_string(),
                payload: request()
            }
        )),
        Err(RingVrfError::NotAllowlisted)
    );
}

#[test]
fn honour_proof_rejects_wrong_chains_collections_and_paths() {
    let (_, authority, session) = fixture(true);
    let mut wrong_chain = request();
    wrong_chain.ring_location.chain_id = [0xaa; 32];
    let mut lite = request();
    lite.ring_location.junctions[1] = v01::RingLocationJunction::CollectionId(
        PersonhoodCollection::LitePeople.identifier().to_vec(),
    );
    let mut duplicate = request();
    duplicate
        .ring_location
        .junctions
        .push(v01::RingLocationJunction::CollectionId(
            PersonhoodCollection::People.identifier().to_vec(),
        ));
    for payload in [wrong_chain, lite, duplicate] {
        assert_eq!(
            futures::executor::block_on(authority.create_honour_proof(
                &CallContext::default(),
                &session,
                ProductRequest {
                    calling_product_id: "dim2.dot".to_string(),
                    payload
                }
            )),
            Err(RingVrfError::RingNotFound)
        );
    }
}

#[test]
fn honour_proof_requires_registration_for_the_requested_ring() {
    let (_, authority, session) = fixture(true);
    let mut unregistered = request();
    unregistered.key_handle.derivation_index = v01::DerivationIndex::Index(9);
    assert_eq!(
        futures::executor::block_on(authority.create_honour_proof(
            &CallContext::default(),
            &session,
            ProductRequest {
                calling_product_id: "dim2.dot".to_string(),
                payload: unregistered
            }
        )),
        Err(RingVrfError::KeyNotRegistered)
    );
    let mut other_location = request();
    other_location.ring_location.junctions.remove(0);
    assert_eq!(
        futures::executor::block_on(authority.create_honour_proof(
            &CallContext::default(),
            &session,
            ProductRequest {
                calling_product_id: "dim2.dot".to_string(),
                payload: other_location
            }
        )),
        Err(RingVrfError::KeyNotInRing)
    );
}

#[test]
fn honour_proof_rejects_a_replaced_signing_session() {
    let (_, authority, session) = fixture(true);
    futures::executor::block_on(authority.activate_local_session(vec![0xcd; 16])).unwrap();
    assert!(
        futures::executor::block_on(authority.create_honour_proof(
            &CallContext::default(),
            &session,
            ProductRequest {
                calling_product_id: "dim2.dot".to_string(),
                payload: request()
            }
        ))
        .is_err()
    );
}

#[test]
fn honour_proof_round_trips_through_the_sso_dispatcher() {
    let (_, authority, _) = fixture(true);
    let payload = request();
    let request = RemoteMessage::request(
        "honour-1".to_string(),
        ProductRequest {
            calling_product_id: "dim2.dot".to_string(),
            payload: payload.clone(),
        },
    );
    assert_eq!(request.data.encode()[1], 25);
    let encoded = request.encode();
    let request = RemoteMessage::decode_all(&mut &encoded[..]).unwrap();
    let service = SigningHostSsoService::new(authority);
    let Dispatch::Response(answer) = futures::executor::block_on(service.answer(request)) else {
        panic!("expected a proof response");
    };
    assert_eq!(answer.outcome.outcome, "ok");
    let encoded = answer.message.encode();
    assert_eq!(answer.message.data.encode()[1], 26);
    let decoded = RemoteMessage::decode_all(&mut &encoded[..]).unwrap();
    let RemoteMessageData::V1(message) = decoded.data;
    let response =
        <ProductRequest<HostAccountCreateHonourProofRequest> as SsoRequest>::response_from_message(
            message,
        )
        .unwrap();
    assert_eq!(response.responding_to, "honour-1");
    verify(
        &response.payload.unwrap(),
        &payload,
        &full_person_ring_resolver().ring,
    );
}

struct NonMemberResolver;

#[async_trait::async_trait]
impl RingResolver for NonMemberResolver {
    async fn members_pallet_index(&self, _chain_id: &[u8; 32]) -> Result<u8, RingVrfError> {
        Ok(42)
    }

    async fn validate(&self, _location: &v01::RingLocation) -> Result<[u8; 32], RingVrfError> {
        Ok(*PersonhoodCollection::People.identifier())
    }

    async fn resolve(
        &self,
        _location: &v01::RingLocation,
        _candidates: &[MemberCandidate],
    ) -> Result<ResolvedRing, RingVrfError> {
        Err(RingVrfError::NotMember)
    }
}

#[test]
fn honour_proof_requires_active_membership_after_registration() {
    let platform = Arc::new(StubPlatform::default());
    cache_grant(&platform, "peopl.dot", r#"{"dim2":["context"]}"#);
    let authority = SigningHostRole::new_with_ring_resolver(platform, Arc::new(NonMemberResolver));
    futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec())).unwrap();
    let session = authority.current_session().unwrap();
    register_full_person_key(&authority, &session, &full_person_ring_location());
    assert_eq!(
        futures::executor::block_on(authority.create_honour_proof(
            &CallContext::default(),
            &session,
            ProductRequest {
                calling_product_id: "dim2.dot".to_string(),
                payload: request()
            },
        )),
        Err(RingVrfError::NotMember)
    );
}

#[test]
fn honour_proof_accepts_a_registered_product_key_at_another_index() {
    let mut request = request();
    request.key_handle.dot_ns_identifier = "dim2.dot".to_string();
    request.key_handle.derivation_index = v01::DerivationIndex::Index(7);
    let entropy =
        derive_ring_vrf_entropy(&ENTROPY, "dim2.dot", &request.key_handle.derivation_index)
            .unwrap();
    let member = futures::executor::block_on(crate::runtime::vrf::load())
        .unwrap()
        .member(&entropy)
        .unwrap();
    let mut resolver = (*full_person_ring_resolver()).clone();
    resolver.ring.selected = MemberCandidate { member };
    resolver.ring.members = vec![member];
    let resolver = Arc::new(resolver);
    let (services, authority) =
        signing_runtime_with_ring_resolver(Arc::new(StubPlatform::default()), resolver.clone());
    futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec())).unwrap();
    let session = authority.current_session().unwrap();
    futures::executor::block_on(authority.register_ring_vrf_key(
        &CallContext::default(),
        &session,
        ProductRequest {
            calling_product_id: "dim2.dot".to_string(),
            payload: HostAccountRegisterRingVrfKeyRequest {
                index: request.key_handle.derivation_index.clone(),
                ring: request.ring_location.clone(),
            },
        },
    ))
    .unwrap();
    let host = product_runtime_for(services, authority, "dim2.dot");
    let truapi::versioned::account::HostAccountCreateHonourProofResponse::V1(response) =
        futures::executor::block_on(
            host.create_honour_proof(&CallContext::default(), Request::V1(request.clone())),
        )
        .unwrap();
    verify(&response, &request, &resolver.ring);
}
