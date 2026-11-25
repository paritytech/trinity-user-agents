use super::*;
use crate::test_support::{
	sign_test_descriptor, test_descriptor, test_descriptor_chain, test_stack_link,
};

const POLKADOT_PEOPLE: [u8; 32] = [
	0x67, 0xfa, 0x17, 0x7a, 0x09, 0x7b, 0xfa, 0x18, 0xf7, 0x7e, 0xa9, 0x5a, 0xb5, 0x6e, 0x9b, 0xcd,
	0xfe, 0xb0, 0xe5, 0xb8, 0xa4, 0x0e, 0x46, 0x29, 0x8b, 0xb9, 0x3e, 0x16, 0xb6, 0xfc, 0x50, 0x08,
];

const POLKADOT_ASSET_HUB: [u8; 32] = [
	0x68, 0xd5, 0x6f, 0x15, 0xf8, 0x5d, 0x31, 0x36, 0x97, 0x0e, 0xc1, 0x69, 0x46, 0x04, 0x0b, 0xc1,
	0x75, 0x26, 0x54, 0xe9, 0x06, 0x14, 0x7f, 0x7e, 0x43, 0xe9, 0xd5, 0x39, 0xd7, 0xc3, 0xde, 0x2f,
];

fn bundled(
	descriptor: &serde_json::Value,
	allows_reserved_values: bool,
) -> Result<Stack, StackError> {
	let (pointer, bytes) = sign_test_descriptor(descriptor);
	Stack::bundled(&pointer, &bytes, allows_reserved_values)
}

fn identity() -> StackIdentity {
	StackIdentity { id: "stack".to_string(), generation: "generation".to_string() }
}

fn info(
	stack: &Stack,
	chain: ChainIdentifier,
) -> Result<RemoteChainInfoResponse, RemoteChainInfoError> {
	stack.chain_info(&RemoteChainInfoRequest { chain })
}

/// `get_chain_info` answers from the descriptor, so every host reports the
/// same chains and network for a stack.
#[test]
fn a_bundled_stack_answers_get_chain_info_from_its_descriptor() {
	let stack = bundled(&test_descriptor("paseo", [0xa1; 32], [0xb2; 32], [0xc3; 32]), false)
		.expect("stack is valid");

	let response = |chain, genesis_hash| {
		Ok(RemoteChainInfoResponse { network: "paseo".to_string(), chain, genesis_hash })
	};
	assert_eq!(
		[
			ChainIdentifier::Relay,
			ChainIdentifier::People,
			ChainIdentifier::Bulletin,
			ChainIdentifier::AssetHub
		]
		.map(|chain| info(&stack, chain)),
		[
			response(ChainIdentifier::Relay, [0x01; 32]),
			response(ChainIdentifier::People, [0xa1; 32]),
			response(ChainIdentifier::Bulletin, [0xb2; 32]),
			response(ChainIdentifier::AssetHub, [0xc3; 32]),
		]
	);
}

/// Only a stack link names a stack, so any other pointer is refused.
#[test]
fn a_pointer_that_is_not_a_stack_link_is_refused() {
	let (_, bytes) =
		sign_test_descriptor(&test_descriptor("paseo", [0xa1; 32], [0xb2; 32], [0xc3; 32]));

	let refused = Stack::bundled("https://example.invalid", &bytes, false);
	assert!(matches!(refused, Err(StackError::Pointer(_))), "{refused:?}");
}

/// A later copy is checked against the pointer's key, so a pointer without
/// one is refused even when the bundled copy is signed.
#[test]
fn a_pointer_without_a_key_is_refused() {
	let (_, bytes) =
		sign_test_descriptor(&test_descriptor("paseo", [0xa1; 32], [0xb2; 32], [0xc3; 32]));

	assert_eq!(Stack::bundled(&test_stack_link(None), &bytes, false), Err(StackError::Unsigned));
}

/// The pointer's key is the trust anchor: a copy signed by any other key does
/// not start.
#[test]
fn a_copy_signed_by_another_key_is_refused() {
	let (_, bytes) =
		sign_test_descriptor(&test_descriptor("paseo", [0xa1; 32], [0xb2; 32], [0xc3; 32]));
	let other_key = test_stack_link(Some(&[0x42; 32]));

	let refused = Stack::bundled(&other_key, &bytes, false).expect_err("another key's copy");
	assert!(
		matches!(&refused, StackError::Descriptor(error) if error.code() == "bad-signature"),
		"{refused:?}"
	);
}

