use subxt::{
	Metadata as SubxtMetadata,
	ext::{frame_decode::extrinsics::encode_call_data, scale_encode::EncodeAsFields},
};

use super::{super::test_fixtures, *};

fn encode_typed(
	metadata_bytes: &[u8],
	pallet: &str,
	call: &str,
	args: &impl EncodeAsFields,
) -> Vec<u8> {
	let metadata = SubxtMetadata::decode_from(metadata_bytes).unwrap();
	encode_call_data(pallet, call, args, &metadata, metadata.types()).unwrap()
}

#[test]
fn the_statement_store_call_has_the_reference_bytes() {
	let args = SetStatementStoreAccountCallArgs { period: 7, seq: 3, target_account: [0x22; 32] };
	let expected =
		"3f0a07000000030000002222222222222222222222222222222222222222222222222222222222222222";

	let encoded = encode_typed(
		test_fixtures::PEOPLE_METADATA,
		"Resources",
		"set_statement_store_account",
		&args,
	);

	assert_eq!(hex::encode(encoded), expected);
}

#[test]
fn the_long_term_storage_call_has_the_reference_bytes() {
	let args = ClaimLongTermStorageCallArgs { period: 9, counter: 2, account_id: [0x33; 32] };
	let expected = "3f0c09000000023333333333333333333333333333333333333333333333333333333333333333";

	let encoded =
		encode_typed(test_fixtures::PEOPLE_METADATA, "Resources", "claim_long_term_storage", &args);

	assert_eq!(hex::encode(encoded), expected);
}

#[test]
fn the_pgas_call_has_the_reference_bytes() {
	let args = ClaimPgasCallArgs { slot_index: 4, target: [0x44; 32] };
	let expected = "6300040000004444444444444444444444444444444444444444444444444444444444444444";

	let encoded = encode_typed(test_fixtures::ASSET_HUB_METADATA, "Pgas", "claim_pgas", &args);

	assert_eq!(hex::encode(encoded), expected);
}
