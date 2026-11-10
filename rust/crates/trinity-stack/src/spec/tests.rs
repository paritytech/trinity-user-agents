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

/// A `Chain` built in code skips the descriptor's validation, so its genesis hash may not decode;
/// the check fails with an error then rather than panicking.
#[test]
fn test_light_spec_malformed_genesis_error() {
	let bytes = PASEO_NEXT_V2_PEOPLE.as_bytes();
	let mut chain = Chain {
		role: "People".into(),
		genesis_hash: PASEO_NEXT_V2_PEOPLE_GENESIS.into(),
		para_id: Some(1004),
		rpc: vec!["wss://people.example".into()],
		statement_store: None,
		light_spec: Some(SpecFile {
			url: "https://people.example/spec.json".into(),
			sha256: format::to_hex(&Sha256::digest(bytes)),
		}),
		full_spec: None,
		forked_at: None,
	};
	chain.check_light_spec(bytes).unwrap();

	chain.genesis_hash = "0xnot-hex".into();
	let error = chain.check_light_spec(bytes).unwrap_err();
	assert!(matches!(error, Error::InvalidHex { field: "genesisHash", .. }));
}
