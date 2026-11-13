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

/// A build that reserves two genesis hashes and the `testnet` TLD.
fn reserving_extras(allowed: bool) -> ReservedValuesPolicy {
	ReservedValuesPolicy {
		allowed,
		extra_genesis_hashes: vec![[0xc3; 32], [0xd4; 32]],
		extra_tlds: vec!["testnet".to_string()],
	}
}

/// A `paseo` descriptor that also names `genesis_hash` under a role the core
/// never reads.
fn with_an_unread_role(genesis_hash: [u8; 32]) -> serde_json::Value {
	let mut descriptor = test_descriptor("paseo", [0xa1; 32], [0xb2; 32], None);
	descriptor["chains"]
		.as_array_mut()
		.expect("chains is an array")
		.push(test_descriptor_chain("Collectives", genesis_hash, Some(1001)));
	descriptor
}

fn bundled(
	descriptor: &serde_json::Value,
	reserved_values: &ReservedValuesPolicy,
) -> Result<Stack, StackError> {
	let (pointer, bytes) = sign_test_descriptor(descriptor);
	Stack::bundled(&pointer, &bytes, reserved_values)
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
	let stack = bundled(
		&test_descriptor("paseo", [0xa1; 32], [0xb2; 32], None),
		&ReservedValuesPolicy::default(),
	)
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
			Err(RemoteChainInfoError::NotSupported),
		]
	);
}

/// Only a stack link names a stack, so any other pointer is refused.
#[test]
fn a_pointer_that_is_not_a_stack_link_is_refused() {
	let (_, bytes) = sign_test_descriptor(&test_descriptor("paseo", [0xa1; 32], [0xb2; 32], None));

	let refused =
		Stack::bundled("https://example.invalid", &bytes, &ReservedValuesPolicy::default());
	assert!(matches!(refused, Err(StackError::Pointer(_))), "{refused:?}");
}

/// A later copy is checked against the pointer's key, so a pointer without
/// one is refused even when the bundled copy is signed.
#[test]
fn a_pointer_without_a_key_is_refused() {
	let (_, bytes) = sign_test_descriptor(&test_descriptor("paseo", [0xa1; 32], [0xb2; 32], None));

	assert_eq!(
		Stack::bundled(&test_stack_link(None), &bytes, &ReservedValuesPolicy::default()),
		Err(StackError::Unsigned)
	);
}

/// The pointer's key is the trust anchor: a copy signed by any other key does
/// not start.
#[test]
fn a_copy_signed_by_another_key_is_refused() {
	let (_, bytes) = sign_test_descriptor(&test_descriptor("paseo", [0xa1; 32], [0xb2; 32], None));
	let other_key = test_stack_link(Some(&[0x42; 32]));

	let refused = Stack::bundled(&other_key, &bytes, &ReservedValuesPolicy::default())
		.expect_err("another key's copy");
	assert!(
		matches!(&refused, StackError::Descriptor(error) if error.code() == "bad-signature"),
		"{refused:?}"
	);
}

/// SSO runs over People and in-core preimages go to Bulletin, so a stack
/// without either cannot start a host.
#[test]
fn a_stack_without_people_or_bulletin_is_refused() {
	let without = |role: &str| {
		let mut descriptor = test_descriptor("paseo", [0xa1; 32], [0xb2; 32], None);
		descriptor["chains"]
			.as_array_mut()
			.expect("chains is an array")
			.retain(|chain| chain["role"] != role);
		bundled(&descriptor, &ReservedValuesPolicy::default())
	};

	assert_eq!(
		[without("People"), without("Bulletin")],
		[ChainIdentifier::People, ChainIdentifier::Bulletin]
			.map(|chain| Err(StackError::MissingChain { chain })),
	);
}

/// Products are named under the TLDs the core serves dotNS for. Until that
/// list comes from the stack, a stack on any other TLD cannot host products.
#[test]
fn a_tld_the_core_does_not_serve_is_refused() {
	let chains =
		StackChains { relay: None, asset_hub: None, people: [0xa1; 32], bulletin: [0xb2; 32] };
	let tlds = ["kusama", "", ".paseo", "Paseo"];

	assert_eq!(
		tlds.map(|tld| Stack::new(
			"paseo".into(),
			tld.into(),
			chains.clone(),
			&ReservedValuesPolicy::ALLOWED
		)),
		tlds.map(|tld| Err(StackError::UnsupportedTld { tld: tld.into() })),
	);
	assert_eq!(
		bundled(
			&test_descriptor("kusama", [0xa1; 32], [0xb2; 32], None),
			&ReservedValuesPolicy::default()
		),
		Err(StackError::UnsupportedTld { tld: "kusama".to_string() })
	);
}

