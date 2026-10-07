//! Synchronous personhood operations for allowance preparation.

use super::collection::PersonhoodCollection;
use super::proof::{domain_for_ring_exponent, ring_vrf_proof, vrf_error};
use super::ring::RingParams;
use super::slot::SlotError;
use super::{CollectionCandidate, StatementAllowanceError};
use crate::runtime::vrf::{self, Vrf};

/// Keep personhood secret use synchronous with its owner's session validation.
pub trait PersonhoodSigner: Send + Sync {
    /// The public member used to discover this collection's rings.
    fn member(&self, collection: PersonhoodCollection)
    -> Result<[u8; 32], StatementAllowanceError>;
    /// The public alias for a collection-specific resource context.
    fn alias(
        &self,
        collection: PersonhoodCollection,
        context: &[u8],
    ) -> Result<[u8; 32], StatementAllowanceError>;
    /// Prove membership in the selected ring for one resource operation.
    fn prove(
        &self,
        ring: &RingParams,
        context: &[u8],
        message: &[u8],
    ) -> Result<Vec<u8>, StatementAllowanceError>;
}

/// Personhood keys selected explicitly by standalone CLI operations.
pub struct FixedPersonhoodSigner<'a> {
    candidates: &'a [CollectionCandidate],
    vrf: Vrf,
}

impl<'a> FixedPersonhoodSigner<'a> {
    /// Load the prover before using the supplied fixed keys.
    pub async fn new(
        candidates: &'a [CollectionCandidate],
    ) -> Result<Self, StatementAllowanceError> {
        Ok(Self {
            candidates,
            vrf: vrf::load().await.map_err(vrf_error)?,
        })
    }

    fn entropy(
        &self,
        collection: PersonhoodCollection,
    ) -> Result<&[u8; 32], StatementAllowanceError> {
        self.candidates
            .iter()
            .find(|candidate| candidate.collection == collection)
            .map(|candidate| &candidate.entropy)
            .ok_or_else(|| SlotError::NoCollectionMembership.into())
    }
}

impl PersonhoodSigner for FixedPersonhoodSigner<'_> {
    fn member(
        &self,
        collection: PersonhoodCollection,
    ) -> Result<[u8; 32], StatementAllowanceError> {
        Ok(self
            .vrf
            .member(self.entropy(collection)?)
            .map_err(vrf_error)?)
    }

    fn alias(
        &self,
        collection: PersonhoodCollection,
        context: &[u8],
    ) -> Result<[u8; 32], StatementAllowanceError> {
        Ok(self
            .vrf
            .alias(self.entropy(collection)?, context)
            .map_err(vrf_error)?)
    }

    fn prove(
        &self,
        ring: &RingParams,
        context: &[u8],
        message: &[u8],
    ) -> Result<Vec<u8>, StatementAllowanceError> {
        ring_vrf_proof(
            &self.vrf,
            domain_for_ring_exponent(ring.exponent)?,
            self.entropy(ring.collection)?,
            &ring.members,
            context,
            message,
        )
    }
}
