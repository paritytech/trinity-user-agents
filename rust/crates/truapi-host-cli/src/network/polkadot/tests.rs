use truapi::platform::DOTNS_TLDS;

use super::{
	super::{
		Network,
		fixtures::{edit_chain, edit_stack},
		stack::stack_network_config,
	},
	*,
};

/// SPEC.md pins Polkadot's genesis hashes, the values a build reserves for
/// real identities. Compiled in with `include_str!` so a moved table breaks
/// the build rather than drifting quietly.
#[test]
fn test_spec_genesis_table_polkadot() {
	const SPEC: &str = include_str!("../../../SPEC.md");

	let config = Network::Polkadot.config(ChainBackend::Rpc);
	for (row, expected) in [
		("People genesis", config.chains.people.genesis),
		("Bulletin genesis", config.chains.bulletin.genesis),
		("Asset Hub genesis", config.chains.asset_hub.genesis),
	] {
		let expected_row = format!("| {row} | `0x{}` |", hex::encode(expected));
		assert!(SPEC.contains(&expected_row));
	}
}

#[test]
fn test_polkadot_served_tld() {
	assert!(DOTNS_TLDS.contains(&Network::Polkadot.config(ChainBackend::Rpc).network_suffix));
}

/// The compile-time reader must agree with a JSON parser on every entry,
/// so Polkadot's controller is exactly what the manifest records.
#[test]
fn manifest_addresses_decode_as_a_json_parser_reads_them() {
	let manifest: serde_json::Map<String, serde_json::Value> =
		serde_json::from_str(DOTNS_DEPLOYMENT).expect("invalid manifest JSON");
	for (name, address) in &manifest {
		let address = address.as_str().expect("manifest entry is not a string");
		assert_eq!(
			hex::encode(manifest_address(DOTNS_DEPLOYMENT, name)),
			address.trim_start_matches("0x").to_lowercase(),
			"{name}"
		);
	}
	assert_eq!(
		Network::Polkadot.config(ChainBackend::Rpc).dotns_pop_controller,
		Some(manifest_address(DOTNS_DEPLOYMENT, "DotnsPopController"))
	);
}

/// A fork has its source's genesis, so a stack routed at a copy of mainnet
/// would sign mainnet-valid transactions with auto-approval; and a `dot`
/// name is a mainnet name.
#[test]
fn test_stack_polkadot_values_error() {
	assert!(
		ensure_no_real_identity_values(&Network::PaseoNextV2.config(ChainBackend::Rpc)).is_ok()
	);
	assert!(ensure_no_real_identity_values(&Network::Polkadot.config(ChainBackend::Rpc)).is_ok());

	let polkadot_people = format!("0x{}", hex::encode(PEOPLE.genesis));
	let forked = edit_chain("People", |chain| chain["genesisHash"] = polkadot_people.into());
	let error =
		ensure_no_real_identity_values(&stack_network_config(&forked, ChainBackend::Rpc).unwrap())
			.expect_err("mainnet genesis accepted");
	assert!(error.to_string().contains("polkadot"));

	let dot = edit_stack(|descriptor| descriptor["tld"] = "dot".into());
	let error =
		ensure_no_real_identity_values(&stack_network_config(&dot, ChainBackend::Rpc).unwrap())
			.expect_err("dot TLD accepted");
	assert!(error.to_string().contains(".dot"));
}

/// Polkadot's namespace contains its real-identity sessions and accounts, so
/// a disposable stack never runs under it.
#[test]
fn test_stack_polkadot_namespace_error() {
	for id in ["polkadot", "polkadot-fork"] {
		let descriptor = edit_stack(|descriptor| descriptor["id"] = id.into());
		let config = stack_network_config(&descriptor, ChainBackend::Rpc).unwrap();
		assert!(ensure_no_real_identity_values(&config).is_err());
	}
}

/// Mainnet identities are real: they must never reach the plaintext account
/// store or auto-accept.
#[test]
fn polkadot_holds_real_identities() {
	assert!(
		Network::Polkadot
			.config(ChainBackend::Rpc)
			.ensure_disposable_identities()
			.is_err()
	);
}