/// Polkadot's values, the `dot` TLD or a Polkadot genesis hash under any role,
/// a role the core reads or one it never reads, start only in a build that
/// allows them.
#[test]
fn reserved_values_need_the_builds_permission() {
	let on_dot = test_descriptor("dot", [0xa1; 32], [0xb2; 32], None);
	let on_polkadot_asset_hub =
		test_descriptor("paseo", [0xa1; 32], [0xb2; 32], Some(POLKADOT_ASSET_HUB));
	let polkadot_people_as_another_role = with_an_unread_role(POLKADOT_PEOPLE);

	assert_eq!(
		[&on_dot, &on_polkadot_asset_hub, &polkadot_people_as_another_role].map(|descriptor| {
			(
				bundled(descriptor, &ReservedValuesPolicy::default()).err(),
				bundled(descriptor, &ReservedValuesPolicy::ALLOWED).is_ok(),
			)
		}),
		[
			(Some(StackError::ReservedTld { tld: "dot".to_string() }), true),
			(Some(StackError::ReservedGenesis { genesis_hash: POLKADOT_ASSET_HUB }), true),
			(Some(StackError::ReservedGenesis { genesis_hash: POLKADOT_PEOPLE }), true),
		]
	);
}

/// A stack a Rust host resolves itself meets the same reserved-value rule.
#[test]
fn a_stack_from_parts_is_held_to_the_reserved_values() {
	let chains =
		StackChains { relay: None, asset_hub: None, people: POLKADOT_PEOPLE, bulletin: [0xb2; 32] };

	assert_eq!(
		(
			Stack::new(
				"polkadot".into(),
				"paseo".into(),
				chains.clone(),
				&ReservedValuesPolicy::default()
			)
			.err(),
			Stack::new("polkadot".into(), "paseo".into(), chains, &ReservedValuesPolicy::ALLOWED)
				.is_ok(),
		),
		(Some(StackError::ReservedGenesis { genesis_hash: POLKADOT_PEOPLE }), true)
	);
}

/// A build reserves the values of a network it ships so that no other stack
/// passes for it. They are held to the rule Polkadot's are.
#[test]
fn a_builds_extra_reserved_values_need_its_permission() {
	let on_extra_tld = test_descriptor("testnet", [0xa1; 32], [0xb2; 32], None);
	let on_extra_asset_hub = test_descriptor("paseo", [0xa1; 32], [0xb2; 32], Some([0xc3; 32]));
	let extra_hash_as_another_role = with_an_unread_role([0xd4; 32]);

	assert_eq!(
		[&on_extra_tld, &on_extra_asset_hub, &extra_hash_as_another_role].map(|descriptor| {
			(
				bundled(descriptor, &ReservedValuesPolicy::default()).is_ok(),
				bundled(descriptor, &reserving_extras(false)).err(),
				bundled(descriptor, &reserving_extras(true)).is_ok(),
			)
		}),
		[
			(true, Some(StackError::ReservedTld { tld: "testnet".to_string() }), true),
			(true, Some(StackError::ReservedGenesis { genesis_hash: [0xc3; 32] }), true),
			(true, Some(StackError::ReservedGenesis { genesis_hash: [0xd4; 32] }), true),
		]
	);
}

/// A build can add to the core's reserved values and cannot replace them.
#[test]
fn a_builds_extra_reserved_values_leave_the_cores_in_force() {
	let on_dot = test_descriptor("dot", [0xa1; 32], [0xb2; 32], None);
	let on_polkadot_people = test_descriptor("paseo", POLKADOT_PEOPLE, [0xb2; 32], None);

	assert_eq!(
		[&on_dot, &on_polkadot_people]
			.map(|descriptor| bundled(descriptor, &reserving_extras(false))),
		[
			Err(StackError::ReservedTld { tld: "dot".to_string() }),
			Err(StackError::ReservedGenesis { genesis_hash: POLKADOT_PEOPLE }),
		]
	);
}

/// A reserved TLD is compared with a stack's as written, so an entry that is
/// not a TLD would reserve nothing. The build is told instead, whether or not
/// it allows its stack reserved values.
#[test]
fn an_extra_tld_that_is_not_a_tld_is_refused() {
	let descriptor = test_descriptor("paseo", [0xa1; 32], [0xb2; 32], None);
	let entries = [".testnet", "Testnet", "testnet ", ""];
	let reserving = |entry: &str, allowed| ReservedValuesPolicy {
		allowed,
		extra_tlds: vec!["testnet".to_string(), entry.to_string()],
		..ReservedValuesPolicy::default()
	};

	assert_eq!(
		entries.map(|entry| {
			[false, true].map(|allowed| bundled(&descriptor, &reserving(entry, allowed)))
		}),
		entries.map(|entry| {
			[false, true].map(|_| Err(StackError::MalformedReservedTld { tld: entry.to_string() }))
		}),
	);
}

/// An all-zero Asset Hub is not a way to say the stack has none: the core
/// would wait on a chain no host can dial. Such a stack leaves the role out.
#[test]
fn an_all_zero_genesis_hash_is_refused() {
	let chains = StackChains {
		relay: None,
		asset_hub: Some([0; 32]),
		people: [0xa1; 32],
		bulletin: [0xb2; 32],
	};

	assert_eq!(
		Stack::new("paseo".into(), "paseo".into(), chains, &ReservedValuesPolicy::ALLOWED),
		Err(StackError::ZeroGenesis { chain: ChainIdentifier::AssetHub })
	);
}
