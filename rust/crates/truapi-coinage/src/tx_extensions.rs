// SPDX-License-Identifier: AGPL-3.0-only
// Derived from paritytech/brevity-dozer, core/crates/brevity-chain/src/tx_extensions.rs.
// Copyright the Brevity contributors. See NOTICE and LICENSE in this crate.

//! Exact SCALE encoding of Coinage transaction-origin extensions.
use parity_scale_codec::{Decode, Encode};

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct AsCoinage(pub Option<AsCoinageInfo>);

impl AsCoinage {
    pub const IDENTIFIER: &'static str = "AsCoinage";

    pub const fn as_coin() -> Self {
        Self(Some(AsCoinageInfo::AsCoin))
    }

    pub const fn infallible_unpaid_signed(nonce: u32) -> Self {
        Self(Some(AsCoinageInfo::InfallibleUnpaidSigned(nonce)))
    }

    pub fn unload_token_people(
        proof: CoinagePeopleProof,
        period: u32,
        counter: u32,
        alias_proofs: Vec<Vec<u8>>,
    ) -> Self {
        Self(Some(AsCoinageInfo::AsUnloadTokenPeople {
            proof,
            period,
            counter,
            alias_proofs,
        }))
    }

    pub fn unload_token_lite_people(
        proof: CoinagePeopleProof,
        period: u32,
        counter: u32,
        alias_proofs: Vec<Vec<u8>>,
    ) -> Self {
        Self(Some(AsCoinageInfo::AsUnloadTokenLitePeople {
            proof,
            period,
            counter,
            alias_proofs,
        }))
    }
}

/// The People-ring half of the Coinage unload-token proof
/// (`indiv_pallet_people::types::MembershipProof`). Live metadata declares
/// the proof as a bounded byte vector; `revision` (2026-08 wipe, spec
/// 1000032) is the ring revision the proof was built against.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct CoinagePeopleProof {
    pub proof: Vec<u8>,
    pub ring: u32,
    pub revision: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum AsCoinageInfo {
    /// Dispatch as the sr25519 coin which signed the extrinsic.
    #[codec(index = 0)]
    AsCoin,
    /// Full-person free unload token.
    #[codec(index = 1)]
    AsUnloadTokenPeople {
        proof: CoinagePeopleProof,
        period: u32,
        counter: u32,
        alias_proofs: Vec<Vec<u8>>,
    },
    /// Lite-person free unload token.
    #[codec(index = 2)]
    AsUnloadTokenLitePeople {
        proof: CoinagePeopleProof,
        period: u32,
        counter: u32,
        alias_proofs: Vec<Vec<u8>>,
    },
    /// Paid unload-token ring proof.
    #[codec(index = 3)]
    AsUnloadTokenPaid {
        proof: Vec<u8>,
        period: u32,
        paid_token_ring_index: u32,
        paid_token_ring_revision: u32,
        alias_proofs: Vec<Vec<u8>>,
    },
    /// A fee-recycler output used as the unload token. `retry_counter`
    /// joined in the 2026-08 runtime (spec 1000032).
    #[codec(index = 4)]
    AsUnloadTokenFromOutput {
        fee_recycler_value: i8,
        fee_recycler_index: u32,
        fee_recycler_revision: u32,
        retry_counter: u8,
        alias_proofs: Vec<Vec<u8>>,
    },
    #[codec(index = 5)]
    InfallibleUnpaidSigned(u32),
}

#[cfg(test)]
mod tests {
    use super::*;
    /// `AsCoinage(None)` is the metadata default. Coin-origin calls opt in
    /// with `Some(AsCoin)`: option tag 1 followed by enum variant 0.
    #[test]
    fn as_coinage_as_coin_encoding_is_pinned() {
        assert_eq!(AsCoinage(None).encode(), vec![0]);
        assert_eq!(AsCoinage::as_coin().encode(), vec![1, 0]);
        assert_eq!(
            AsCoinage::decode(&mut AsCoinage::as_coin().encode().as_slice()).unwrap(),
            AsCoinage::as_coin()
        );
    }

    #[test]
    fn as_coinage_infallible_unpaid_signed_encoding_is_pinned() {
        assert_eq!(
            AsCoinage::infallible_unpaid_signed(7).encode(),
            vec![1, 5, 7, 0, 0, 0]
        );
        assert_eq!(
            AsCoinage::decode(&mut AsCoinage::infallible_unpaid_signed(7).encode().as_slice())
                .unwrap(),
            AsCoinage::infallible_unpaid_signed(7)
        );
    }

    #[test]
    fn as_coinage_people_unload_encoding_is_pinned() {
        let extension = AsCoinage::unload_token_people(
            CoinagePeopleProof {
                proof: vec![0xAA, 0xBB],
                ring: 7,
                revision: 3,
            },
            9,
            2,
            vec![vec![0x11], vec![0x22, 0x33]],
        );
        // Option::Some, variant 1, compact proof len, proof, ring, revision,
        // period, counter, compact alias-proof count and one compact length
        // each.
        assert_eq!(
            extension.encode(),
            vec![
                1, 1, 8, 0xAA, 0xBB, 7, 0, 0, 0, 3, 0, 0, 0, 9, 0, 0, 0, 2, 0, 0, 0, 8, 4, 0x11, 8,
                0x22, 0x33,
            ]
        );
        assert_eq!(
            AsCoinage::decode(&mut extension.encode().as_slice()).unwrap(),
            extension
        );
    }

    /// `retry_counter` sits between the recycler revision and the alias
    /// proofs (2026-08 runtime).
    #[test]
    fn as_coinage_from_output_encoding_is_pinned() {
        let extension = AsCoinage(Some(AsCoinageInfo::AsUnloadTokenFromOutput {
            fee_recycler_value: -2,
            fee_recycler_index: 5,
            fee_recycler_revision: 9,
            retry_counter: 4,
            alias_proofs: vec![vec![0x77]],
        }));
        assert_eq!(
            extension.encode(),
            vec![1, 4, 0xFE, 5, 0, 0, 0, 9, 0, 0, 0, 4, 4, 4, 0x77]
        );
        assert_eq!(
            AsCoinage::decode(&mut extension.encode().as_slice()).unwrap(),
            extension
        );
    }
}
