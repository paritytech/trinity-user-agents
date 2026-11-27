use clap::ValueEnum;

use super::{
	super::testing::{stack_on, test_access},
	*,
};
use crate::chain::ChainBackend;

/// A bundled copy that no longer opens with its pointer would leave the
/// built-in network, or the check that tells a Polkadot fork from Polkadot,
/// with nothing to start from when the published copy is unreachable. A
/// built-in's descriptor `id` is its `--network` value, so the CLI prints the
/// network under the name a user selects it by.
#[test]
fn test_bundled_stack_pointers() {
	let built_ins = Network::value_variants()
		.iter()
		.map(|network| {
			let value = network.to_possible_value().unwrap();
			(network.built_in_stack(), value.get_name().to_owned())
		})
		.chain([(POLKADOT_REFERENCE_STACK, "polkadot".to_owned())]);
	for (built_in, id) in built_ins {
		let bundled = trinity_stack::open(built_in.bundle, &built_in.pointer())
			.unwrap_or_else(|error| panic!("{id}: {error}"));
		assert!(bundled.signed);
		assert_eq!(bundled.descriptor.id, id);
	}
}

/// `stack bundle` writes the copy every command starts from, so a built-in
/// network's copy the CLI cannot run on is not written; any other stack is
/// bundled as fetched.
#[test]
fn test_built_in_runnable_check() {
	let without_dub = super::super::testing::edit_stack(|descriptor| {
		descriptor["services"]
			.as_array_mut()
			.unwrap()
			.retain(|service| service["kind"] != "dub");
	});
	let paseo = Network::PaseoNextV2.built_in_stack().pointer();
	let error = ensure_built_in_runnable(&paseo, &without_dub).unwrap_err();
	assert!(error.to_string().contains("no dub service"));
	ensure_built_in_runnable(&POLKADOT_REFERENCE_STACK.pointer(), &without_dub)
		.expect("a stack other than a built-in network checked");
	ensure_built_in_runnable(&paseo, &Network::PaseoNextV2.built_in_stack().open_bundled())
		.expect("bundled paseo-next-v2 copy not runnable");
}

/// A built-in's copy runs on either chain backend, so one without a light
/// spec is not written: every light-client command would fail on it. Any
/// other stack may run over RPC only, so this check accepts it without one.
#[test]
fn test_built_in_runnable_check_on_the_light_client() {
	let paseo = Network::PaseoNextV2.built_in_stack().pointer();
	let url = paseo.url();
	for role in ["Relay", "People", "AssetHub", "Bulletin"] {
		let without_spec = super::super::testing::edit_chain(role, |chain| {
			chain.as_object_mut().unwrap().remove("lightSpec");
		});
		let error = ensure_built_in_runnable(&paseo, &without_spec).unwrap_err();
		assert_eq!(
			error.to_string(),
			format!(
				"{url} is a built-in network's stack the CLI cannot run on the light client: its \
				 descriptor names no lightSpec for {role}"
			)
		);
		ensure_built_in_runnable(&POLKADOT_REFERENCE_STACK.pointer(), &without_spec)
			.expect("a stack other than a built-in network checked for its light specs");
	}
}

/// `stack bundle` refuses a built-in's copy by this check and a light-client
/// command refuses it by `Stack::new`, each with its own list of the chains
/// that need a light spec. They give the same answer for a copy that lacks one
/// chain's light spec, for every chain of every built-in, so this check
/// refuses such a copy exactly when a light-client command would.
#[test]
fn test_built_in_runnable_check_refuses_what_the_light_client_refuses() {
	let light_client = test_access(ChainBackend::LightClient);
	let mut refused_by_check = Vec::new();
	let mut refused_by_light_client = Vec::new();
	for network in Network::value_variants() {
		let built_in = network.built_in_stack();
		let bundled = built_in.open_bundled();
		for (index, chain) in bundled.chains.iter().enumerate() {
			let mut document = serde_json::to_value(&bundled).expect("descriptor not JSON");
			let edited = document["chains"][index].as_object_mut().expect("a chain is an object");
			edited.remove("lightSpec");
			let without_spec = Descriptor::from_json(document).expect("invalid edited descriptor");

			let refused = ensure_built_in_runnable(&built_in.pointer(), &without_spec).is_err();
			refused_by_check.push((*network, chain.role.clone(), refused));
			let refused =
				stack_on(&built_in.pointer().stack_id(), without_spec, &light_client).is_err();
			refused_by_light_client.push((*network, chain.role.clone(), refused));
		}
	}

	assert_eq!(refused_by_check, refused_by_light_client);
}

/// A chain the light client does not run needs no light spec, so this check
/// and a light-client command both accept a built-in's copy that names none
/// for it. The chain is added here because the bundled copies may stop
/// listing one.
#[test]
fn test_built_in_runnable_check_accepts_a_chain_the_light_client_does_not_run() {
	let with_unrouted_chain = super::super::testing::edit_stack(|descriptor| {
		let chains = descriptor["chains"].as_array_mut().unwrap();
		chains.push(serde_json::json!({
			"role": "Unrouted",
			"genesisHash": format!("0x{}", "11".repeat(32)),
			"paraId": u32::MAX,
			"rpc": ["wss://unrouted.example"],
		}));
	});
	let paseo = Network::PaseoNextV2.built_in_stack().pointer();

	let by_check =
		ensure_built_in_runnable(&paseo, &with_unrouted_chain).map_err(|error| error.to_string());
	let by_light_client =
		stack_on(&paseo.stack_id(), with_unrouted_chain, &test_access(ChainBackend::LightClient))
			.map(drop)
			.map_err(|error| error.to_string());

	assert_eq!((by_check, by_light_client), (Ok(()), Ok(())));
}

/// A light client on a fresh state directory starts from the specs this build
/// ships, so every spec a built-in's bundled copy names for a chain it runs
/// must be shipped and pass the check. `scripts/update-stacks.sh` refreshes
/// both together.
#[test]
fn test_bundled_light_specs_match_the_bundled_copies() {
	for network in Network::value_variants() {
		let descriptor = network.built_in_stack().open_bundled();
		for chain in crate::stack_descriptor::light_client_chains(&descriptor) {
			let named = chain.light_spec.as_ref().expect("a built-in names every light spec");
			let spec = bundled_light_spec(&named.sha256)
				.unwrap_or_else(|| panic!("{network:?} ships no light spec for {}", chain.role));
			chain
				.check_light_spec(spec.as_bytes())
				.unwrap_or_else(|error| panic!("{network:?} {}: {error}", chain.role));
		}
	}
}

/// A built-in stack is signed, so its bare URL has to resolve to the same
/// stack as its network name. Read as an unsigned stack it would get another
/// id, and the core would find none of the state stored under the name.
#[test]
fn a_built_in_stack_named_by_its_url_keeps_its_key() {
	for network in Network::value_variants() {
		let pointer = network.built_in_stack().pointer();
		let key = pointer.key().map(|key| format!("0x{}", hex::encode(key)));
		let link = pointer.to_link(trinity_stack::DEFAULT_LINK_BASE);

		assert_eq!(
			(built_in_key(pointer.url().as_str()), built_in_key(&link)),
			(key, None),
			"{network:?}"
		);
	}
	assert_eq!(built_in_key("https://stack.test.invalid/other.json"), None);
	assert_eq!(built_in_key("./stack.json"), None);
}