/// dotNS is read from Asset Hub, SSO runs over People and in-core preimages go
/// to Bulletin, so a stack without any one of them cannot start a host.
#[test]
fn a_stack_without_asset_hub_people_or_bulletin_is_refused() {
	let without = |role: &str| {
		let mut descriptor = test_descriptor("paseo", [0xa1; 32], [0xb2; 32], [0xc3; 32]);
		descriptor["chains"]
			.as_array_mut()
			.expect("chains is an array")
			.retain(|chain| chain["role"] != role);
		bundled(&descriptor, false)
	};

	assert_eq!(
		[without("AssetHub"), without("People"), without("Bulletin")],
		[ChainIdentifier::AssetHub, ChainIdentifier::People, ChainIdentifier::Bulletin]
			.map(|chain| Err(StackError::MissingChain { chain })),
	);
}

/// Products are named under the TLDs the core serves dotNS for. Until that
/// list comes from the stack, a stack on any other TLD cannot host products.
#[test]
fn a_tld_the_core_does_not_serve_is_refused() {
	let chains = StackChains {
		relay: None,
		asset_hub: [0xc3; 32],
		people: [0xa1; 32],
		bulletin: [0xb2; 32],
	};
	let tlds = ["kusama", "", ".paseo", "Paseo"];

	assert_eq!(
		tlds.map(|tld| Stack::new(identity(), "paseo".into(), tld.into(), chains.clone(), 0, true)),
		tlds.map(|tld| Err(StackError::UnsupportedTld { tld: tld.into() })),
	);
	assert_eq!(
		bundled(&test_descriptor("kusama", [0xa1; 32], [0xb2; 32], [0xc3; 32]), false),
		Err(StackError::UnsupportedTld { tld: "kusama".to_string() })
	);
}

/// Polkadot's values, the `dot` TLD or a Polkadot genesis hash under any role,
/// a role the core reads or one it never reads, start only in a build that
/// allows them.
#[test]
fn reserved_values_need_the_builds_permission() {
	let on_dot = test_descriptor("dot", [0xa1; 32], [0xb2; 32], [0xc3; 32]);
	let on_polkadot_asset_hub =
		test_descriptor("paseo", [0xa1; 32], [0xb2; 32], POLKADOT_ASSET_HUB);
	let mut polkadot_people_as_another_role =
		test_descriptor("paseo", [0xa1; 32], [0xb2; 32], [0xc3; 32]);
	polkadot_people_as_another_role["chains"]
		.as_array_mut()
		.expect("chains is an array")
		.push(test_descriptor_chain("Collectives", POLKADOT_PEOPLE, Some(1001)));

	assert_eq!(
		[&on_dot, &on_polkadot_asset_hub, &polkadot_people_as_another_role].map(|descriptor| {
			(bundled(descriptor, false).err(), bundled(descriptor, true).is_ok())
		}),
		[
			(Some(StackError::ReservedTld), true),
			(Some(StackError::ReservedGenesis { genesis_hash: POLKADOT_ASSET_HUB }), true),
			(Some(StackError::ReservedGenesis { genesis_hash: POLKADOT_PEOPLE }), true),
		]
	);
}

/// A stack a Rust host resolves itself meets the same reserved-value rule.
#[test]
fn a_stack_from_parts_is_held_to_the_reserved_values() {
	let chains = StackChains {
		relay: None,
		asset_hub: [0xc3; 32],
		people: POLKADOT_PEOPLE,
		bulletin: [0xb2; 32],
	};

	assert_eq!(
		(
			Stack::new(identity(), "polkadot".into(), "paseo".into(), chains.clone(), 0, false)
				.err(),
			Stack::new(identity(), "polkadot".into(), "paseo".into(), chains, 0, true).is_ok(),
		),
		(Some(StackError::ReservedGenesis { genesis_hash: POLKADOT_PEOPLE }), true)
	);
}

