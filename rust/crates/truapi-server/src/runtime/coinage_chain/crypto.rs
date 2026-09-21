// SPDX-License-Identifier: AGPL-3.0-only
// Derived from paritytech/brevity-dozer core/crates/brevity-core/src/personhood_keys.rs.
// Copyright the Brevity contributors. See truapi-coinage/NOTICE and LICENSE.

use crate::host_logic::product_account::{
    derive_full_person_ring_vrf_entropy, derive_lite_person_ring_vrf_entropy,
};
use crate::runtime::statement_allowance::proof;
use std::sync::Arc;
use truapi_coinage::{
    PersonOriginKind, PersonRingProofSigner, RingProofParams, VoucherCryptography, VoucherSeed,
};
use verifiable::{GenerateVerifiable, ring::bandersnatch::BandersnatchVrfVerifiable};
use zeroize::Zeroizing;

/// Session-checked concrete Bandersnatch primitives; no secrets escape this adapter.
pub(crate) struct HostVoucherCryptography {
    valid: Arc<dyn Fn() -> bool + Send + Sync>,
}

impl HostVoucherCryptography {
    /// Bind proof generation to the wallet session that owns the inputs.
    pub(crate) fn new(valid: Arc<dyn Fn() -> bool + Send + Sync>) -> Self {
        Self { valid }
    }
    fn check(&self) -> Result<(), String> {
        if (self.valid)() {
            Ok(())
        } else {
            Err("Coinage signing session expired".into())
        }
    }
}

impl VoucherCryptography for HostVoucherCryptography {
    fn member_key(&self, seed: &VoucherSeed) -> Result<[u8; 32], String> {
        self.check()?;
        Ok(proof::member_key(seed.0))
    }
    fn sign(&self, seed: &VoucherSeed, message: &[u8]) -> Result<[u8; 64], String> {
        self.check()?;
        let secret = BandersnatchVrfVerifiable::new_secret(seed.0);
        BandersnatchVrfVerifiable::sign(&secret, message)
            .map_err(|_| "Coinage ownership proof failed".into())
    }
    fn alias(&self, seed: &VoucherSeed, context: &[u8]) -> Result<[u8; 32], String> {
        self.check()?;
        let secret = BandersnatchVrfVerifiable::new_secret(seed.0);
        BandersnatchVrfVerifiable::alias_in_context(&secret, context)
            .map_err(|_| "Coinage alias derivation failed".into())
    }
    fn ring_vrf_proof(
        &self,
        seed: &VoucherSeed,
        ring_exponent: u8,
        ring_members: &[[u8; 32]],
        context: &[u8],
        message: &[u8],
    ) -> Result<Vec<u8>, String> {
        self.check()?;
        let domain = proof::domain_for_ring_exponent(ring_exponent)
            .map_err(|_| "Coinage ring exponent is unsupported")?;
        proof::ring_vrf_proof(domain, seed.0, ring_members, context, message)
            .map_err(|_| "Coinage ring proof failed".into())
    }
}

pub(super) struct HostPersonProof {
    full: Zeroizing<[u8; 32]>,
    lite: Zeroizing<[u8; 32]>,
    valid: Arc<dyn Fn() -> bool + Send + Sync>,
}

impl HostPersonProof {
    pub fn new(
        entropy: &[u8],
        suffix: &str,
        valid: Arc<dyn Fn() -> bool + Send + Sync>,
    ) -> Result<Self, String> {
        if !valid() {
            return Err("Coinage signing session expired".into());
        }
        if !matches!(suffix, "dot" | "paseo" | "testnet") {
            return Err("unsupported Coinage personhood network suffix".into());
        }
        Ok(Self {
            full: Zeroizing::new(derive_full_person_ring_vrf_entropy(entropy, suffix)),
            lite: Zeroizing::new(derive_lite_person_ring_vrf_entropy(entropy, suffix)),
            valid,
        })
    }
    fn seed(&self, origin: PersonOriginKind) -> Result<VoucherSeed, String> {
        if !(self.valid)() {
            return Err("Coinage signing session expired".into());
        }
        Ok(VoucherSeed(match origin {
            PersonOriginKind::Full => *self.full,
            PersonOriginKind::Lite => *self.lite,
        }))
    }
    pub fn member(&self, origin: PersonOriginKind) -> Result<[u8; 32], String> {
        HostVoucherCryptography::new(self.valid.clone()).member_key(&self.seed(origin)?)
    }
    pub fn alias(&self, origin: PersonOriginKind, context: &[u8]) -> Result<[u8; 32], String> {
        HostVoucherCryptography::new(self.valid.clone()).alias(&self.seed(origin)?, context)
    }
}

impl PersonRingProofSigner for HostPersonProof {
    fn ring_vrf_proof(
        &self,
        origin: PersonOriginKind,
        ring: &RingProofParams,
        context: &[u8],
        message: &[u8],
    ) -> Result<Vec<u8>, String> {
        HostVoucherCryptography::new(self.valid.clone()).ring_vrf_proof(
            &self.seed(origin)?,
            ring.ring_exponent,
            &ring.ring_members,
            context,
            message,
        )
    }
}
