//! A stack over the spec fixtures, shared by the tests of `stack` and
//! `provider`.

use sha2::{Digest, Sha256};
use trinity_stack::Descriptor;

use crate::LightStack;

pub use crate::test_specs::{PARACHAIN_SPEC as PEOPLE_SPEC, RELAY_SPEC};

pub const ASSET_HUB_SPEC: &str = include_str!("../networks/paseo-next-v2-asset-hub.json");

/// What identifies the test stack.
pub const STACK_ID: &str = "https://stack.example/descriptor.json";

/// The genesis hash `spec` implies.
pub fn genesis(spec: &str) -> [u8; 32] {
	trinity_stack::light_spec_genesis(spec.as_bytes()).expect("the fixture is a light spec")
}

pub fn sha256(spec: &str) -> String {
	format!("0x{}", hex::encode(Sha256::digest(spec.as_bytes())))
}

fn chain(role: &str, spec: &str) -> serde_json::Value {
	serde_json::json!({
		"role": role,
		"genesisHash": format!("0x{}", hex::encode(genesis(spec))),
		"rpc": ["wss://rpc.example"],
		"lightSpec": { "url": "https://stack.example/spec.json", "sha256": sha256(spec) },
	})
}

/// A descriptor of the relay, People with the statement store, and Asset Hub.
pub fn descriptor_json(generation: &str) -> serde_json::Value {
	let mut people = chain("People", PEOPLE_SPEC);
	people["paraId"] = 1004.into();
	people["statementStore"] = true.into();
	let mut asset_hub = chain("AssetHub", ASSET_HUB_SPEC);
	asset_hub["paraId"] = 1000.into();
	serde_json::json!({
		"$v": 1,
		"sequence": 1,
		"generation": generation,
		"id": "test",
		"name": "Test",
		"network": "paseo",
		"tld": "testnet",
		"chains": [chain("Relay", RELAY_SPEC), people, asset_hub],
		"services": [],
	})
}

/// The test stack at `generation`, with no light spec yet.
pub fn stack(id: &str, generation: &str) -> LightStack {
	let descriptor = Descriptor::from_json(descriptor_json(generation)).expect("valid descriptor");
	LightStack::new(id, &descriptor).expect("no two chains share a genesis hash")
}

/// The test stack at `generation` with the relay's and People's specs.
pub fn synced_stack(id: &str, generation: &str) -> LightStack {
	let stack = stack(id, generation);
	for spec in [RELAY_SPEC, PEOPLE_SPEC] {
		stack.add_light_spec(genesis(spec), spec.as_bytes()).expect("the spec matches");
	}
	stack
}
