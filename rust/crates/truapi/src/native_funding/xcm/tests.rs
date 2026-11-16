//! The programs against getcash's own builders, byte for byte.
//!
//! Each expected hex is what getcash-community 06916ddd4's builder of the
//! same shape (`funding-program.ts`, `psm-batch.ts`) produces for the same
//! inputs, encoded by polkadot-api against Paseo Asset Hub Next metadata.

use subxt::{
	Metadata,
	ext::{frame_decode::extrinsics::encode_call_data, scale_encode::EncodeAsType},
};

use super::*;

/// Paseo Asset Hub Next at spec 3004000, which declares `Psm`.
const ASSET_HUB_METADATA: &[u8] =
	include_bytes!("../../../tests/fixtures/paseo-next-asset-hub-3004000-metadata.scale");

const BENEFICIARY: [u8; 32] = [0x11; 32];
const PEOPLE_PARA_ID: u32 = 1502;
const ASSET_HUB_PARA_ID: u32 = 1500;
const CASH_ASSET_ID: u32 = 50_000_413;
const USDT_ASSET_ID: u32 = 1984;
const USDC_ASSET_ID: u32 = 1337;
const MAX_WEIGHT: Weight = Weight { ref_time: 1_234_567_890, proof_size: 65_536 };

fn metadata() -> Metadata {
	Metadata::decode_from(ASSET_HUB_METADATA).expect("the fixture decodes")
}

fn leg(transfer: CashTransfer) -> PeopleLeg {
	PeopleLeg {
		people_para_id: PEOPLE_PARA_ID,
		cash_asset_id: CASH_ASSET_ID,
		transfer,
		beneficiary: BENEFICIARY,
		remote_fees_cash: 100_000,
	}
}

fn call(pallet: &str, name: &str, args: &impl scale_encode::EncodeAsFields) -> Vec<u8> {
	let metadata = metadata();
	encode_call_data(pallet, name, args, &metadata, metadata.types()).expect("the call encodes")
}

fn execute_hex(args: &ExecuteArgs) -> String {
	format!("0x{}", hex::encode(call("PolkadotXcm", "execute", args)))
}

#[test]
fn the_pool_program_matches_getcash_for_a_teleport() {
	let args = pool_program(
		&leg(CashTransfer::Teleport),
		123_456_789_012,
		1_234_567,
		9_900_000,
		MAX_WEIGHT,
	);

	assert_eq!(
		execute_hex(&args),
		"0x1f030510000401000007141a99be1c300100001e5a4b000f0004010000078d4386be1c04000204320576c8eb0b00823f5c020131010100791701000004000204320576c8eb0b00821a06000004000101000204320576c8eb0b000c33040000010101021111111111111111111111111111111111111111111111111111111111111111140d01020400010100111111111111111111111111111111111111111111111111111111111111111103d202964902000400"
	);
}

#[test]
fn the_pool_program_matches_getcash_for_a_reserve_transfer() {
	let args =
		pool_program(&leg(CashTransfer::Reserve), 123_456_789_012, 1_234_567, 9_900_000, MAX_WEIGHT);

	assert_eq!(
		execute_hex(&args),
		"0x1f030510000401000007141a99be1c300100001e5a4b000f0004010000078d4386be1c04000204320576c8eb0b00823f5c020131010100791701010004000204320576c8eb0b00821a06000004010101000204320576c8eb0b000c33040000010101021111111111111111111111111111111111111111111111111111111111111111140d01020400010100111111111111111111111111111111111111111111111111111111111111111103d202964902000400"
	);
}

#[test]
fn the_psm_program_matches_getcash_with_its_assets_sorted() {
	let fees = Asset::fungible(Location::asset_hub_asset(USDT_ASSET_ID), 31_000);
	let args = psm_program(&leg(CashTransfer::Teleport), 9_950_000, fees, MAX_WEIGHT);

	assert_eq!(
		execute_hex(&args),
		"0x1f03051400080002043205011f0062e40100000204320576c8eb0b00c24c5f02300002043205011f0062e4010031010100791701000004000204320576c8eb0b00821a06000004000101000204320576c8eb0b000c33040000010101021111111111111111111111111111111111111111111111111111111111111111140d010204000101001111111111111111111111111111111111111111111111111111111111111111140d01020400010100111111111111111111111111111111111111111111111111111111111111111103d202964902000400"
	);
}