/// An all-zero hash names no chain. On Asset Hub the core would wait on a
/// chain no host can dial, and a stack without a Relay leaves the role out.
#[test]
fn an_all_zero_relay_or_asset_hub_is_refused() {
	let stack = |relay, asset_hub| {
		let chains = StackChains { relay, asset_hub, people: [0xa1; 32], bulletin: [0xb2; 32] };
		Stack::new(identity(), "paseo".into(), "paseo".into(), chains, 0, true)
	};

	assert_eq!(
		[stack(Some([0; 32]), [0xc3; 32]), stack(None, [0; 32])],
		[ChainIdentifier::Relay, ChainIdentifier::AssetHub]
			.map(|chain| Err(StackError::ZeroGenesis { chain })),
	);
}

/// The id comes from the pointer's key and the generation from the
/// descriptor.
#[test]
fn a_bundled_stack_is_identified_by_its_key_and_its_descriptors_generation() {
	let descriptor = test_descriptor("paseo", [0xa1; 32], [0xb2; 32], [0xc3; 32]);
	let (link, _) = sign_test_descriptor(&descriptor);
	let stack = bundled(&descriptor, false).expect("stack is valid");

	assert_eq!(
		(stack.id(), stack.generation()),
		(
			Pointer::from_link(&link)
				.expect("test link is a stack link")
				.stack_id()
				.as_str(),
			descriptor["generation"].as_str().expect("test descriptor names a generation"),
		)
	);
}

/// An empty id or generation names no stack and no run of its chains, so
/// either is refused.
#[test]
fn a_stack_without_an_id_or_a_generation_is_refused() {
	let chains = StackChains {
		relay: None,
		asset_hub: [0xc3; 32],
		people: [0xa1; 32],
		bulletin: [0xb2; 32],
	};
	let stack = |id: &str, generation: &str| {
		let identity = StackIdentity { id: id.to_string(), generation: generation.to_string() };
		Stack::new(identity, "paseo".into(), "paseo".into(), chains.clone(), 0, false).err()
	};

	assert_eq!(
		(stack("", "generation"), stack("stack", "")),
		(
			Some(StackError::EmptyIdentity { field: "id" }),
			Some(StackError::EmptyIdentity { field: "generation" }),
		)
	);
}

/// A stack in `generation` over `chains`, read from a descriptor with
/// `sequence`.
fn stack_in_run(generation: &str, chains: StackChains, sequence: u32) -> Stack {
	let identity = StackIdentity { id: "stack".to_string(), generation: generation.to_string() };
	Stack::new(identity, "paseo".to_string(), "paseo".to_string(), chains, sequence, false)
		.expect("stack is valid")
}

/// The chains of the stamp tests.
fn stamp_test_chains() -> StackChains {
	StackChains { relay: None, asset_hub: [0xc3; 32], people: [0xa1; 32], bulletin: [0xb2; 32] }
}

/// A value stamped in one run is read back in that run only, whichever
/// descriptor `sequence` a runtime in it started from. A new `generation`, or
/// a new Asset Hub, People or Bulletin chain, is another run. The relay chain,
/// which the core never reads, is not part of the run, so a host that starts
/// or stops serving it keeps what the core stored. A value stamped before the
/// stamp carried the `sequence` is not read, like one stored without a run, so
/// the earlier layout cannot be misread as this one.
#[test]
fn a_stamped_value_is_read_back_in_its_own_run_only() {
	let chains = stamp_test_chains();
	let run = stack_in_run("g1", chains.clone(), 10).run();
	let stamped = run.stamp(b"value");
	let read_in = |generation: &str, chains: StackChains, sequence: u32| {
		stack_in_run(generation, chains, sequence)
			.run()
			.read_stamped(&stamped)
			.map(<[u8]>::to_vec)
	};
	let mut first_layout = vec![1];
	run.chains.encode_to(&mut first_layout);
	first_layout.extend_from_slice(b"value");

	assert_eq!(
		[
			read_in("g1", chains.clone(), 10),
			read_in("g1", chains.clone(), 11),
			read_in("g1", StackChains { relay: Some([0x01; 32]), ..chains.clone() }, 10),
			read_in("g2", chains.clone(), 10),
			read_in("g1", StackChains { asset_hub: [0xc4; 32], ..chains.clone() }, 10),
			read_in("g1", StackChains { people: [0xa2; 32], ..chains.clone() }, 10),
			read_in("g1", StackChains { bulletin: [0xb3; 32], ..chains }, 10),
			run.read_stamped(&first_layout).map(<[u8]>::to_vec),
		],
		[
			Some(b"value".to_vec()),
			Some(b"value".to_vec()),
			Some(b"value".to_vec()),
			None,
			None,
			None,
			None,
			None,
		]
	);
}

