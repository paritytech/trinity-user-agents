use super::*;
use crate::test_support::{
	TEST_STACK_PEOPLE, TEST_STACK_WIPED_PEOPLE, published_test_descriptor,
	reserved_test_descriptor, test_descriptor_chain,
};

fn descriptor(json: &serde_json::Value) -> Descriptor {
	serde_json::from_value(json.clone()).expect("test descriptor is valid")
}

/// `length` bytes of `byte` as the descriptor writes a hash or an address.
fn hex_of(byte: u8, length: usize) -> serde_json::Value {
	format!("0x{}", hex::encode(vec![byte; length])).into()
}

fn remove(object: &mut serde_json::Value, field: &str) {
	object.as_object_mut().expect("a JSON object").remove(field);
}

fn chains(descriptor: &mut serde_json::Value) -> &mut Vec<serde_json::Value> {
	descriptor["chains"].as_array_mut().expect("chains is an array")
}

/// The reserved stack's next copy with one `change` made to it.
fn changed(change: fn(&mut serde_json::Value)) -> Descriptor {
	let mut copy = reserved_test_descriptor(2);
	change(&mut copy);
	descriptor(&copy)
}

/// A change to a field a stack allowed reserved values pins.
struct PinnedChange {
	make: fn(&mut serde_json::Value),
	/// The field a refusal names for it.
	field: &'static str,
}

/// One change or more to each field a stack allowed reserved values pins.
/// The chains are the Relay, Asset Hub, People, Bulletin and Collectives, in
/// that order.
const PINNED_FIELD_CHANGES: [PinnedChange; 14] = [
	PinnedChange { make: |copy| copy["tld"] = "paseo".into(), field: "tld" },
	PinnedChange {
		make: |copy| copy["chains"][2]["genesisHash"] = hex_of(0xa9, 32),
		field: "a chain's genesisHash",
	},
	PinnedChange {
		make: |copy| copy["chains"][4]["genesisHash"] = hex_of(0xa9, 32),
		field: "a chain's genesisHash",
	},
	PinnedChange {
		make: |copy| chains(copy).push(test_descriptor_chain("Coretime", [0xa9; 32], Some(1005))),
		field: "a chain's genesisHash",
	},
	PinnedChange { make: |copy| chains(copy).truncate(4), field: "a chain's genesisHash" },
	PinnedChange { make: |copy| copy["assets"][0]["id"] = 1.into(), field: "assets" },
	PinnedChange {
		make: |copy| copy["assets"].as_array_mut().expect("assets is an array").reverse(),
		field: "assets",
	},
	PinnedChange {
		make: |copy| copy["contracts"]["multicall"] = hex_of(0xee, 20),
		field: "contracts",
	},
	PinnedChange { make: |copy| copy["contracts"]["extra"] = hex_of(0xee, 20), field: "contracts" },
	PinnedChange { make: |copy| copy["dotns"]["popController"] = hex_of(0xee, 20), field: "dotns" },
	PinnedChange { make: |copy| remove(copy, "dotns"), field: "dotns" },
	PinnedChange { make: |copy| copy["coinage"]["instance"] = 1.into(), field: "coinage" },
	PinnedChange {
		make: |copy| copy["chains"][0]["lightSpec"]["sha256"] = hex_of(0xee, 32),
		field: "the relay's lightSpec.sha256",
	},
	PinnedChange {
		make: |copy| remove(&mut copy["chains"][0], "lightSpec"),
		field: "the relay's lightSpec.sha256",
	},
];

/// Accounts derive under the TLD, so a copy on another one would change every
/// account the host holds. Everything else may differ: a stack follows a wipe
/// without a new build.
#[test]
fn a_copy_passes_only_on_the_bundled_tld() {
	let bundled = published_test_descriptor(1, "first", TEST_STACK_PEOPLE);
	let wiped = published_test_descriptor(2, "second", TEST_STACK_WIPED_PEOPLE);
	let mut on_another_tld = wiped.clone();
	on_another_tld["tld"] = "testnet".into();
	let pinned = Pinned::new(&descriptor(&bundled), &ReservedValuesPolicy::default());

	assert_eq!(
		[&bundled, &wiped, &on_another_tld].map(|copy| pinned.refuse_change(&descriptor(copy))),
		[Ok(()), Ok(()), Err(StackError::PinnedFieldChanged { field: "tld" })]
	);
}

/// A key that signs Polkadot's descriptor could otherwise point every host at
/// other chains, another payment asset or other contracts. The build's copy
/// decides those, so a copy that changes one is refused and names it.
#[test]
fn a_stack_allowed_reserved_values_pins_its_chains_assets_and_contracts() {
	let bundled = descriptor(&reserved_test_descriptor(1));
	let pinned = Pinned::new(&bundled, &ReservedValuesPolicy::ALLOWED);

	assert_eq!(
		PINNED_FIELD_CHANGES.map(|change| pinned.refuse_change(&changed(change.make))),
		PINNED_FIELD_CHANGES
			.map(|change| Err(StackError::PinnedFieldChanged { field: change.field }))
	);
}

/// Any other stack follows its operator: a wipe, a redeployed contract or a
/// new payment asset reaches its hosts without a new build. Only the TLD
/// stays.
#[test]
fn a_stack_without_reserved_values_pins_only_its_tld() {
	let bundled = descriptor(&reserved_test_descriptor(1));
	let pinned = Pinned::new(&bundled, &ReservedValuesPolicy::default());

	assert_eq!(
		PINNED_FIELD_CHANGES.map(|change| pinned.refuse_change(&changed(change.make))),
		PINNED_FIELD_CHANGES.map(|change| match change.field {
			"tld" => Err(StackError::PinnedFieldChanged { field: "tld" }),
			_ => Ok(()),
		})
	);
}

/// The operator of a reserved stack still moves endpoints, republishes and
/// re-forks without a new build: none of that decides which chains, assets or
/// contracts a host uses.
#[test]
fn a_stack_allowed_reserved_values_follows_every_other_change() {
	let bundled = descriptor(&reserved_test_descriptor(1));
	let pinned = Pinned::new(&bundled, &ReservedValuesPolicy::ALLOWED);
	let unpinned_changes: [fn(&mut serde_json::Value); 7] = [
		|_unchanged| {},
		|copy| copy["generation"] = "second".into(),
		|copy| copy["network"] = "polkadot-test".into(),
		|copy| copy["validUntil"] = 1_900_000_000.into(),
		|copy| copy["chains"][2]["rpc"] = serde_json::json!(["wss://moved.test.invalid"]),
		|copy| {
			copy["chains"][0]["lightSpec"]["url"] = "https://moved.test.invalid/relay.json".into()
		},
		|copy| {
			copy["chains"][2]["lightSpec"] = serde_json::json!({
				"url": "https://stack.test.invalid/people.json",
				"sha256": hex_of(0xee, 32),
			})
		},
	];

	assert_eq!(
		unpinned_changes.map(|change| pinned.refuse_change(&changed(change))),
		unpinned_changes.map(|_| Ok(()))
	);
}
