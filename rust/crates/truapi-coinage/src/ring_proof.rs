// SPDX-License-Identifier: AGPL-3.0-only
// Derived from paritytech/brevity-dozer, core/crates/brevity-coinage.
// Copyright the Brevity contributors. See NOTICE and LICENSE in this crate.

use parity_scale_codec::Encode;

use crate::selection::RecyclerKey;

/// `"pop:polkadot.network/coinrecyclr"` — the recycler alias context.
pub const RECYCLER_ALIAS_CONTEXT: &[u8; 32] = b"pop:polkadot.network/coinrecyclr";
/// Prefix of the free unload-token context.
pub const FREE_UNLOAD_TOKEN_CONTEXT_PREFIX: &[u8] = b"pop:polkadot.net/coinftk";
/// A single-context Bandersnatch ring-VRF proof is fixed at 785 bytes.
pub const RING_VRF_PROOF_LEN: usize = 785;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PersonOriginKind {
    Full,
    Lite,
}

/// One finalized ring snapshot. The exponent is the on-chain
/// `Members.CollectionInfo.ring_size` exponent (9/10/14), not the
/// Bandersnatch PCS exponent (11/12/16). `ring_revision` is the live
/// `Members.Root.revision` of the snapshot — the 2026-08 runtime
/// (spec 1000032) requires it inside every proof-bearing extension.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RingProofParams {
    pub ring_exponent: u8,
    pub ring_index: u32,
    pub ring_revision: u32,
    pub ring_members: Vec<[u8; 32]>,
}

/// One distinct free unload-token slot selected for a recycler group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedUnloadToken {
    pub period: u32,
    pub counter: u32,
}

impl ResolvedUnloadToken {
    /// `coinftk || period(le u32) || counter(le u32)`.
    pub fn context(self) -> Vec<u8> {
        let mut context = Vec::with_capacity(FREE_UNLOAD_TOKEN_CONTEXT_PREFIX.len() + 8);
        context.extend_from_slice(FREE_UNLOAD_TOKEN_CONTEXT_PREFIX);
        context.extend_from_slice(&self.period.to_le_bytes());
        context.extend_from_slice(&self.counter.to_le_bytes());
        context
    }
}

/// Everything the proof signer needs after the transaction builder has
/// produced the inherited implication.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnloadProofRequest {
    pub recycler: RecyclerKey,
    pub voucher_derivation_indices: Vec<u32>,
    pub recycler_ring: RingProofParams,
    pub person_origin: PersonOriginKind,
    pub people_ring: RingProofParams,
    pub token: ResolvedUnloadToken,
    pub inherited_implication: Vec<u8>,
}

/// The complete proof-bearing `AsCoinage` payload plus the aliases carried by
/// the unload call itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnloadTokenProof {
    pub person_origin: PersonOriginKind,
    pub people_proof: Vec<u8>,
    pub people_ring_index: u32,
    pub people_ring_revision: u32,
    pub period: u32,
    pub counter: u32,
    pub aliases: Vec<[u8; 32]>,
    pub alias_proofs: Vec<Vec<u8>>,
}

impl UnloadTokenProof {
    /// SCALE-ready `AsCoinage(Some(…))` value for installation into the
    /// prepared transaction extension.
    pub fn as_coinage_extension(&self) -> crate::tx_extensions::AsCoinage {
        use crate::tx_extensions::{AsCoinage, CoinagePeopleProof};

        let proof = CoinagePeopleProof {
            proof: self.people_proof.clone(),
            ring: self.people_ring_index,
            revision: self.people_ring_revision,
        };
        match self.person_origin {
            PersonOriginKind::Full => AsCoinage::unload_token_people(
                proof,
                self.period,
                self.counter,
                self.alias_proofs.clone(),
            ),
            PersonOriginKind::Lite => AsCoinage::unload_token_lite_people(
                proof,
                self.period,
                self.counter,
                self.alias_proofs.clone(),
            ),
        }
    }
}

/// A fail-closed proof construction error. Every variant carries a
/// concrete runtime/data cause (empty input, mismatched ring parameters,
/// or an underlying proof-generation failure) rather than standing in for
/// missing crypto support.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RingProofError {
    EmptyVoucherGroup,
    RecyclerMismatch,
    EmptyRing(&'static str),
    Proof(String),
}

