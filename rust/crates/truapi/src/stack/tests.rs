use super::*;
use crate::test_support::{sign_test_descriptor, test_descriptor, test_descriptor_chain};

const POLKADOT_PEOPLE: [u8; 32] = [
	0x67, 0xfa, 0x17, 0x7a, 0x09, 0x7b, 0xfa, 0x18, 0xf7, 0x7e, 0xa9, 0x5a, 0xb5, 0x6e, 0x9b, 0xcd,
	0xfe, 0xb0, 0xe5, 0xb8, 0xa4, 0x0e, 0x46, 0x29, 0x8b, 0xb9, 0x3e, 0x16, 0xb6, 0xfc, 0x50, 0x08,
];

fn bundled(
	descriptor: &serde_json::Value,
	allows_reserved_values: bool,
) -> Result<Stack, StackError> {
	let (pointer, bytes) = sign_test_descriptor(descriptor);
	Stack::bundled(&pointer, &bytes, allows_reserved_values)
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
	let stack = bundled(&test_descriptor("paseo", [0xa1; 32], [0xb2; 32], None), false)
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

/// A host refreshes its stack unseen, so a pointer with no key to check a
/// refresh against is refused even when the bundled copy is signed.
#[test]
fn a_pointer_without_a_key_is_refused() {
	let (_, bytes) = sign_test_descriptor(&test_descriptor("paseo", [0xa1; 32], [0xb2; 32], None));
	let pointer = "polkadotapp://stack?url=https%3A%2F%2Fstack.test.invalid%2Fstack.json";

	assert_eq!(Stack::bundled(pointer, &bytes, false), Err(StackError::Unsigned));
}

/// The pointer's key is the trust anchor: a copy signed by anyone else does
/// not start, whatever it claims.
#[test]
fn a_copy_signed_by_another_key_is_refused() {
	let (_, bytes) = sign_test_descriptor(&test_descriptor("paseo", [0xa1; 32], [0xb2; 32], None));
	let other_key = format!("0x{}", hex::encode(trinity_stack::public_key(&[0x42; 32])));
	let pointer = Pointer::new("https://stack.test.invalid/stack.json", Some(&other_key))
		.expect("pointer is valid")
		.to_link(trinity_stack::DEFAULT_LINK_BASE);

	let refused = Stack::bundled(&pointer, &bytes, false).expect_err("another key's copy");
	assert!(
		matches!(&refused, StackError::Descriptor(error) if error.code() == "bad-signature"),
		"{refused:?}"
	);
}

/// SSO runs over People and in-core preimages go to Bulletin, so a stack
/// without either cannot start a host.
#[test]
fn a_stack_without_people_or_bulletin_is_refused() {
	let mut descriptor = test_descriptor("paseo", [0xa1; 32], [0xb2; 32], None);
	descriptor["chains"] = serde_json::json!([
		test_descriptor_chain("Relay", [0x01; 32], None),
		test_descriptor_chain("People", [0xa1; 32], Some(1004))
	]);

	assert_eq!(bundled(&descriptor, false), Err(StackError::MissingChain { role: "Bulletin" }));
}

/// Products are named under the TLDs the core serves dotNS for. Until that
/// list comes from the stack, a stack on any other TLD cannot host products.
#[test]
fn a_tld_the_core_does_not_serve_is_refused() {
	let chains =
		|| StackChains { relay: None, asset_hub: None, people: [0xa1; 32], bulletin: [0xb2; 32] };

	assert_eq!(
		["kusama", "", ".paseo", "Paseo"].map(|tld| Stack::new(
			"paseo".into(),
			tld.into(),
			chains(),
			true
		)
		.err()),
		["kusama", "", ".paseo", "Paseo"]
			.map(|tld| Some(StackError::UnsupportedTld { tld: tld.to_string() })),
	);
	assert_eq!(
		bundled(&test_descriptor("kusama", [0xa1; 32], [0xb2; 32], None), false),
		Err(StackError::UnsupportedTld { tld: "kusama".to_string() })
	);
}

/// Only a build that vouches for a stack may give it Polkadot's names: the
/// `dot` TLD, or a Polkadot genesis hash under any role, including ones the
/// core never reads.
#[test]
fn reserved_values_need_the_builds_permission() {
	let on_dot = test_descriptor("dot", [0xa1; 32], [0xb2; 32], None);
	let mut polkadot_people_as_another_role =
		test_descriptor("paseo", [0xa1; 32], [0xb2; 32], None);
	polkadot_people_as_another_role["chains"]
		.as_array_mut()
		.expect("chains is an array")
		.push(test_descriptor_chain("Collectives", POLKADOT_PEOPLE, Some(1001)));
	let polkadot_people = format!("genesis hash 0x{}", hex::encode(POLKADOT_PEOPLE));

	assert_eq!(
		[&on_dot, &polkadot_people_as_another_role].map(|descriptor| {
			(bundled(descriptor, false).err(), bundled(descriptor, true).is_ok())
		}),
		[
			(Some(StackError::Reserved { value: "tld dot".to_string() }), true),
			(Some(StackError::Reserved { value: polkadot_people }), true),
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
			Stack::new("polkadot".into(), "paseo".into(), chains.clone(), false).err(),
			Stack::new("polkadot".into(), "paseo".into(), chains, true).is_ok(),
		),
		(
			Some(StackError::Reserved {
				value: format!("genesis hash 0x{}", hex::encode(POLKADOT_PEOPLE))
			}),
			true,
		)
	);
}
