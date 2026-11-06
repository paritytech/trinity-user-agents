use clap::ValueEnum;

use super::{
	super::{
		stack::stack_network_config,
		testing::{edit_chain, edit_stack},
	},
	*,
};
use crate::chain::ChainBackend;

/// SPEC.md lists Polkadot's genesis hashes as reserved values. Compiled in
/// with `include_str!` so a moved table breaks the build rather than drifting
/// quietly.
#[test]
fn test_spec_reserved_genesis_table() {
	const SPEC: &str = include_str!("../../../SPEC.md");

	for (row, genesis) in ["People genesis", "Asset Hub genesis", "Bulletin genesis"]
		.into_iter()
		.zip(POLKADOT_GENESIS)
	{
		assert!(SPEC.contains(&format!("| {row} | `0x{genesis}` |")));
	}
}

/// A fork has its source's genesis, so a network routed at a copy of
/// mainnet would sign mainnet-valid transactions with auto-approval. The
/// check applies to the built-in networks as well as to a stack.
#[test]
fn test_polkadot_genesis_error() {
	for network in Network::value_variants() {
		let config = network.config(ChainBackend::Rpc);
		assert!(ensure_no_reserved_values(&config).is_ok());
	}

	let polkadot_people = format!("0x{}", POLKADOT_GENESIS[0]);
	let forked = edit_chain("People", |chain| chain["genesisHash"] = polkadot_people.into());
	let error =
		ensure_no_reserved_values(&stack_network_config(&forked, ChainBackend::Rpc).unwrap())
			.expect_err("mainnet genesis accepted");
	assert!(error.to_string().contains("Polkadot's chain"));

	let mut pcf_devnet = Network::PcfDevnet.config(ChainBackend::Rpc);
	hex::decode_to_slice(POLKADOT_GENESIS[1], &mut pcf_devnet.chains.asset_hub.genesis).unwrap();
	let error = ensure_no_reserved_values(&pcf_devnet).expect_err("mainnet genesis accepted");
	assert!(error.to_string().contains("Polkadot's chain"));
}

/// Earlier releases stored mainnet sessions under `polkadot`, so no stack
/// runs under it.
#[test]
fn test_retired_polkadot_namespace_error() {
	for id in ["polkadot", "polkadot-fork"] {
		let descriptor = edit_stack(|descriptor| descriptor["id"] = id.into());
		let config = stack_network_config(&descriptor, ChainBackend::Rpc).unwrap();
		let error = ensure_no_reserved_values(&config).expect_err("polkadot namespace accepted");
		assert!(error.to_string().contains("the polkadot namespace"));
	}
}

/// A `dot` identity derives the same keys as on Polkadot, so only the
/// bundled pcf-devnet copy may use the TLD.
#[test]
fn test_stack_dot_tld_error() {
	let pcf_devnet = Network::PcfDevnet.config(ChainBackend::Rpc);
	assert_eq!(pcf_devnet.network_suffix, POLKADOT_TLD);
	assert!(ensure_no_reserved_values(&pcf_devnet).is_ok());

	let dot = edit_stack(|descriptor| descriptor["tld"] = "dot".into());
	let error = ensure_stack_not_reserved(&stack_network_config(&dot, ChainBackend::Rpc).unwrap())
		.expect_err("dot TLD accepted");
	assert!(error.to_string().contains(".dot"));
}

/// A stack cannot pose as the built-in pcf-devnet network.
#[test]
fn test_stack_pcf_devnet_namespace_error() {
	for id in ["pcf-devnet", "pcf-devnet-fork"] {
		let descriptor = edit_stack(|descriptor| descriptor["id"] = id.into());
		let config = stack_network_config(&descriptor, ChainBackend::Rpc).unwrap();
		let error = ensure_stack_not_reserved(&config).expect_err("pcf-devnet namespace accepted");
		assert!(error.to_string().contains("the pcf-devnet namespace"));
	}
	let paseo = stack_network_config(&edit_stack(|_| {}), ChainBackend::Rpc).unwrap();
	assert!(ensure_stack_not_reserved(&paseo).is_ok());
}
