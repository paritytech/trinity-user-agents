//! Personhood ring collections on the People chain.
//!
//! A person can hold membership in more than one collection, and each one is a
//! separate alias space with its own slot budget: aliases are derived from
//! collection-specific entropy, so the same `(period, seq)` in two collections
//! is two distinct storage entries. Capacity is therefore the sum over the
//! collections a device can prove membership in, which is why the allowance
//! path takes a collection rather than assuming one.

use super::{StatementAllowanceError, extension::Metadata, rpc::RpcClient, view};

/// A personhood ring collection in the People chain's `Members` pallet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, derive_more::Display)]
#[display("{}", self.metadata_variant())]
pub enum PersonhoodCollection {
	/// Full personhood, established by proof-of-personhood registration.
	People,
	/// Light personhood, backed by the wallet account.
	LitePeople,
}

/// Full-personhood collection identifier: ASCII, space-padded to 32 bytes.
const PEOPLE_IDENTIFIER: &[u8; 32] = b"pop:polkadot.network/people     ";
/// Light-personhood collection identifier: ASCII, exactly 32 bytes.
const LITE_PEOPLE_IDENTIFIER: &[u8; 32] = b"pop:polkadot.network/people-lite";

impl PersonhoodCollection {
	/// Every collection the allowance path knows how to prove membership in,
	/// widest slot budget first so a caller that stops at the first success
	/// prefers the collection with the most capacity.
	pub const ALL: [Self; 2] = [Self::People, Self::LitePeople];

	/// The 32-byte collection identifier used as the first key in every
	/// `Members` storage map.
	pub fn identifier(self) -> &'static [u8; 32] {
		match self {
			Self::People => PEOPLE_IDENTIFIER,
			Self::LitePeople => LITE_PEOPLE_IDENTIFIER,
		}
	}

	/// The `MembershipCollection` / `PgasCollection` variant naming this
	/// collection inside a transaction extension.
	pub fn metadata_variant(self) -> &'static str {
		match self {
			Self::People => "People",
			Self::LitePeople => "LitePeople",
		}
	}

	/// The `Resources` view function returning StatementStore slots per period
	/// for this collection.
	pub fn slots_per_period_view(self) -> &'static str {
		match self {
			Self::People => "get_stmt_store_slots_per_period",
			Self::LitePeople => "get_lite_stmt_store_slots_per_period",
		}
	}

	/// The `Pgas` constant bounding claims per period for this collection.
	pub fn pgas_claims_per_period_constant(self) -> &'static str {
		match self {
			Self::People => "MaxClaimsPerPeriodPerPerson",
			Self::LitePeople => "MaxClaimsPerPeriodPerLitePerson",
		}
	}

	/// Whether this chain exposes a StatementStore slot budget for this collection.
	pub fn is_supported(self, metadata: &Metadata) -> bool {
		view::supports_resource_u32(metadata, self.slots_per_period_view())
	}

	/// Max StatementStore slots per period for this collection.
	pub async fn slots_per_period(
		self,
		rpc: &RpcClient,
		metadata: &Metadata,
	) -> Result<u32, StatementAllowanceError> {
		view::read_resource_u32(rpc, metadata, self.slots_per_period_view()).await
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn every_collection_identifier_is_distinct_and_exactly_32_bytes() {
		// The identifier is a fixed-width storage key, so a padding slip would
		// silently read a neighbouring collection rather than fail.
		for collection in PersonhoodCollection::ALL {
			assert_eq!(collection.identifier().len(), 32, "{collection}");
			assert!(collection.identifier().is_ascii(), "{collection} identifier is not ASCII");
		}
		assert_ne!(
			PersonhoodCollection::People.identifier(),
			PersonhoodCollection::LitePeople.identifier(),
		);
	}

	#[test]
	fn the_people_identifier_is_the_padded_form_the_wallet_uses() {
		assert_eq!(
			PersonhoodCollection::People.identifier().as_slice(),
			b"pop:polkadot.network/people     ".as_slice(),
		);
		assert_eq!(
			&PersonhoodCollection::People.identifier()[..27],
			b"pop:polkadot.network/people",
		);
		// Padding is spaces, not zeroes: a zero-padded key addresses nothing.
		assert!(PersonhoodCollection::People.identifier()[27..].iter().all(|byte| *byte == b' '));
	}

	#[test]
	fn people_is_offered_before_lite_people() {
		// Callers stop at the first collection that yields a slot, so ordering
		// is what makes a full person spend their wider budget first.
		assert_eq!(
			PersonhoodCollection::ALL,
			[PersonhoodCollection::People, PersonhoodCollection::LitePeople],
		);
	}

	#[test]
	fn each_collection_names_its_own_pgas_claim_constant() {
		// Asset Hub declares a separate claim budget per collection, so a full
		// person must not be scanned against the light one's share.
		assert_eq!(
			PersonhoodCollection::People.pgas_claims_per_period_constant(),
			"MaxClaimsPerPeriodPerPerson",
		);
		assert_eq!(
			PersonhoodCollection::LitePeople.pgas_claims_per_period_constant(),
			"MaxClaimsPerPeriodPerLitePerson",
		);
		assert_ne!(
			PersonhoodCollection::People.pgas_claims_per_period_constant(),
			PersonhoodCollection::LitePeople.pgas_claims_per_period_constant(),
		);
	}

	#[test]
	fn each_collection_names_its_own_variant_and_slot_view() {
		assert_eq!(PersonhoodCollection::People.metadata_variant(), "People");
		assert_eq!(PersonhoodCollection::LitePeople.metadata_variant(), "LitePeople");
		assert_eq!(
			PersonhoodCollection::People.slots_per_period_view(),
			"get_stmt_store_slots_per_period",
		);
		assert_eq!(
			PersonhoodCollection::LitePeople.slots_per_period_view(),
			"get_lite_stmt_store_slots_per_period",
		);
	}
}
