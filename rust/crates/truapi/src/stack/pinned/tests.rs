use super::*;
use crate::test_support::{
	TEST_STACK_PEOPLE, TEST_STACK_WIPED_PEOPLE, published_test_descriptor,
	reserved_test_descriptor, test_chain, test_descriptor_chain, test_light_spec,
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
const PINNED_FIELD_CHANGES: [PinnedChange; 22] = [
	PinnedChange { make: |copy| copy["tld"] = "paseo".into(), field: "tld" },
	PinnedChange {
		make: |copy| chains(copy).push(test_descriptor_chain("Coretime", [0xa9; 32], Some(1005))),
		field: "the chain roles",
	},
	PinnedChange {
		make: |copy| chains(copy).retain(|chain| chain["role"] != "Collectives"),
		field: "the chain roles",
	},
	PinnedChange {
		make: |copy| test_chain(copy, "Collectives")["role"] = "Coretime".into(),
		field: "the chain roles",
	},
	PinnedChange {
		make: |copy| test_chain(copy, "People")["genesisHash"] = hex_of(0xa9, 32),
		field: "a chain's genesisHash",
	},
	PinnedChange {
		make: |copy| test_chain(copy, "Collectives")["genesisHash"] = hex_of(0xa9, 32),
		field: "a chain's genesisHash",
	},
	PinnedChange {
		make: |copy| {
			let people = test_chain(copy, "People")["genesisHash"].take();
			let bulletin = test_chain(copy, "Bulletin")["genesisHash"].take();
			test_chain(copy, "People")["genesisHash"] = bulletin;
			test_chain(copy, "Bulletin")["genesisHash"] = people;
		},
		field: "a chain's genesisHash",
	},
	PinnedChange { make: |copy| copy["assets"][0]["id"] = 1.into(), field: "assets" },
	PinnedChange { make: |copy| copy["assets"][0]["chain"] = "People".into(), field: "assets" },
	PinnedChange {
		make: |copy| copy["assets"][0]["pallet"] = "ForeignAssets".into(),
		field: "assets",
	},
	PinnedChange { make: |copy| copy["assets"][1]["role"] = "tip".into(), field: "assets" },
	PinnedChange { make: |copy| copy["assets"] = serde_json::json!([]), field: "assets" },
	PinnedChange {
		make: |copy| copy["contracts"]["multicall"] = hex_of(0xee, 20),
		field: "contracts",
	},
	PinnedChange { make: |copy| copy["contracts"] = serde_json::json!({}), field: "contracts" },
	PinnedChange { make: |copy| copy["contracts"]["extra"] = hex_of(0xee, 20), field: "contracts" },
	PinnedChange { make: |copy| copy["dotns"]["popController"] = hex_of(0xee, 20), field: "dotns" },
	PinnedChange { make: |copy| remove(copy, "dotns"), field: "dotns" },
	PinnedChange { make: |copy| copy["coinage"]["instance"] = 1.into(), field: "coinage" },
	PinnedChange { make: |copy| copy["coinage"]["chain"] = "AssetHub".into(), field: "coinage" },
	PinnedChange {
		make: |copy| test_chain(copy, "Relay")["lightSpec"] = test_light_spec(0xee),
		field: "the relay's lightSpec.sha256",
	},
	PinnedChange {
		make: |copy| remove(test_chain(copy, "Relay"), "lightSpec"),
		field: "the relay's lightSpec.sha256",
	},
	PinnedChange {
		make: |copy| {
			test_chain(copy, "Relay")["lightSpec"] = test_light_spec(0xee);
			test_chain(copy, "Collectives")["lightSpec"] = test_light_spec(0x5a);
		},
		field: "the relay's lightSpec.sha256",
	},
];

/// The TLD is pinned because accounts derive under it. Anything else may
/// change, so a stack can follow a wipe without a new build.
#[test]
fn a_copy_passes_only_on_the_bundled_tld() {
	let bundled = published_test_descriptor(1, "first", TEST_STACK_PEOPLE);
	let wiped = published_test_descriptor(2, "second", TEST_STACK_WIPED_PEOPLE);
	let mut on_another_tld = wiped.clone();
	on_another_tld["tld"] = "testnet".into();
	let pinned = Pinned::new(descriptor(&bundled));
	let unreserved = ReservedValuesPolicy::default();

	assert_eq!(
		[&bundled, &wiped, &on_another_tld]
			.map(|copy| pinned.refuse_change(&descriptor(copy), &unreserved)),
		[Ok(()), Ok(()), Err(StackError::PinnedFieldChanged { field: "tld" })]
	);
}

/// A key that signs the descriptor of a stack allowed reserved values could
/// otherwise have every host read that the stack runs on other chains, takes
/// another payment asset or has other contracts. The build's copy decides
/// what the descriptor names for those, so a copy that changes one is refused
/// and names it.
#[test]
fn a_stack_allowed_reserved_values_pins_its_chains_assets_and_contracts() {
	let pinned = Pinned::new(descriptor(&reserved_test_descriptor(1)));
	let allowed = ReservedValuesPolicy::ALLOWED;

	assert_eq!(
		PINNED_FIELD_CHANGES.map(|change| pinned.refuse_change(&changed(change.make), &allowed)),
		PINNED_FIELD_CHANGES
			.map(|change| Err(StackError::PinnedFieldChanged { field: change.field }))
	);
}

/// A relay light spec carries the checkpoint a light client starts from. A
/// bundled copy that names none pins that too: a newer copy cannot hand the
/// light client a checkpoint the build never saw.
#[test]
fn a_relay_light_spec_the_bundled_copy_does_not_name_is_refused() {
	let mut bundled = reserved_test_descriptor(1);
	remove(test_chain(&mut bundled, "Relay"), "lightSpec");
	let pinned = Pinned::new(descriptor(&bundled));

	assert_eq!(
		pinned.refuse_change(
			&descriptor(&reserved_test_descriptor(2)),
			&ReservedValuesPolicy::ALLOWED
		),
		Err(StackError::PinnedFieldChanged { field: "the relay's lightSpec.sha256" })
	);
}

/// Any other stack follows its operator: a wipe, a redeployed contract or a
/// new payment asset reaches its hosts without a new build. Only the TLD
/// stays.
#[test]
fn a_stack_without_reserved_values_pins_only_its_tld() {
	let pinned = Pinned::new(descriptor(&reserved_test_descriptor(1)));
	let unreserved = ReservedValuesPolicy::default();

	assert_eq!(
		PINNED_FIELD_CHANGES.map(|change| pinned.refuse_change(&changed(change.make), &unreserved)),
		PINNED_FIELD_CHANGES.map(|change| match change.field {
			"tld" => Err(StackError::PinnedFieldChanged { field: "tld" }),
			_ => Ok(()),
		})
	);
}

/// The operator of a stack allowed reserved values still republishes, moves
/// endpoints and names a new `generation` without a new build, as long as no
/// pinned field changes. Listing chains or assets in another order changes
/// none.
#[test]
fn a_stack_allowed_reserved_values_follows_every_other_change() {
	let pinned = Pinned::new(descriptor(&reserved_test_descriptor(1)));
	let allowed = ReservedValuesPolicy::ALLOWED;
	let unpinned_changes: [fn(&mut serde_json::Value); 10] = [
		|_unchanged| {},
		|copy| chains(copy).reverse(),
		|copy| copy["assets"].as_array_mut().expect("assets is an array").reverse(),
		|copy| copy["generation"] = "second".into(),
		|copy| copy["network"] = "polkadot-test".into(),
		|copy| copy["validUntil"] = 1_900_000_000.into(),
		|copy| test_chain(copy, "People")["rpc"] = serde_json::json!(["wss://moved.test.invalid"]),
		|copy| {
			test_chain(copy, "Relay")["lightSpec"]["url"] =
				"https://moved.test.invalid/relay.json".into()
		},
		|copy| test_chain(copy, "People")["lightSpec"] = test_light_spec(0xee),
		|copy| test_chain(copy, "Collectives")["lightSpec"] = test_light_spec(0xee),
	];

	assert_eq!(
		unpinned_changes.map(|change| pinned.refuse_change(&changed(change), &allowed)),
		unpinned_changes.map(|_| Ok(()))
	);
}

/// A publisher may write `[]` or `{}` for a field the bundled copy leaves
/// out, or leave out one it writes empty. Neither names anything, so a stack
/// allowed reserved values takes the copy either way.
#[test]
fn an_empty_list_or_map_pins_the_same_as_one_left_out() {
	let mut left_out = reserved_test_descriptor(1);
	for field in ["assets", "contracts", "dotns"] {
		remove(&mut left_out, field);
	}
	let mut written_empty = reserved_test_descriptor(1);
	written_empty["assets"] = serde_json::json!([]);
	written_empty["contracts"] = serde_json::json!({});
	written_empty["dotns"] = serde_json::json!({});
	let allowed = ReservedValuesPolicy::ALLOWED;
	let refusal = |bundled: &serde_json::Value, copy: &serde_json::Value| {
		Pinned::new(descriptor(bundled)).refuse_change(&descriptor(copy), &allowed)
	};

	assert_eq!(
		[refusal(&left_out, &written_empty), refusal(&written_empty, &left_out)],
		[Ok(()), Ok(())]
	);
}

/// A field the bundled copy leaves out or writes empty is pinned as it is: a
/// copy that fills it in names assets, contracts or a Coinage instance the
/// build never saw, and is refused naming the field.
#[test]
fn a_field_the_bundled_copy_leaves_out_or_empties_stays_empty() {
	let filled = descriptor(&reserved_test_descriptor(2));
	let bundles: [(fn(&mut serde_json::Value), &str); 7] = [
		(|bundled| remove(bundled, "assets"), "assets"),
		(|bundled| bundled["assets"] = serde_json::json!([]), "assets"),
		(|bundled| remove(bundled, "contracts"), "contracts"),
		(|bundled| bundled["contracts"] = serde_json::json!({}), "contracts"),
		(|bundled| remove(bundled, "dotns"), "dotns"),
		(|bundled| bundled["dotns"] = serde_json::json!({}), "dotns"),
		(|bundled| remove(bundled, "coinage"), "coinage"),
	];
	let refusal = |empty: fn(&mut serde_json::Value)| {
		let mut bundled = reserved_test_descriptor(1);
		empty(&mut bundled);
		Pinned::new(descriptor(&bundled)).refuse_change(&filled, &ReservedValuesPolicy::ALLOWED)
	};

	assert_eq!(
		bundles.map(|(empty, _)| refusal(empty)),
		bundles.map(|(_, field)| Err(StackError::PinnedFieldChanged { field }))
	);
}
