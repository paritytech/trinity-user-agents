use super::*;
use crate::test_stack::{
	ASSET_HUB_SPEC, PEOPLE_SPEC, RELAY_SPEC, STACK_ID, descriptor_json, genesis, stack,
	synced_stack,
};

fn statement_protocol(stack: &LightStack, genesis_hash: [u8; 32]) -> Option<bool> {
	match stack.source(genesis_hash)? {
		ChainSource::LightClient { statement_protocol, .. } => Some(statement_protocol),
		#[cfg(feature = "ws")]
		ChainSource::RpcNode { .. } => panic!("a stack only has light-client chains"),
	}
}

/// The descriptor decides the topology: every parachain syncs through the one
/// relay, and only the chain marked `statementStore` runs the statement
/// protocol.
#[test]
fn wiring_and_statement_store_come_from_the_descriptor() {
	let mut stack = synced_stack(STACK_ID, "one");
	stack
		.add_light_spec(genesis(ASSET_HUB_SPEC), ASSET_HUB_SPEC.as_bytes())
		.expect("the spec matches");
	let relay = genesis(RELAY_SPEC);

	assert_eq!(
		[RELAY_SPEC, PEOPLE_SPEC, ASSET_HUB_SPEC].map(|spec| stack.relay_of(genesis(spec))),
		[None, Some(relay), Some(relay)]
	);
	assert_eq!(
		[RELAY_SPEC, PEOPLE_SPEC, ASSET_HUB_SPEC]
			.map(|spec| statement_protocol(&stack, genesis(spec))),
		[Some(false), Some(true), Some(false)]
	);
}

/// A spec served with other bytes than the descriptor names is refused with
/// the chain named, so the host can tell which chain to fetch again.
#[test]
fn a_spec_with_another_sha256_is_refused_and_names_the_chain() {
	let mut stack = stack(STACK_ID, "one");
	let people = genesis(PEOPLE_SPEC);

	let error = stack
		.add_light_spec(people, format!("{PEOPLE_SPEC} ").as_bytes())
		.expect_err("the bytes differ from the descriptor's sha256");

	assert_eq!(
		error,
		ProviderError::LightSpec {
			role: "People".to_owned(),
			genesis: people,
			reason: trinity_stack::Error::SpecDigest.to_string(),
		}
	);
	assert!(stack.source(people).is_none(), "a refused spec is not kept");
}

/// The digest alone does not bind a spec to its chain: a descriptor can name
/// the right file under the wrong chain, and the light client would then sync
/// another chain than the one the host asked for.
#[test]
fn a_spec_of_another_chain_is_refused() {
	let mut json = descriptor_json("one");
	let asset_hub_spec = json["chains"][2]["lightSpec"].clone();
	json["chains"][1]["lightSpec"] = asset_hub_spec;
	let descriptor = Descriptor::from_json(json).expect("valid descriptor");
	let mut stack = LightStack::new(STACK_ID, &descriptor).expect("distinct genesis hashes");
	let people = genesis(PEOPLE_SPEC);

	let error = stack
		.add_light_spec(people, ASSET_HUB_SPEC.as_bytes())
		.expect_err("the spec implies Asset Hub's genesis hash");

	let implied = format!("0x{}", hex::encode(genesis(ASSET_HUB_SPEC)));
	assert_eq!(
		error,
		ProviderError::LightSpec {
			role: "People".to_owned(),
			genesis: people,
			reason: trinity_stack::Error::SpecGenesis(implied).to_string(),
		}
	);
}

#[test]
fn a_spec_for_a_chain_outside_the_stack_is_refused() {
	let mut stack = stack(STACK_ID, "one");

	assert_eq!(
		stack.add_light_spec([7; 32], PEOPLE_SPEC.as_bytes()),
		Err(ProviderError::UnknownGenesis { genesis: [7; 32] })
	);
}

/// Bindings hand over the link and the descriptor as served, and the stack's
/// identity has to be the one every other reader derives from that link.
#[test]
fn a_stack_opened_from_its_link_takes_the_links_identity() {
	let descriptor = serde_json::to_vec(&descriptor_json("one")).expect("JSON");
	let link = "polkadotapp://stack?url=https%3A%2F%2Fstack.example%2Fdescriptor.json%23top";

	let stack = LightStack::open(link, &descriptor).expect("an unsigned link opens a plain file");

	assert_eq!((stack.id(), stack.generation()), (STACK_ID, "one"));
}

/// A link with a key accepts only a document signed with it, so a plain
/// descriptor cannot stand in for a signed stack.
#[test]
fn a_keyed_link_refuses_an_unsigned_descriptor() {
	let descriptor = serde_json::to_vec(&descriptor_json("one")).expect("JSON");
	let key = "11".repeat(32);
	let link = format!("polkadotapp://stack?key=0x{key}&url=https%3A%2F%2Fstack.example%2Fd.json");

	assert_eq!(
		LightStack::open(&link, &descriptor).map(|_| ()),
		Err(ProviderError::Stack { reason: trinity_stack::Error::Unsigned.to_string() })
	);
}
