//! Bandersnatch ring-VRF one-shot membership proof (prover side).
//!
//! Wraps `verifiable`'s prover-gated `open` + `create` into the single-shot
//! proof a `RegisterStatementStoreAllowance` needs: prove that our member key is
//! in the collection's ring, bound to a slot `context` and the extrinsic proof
//! `message`. Mirrors signing-bot `ring-proof.ts` `oneShotProof`.

use thiserror::Error;

use super::StatementAllowanceError;
use crate::host_internal::sso_messages::RingVrfError;
use crate::runtime::vrf;

/// A single-context ring-VRF signature is exactly 785 bytes.
pub const RING_VRF_PROOF_LEN: usize = 785;

/// Error while constructing a ring-VRF allowance proof.
#[derive(Debug, Error)]
pub enum ProofError {
    /// On-chain ring exponent does not map to a supported proof domain.
    #[error("unsupported ring exponent {exponent}")]
    UnsupportedRingExponent {
        /// Ring exponent.
        exponent: u8,
    },
    /// The ring-VRF operation failed, including when the member key is not in
    /// the ring.
    #[error("ring-VRF failed: {0}")]
    Vrf(String),
    /// Proof encoded to an unexpected length.
    #[error("ring-VRF proof is {actual} bytes, expected {expected}")]
    InvalidProofLength {
        /// Actual byte length.
        actual: usize,
        /// Expected byte length.
        expected: usize,
    },
}

/// Map an on-chain `RingExponent` (9 / 10 / 14) to the FFT domain size
/// (power = exponent + 2).
pub fn domain_for_ring_exponent(exponent: u8) -> Result<u32, StatementAllowanceError> {
    match exponent {
        9 => Ok(vrf::DOMAIN_2E11),
        10 => Ok(vrf::DOMAIN_2E12),
        14 => Ok(vrf::DOMAIN_2E16),
        other => Err(ProofError::UnsupportedRingExponent { exponent: other }.into()),
    }
}

/// The ring member key for a bandersnatch entropy (`blake2b256(bip39_entropy)`).
pub async fn member_key(entropy: [u8; 32]) -> Result<[u8; 32], StatementAllowanceError> {
    let vrf = vrf::load().await.map_err(vrf_error)?;
    Ok(vrf.member(&entropy).map_err(vrf_error)?)
}

/// The ring member for fixed test entropy.
#[cfg(test)]
pub fn member_key_now(entropy: [u8; 32]) -> [u8; 32] {
    futures::executor::block_on(member_key(entropy)).expect("the ring member derives")
}

/// Produce the 785-byte ring-VRF membership proof over `members` (already
/// sliced to the ring's included prefix), bound to `context` and `message`.
///
/// `entropy` is the bandersnatch entropy; its member key must be present in
/// `members` or `open` fails with `NotInRing`.
pub async fn ring_vrf_proof(
    domain: u32,
    entropy: [u8; 32],
    members: &[[u8; 32]],
    context: &[u8],
    message: &[u8],
) -> Result<Vec<u8>, StatementAllowanceError> {
    let vrf = vrf::load().await.map_err(vrf_error)?;
    let member = vrf.member(&entropy).map_err(vrf_error)?;
    let (bytes, _alias) = vrf
        .prove(&entropy, domain, &member, members, context, message)
        .map_err(vrf_error)?;
    if bytes.len() != RING_VRF_PROOF_LEN {
        return Err(ProofError::InvalidProofLength {
            actual: bytes.len(),
            expected: RING_VRF_PROOF_LEN,
        }
        .into());
    }
    Ok(bytes)
}

/// A failed ring-VRF operation, or a module that did not load.
pub fn vrf_error(error: RingVrfError) -> ProofError {
    ProofError::Vrf(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor::block_on;

    #[test]
    fn exponent_maps_to_domain() {
        assert_eq!(domain_for_ring_exponent(9).unwrap(), vrf::DOMAIN_2E11);
        assert_eq!(domain_for_ring_exponent(10).unwrap(), vrf::DOMAIN_2E12);
        assert_eq!(domain_for_ring_exponent(14).unwrap(), vrf::DOMAIN_2E16);
        assert!(domain_for_ring_exponent(11).is_err());
    }

    #[test]
    fn proof_is_785_bytes_for_a_single_member_ring() {
        let entropy = [0x11u8; 32];
        let member = member_key_now(entropy);
        let members = vec![member];
        let proof = block_on(ring_vrf_proof(
            vrf::DOMAIN_2E11,
            entropy,
            &members,
            &[0x33; 32],
            &[0x42; 32],
        ))
        .unwrap();
        assert_eq!(proof.len(), RING_VRF_PROOF_LEN);
    }

    #[test]
    fn open_fails_when_member_absent_from_ring() {
        let entropy = [0x11u8; 32];
        let other = member_key_now([0x22u8; 32]);
        let err = block_on(ring_vrf_proof(
            vrf::DOMAIN_2E11,
            entropy,
            &[other],
            &[0x33; 32],
            &[0x42; 32],
        ))
        .unwrap_err();
        assert!(
            err.to_string().contains("NotInRing"),
            "unexpected error: {err}"
        );
    }
}
