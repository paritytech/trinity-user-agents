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
