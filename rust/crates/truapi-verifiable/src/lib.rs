//! The Bandersnatch ring-VRF operations truapi uses, over `verifiable`.
//!
//! Native builds link this crate. The browser core loads it as a WASM module of
//! its own, off its startup path: `verifiable`'s ring prover compiles in
//! 4.5 MiB of powers of tau.
//!
//! Each function takes a key's 32-byte entropy and returns a SCALE-encoded
//! `Result<T, String>`.

use parity_scale_codec::Encode;
use verifiable::GenerateVerifiable;
use verifiable::ring::RingDomainSize;
use verifiable::ring::bandersnatch::BandersnatchVrfVerifiable;
use wasm_bindgen::prelude::*;

type Secret = <BandersnatchVrfVerifiable as GenerateVerifiable>::Secret;

/// The key's ring member, its 32-byte public key: `Result<[u8; 32], String>`.
#[wasm_bindgen]
pub fn member(entropy: &[u8]) -> Vec<u8> {
    with_secret(entropy, |secret| {
        Ok(BandersnatchVrfVerifiable::member_from_secret(secret))
    })
}

/// A signature over `message`: `Result<Vec<u8>, String>`.
#[wasm_bindgen]
pub fn sign(entropy: &[u8], message: &[u8]) -> Vec<u8> {
    with_secret(entropy, |secret| {
        BandersnatchVrfVerifiable::sign(secret, message)
            .map(|signature| signature.to_vec())
            .map_err(describe)
    })
}

/// The key's alias in `context`: `Result<[u8; 32], String>`.
#[wasm_bindgen]
pub fn alias(entropy: &[u8], context: &[u8]) -> Vec<u8> {
    with_secret(entropy, |secret| {
        BandersnatchVrfVerifiable::alias_in_context(secret, context).map_err(describe)
    })
}

/// A proof that `member` belongs to the ring `members` (32-byte keys,
/// concatenated) of the ring domain of size `domain`, in `context` and over
/// `message`, and the member's alias in `context`:
/// `Result<(Vec<u8>, [u8; 32]), String>`.
#[wasm_bindgen]
pub fn prove(
    entropy: &[u8],
    domain: u32,
    member: &[u8],
    members: &[u8],
    context: &[u8],
    message: &[u8],
) -> Vec<u8> {
    with_secret(entropy, |secret| {
        let domain = RingDomainSize::try_from(domain).map_err(describe)?;
        let member: [u8; 32] = member.try_into().map_err(describe)?;
        let (members, []) = members.as_chunks::<32>() else {
            return Err("ring members are not 32-byte keys".to_owned());
        };
        let commitment = BandersnatchVrfVerifiable::open(domain, &member, members.iter().copied())
            .map_err(describe)?;
        let (proof, alias) =
            BandersnatchVrfVerifiable::create(commitment, secret, context, message)
                .map_err(describe)?;
        Ok((proof.into_inner(), alias))
    })
}

/// A proof over several 32-byte contexts and the corresponding aliases:
/// `Result<(Vec<u8>, Vec<[u8; 32]>), String>`.
#[wasm_bindgen]
pub fn prove_multi_context(
    entropy: &[u8],
    domain: u32,
    member: &[u8],
    members: &[u8],
    contexts: &[u8],
    message: &[u8],
) -> Vec<u8> {
    with_secret(entropy, |secret| {
        let domain = RingDomainSize::try_from(domain).map_err(describe)?;
        let member: [u8; 32] = member.try_into().map_err(describe)?;
        let (members, []) = members.as_chunks::<32>() else {
            return Err("ring members are not 32-byte keys".to_owned());
        };
        let (contexts, []) = contexts.as_chunks::<32>() else {
            return Err("contexts are not 32-byte values".to_owned());
        };
        if contexts.is_empty() {
            return Err("at least one context is required".to_owned());
        }
        let commitment = BandersnatchVrfVerifiable::open(domain, &member, members.iter().copied())
            .map_err(describe)?;
        let context_slices: Vec<&[u8]> =
            contexts.iter().map(|context| context.as_slice()).collect();
        let (proof, aliases) = BandersnatchVrfVerifiable::create_multi_context(
            commitment,
            secret,
            &context_slices,
            message,
        )
        .map_err(describe)?;
        Ok((proof.into_inner(), aliases.into_iter().collect::<Vec<_>>()))
    })
}

