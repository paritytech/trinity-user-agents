use super::{
	super::{
		Network,
		testing::{edit_chain, edit_stack},
	},
	*,
};
use crate::stack_descriptor::Roles;

/// A descriptor carries everything a network needs: the routes and genesis of
/// each role, the identity backend, the TLD the CLI derives identities under,
/// and the dotNS controller.
#[test]
fn test_stack_config_from_descriptor() {
	let people_genesis = format!("0x{}", "11".repeat(32));
	let descriptor = edit_stack(|descriptor| {
		descriptor["id"] = "staging-fork".into();
		descriptor["tld"] = "testnet".into();
		descriptor["services"] = serde_json::json!([
			{ "kind": "dub", "urls": ["https://dub.example/"] },
		]);
		descriptor["dotns"] =
			serde_json::json!({ "popController": format!("0x{}", "ab".repeat(20)) });
		for chain in descriptor["chains"].as_array_mut().unwrap() {
			if chain["role"] == "People" {
				chain["rpc"] = serde_json::json!(["ws://127.0.0.1:9944", "wss://b.example"]);
				chain["genesisHash"] = people_genesis.clone().into();
			}
		}
	});

	let config = stack_network_config(&descriptor, ChainBackend::Rpc).unwrap();

	assert_eq!(config.id, "staging");
	assert_eq!(config.network_suffix, "testnet");
	assert_eq!(config.identity_backend_base, "https://dub.example/api/v1");
	assert_eq!(config.dotns_pop_controller, [0xab; 20]);
	let paseo = Network::PaseoNextV2.config(ChainBackend::Rpc);
	let people = ChainEndpoint { genesis: [0x11; 32], ws: "ws://127.0.0.1:9944".to_owned() };
	assert_eq!(config.chains, Roles { people, ..paseo.chains });
}

/// Reserved identities derive under the TLD, and navigation accepts only the
/// TLDs the core serves.
#[test]
fn test_stack_unserved_tld_error() {
	let staging = edit_stack(|descriptor| descriptor["tld"] = "staging".into());
	let error = stack_network_config(&staging, ChainBackend::Rpc).unwrap_err();
	assert!(error.to_string().contains("staging"));
}

/// The host routes each role by its genesis, so two roles on one genesis
/// would send one role's traffic to the other's chain.
#[test]
fn test_stack_shared_genesis_error() {
	let paseo = Network::PaseoNextV2.config(ChainBackend::Rpc);
	let shared = format!("0x{}", hex::encode(paseo.chains.people.genesis));
	let descriptor = edit_chain("Bulletin", |chain| chain["genesisHash"] = shared.into());
	assert!(stack_network_config(&descriptor, ChainBackend::Rpc).is_err());
}

/// A stack id becomes a state directory name, so one that is not a plain
/// label is not used as the namespace.
#[test]
fn test_stack_id_not_label() {
	let descriptor = edit_stack(|descriptor| descriptor["id"] = "../elsewhere".into());
	assert!(stack_network_config(&descriptor, ChainBackend::Rpc).is_err());
}

/// Earlier releases stored Polkadot mainnet sessions under `polkadot`, so no
/// stack runs there; a `polkadot-fork` has its own state instead. A fork of
/// any other stack shares its source's namespace, a `dot` TLD included.
#[test]
fn test_stack_namespace() {
	let polkadot = edit_stack(|descriptor| descriptor["id"] = "polkadot".into());
	let error = stack_network_config(&polkadot, ChainBackend::Rpc).unwrap_err();
	assert!(error.to_string().contains("the polkadot namespace"));

	for (id, namespace) in [
		("polkadot-fork", "polkadot-fork"),
		("pcf-devnet", "pcf-devnet"),
		("pcf-devnet-fork", "pcf-devnet"),
	] {
		let descriptor = edit_stack(|descriptor| {
			descriptor["id"] = id.into();
			descriptor["tld"] = "dot".into();
		});
		let config = stack_network_config(&descriptor, ChainBackend::Rpc).unwrap();
		assert_eq!(config.id, namespace);
	}
}

#[test]
fn test_stack_missing_role_error() {
	let descriptor = edit_stack(|descriptor| {
		descriptor["chains"]
			.as_array_mut()
			.unwrap()
			.retain(|chain| chain["role"] != "Bulletin");
	});
	let error = stack_network_config(&descriptor, ChainBackend::Rpc).unwrap_err();
	assert!(error.to_string().contains("no Bulletin chain"));
}

/// The CLI registers usernames through the stack's `dub` service and resolves
/// them through its `dotns.popController`, so a stack without either cannot
/// run any command.
#[test]
fn test_stack_missing_dub_or_pop_controller_error() {
	let without_dub = edit_stack(|descriptor| {
		descriptor["services"]
			.as_array_mut()
			.unwrap()
			.retain(|service| service["kind"] != "dub");
	});
	let error = stack_network_config(&without_dub, ChainBackend::Rpc).unwrap_err();
	assert!(error.to_string().contains("no dub service"));

	let without_pop_controller = edit_stack(|descriptor| {
		descriptor["dotns"].as_object_mut().unwrap().remove("popController");
	});
	let error = stack_network_config(&without_pop_controller, ChainBackend::Rpc).unwrap_err();
	assert!(error.to_string().contains("no dotns.popController"));
}
