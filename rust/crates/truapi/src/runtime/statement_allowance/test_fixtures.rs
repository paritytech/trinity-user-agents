//! Captured chain artefacts shared by the allowance unit tests: Asset Hub
//! metadata and one storage value read against it.

use std::sync::LazyLock;

use super::{extension::Metadata, view_cache::MetadataViewCache};

/// `MembersSubscriber.RingRoots[(LitePeople, 5)]` captured alongside the
/// metadata, at block
/// `0xf25d4e330ade1ce230695976f019df50cdaf97c96b6996838af93b68550654f3`.
///
/// Ring 5 is the one whose window skips a revision, holding `[105, 106, 108]`
/// where every other lite-people ring is contiguous.
pub const ASSET_HUB_RING_5_ROOTS: &[u8] =
	include_bytes!("../../../tests/fixtures/paseo-next-asset-hub-ring-5-roots.scale");

/// Asset Hub metadata captured from paseo Asset Hub Next at spec 3000000.
///
/// The only fixture declaring `AsPgas`, `Pgas` and `MembersSubscriber`, so it is
/// also the only one that can exercise the ring-root generation.
static ASSET_HUB: LazyLock<Metadata> = LazyLock::new(|| {
	Metadata::decode(include_bytes!("../../../tests/fixtures/paseo-next-asset-hub-metadata.scale"))
		.expect("the committed Asset Hub fixture decodes")
});

/// Borrow the decoded Asset Hub fixture.
pub fn asset_hub() -> &'static Metadata {
	&ASSET_HUB
}

/// The `Resources` budgets the allowance tests are written against, keyed by
/// the view function that serves each one.
///
/// Priming them keeps a test's scripted RPC list about the slot table it is
/// exercising: without this every scan would also have to script the budget
/// view calls, so adding one read would renumber every later response.
const PEOPLE_RESOURCE_BUDGETS: [(&str, u32); 5] = [
	("get_stmt_store_slots_per_period", 20),
	("get_lite_stmt_store_slots_per_period", 10),
	("get_stmt_store_replacement_cooldown", 60),
	("get_stmt_store_grace_window", 3600),
	("get_long_term_storage_claims_per_period", 10),
];

/// People-chain V16 metadata with [`PEOPLE_RESOURCE_BUDGETS`] already resolved.
static PEOPLE: LazyLock<Metadata> = LazyLock::new(|| {
	let metadata = Metadata::decode(include_bytes!(
		"../../../tests/fixtures/paseo-next-v2-metadata-v16.scale"
	))
	.expect("the committed People fixture decodes");
	for (function, value) in PEOPLE_RESOURCE_BUDGETS {
		let definition = metadata
			.view_function("Resources", function)
			.unwrap_or_else(|| panic!("the People fixture declares Resources.{function}"));
		metadata.cache_view_u32(definition.id, value);
	}
	metadata
});

/// Borrow the decoded People fixture.
pub fn people() -> &'static Metadata {
	&PEOPLE
}

#[cfg(test)]
mod tests {
	use super::*;

	/// A truncated or mis-versioned capture would otherwise surface as a
	/// confusing decode failure inside an unrelated test.
	///
	/// The roots length is pinned rather than checked for emptiness, so a
	/// re-capture has to re-confirm the record arithmetic.
	#[test]
	fn the_captured_asset_hub_artefacts_load() {
		assert_eq!(asset_hub().metadata_version(), 16);
		assert_eq!(
			ASSET_HUB_RING_5_ROOTS.len(),
			925,
			"3 records of 308 bytes plus a compact length prefix"
		);
	}
}
