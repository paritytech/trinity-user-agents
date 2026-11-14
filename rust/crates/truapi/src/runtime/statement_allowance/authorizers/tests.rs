use parity_scale_codec::Encode;
use subxt::{Metadata, ext::frame_decode::extrinsics::ExtrinsicTypeInfo};

use super::*;
use crate::runtime::statement_allowance::test_fixtures::{ASSET_HUB_METADATA, PEOPLE_METADATA};

/// Encodes `value` as the explicit value of `extension` in `metadata`'s
/// pipeline 0.
fn encode_explicit(metadata_bytes: &[u8], extension: &str, value: impl EncodeAsType) -> Vec<u8> {
	let metadata = Metadata::decode_from(metadata_bytes).unwrap();
	let pipeline = metadata.extrinsic_extension_info(Some(0)).unwrap();
	let slot = pipeline
		.extension_ids
		.iter()
		.find(|declared| declared.name == extension)
		.unwrap();
	value.encode_as_type(slot.id, metadata.types()).unwrap()
}

/// `Some ‖ variant ‖ proof ‖ ring index ‖ revision ‖ LitePeople`, the
/// positional layout People decodes.
fn as_resources_bytes(variant: u8) -> Vec<u8> {
	[vec![0x01, variant], vec![0xEEu8; 4].encode(), 3u32.encode(), 9u32.encode(), vec![0x01]]
		.concat()
}

#[test]
fn a_statement_store_allowance_encodes_as_people_expects() {
	let info = AsResourcesInfo::RegisterStatementStoreAllowance(
		vec![0xEE; 4],
		3,
		9,
		PersonhoodCollection::LitePeople,
	);

	let encoded = encode_explicit(PEOPLE_METADATA, "AsResources", Some(info));

	assert_eq!(encoded, as_resources_bytes(2));
}

#[test]
fn a_long_term_storage_claim_encodes_as_people_expects() {
	let info = AsResourcesInfo::ClaimLongTermStorage(
		vec![0xEE; 4],
		3,
		9,
		PersonhoodCollection::LitePeople,
	);

	let encoded = encode_explicit(PEOPLE_METADATA, "AsResources", Some(info));

	assert_eq!(encoded, as_resources_bytes(3));
}

#[test]
fn a_pgas_claim_encodes_as_asset_hub_expects() {
	let info = AsPgasInfo::Claim {
		proof: vec![0xEE; 4],
		ring_index: 3,
		revision: 9,
		collection: PersonhoodCollection::People,
		day: 11,
	};

	let encoded = encode_explicit(ASSET_HUB_METADATA, "AsPgas", Some(info));

	let expected = [
		vec![0x01, 0x00],
		vec![0xEEu8; 4].encode(),
		3u32.encode(),
		9u32.encode(),
		vec![0x00],
		11u32.encode(),
	]
	.concat();
	assert_eq!(encoded, expected);
}
