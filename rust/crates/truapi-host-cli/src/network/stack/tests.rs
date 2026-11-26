use super::{
	super::{
		ChainEndpoint, Network,
		testing::{edit_chain, edit_descriptor, edit_stack, test_access},
	},
	*,
};
use crate::{chain::ChainBackend, stack_descriptor::Roles};

/// `descriptor` as a stack, for a descriptor `Stack::new` may reject.
fn read_stack(descriptor: Descriptor) -> Result<Stack> {
	Stack::new("test-stack".to_string(), descriptor, &ChainAccess::Rpc)
}

/// A descriptor carries everything a network needs: the routes and genesis of
/// each role, the identity backend, the TLD the CLI derives identities under,
/// and the dotNS controller.
#[test]
fn test_stack_from_descriptor() {
	let paseo = Network::PaseoNextV2.bundled_stack(ChainBackend::Rpc);
	let stack = paseo
		.with_edit(|descriptor| {
			descriptor["id"] = "staging-fork".into();
			descriptor["tld"] = "testnet".into();
			descriptor["services"] = serde_json::json!([
				{ "kind": "dub", "urls": ["https://dub.example/"] },
			]);
			descriptor["dotns"] =
				serde_json::json!({ "popController": format!("0x{}", "ab".repeat(20)) });
		})
		.with_chain_edit("People", |chain| {
			chain["rpc"] = serde_json::json!(["ws://127.0.0.1:9944", "wss://b.example"]);
			chain["genesisHash"] = format!("0x{}", "11".repeat(32)).into();
		});

	assert_eq!(stack.namespace(), "staging");
	assert_eq!(stack.tld(), "testnet");
	assert_eq!(stack.identity_backend(), "https://dub.example/api/v1");
	assert_eq!(stack.pop_controller(), [0xab; 20]);
	let people = ChainEndpoint { genesis: [0x11; 32], ws: "ws://127.0.0.1:9944".to_owned() };
	assert_eq!(*stack.chains(), Roles { people, ..paseo.chains().clone() });
}

/// The core serves names under a stack's TLD without knowing that TLD
/// in advance, and reads a `dot` name as one of them.
#[test]
fn test_stack_runs_on_its_own_tld() {
	let staging = edit_stack(|descriptor| descriptor["tld"] = "staging".into());
	let stack = read_stack(staging)
		.expect("any label is a TLD")
		.core_stack()
		.expect("core stack");
	let served = |product_id: &str| {
		stack
			.served_id(&truapi::platform::ProductId::for_test(product_id))
			.ok()
			.map(String::from)
	};
	assert_eq!(
		["app.staging", "app.dot", "app.paseo"].map(served),
		[Some("app.staging".to_string()), Some("app.staging".to_string()), None]
	);
}

/// The host routes each role by its genesis, so two roles on one genesis
/// would send one role's traffic to the other's chain. Such a descriptor is
/// refused as it is read, before it can become a stack.
#[test]
fn test_stack_shared_genesis_error() {
	let paseo = Network::PaseoNextV2.bundled_stack(ChainBackend::Rpc);
	let shared = format!("0x{}", hex::encode(paseo.genesis().people));
	let descriptor = edit_descriptor(|descriptor| {
		let chains = descriptor["chains"].as_array_mut().expect("chains is an array");
		let bulletin =
			chains.iter_mut().find(|chain| chain["role"] == "Bulletin").expect("Bulletin");
		bulletin["genesisHash"] = shared.clone().into();
	});

	assert_eq!(
		trinity_stack::Document::parse(&descriptor).map(|_| ()),
		Err(trinity_stack::Error::DuplicateGenesisHash(shared))
	);
}

/// The namespace is what the CLI prints for the network, so an id that is not
/// a plain label is not used as one.
#[test]
fn test_stack_id_not_label() {
	let descriptor = edit_stack(|descriptor| descriptor["id"] = "../elsewhere".into());
	assert!(read_stack(descriptor).is_err());
}

/// No stack runs under `polkadot`, so that none is presented as Polkadot; a
/// `polkadot-fork` keeps its full id instead. A fork of any other stack has
/// its source's namespace, a `dot` TLD included.
#[test]
fn test_stack_namespace() {
	let polkadot = edit_stack(|descriptor| descriptor["id"] = "polkadot".into());
	let error = read_stack(polkadot).unwrap_err();
	assert!(error.to_string().contains("the polkadot namespace"));

	for (id, namespace) in [
		("polkadot-fork", "polkadot-fork"),
		("pcf-devnet", "pcf-devnet"),
		("pcf-devnet-fork", "pcf-devnet"),
	] {
		let stack = Network::PaseoNextV2.bundled_stack(ChainBackend::Rpc).with_edit(|descriptor| {
			descriptor["id"] = id.into();
			descriptor["tld"] = "dot".into();
		});
		assert_eq!(stack.namespace(), namespace);
	}
}

