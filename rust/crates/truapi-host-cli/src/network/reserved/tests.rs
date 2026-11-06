use super::{
	super::{
		stack::stack_network_config,
		testing::{edit_chain, edit_stack},
	},
	*,
};
use crate::chain::ChainBackend;

/// SPEC.md lists Polkadot's genesis hashes as the values a build reserves for
/// real identities. Compiled in with `include_str!` so a moved table breaks
/// the build rather than drifting quietly.
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

/// A fork has its source's genesis, so a stack routed at a copy of mainnet
/// would sign mainnet-valid transactions with auto-approval; and a `dot`
/// name is a mainnet name.
#[test]
fn test_stack_polkadot_values_error() {
	assert!(ensure_no_reserved_values(&Network::PaseoNextV2.config(ChainBackend::Rpc)).is_ok());

	let polkadot_people = format!("0x{}", POLKADOT_GENESIS[0]);
	let forked = edit_chain("People", |chain| chain["genesisHash"] = polkadot_people.into());
	let error =
		ensure_no_reserved_values(&stack_network_config(&forked, ChainBackend::Rpc).unwrap())
			.expect_err("mainnet genesis accepted");
	assert!(error.to_string().contains("Polkadot's chain"));

	let dot = edit_stack(|descriptor| descriptor["tld"] = "dot".into());
	let error = ensure_no_reserved_values(&stack_network_config(&dot, ChainBackend::Rpc).unwrap())
		.expect_err("dot TLD accepted");
	assert!(error.to_string().contains(".dot"));
}

/// The bundled pcf-devnet copy is the one network this build lets use the
/// `dot` TLD, and only because its identities are real.
#[test]
fn test_pcf_devnet_reserved_tld() {
	let pcf_devnet = Network::PcfDevnet.config(ChainBackend::Rpc);
	assert_eq!(pcf_devnet.network_suffix, POLKADOT_TLD);
	assert!(!pcf_devnet.disposable_identities);
	assert!(ensure_no_reserved_values(&pcf_devnet).is_ok());

	let disposable = NetworkConfig {
		id: "pcf-devnet-copy".to_owned(),
		disposable_identities: true,
		..pcf_devnet
	};
	let error = ensure_no_reserved_values(&disposable).expect_err("dot TLD accepted");
	assert!(error.to_string().contains(".dot"));
}

/// Real identities do not license Polkadot's chains: a network that signs
/// against one could sign valid mainnet transactions.
#[test]
fn test_real_identity_polkadot_genesis_error() {
	let mut pcf_devnet = Network::PcfDevnet.config(ChainBackend::Rpc);
	hex::decode_to_slice(POLKADOT_GENESIS[1], &mut pcf_devnet.chains.asset_hub.genesis).unwrap();
	let error = ensure_no_reserved_values(&pcf_devnet).expect_err("mainnet genesis accepted");
	assert!(error.to_string().contains("Polkadot's chain"));
}

/// A real-identity network's namespace contains its sessions, and earlier
/// releases stored real-identity sessions under `polkadot`, so a disposable
/// stack never runs under either.
#[test]
fn test_stack_real_identity_namespace_error() {
	for (id, namespace) in [
		("pcf-devnet", "pcf-devnet"),
		("pcf-devnet-fork", "pcf-devnet"),
		("polkadot", "polkadot"),
		("polkadot-fork", "polkadot"),
	] {
		let descriptor = edit_stack(|descriptor| descriptor["id"] = id.into());
		let config = stack_network_config(&descriptor, ChainBackend::Rpc).unwrap();
		let error =
			ensure_no_reserved_values(&config).expect_err("real-identity namespace accepted");
		assert!(error.to_string().contains(&format!("the {namespace} namespace")));
	}
}

/// Real identities must never be saved to the plaintext account store or
/// approved without a prompt.
#[test]
fn test_pcf_devnet_real_identities() {
	assert!(
		Network::PcfDevnet
			.config(ChainBackend::Rpc)
			.ensure_disposable_identities()
			.is_err()
	);
}
