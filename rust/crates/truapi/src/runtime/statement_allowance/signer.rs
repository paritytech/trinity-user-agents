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

/// A signer whose keys may be used only while `current` says so: every
/// member, alias and proof is refused once it answers false, as after the
/// wallet session that lent the keys ended. The check is synchronous with
/// each use, so a proof is never made after it.
pub struct CheckedPersonhoodSigner<'a> {
	inner: &'a dyn PersonhoodSigner,
	current: &'a (dyn Fn() -> bool + Send + Sync),
}

impl<'a> CheckedPersonhoodSigner<'a> {
	/// `inner`'s keys, while `current` answers true.
	pub fn new(
		inner: &'a dyn PersonhoodSigner,
		current: &'a (dyn Fn() -> bool + Send + Sync),
	) -> Self {
		Self { inner, current }
	}

	fn still_current(&self) -> Result<(), StatementAllowanceError> {
		if (self.current)() { Ok(()) } else { Err(StatementAllowanceError::SessionEnded) }
	}
}

impl PersonhoodSigner for CheckedPersonhoodSigner<'_> {
	fn member(
		&self,
		collection: PersonhoodCollection,
	) -> Result<[u8; 32], StatementAllowanceError> {
		self.still_current()?;
		self.inner.member(collection)
	}

	fn alias(
		&self,
		collection: PersonhoodCollection,
		context: &[u8],
	) -> Result<[u8; 32], StatementAllowanceError> {
		self.still_current()?;
		self.inner.alias(collection, context)
	}

	fn prove(
		&self,
		ring: &RingParams,
		context: &[u8],
		message: &[u8],
	) -> Result<Vec<u8>, StatementAllowanceError> {
		self.still_current()?;
		self.inner.prove(ring, context, message)
	}
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
		Ok(self.vrf.member(self.entropy(collection)?).map_err(vrf_error)?)
	}

	fn alias(
		&self,
		collection: PersonhoodCollection,
		context: &[u8],
	) -> Result<[u8; 32], StatementAllowanceError> {
		Ok(self.vrf.alias(self.entropy(collection)?, context).map_err(vrf_error)?)
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