/// What one run stamped is stale in another run that came later by the
/// descriptor's `sequence`, and is replaced. A runtime started from an older
/// copy of the descriptor, as when the stored copy could not be read, finds
/// what the later run stamped and leaves it as it is. Two runs with one
/// `sequence`, which a signed stack never has, and a stamp in the earlier
/// layout or none, are stale as before.
#[test]
fn a_value_another_run_stamped_is_left_only_when_that_run_came_later() {
	let chains = stamp_test_chains();
	let reading = stack_in_run("g2", chains.clone(), 11).run();
	let stamped_in = |generation: &str, sequence: u32| {
		stack_in_run(generation, chains.clone(), sequence).run().stamp(b"value")
	};
	let held = |stamped: Vec<u8>| match reading.read(&stamped) {
		Stamped::ThisRun(value) => format!("this run: {}", String::from_utf8_lossy(value)),
		Stamped::LaterRun => "later run".to_string(),
		Stamped::Stale => "stale".to_string(),
	};
	let mut first_layout = vec![1];
	stack_in_run("g3", chains.clone(), 12).run().chains.encode_to(&mut first_layout);

	assert_eq!(
		[
			held(stamped_in("g2", 9)),
			held(stamped_in("g3", 12)),
			held(stamped_in("g1", 10)),
			held(stamped_in("g0", 11)),
			held(first_layout),
			held(b"value".to_vec()),
		],
		["this run: value", "later run", "stale", "stale", "stale", "stale"]
	);
}

/// A descriptor refresh the core takes while it runs, such as new RPC
/// endpoints, keeps the stack and so the run its stored values carry. One that
/// restarts the chains, by a new generation or a new genesis hash, waits for
/// the next start and is another run, so that start ignores what the old run
/// stored.
#[test]
fn a_refresh_keeps_the_run_unless_it_restarts_the_chains() {
	let base = test_descriptor("paseo", [0xa1; 32], [0xb2; 32], [0xc3; 32]);
	let edited = |edit: fn(&mut serde_json::Value)| {
		let mut descriptor = base.clone();
		edit(&mut descriptor);
		bundled(&descriptor, false).expect("stack is valid")
	};
	let running = bundled(&base, false).expect("stack is valid");
	let republished = |descriptor: &mut serde_json::Value| {
		let sequence = descriptor["sequence"].as_u64().expect("the descriptor has a sequence");
		descriptor["sequence"] = (sequence + 1).into();
	};
	let refreshed = [
		edited(|descriptor| {
			descriptor["chains"][1]["rpc"] = serde_json::json!(["wss://other.test.invalid"]);
		}),
		edited(republished),
		edited(|descriptor| descriptor["generation"] = "next".into()),
		edited(|descriptor| {
			descriptor["chains"][2]["genesisHash"] =
				format!("0x{}", hex::encode([0xa9; 32])).into();
		}),
	];

	assert_eq!(
		refreshed.map(|stack| (running.runs_as(&stack), stack.run() == running.run())),
		[(true, true), (true, true), (false, false), (false, false)]
	);
}

/// Installation recovery reads the registry at the stack's address, so a stack
/// that names none, or names the all-zero address, leaves it off rather than
/// reading the wrong contract.
#[test]
fn the_account_data_store_comes_from_the_descriptor() {
	let account_data_store = |contracts: Option<serde_json::Value>| {
		let mut descriptor = test_descriptor("paseo", [0xa1; 32], [0xb2; 32], [0xc3; 32]);
		if let Some(contracts) = contracts {
			descriptor["contracts"] = contracts;
		}
		bundled(&descriptor, false).expect("stack is valid").account_data_store()
	};
	let named = |address: String| Some(serde_json::json!({ "accountDataStore": address }));
	let other_contract = serde_json::json!({ "other": format!("0x{}", "22".repeat(20)) });

	assert_eq!(account_data_store(named(format!("0x{}", "22".repeat(20)))), Some([0x22; 20]));
	assert_eq!(account_data_store(named(format!("0x{}", "00".repeat(20)))), None);
	assert_eq!(account_data_store(Some(other_contract)), None);
	assert_eq!(account_data_store(None), None);
}