impl std::fmt::Display for RingProofError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyVoucherGroup => f.write_str("cannot prove an empty voucher group"),
            Self::RecyclerMismatch => {
                f.write_str("recycler proof parameters do not match the voucher group")
            }
            Self::EmptyRing(label) => write!(f, "{label} ring has no included members"),
            Self::Proof(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for RingProofError {}

/// The signing effect consumed by an offboard transaction submitter.
/// Chain-state preparation (person-origin selection, ring pages, and free
/// token slot resolution) stays outside this secret-holding trait. Both
/// methods are intentionally synchronous: once those inputs and the
/// implication exist, proof generation is local CPU work only.
pub trait RingProofProvider: Send + Sync {
    /// Derives the public aliases needed to build the unload call. Aliases do
    /// not depend on the transaction implication, so this is the first half of
    /// the submitter's two-step flow.
    fn unload_aliases(
        &self,
        voucher_derivation_indices: &[u32],
    ) -> Result<Vec<[u8; 32]>, RingProofError>;

    /// Creates the proof-bearing extension after the aliases are in the call
    /// and the resulting inherited implication has been derived.
    fn unload_proof(
        &self,
        request: &UnloadProofRequest,
    ) -> Result<UnloadTokenProof, RingProofError>;
}

/// The production local signer. Root entropy is retained only in zeroizing
/// memory and no derived secret appears in the returned payload.
pub struct BandersnatchRingProofProvider {
    vouchers: crate::keys::VoucherKeypairFactory,
    crypto: std::sync::Arc<dyn crate::keys::VoucherCryptography>,
    people: std::sync::Arc<dyn PersonRingProofSigner>,
}

impl BandersnatchRingProofProvider {
    pub fn new(
        entropy: &[u8],
        crypto: std::sync::Arc<dyn crate::keys::VoucherCryptography>,
        people: std::sync::Arc<dyn PersonRingProofSigner>,
    ) -> Self {
        Self {
            vouchers: crate::keys::VoucherKeypairFactory::new(entropy),
            crypto,
            people,
        }
    }
}

impl RingProofProvider for BandersnatchRingProofProvider {
    fn unload_aliases(
        &self,
        voucher_derivation_indices: &[u32],
    ) -> Result<Vec<[u8; 32]>, RingProofError> {
        if voucher_derivation_indices.is_empty() {
            return Err(RingProofError::EmptyVoucherGroup);
        }
        let vouchers = &self.vouchers;
        voucher_derivation_indices
            .iter()
            .map(|index| {
                vouchers
                    .alias(*index, RECYCLER_ALIAS_CONTEXT, self.crypto.as_ref())
                    .map_err(RingProofError::Proof)
            })
            .collect()
    }

    fn unload_proof(
        &self,
        request: &UnloadProofRequest,
    ) -> Result<UnloadTokenProof, RingProofError> {
        if request.voucher_derivation_indices.is_empty() {
            return Err(RingProofError::EmptyVoucherGroup);
        }
        if request.recycler_ring.ring_index != request.recycler.index {
            return Err(RingProofError::RecyclerMismatch);
        }
        if request.recycler_ring.ring_members.is_empty() {
            return Err(RingProofError::EmptyRing("recycler"));
        }
        if request.people_ring.ring_members.is_empty() {
            return Err(RingProofError::EmptyRing("People"));
        }

        let alias_message = blake2b_256(&request.inherited_implication);
        let vouchers = &self.vouchers;
        let aliases = self.unload_aliases(&request.voucher_derivation_indices)?;
        let mut alias_proofs = Vec::with_capacity(request.voucher_derivation_indices.len());
        for index in &request.voucher_derivation_indices {
            alias_proofs.push(
                vouchers
                    .ring_vrf_proof(
                        *index,
                        request.recycler_ring.ring_exponent,
                        &request.recycler_ring.ring_members,
                        RECYCLER_ALIAS_CONTEXT,
                        &alias_message,
                        self.crypto.as_ref(),
                    )
                    .map_err(RingProofError::Proof)?,
            );
        }

        let mut people_payload = alias_proofs.encode();
        people_payload.extend_from_slice(&request.inherited_implication);
        let people_message = blake2b_256(&people_payload);
        let people_proof = self
            .people
            .ring_vrf_proof(
                request.person_origin,
                &request.people_ring,
                &request.token.context(),
                &people_message,
            )
            .map_err(RingProofError::Proof)?;
        if alias_proofs
            .iter()
            .any(|proof| proof.len() != RING_VRF_PROOF_LEN)
            || people_proof.len() != RING_VRF_PROOF_LEN
        {
            return Err(RingProofError::Proof(
                "Bandersnatch proof has an invalid length".into(),
            ));
        }

        debug_assert!(
            alias_proofs
                .iter()
                .all(|proof| proof.len() == RING_VRF_PROOF_LEN)
        );
        debug_assert_eq!(people_proof.len(), RING_VRF_PROOF_LEN);
        Ok(UnloadTokenProof {
            person_origin: request.person_origin,
            people_proof,
            people_ring_index: request.people_ring.ring_index,
            people_ring_revision: request.people_ring.ring_revision,
            period: request.token.period,
            counter: request.token.counter,
            aliases,
            alias_proofs,
        })
    }
}

fn blake2b_256(message: &[u8]) -> [u8; 32] {
    blake2b_simd::Params::new()
        .hash_length(32)
        .hash(message)
        .as_bytes()
        .try_into()
        .expect("BLAKE2b-256 returns 32 bytes")
}

/// Authority-owned People signer. The adapter chooses the deployed full/lite
/// identity derivation for its network; Coinage never exports that identity.
pub trait PersonRingProofSigner: Send + Sync {
    /// Sign the exact ring/context/implication supplied by the unload builder.
    fn ring_vrf_proof(
        &self,
        origin: PersonOriginKind,
        ring: &RingProofParams,
        context: &[u8],
        message: &[u8],
    ) -> Result<Vec<u8>, String>;
}
