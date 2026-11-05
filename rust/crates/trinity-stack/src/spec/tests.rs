use super::*;

/// A real light spec smoldot synced from, frozen here so a testnet wipe cannot break the test,
/// with the genesis hash its chain reported.
const PASEO_NEXT_V2_PEOPLE: &str =
	include_str!("../../tests/fixtures/paseo-next-v2-people.smol.json");
const PASEO_NEXT_V2_PEOPLE_GENESIS: &str =
	"0x4a2b5b737de1da59e209b0000a876ec2fa20035dc34fd292a848da32d255ad48";

#[test]
fn a_light_spec_implies_its_chains_genesis_hash() {
	let implied = light_spec_genesis(PASEO_NEXT_V2_PEOPLE.as_bytes()).unwrap();
	assert_eq!(format::to_hex(&implied), PASEO_NEXT_V2_PEOPLE_GENESIS);
}