fn with_secret<T: Encode>(
    entropy: &[u8],
    operation: impl FnOnce(&Secret) -> Result<T, String>,
) -> Vec<u8> {
    <[u8; 32]>::try_from(entropy)
        .map_err(describe)
        .and_then(|entropy| operation(&BandersnatchVrfVerifiable::new_secret(entropy)))
        .encode()
}

fn describe(error: impl core::fmt::Debug) -> String {
    format!("{error:?}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use parity_scale_codec::{Decode, DecodeAll};
    use verifiable::ring::ark_vrf::ring::SrsLookup;
    use verifiable::ring::bandersnatch::BandersnatchSha512Ell2;
    use verifiable::ring::{StaticChunk, ring_verifier_builder_params};

    const DOMAIN: RingDomainSize = RingDomainSize::Domain11;

    fn answer<T: Decode>(encoded: Vec<u8>) -> T {
        Result::<T, String>::decode_all(&mut &encoded[..])
            .expect("a SCALE-encoded answer")
            .expect("the operation succeeds")
    }

    fn key(seed: u8) -> [u8; 32] {
        answer(member(&[seed; 32]))
    }

    #[test]
    fn member_and_alias_are_verifiable_s() {
        let secret = BandersnatchVrfVerifiable::new_secret([4; 32]);
        assert_eq!(
            key(4),
            BandersnatchVrfVerifiable::member_from_secret(&secret)
        );
        assert_eq!(
            answer::<[u8; 32]>(alias(&[4; 32], b"context")),
            BandersnatchVrfVerifiable::alias_in_context(&secret, b"context").unwrap(),
        );
    }

    #[test]
    fn a_signature_verifies() {
        let signature: Vec<u8> = answer(sign(&[4; 32], b"message"));
        assert!(BandersnatchVrfVerifiable::verify_signature(
            &signature.try_into().expect("a 64-byte signature"),
            b"message",
            &key(4),
        ));
    }

    #[test]
    fn a_proof_verifies_against_the_ring_commitment() {
        let ring = [key(1), key(2), key(3)];
        let (proof, alias): (Vec<u8>, [u8; 32]) = answer(prove(
            &[2; 32],
            DOMAIN.value(),
            &ring[1],
            &ring.concat(),
            b"context",
            b"message",
        ));

        let builder = ring_verifier_builder_params::<BandersnatchSha512Ell2>(DOMAIN);
        let lookup = |range: core::ops::Range<usize>| {
            (&builder)
                .lookup(range)
                .map(|chunks| chunks.into_iter().map(StaticChunk).collect())
                .ok_or(())
        };
        let mut members = BandersnatchVrfVerifiable::start_members(DOMAIN);
        BandersnatchVrfVerifiable::push_members(&mut members, ring.iter().copied(), lookup)
            .expect("the ring fits the domain");

        assert_eq!(
            BandersnatchVrfVerifiable::validate(
                DOMAIN,
                &proof.try_into().expect("the proof fits its bound"),
                &BandersnatchVrfVerifiable::finish_members(members),
                b"context",
                b"message",
            ),
            Ok(alias),
        );
    }

    #[test]
    fn multi_context_proof_rejects_malformed_inputs() {
        let member = key(2);
        for (entropy, selected, members, contexts) in [
            (&[2; 31][..], &member[..], &member[..], &[1; 64][..]),
            (&[2; 32][..], &[0; 31][..], &member[..], &[1; 64][..]),
            (&[2; 32][..], &member[..], &[0; 31][..], &[1; 64][..]),
            (&[2; 32][..], &member[..], &member[..], &[1; 63][..]),
            (&[2; 32][..], &member[..], &member[..], &[][..]),
        ] {
            let encoded = prove_multi_context(
                entropy,
                DOMAIN.value(),
                selected,
                members,
                contexts,
                b"message",
            );
            assert!(
                Result::<(Vec<u8>, Vec<[u8; 32]>), String>::decode_all(&mut &encoded[..])
                    .unwrap()
                    .is_err()
            );
        }
    }
}
