//! Contexts and proofs for the Honour pallet.

use super::signing_host::ring_vrf::ResolvedRing;
use super::statement_allowance::collection::PersonhoodCollection;
use super::vrf::Vrf;
use crate::host_internal::sso_messages::RingVrfError;
use crate::host_logic::features::genesis_for;
use crate::platform::Platform;
use truapi::latest::{
    ChainIdentifier, ContextualAlias, HostAccountCreateHonourProofRequest,
    HostAccountCreateHonourProofResponse, RingLocation, RingLocationJunction,
};

/// Check that the requested ring is the full People collection on the configured People chain.
pub async fn validate_ring(
    platform: &dyn Platform,
    location: &RingLocation,
) -> Result<(), RingVrfError> {
    let chains = platform
        .supported_chains()
        .await
        .map_err(|error| RingVrfError::Unknown {
            reason: error.reason,
        })?;
    if genesis_for(&chains, ChainIdentifier::People) != Some(location.chain_id) {
        return Err(RingVrfError::RingNotFound);
    }
    let collection = match location.junctions.as_slice() {
        [RingLocationJunction::CollectionId(collection)]
        | [
            RingLocationJunction::PalletInstance(_),
            RingLocationJunction::CollectionId(collection),
        ] => collection,
        _ => return Err(RingVrfError::RingNotFound),
    };
    if collection.as_slice() != PersonhoodCollection::People.identifier() {
        return Err(RingVrfError::RingNotFound);
    }
    Ok(())
}

fn contexts(subject: &[u8; 32], point: u8) -> [[u8; 32]; 2] {
    let mut subject_input = b"pop:polkadot.network/honour/subject:".to_vec();
    subject_input.extend_from_slice(subject);
    let mut point_input = b"pop:polkadot.network/honour/point:".to_vec();
    point_input.push(point);
    [
        sp_crypto_hashing::twox_256(&subject_input),
        sp_crypto_hashing::twox_256(&point_input),
    ]
}

/// Create the subject and point proof in the order checked by `HonourAuth`.
pub fn create_proof(
    vrf: &Vrf,
    entropy: &[u8; 32],
    ring: &ResolvedRing,
    request: &HostAccountCreateHonourProofRequest,
) -> Result<HostAccountCreateHonourProofResponse, RingVrfError> {
    let contexts = contexts(&request.subject, request.point);
    let (proof, aliases) = vrf.prove_multi_context(
        entropy,
        ring.domain_size,
        &ring.selected.member,
        &ring.members,
        &contexts,
        &request.message,
    )?;
    Ok(HostAccountCreateHonourProofResponse {
        proof,
        contextual_aliases: contexts
            .into_iter()
            .zip(aliases)
            .map(|(context, alias)| ContextualAlias {
                context,
                alias: alias.to_vec(),
            })
            .collect(),
        ring_index: ring.ring_index,
        ring_revision: ring.ring_revision,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn honour_contexts_match_the_pallet_at_the_highest_point() {
        let result = contexts(&[1; 32], u8::MAX);
        assert_eq!(
            hex::encode(result[0]),
            "eb5adb96e9dda844f7585424f518ea5156ff2f1598d22e80c4b4a6225eddc05a"
        );
        assert_eq!(
            hex::encode(result[1]),
            "dea9127dae5a3a83b91f30371b9534e8bf7e48d64ac8a0c27ea269854490b49f"
        );
    }
}
