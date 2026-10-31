//! Personhood ring collections on the People chain.
//!
//! A person can hold membership in more than one collection, and each one is a
//! separate alias space with its own slot budget: aliases are derived from
//! collection-specific entropy, so the same `(period, seq)` in two collections
//! is two distinct storage entries. Capacity is therefore the sum over the
//! collections a device can prove membership in, which is why the allowance
//! path takes a collection rather than assuming one.

use subxt::ext::scale_encode::EncodeAsType;

use super::{StatementAllowanceError, extension::Metadata, rpc::RpcClient, view};

/// A personhood ring collection in the People chain's `Members` pallet. It
/// encodes by variant name as the runtime's `MembershipCollection` and
/// `PgasCollection`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, derive_more::Display, EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
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

	/// The `Resources` view function returning the highest notification seq a
	/// person of this collection may claim in a period.
	pub fn notification_slots_view(self) -> &'static str {
		match self {
			Self::People => "get_notification_slots_per_period",
			Self::LitePeople => "get_lite_notification_slots_per_period",
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
mod tests;