#[test]
fn the_psm_batch_matches_getcash() {
	let fees = Asset::fungible(Location::asset_hub_asset(USDT_ASSET_ID), 31_000);
	let execute = psm_program(&leg(CashTransfer::Teleport), 9_950_000, fees, MAX_WEIGHT);
	let mint = PsmMintArgs {
		internal_asset: Location::asset_hub_asset(CASH_ASSET_ID),
		external_asset: Location::asset_hub_asset(USDT_ASSET_ID),
		external_amount: 10_050_000,
		max_fee: 5_000,
	};
	let batch = BatchAllArgs {
		calls: vec![
			EncodedCall(call("Psm", "mint", &mint)),
			EncodedCall(call("PolkadotXcm", "execute", &execute)),
		],
	};

	assert_eq!(
		format!("0x{}", hex::encode(call("Utility", "batch_all", &batch))),
		"0x2802083b00000204320576c8eb0b0002043205011fd0599900000000000000000000000000881300001f03051400080002043205011f0062e40100000204320576c8eb0b00c24c5f02300002043205011f0062e4010031010100791701000004000204320576c8eb0b00821a06000004000101000204320576c8eb0b000c33040000010101021111111111111111111111111111111111111111111111111111111111111111140d010204000101001111111111111111111111111111111111111111111111111111111111111111140d01020400010100111111111111111111111111111111111111111111111111111111111111111103d202964902000400"
	);
}

#[test]
fn the_stable_pool_program_matches_getcash() {
	let args = stable_pool_program(
		&leg(CashTransfer::Teleport),
		Location::asset_hub_asset(USDC_ASSET_ID),
		10_500_000,
		40_000,
		30_000_000_000,
		10_000_000,
		MAX_WEIGHT,
	);

	assert_eq!(
		execute_hex(&args),
		"0x1f03051c00040002043205e5140082de8002300002043205e51400027102000f00040002043205e51400826d7e02040100000700ac23fc06010f010101000004000204320576c8eb0b00025a62020131010100791701000004000204320576c8eb0b00821a06000004000101000204320576c8eb0b000c33040000010101021111111111111111111111111111111111111111111111111111111111111111140d010204000101001111111111111111111111111111111111111111111111111111111111111111140d01020400010100111111111111111111111111111111111111111111111111111111111111111103d202964902000400"
	);
}

#[test]
fn the_dot_usd_program_matches_getcash() {
	let args = dot_usd_program(&leg(CashTransfer::Reserve), 10_200_000, 50_000, MAX_WEIGHT);

	assert_eq!(
		execute_hex(&args),
		"0x1f0305140004000204320576c8eb0b00028f6e0230000204320576c8eb0b00420d030031010100791701010004000204320576c8eb0b00821a06000004010101000204320576c8eb0b000c33040000010101021111111111111111111111111111111111111111111111111111111111111111140d010204000101001111111111111111111111111111111111111111111111111111111111111111140d01020400010100111111111111111111111111111111111111111111111111111111111111111103d202964902000400"
	);
}

#[test]
fn the_forwarded_stand_in_matches_getcash() {
	let program = forwarded_stand_in(&leg(CashTransfer::Teleport), ASSET_HUB_PARA_ID, 10_000_000);
	let metadata = metadata();
	let message_type = metadata
		.pallet_by_name("PolkadotXcm")
		.and_then(|pallet| pallet.call_variant_by_name("execute"))
		.and_then(|call| call.fields.iter().find(|field| field.name.as_deref() == Some("message")))
		.map(|field| field.ty.id)
		.expect("execute takes a message");

	let encoded = program.encode_as_type(message_type, metadata.types()).expect("it encodes");

	assert_eq!(
		format!("0x{}", hex::encode(encoded)),
		"0x05200204010300711704320576c8eb0b00025a620230010300711704320576c8eb0b00025a62020204010300711704320576c8eb0b00025a62020a33040000010101021111111111111111111111111111111111111111111111111111111111111111140d0102040001010011111111111111111111111111111111111111111111111111111111111111112c0000000000000000000000000000000000000000000000000000000000000000"
	);
}

/// The runtime orders locations by parents, then arity, then each junction
/// by variant and value: a stable at a lower asset id sorts first, and a
/// local asset before the relay native.
#[test]
fn locations_sort_as_the_runtime_orders_them() {
	let mut locations = vec![
		Location::relay_native(),
		Location::asset_hub_asset(CASH_ASSET_ID),
		Location::asset_hub_asset(USDT_ASSET_ID),
		Location::sibling(PEOPLE_PARA_ID),
	];
	locations.sort();

	assert_eq!(
		locations,
		vec![
			Location::asset_hub_asset(USDT_ASSET_ID),
			Location::asset_hub_asset(CASH_ASSET_ID),
			Location::relay_native(),
			Location::sibling(PEOPLE_PARA_ID),
		]
	);
}

#[test]
fn fee_margins_round_up_and_earmarks_keep_a_floor() {
	assert_eq!(with_fee_margin(0), 0);
	assert_eq!(with_fee_margin(1), 2);
	assert_eq!(with_fee_margin(76_204), 83_825);
	assert_eq!(destination_earmark(10_000_000, 30_000), 100_000);
	assert_eq!(destination_earmark(1_000_000, 30_000), 30_000);
	assert_eq!(destination_earmark(0, 0), 1);
}
