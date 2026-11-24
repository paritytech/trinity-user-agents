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

/// A stack on `tld`, allowed reserved values so that it may be on `dot`.
fn on(tld: &str) -> Stack {
	crate::test_support::test_stack(tld, [0xa1; 32], [0xb2; 32], [0xc3; 32])
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

/// Product names end in the stack's TLD, so it has to be a label dotNS could
/// register.
#[test]
fn a_tld_that_is_not_a_dns_label_is_refused() {
	let chains = StackChains {
		relay: None,
		asset_hub: [0xc3; 32],
		people: [0xa1; 32],
		bulletin: [0xb2; 32],
	};
	let tlds = ["", ".paseo", "Paseo", "pa.seo", "-paseo"];

	assert_eq!(
		tlds.map(|tld| Stack::new(identity(), "paseo".into(), tld.into(), chains.clone(), true)),
		tlds.map(|tld| Err(StackError::InvalidTld { tld: tld.into() })),
	);
}

/// Share hosts end in `.li`, and every name under `localhost` resolves to the
/// local machine (RFC 6761), so a product under either TLD would not reach its
/// dotNS content. The error says which, whether the stack comes from a
/// descriptor or from parts.
#[test]
fn a_tld_whose_names_mean_something_else_is_refused() {
	let chains = StackChains {
		relay: None,
		asset_hub: [0xc3; 32],
		people: [0xa1; 32],
		bulletin: [0xb2; 32],
	};
	let refusal = |tld: &str| {
		let from_parts = Stack::new(identity(), "paseo".into(), tld.into(), chains.clone(), true);
		let bundled = bundled(&test_descriptor(tld, [0xa1; 32], [0xb2; 32], [0xc3; 32]), false);
		[from_parts.err(), bundled.err()].map(|error| error.map(|error| error.to_string()))
	};

	let li = Some(r#"stack tld "li" is reserved: share hosts end in .li"#.to_string());
	let localhost = Some(
		r#"stack tld "localhost" is reserved: every name under localhost resolves to the local machine"#
			.to_string(),
	);
	assert_eq!(
		[refusal("li"), refusal("localhost")],
		[[li.clone(), li], [localhost.clone(), localhost]]
	);
}

/// A fork with its own TLD runs products under it and under Polkadot's, and
/// under no third TLD.
#[test]
fn a_stack_serves_its_own_tld_and_the_reserved_one() {
	let chains = StackChains {
		relay: None,
		asset_hub: [0xc3; 32],
		people: [0xa1; 32],
		bulletin: [0xb2; 32],
	};
	let fork =
		Stack::new(identity(), "fork".into(), "fork".into(), chains, false).expect("fork starts");
	let served = |product_id: &str| fork.require_served(&ProductId::for_test(product_id)).is_ok();

	assert_eq!(
		["app.fork", "app.dot", "app.paseo", "app.testnet", "localhost:3000"].map(served),
		[true, true, false, false, true]
	);
	assert_eq!(
		bundled(&test_descriptor("fork", [0xa1; 32], [0xb2; 32], [0xc3; 32]), false)
			.map(|stack| stack.tld().to_owned()),
		Ok("fork".to_owned())
	);
}

/// A trusted label skips every prompt, so it counts only under a TLD the
/// stack serves: `peopl.com` parses, and must still prompt.
#[test]
fn a_trusted_label_is_trusted_only_under_a_served_tld() {
	let trusted = |stack_tld, product_id| on(stack_tld).trusts(&ProductId::for_test(product_id));

	assert_eq!(
		[
			trusted("paseo", "peopl.paseo"),
			trusted("paseo", "peopl.dot"),
			trusted("fork", "dim2.fork"),
			trusted("paseo", "peopl.testnet"),
			trusted("paseo", "peopl.com"),
			trusted("paseo", "app.peopl.paseo"),
		],
		[true, true, true, false, false, false]
	);
}

#[test]
fn the_reserved_tld_is_served_once() {
	assert_eq!(on("dot").served_tlds().collect::<Vec<_>>(), ["dot"]);
	assert_eq!(on("paseo").served_tlds().collect::<Vec<_>>(), ["dot", "paseo"]);
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
			Stack::new(identity(), "polkadot".into(), "paseo".into(), chains.clone(), false).err(),
			Stack::new(identity(), "polkadot".into(), "paseo".into(), chains, true).is_ok(),
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
		Stack::new(identity(), "paseo".into(), "paseo".into(), chains, true)
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
		Stack::new(identity, "paseo".into(), "paseo".into(), chains.clone(), false).err()
	};

	assert_eq!(
		(stack("", "generation"), stack("stack", "")),
		(
			Some(StackError::EmptyIdentity { field: "id" }),
			Some(StackError::EmptyIdentity { field: "generation" }),
		)
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
