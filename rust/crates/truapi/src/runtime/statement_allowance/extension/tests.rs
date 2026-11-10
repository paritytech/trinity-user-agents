use super::*;

/// Fixture metadata captured from paseo-next-v2 (raw `RuntimeMetadataPrefixed`).
const FIXTURE: &[u8] = include_bytes!("../../../../tests/fixtures/paseo-next-v2-metadata.scale");

/// V16 metadata captured from paseo-next-v2 (spec 3000000), the version the
/// runtime API serves. Distinct from `FIXTURE`, which is the V14 the legacy
/// RPC answers with and predates the `revision` field.
const FIXTURE_V16: &[u8] =
	include_bytes!("../../../../tests/fixtures/paseo-next-v2-metadata-v16.scale");

/// Preferring V16 makes this decode path load-bearing, so cover it: it has to
/// yield a usable `Metadata`, not merely decode.
#[test]
fn v16_metadata_decodes_into_a_usable_metadata() {
	let metadata = Metadata::decode(FIXTURE_V16).unwrap();

	assert_eq!(metadata.metadata_version(), 16);
	// Resolved from the version map rather than assumed.
	assert_eq!(metadata.extension_version(), 0);
	// The extension pipeline is populated and carries the one we authorize with.
	assert!(metadata.extension_index(AS_RESOURCES).is_some());
}

/// Mainnet keeps more than one transaction-extension pipeline, and a
/// transaction has to declare the one it was encoded for. Encoding for the
/// newest matches Subxt, so both builders in this crate agree.
#[test]
fn the_encoding_version_is_the_highest_the_runtime_declares() {
	assert_eq!(
		encoding_extension_version([].iter()),
		0,
		"metadata with no version map has only pipeline 0"
	);
	assert_eq!(encoding_extension_version([0u8].iter()), 0);
	assert_eq!(
		encoding_extension_version([0u8, 1].iter()),
		1,
		"two pipelines: encode for the newer"
	);
	assert_eq!(encoding_extension_version([2u8, 0, 1].iter()), 2);
}

/// Constants are encoded at their declared width, so a `u8` count has to read
/// as that count rather than failing. A constant the pallet does not declare is
/// still an error.
#[test]
fn constants_read_as_u32_whatever_width_they_declare() {
	let metadata = Metadata::decode(FIXTURE).unwrap();

	assert_eq!(
		metadata.constant_u32("Resources", "LiteStmtStoreSlotsPerPeriod").unwrap(),
		10,
		"a u32 constant"
	);
	assert_eq!(
		metadata.constant_u32("Resources", "LongTermStorageClaimsPerPeriod").unwrap(),
		10,
		"a u8 constant zero-extends rather than erroring"
	);
	assert!(metadata.constant_u32("Resources", "NoSuchConstant").is_err());
}

/// A pipeline may list a subset, in its own order, so the map decides both
/// which extensions are encoded and in what order. Declaration order is only
/// the fallback for metadata that declares no pipeline at all.
#[test]
fn a_pipeline_selects_its_own_extensions_in_its_own_order() {
	let mut by_version = std::collections::BTreeMap::new();
	by_version.insert(0u8, vec![Compact(0u32), Compact(1), Compact(2)]);
	by_version.insert(1u8, vec![Compact(2u32), Compact(0)]);

	assert_eq!(
		encoding_extension_indexes(&by_version, 1, 3),
		vec![2, 0],
		"the newer pipeline's own list, in its own order"
	);
	assert_eq!(encoding_extension_indexes(&by_version, 0, 3), vec![0, 1, 2]);
	assert_eq!(
		encoding_extension_indexes(&std::collections::BTreeMap::new(), 0, 3),
		vec![0, 1, 2],
		"no map declared: every extension, in declaration order"
	);
	assert_eq!(
		encoding_extension_indexes(&by_version, 7, 3),
		vec![0, 1, 2],
		"an undeclared pipeline falls back rather than encoding nothing"
	);
}

/// The fixture is V14, which carries no version map, so it must resolve to 0.
/// The frozen proof-message answer below depends on this.
#[test]
fn pre_v16_metadata_resolves_to_pipeline_zero() {
	let metadata = Metadata::decode(FIXTURE).unwrap();

	assert_eq!(metadata.extension_version(), 0);
	assert_eq!(metadata.metadata_version(), 14);
}

#[test]
fn as_resources_tail_is_indices_10_through_20() {
	let metadata = Metadata::decode(FIXTURE).unwrap();
	let idx = metadata.as_resources_index().unwrap();
	// AsResources sits at index 9; the proof tail is everything after it.
	assert_eq!(idx, 9);
	let ids = metadata.extension_ids();
	assert_eq!(
		ids[idx + 1..].to_vec(),
		vec![
			"AuthorizeCall",
			"RestrictOrigins",
			"CheckNonZeroSender",
			"CheckSpecVersion",
			"CheckTxVersion",
			"CheckGenesis",
			"CheckMortality",
			"CheckNonce",
			"CheckWeight",
			"ChargeAssetTxPayment",
			"StorageWeightReclaim",
		],
	);
}

#[test]
fn call_indices_resolve_by_name() {
	let metadata = Metadata::decode(FIXTURE).unwrap();

	assert_eq!(
		metadata.call_indices("Resources", "set_statement_store_account").unwrap(),
		[0x3f, 0x0a]
	);
	assert_eq!(
		metadata.call_indices("Resources", "claim_long_term_storage").unwrap(),
		[0x3f, 0x0c]
	);
}

#[test]
fn index_resolution_fails_for_unknown_names() {
	let metadata = Metadata::decode(FIXTURE).unwrap();

	assert!(metadata.call_indices("Resources", "no_such_call").is_err());
	assert!(metadata.call_indices("NoSuchPallet", "transfer").is_err());
}

/// A dispatch error is matched by name, so the indices come from the
/// runtime: the pallet's index and the error's variant index.
#[test]
fn error_indices_resolve_by_name() {
	let metadata = super::super::test_fixtures::asset_hub();
	let [revive_index, _] = metadata.call_indices("Revive", "call").unwrap();
	assert_eq!(
		metadata.error_indices("Revive", "StorageDepositNotEnoughFunds").unwrap(),
		[revive_index, 0x17]
	);
	assert!(metadata.error_indices("Revive", "NoSuchError").is_err());
	assert!(metadata.error_indices("NoSuchPallet", "StorageDepositNotEnoughFunds").is_err());
}