/// The state directory follows the stack's id and ignores the descriptor's
/// `id`. A fork has an id of its own, so it does not read its source's state,
/// and neither does a stack that only declares a built-in network's `id`. A
/// descriptor that renames itself keeps its stack's state.
#[test]
fn test_stack_state_directory() {
	let read_from = |stack_id: &str, id: &str| {
		let descriptor = edit_stack(|descriptor| descriptor["id"] = id.into());
		Stack::new(stack_id.to_string(), descriptor, &ChainAccess::Rpc).expect("stack is valid")
	};
	let fork = "https://fork.test.invalid/trinity-stack.json";
	let namesake = "https://namesake.test.invalid/trinity-stack.json";

	assert_eq!(
		[
			Network::PaseoNextV2.bundled_stack(ChainBackend::Rpc).state_directory(),
			read_from(fork, "paseo-next-v2-fork").state_directory(),
			read_from(namesake, "paseo-next-v2").state_directory(),
			read_from(namesake, "renamed").state_directory(),
		],
		[
			"paseo-next-v2",
			"stack-c1ff7d592da6515589a30362b8ab952c",
			"stack-458b86f54a9e42d4d99538ba9ca22a53",
			"stack-458b86f54a9e42d4d99538ba9ca22a53",
		]
	);
}

/// A fork has its source's namespace, so a message about stored state, such as
/// the one that asks before every session is cleared, names the directory too.
/// A built-in network's two names are one.
#[test]
fn test_state_label_tells_a_fork_from_its_source() {
	let fork = edit_stack(|descriptor| descriptor["id"] = "paseo-next-v2-fork".into());
	let fork_id = "https://fork.test.invalid/trinity-stack.json".to_string();
	let fork = Stack::new(fork_id, fork, &ChainAccess::Rpc).expect("stack is valid");

	assert_eq!(
		[Network::PaseoNextV2.bundled_stack(ChainBackend::Rpc).state_label(), fork.state_label()],
		["paseo-next-v2", "paseo-next-v2 (stack-c1ff7d592da6515589a30362b8ab952c)"]
	);
}

/// A built-in network's state is kept under the name `--network` takes, which
/// is where its users and the e2e scripts look for it.
#[test]
fn test_built_in_state_directory_is_the_network_name() {
	use clap::ValueEnum as _;

	for network in Network::value_variants() {
		let name = network.to_possible_value().expect("every network has a value");
		assert_eq!(network.bundled_stack(ChainBackend::Rpc).state_directory(), name.get_name());
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
	let error = read_stack(descriptor).unwrap_err();
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
	let error = read_stack(without_dub).unwrap_err();
	assert!(error.to_string().contains("no dub service"));

	let without_pop_controller = edit_stack(|descriptor| {
		descriptor["dotns"].as_object_mut().unwrap().remove("popController");
	});
	let error = read_stack(without_pop_controller).unwrap_err();
	assert!(error.to_string().contains("no dotns.popController"));
}

/// Every built-in network runs on the light client, whether or not
/// `truapi-provider` bundles its specs: the descriptor names them.
#[test]
fn test_built_in_networks_name_their_light_specs() {
	use clap::ValueEnum as _;

	for network in Network::value_variants() {
		assert!(network.bundled_stack(ChainBackend::LightClient).light_client().is_some());
	}
}

/// A stack with no light spec for a chain the light client runs fails before
/// the command starts, naming the chain, rather than on every connect.
#[test]
fn test_light_client_without_a_light_spec_error() {
	let descriptor = edit_chain("Bulletin", |chain| {
		chain.as_object_mut().expect("a chain is an object").remove("lightSpec");
	});

	let error = Stack::new(
		"test-stack".to_string(),
		descriptor.clone(),
		&test_access(ChainBackend::LightClient),
	)
	.unwrap_err()
	.to_string();

	assert!(error.contains("Bulletin") && error.contains("--chain-backend rpc"), "{error}");
	read_stack(descriptor).expect("RPC needs no light spec");
}
