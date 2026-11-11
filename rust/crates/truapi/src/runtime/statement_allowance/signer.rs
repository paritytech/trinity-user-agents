//! Synchronous personhood operations for allowance preparation.

use super::{
	CollectionCandidate, StatementAllowanceError,
	collection::PersonhoodCollection,
	proof::{domain_for_ring_exponent, ring_vrf_proof, vrf_error},
	ring::RingParams,
	slot::SlotError,
};
use crate::runtime::vrf::{self, Vrf};

/// Keep personhood secret use synchronous with its owner's session validation.
///
/// A key ordinal is local to its collection and stable for this signer's
/// lifetime. Keep the same signer for discovery, scans and submission: selecting
/// a ring never changes which key another operation uses.
pub trait PersonhoodSigner: Send + Sync {
	/// Number of candidate keys for a collection, in primary-first order.
	fn key_count(&self, collection: PersonhoodCollection) -> usize;
	/// The public member used to discover a particular key's rings.
	fn member(
		&self,
		collection: PersonhoodCollection,
		key_index: usize,
	) -> Result<[u8; 32], StatementAllowanceError>;
	/// The public alias for a specific key's collection-scoped resource context.
	fn alias(
		&self,
		collection: PersonhoodCollection,
		key_index: usize,
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

/// Personhood keys the caller holds directly, with no wallet session to check.
pub struct FixedPersonhoodSigner<'a> {
	candidates: &'a [CollectionCandidate],
	vrf: Vrf,
}

impl<'a> FixedPersonhoodSigner<'a> {
	/// Load the prover before using the supplied fixed keys.
	pub async fn new(
		candidates: &'a [CollectionCandidate],
	) -> Result<Self, StatementAllowanceError> {
		Ok(Self { candidates, vrf: vrf::load().await.map_err(vrf_error)? })
	}

	fn entropy(
		&self,
		collection: PersonhoodCollection,
		key_index: usize,
	) -> Result<&[u8; 32], StatementAllowanceError> {
		self.candidates
			.iter()
			.filter(|candidate| candidate.collection == collection)
			.nth(key_index)
			.map(|candidate| &candidate.entropy)
			.ok_or_else(|| SlotError::NoCollectionMembership.into())
	}
}

impl PersonhoodSigner for FixedPersonhoodSigner<'_> {
	fn key_count(&self, collection: PersonhoodCollection) -> usize {
		self.candidates
			.iter()
			.filter(|candidate| candidate.collection == collection)
			.count()
	}

	fn member(
		&self,
		collection: PersonhoodCollection,
		key_index: usize,
	) -> Result<[u8; 32], StatementAllowanceError> {
		Ok(self.vrf.member(self.entropy(collection, key_index)?).map_err(vrf_error)?)
	}

	fn alias(
		&self,
		collection: PersonhoodCollection,
		key_index: usize,
		context: &[u8],
	) -> Result<[u8; 32], StatementAllowanceError> {
		Ok(self
			.vrf
			.alias(self.entropy(collection, key_index)?, context)
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
			self.entropy(ring.collection, ring.key_index)?,
			&ring.members,
			context,
			message,
		)
	}
}
