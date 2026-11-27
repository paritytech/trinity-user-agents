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
const PINNED_FIELD_CHANGES: [PinnedChange; 19] = [
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

/// A relay light spec carries the sync checkpoint a light client starts from,
/// and an operator publishes a fresher one from time to time. A stack allowed
/// reserved values runs the one its build ships, so a key that signs a newer
/// copy cannot put a checkpoint the build never saw in the descriptor the core
/// runs, and the rest of that copy is still taken. A copy that names none, or a bundled
/// copy that names none, leaves the relay with the bundled copy's.
#[test]
fn a_stack_allowed_reserved_values_keeps_its_bundled_relay_light_spec() {
	let bundled_spec = test_light_spec(0x5a);
	let fresher_spec = test_light_spec(0xee);
	let with_relay_spec = |sequence, light_spec: &Option<serde_json::Value>| {
		let mut copy = reserved_test_descriptor(sequence);
		test_chain(&mut copy, "People")["rpc"] = serde_json::json!(["wss://moved.test.invalid"]);
		let relay = test_chain(&mut copy, "Relay");
		match light_spec {
			Some(spec) => relay["lightSpec"] = spec.clone(),
			None => remove(relay, "lightSpec"),
		}
		descriptor(&copy)
	};
	let held = |bundled: &Option<serde_json::Value>, fetched: &Option<serde_json::Value>| {
		let pinned = Pinned::new(with_relay_spec(1, bundled));
		let copy = with_relay_spec(2, fetched);
		let allowed = ReservedValuesPolicy::ALLOWED;
		(
			pinned.refuse_change(&copy, &allowed),
			pinned.keep_bundled_relay_light_spec(copy, &allowed),
		)
	};
	let (bundled_spec, fresher_spec) = (Some(bundled_spec), Some(fresher_spec));

	assert_eq!(
		[
			held(&bundled_spec, &fresher_spec),
			held(&bundled_spec, &None),
			held(&None, &fresher_spec)
		],
		[
			(Ok(()), with_relay_spec(2, &bundled_spec)),
			(Ok(()), with_relay_spec(2, &bundled_spec)),
			(Ok(()), with_relay_spec(2, &None)),
		]
	);
}

/// A stack without reserved values runs each copy's relay light spec, so it
/// follows a fresher checkpoint without a new build.
#[test]
fn a_stack_without_reserved_values_runs_the_copys_relay_light_spec() {
	let pinned = Pinned::new(descriptor(&reserved_test_descriptor(1)));
	let mut fresher = reserved_test_descriptor(2);
	test_chain(&mut fresher, "Relay")["lightSpec"] = test_light_spec(0xee);

	assert_eq!(
		pinned
			.keep_bundled_relay_light_spec(descriptor(&fresher), &ReservedValuesPolicy::default()),
		descriptor(&fresher)
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
/// endpoints, publishes a fresher relay light spec and names a new
/// `generation` without a new build, as long as no
/// pinned field changes. Listing chains or assets in another order changes
/// none.
#[test]
fn a_stack_allowed_reserved_values_follows_every_other_change() {
	let pinned = Pinned::new(descriptor(&reserved_test_descriptor(1)));
	let allowed = ReservedValuesPolicy::ALLOWED;
	let unpinned_changes: [fn(&mut serde_json::Value); 11] = [
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
		|copy| test_chain(copy, "Relay")["lightSpec"] = test_light_spec(0xee),
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
	/// Each way the bundled copy can leave a field empty.
	const EMPTIED: [PinnedChange; 7] = [
		PinnedChange { make: |bundled| remove(bundled, "assets"), field: "assets" },
		PinnedChange { make: |bundled| bundled["assets"] = serde_json::json!([]), field: "assets" },
		PinnedChange { make: |bundled| remove(bundled, "contracts"), field: "contracts" },
		PinnedChange {
			make: |bundled| bundled["contracts"] = serde_json::json!({}),
			field: "contracts",
		},
		PinnedChange { make: |bundled| remove(bundled, "dotns"), field: "dotns" },
		PinnedChange { make: |bundled| bundled["dotns"] = serde_json::json!({}), field: "dotns" },
		PinnedChange { make: |bundled| remove(bundled, "coinage"), field: "coinage" },
	];
	let refusal = |empty: fn(&mut serde_json::Value)| {
		let mut bundled = reserved_test_descriptor(1);
		empty(&mut bundled);
		Pinned::new(descriptor(&bundled)).refuse_change(&filled, &ReservedValuesPolicy::ALLOWED)
	};

	assert_eq!(
		EMPTIED.map(|emptied| refusal(emptied.make)),
		EMPTIED.map(|emptied| Err(StackError::PinnedFieldChanged { field: emptied.field }))
	);
}
