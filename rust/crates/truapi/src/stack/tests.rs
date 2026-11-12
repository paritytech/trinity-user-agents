use truapi::latest::GenericError;

use super::*;
use crate::{
	platform::{CoreStorageKey, ProductId, StackStorageKey},
	test_support::{
		MemoryCoreStorage, sign_test_descriptor, test_descriptor, test_descriptor_chain,
		test_stack_link,
	},
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

/// Only a stack link names a stack, so any other pointer is refused.
#[test]
fn a_pointer_that_is_not_a_stack_link_is_refused() {
	let (_, bytes) = sign_test_descriptor(&test_descriptor("paseo", [0xa1; 32], [0xb2; 32], None));

	let refused = Stack::bundled("https://example.invalid", &bytes, false);
	assert!(matches!(refused, Err(StackError::Pointer(_))), "{refused:?}");
}

/// A later copy is checked against the pointer's key, so a pointer without
/// one is refused even when the bundled copy is signed.
#[test]
fn a_pointer_without_a_key_is_refused() {
	let (_, bytes) = sign_test_descriptor(&test_descriptor("paseo", [0xa1; 32], [0xb2; 32], None));

	assert_eq!(Stack::bundled(&test_stack_link(None), &bytes, false), Err(StackError::Unsigned));
}

/// The pointer's key is the trust anchor: a copy signed by any other key does
/// not start.
#[test]
fn a_copy_signed_by_another_key_is_refused() {
	let (_, bytes) = sign_test_descriptor(&test_descriptor("paseo", [0xa1; 32], [0xb2; 32], None));
	let other_key = test_stack_link(Some(&[0x42; 32]));

	let refused = Stack::bundled(&other_key, &bytes, false).expect_err("another key's copy");
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
		bundled(&descriptor, false)
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
		tlds.map(|tld| Stack::new(identity(), "paseo".into(), tld.into(), chains.clone(), true)),
		tlds.map(|tld| Err(StackError::UnsupportedTld { tld: tld.into() })),
	);
	assert_eq!(
		bundled(&test_descriptor("kusama", [0xa1; 32], [0xb2; 32], None), false),
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
	let mut polkadot_people_as_another_role =
		test_descriptor("paseo", [0xa1; 32], [0xb2; 32], None);
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
	let chains =
		StackChains { relay: None, asset_hub: None, people: POLKADOT_PEOPLE, bulletin: [0xb2; 32] };

	assert_eq!(
		(
			Stack::new(identity(), "polkadot".into(), "paseo".into(), chains.clone(), false).err(),
			Stack::new(identity(), "polkadot".into(), "paseo".into(), chains, true).is_ok(),
		),
		(Some(StackError::ReservedGenesis { genesis_hash: POLKADOT_PEOPLE }), true)
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
		Stack::new(identity(), "paseo".into(), "paseo".into(), chains, true),
		Err(StackError::ZeroGenesis { chain: ChainIdentifier::AssetHub })
	);
}

/// One slot of each kind: the session, a permission, a cached manifest and
/// the device key.
fn slots() -> [CoreStorageKey; 4] {
	let product = ProductId::for_test("app.paseo");
	[
		CoreStorageKey::AuthSession,
		CoreStorageKey::identity_disclosure_authorization(&product),
		CoreStorageKey::product_manifest(&product),
		CoreStorageKey::DeviceEncryptionKey,
	]
}

async fn read_slots(storage: &StackStorage<'_, MemoryCoreStorage>) -> [Option<Vec<u8>>; 4] {
	let mut values = [const { None }; 4];
	for (value, slot) in values.iter_mut().zip(slots()) {
		*value = storage.read(slot).await.expect("memory storage reads");
	}
	values
}

/// The id comes from the pointer's key and the generation from the
/// descriptor.
#[test]
fn a_bundled_stack_is_identified_by_its_key_and_its_descriptors_generation() {
	let descriptor = test_descriptor("paseo", [0xa1; 32], [0xb2; 32], None);
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

/// A fork has the same genesis hashes as its source, so only the stack's id
/// separates their state. What is stored for the source must not be readable
/// on the fork.
#[test]
fn a_fork_and_its_source_share_no_stored_state() {
	let chains = StackChains {
		relay: None,
		asset_hub: Some([0xc3; 32]),
		people: [0xa1; 32],
		bulletin: [0xb2; 32],
	};
	let stack = |id: &str| {
		let identity = StackIdentity { id: id.to_string(), generation: "generation".to_string() };
		Stack::new(identity, "paseo".into(), "paseo".into(), chains.clone(), false)
			.expect("stack is valid")
	};
	let (source, fork) = (stack("source"), stack("fork"));
	let host = MemoryCoreStorage::default();

	futures::executor::block_on(async {
		let source = source.storage(&host);
		for slot in slots() {
			source.write(slot, b"source's".to_vec()).await.expect("memory storage writes");
		}
		let fork = fork.storage(&host);
		assert_eq!(read_slots(&fork).await, [const { None }; 4]);

		for slot in slots() {
			fork.clear(slot).await.expect("memory storage clears");
		}
		assert_eq!(read_slots(&source).await, slots().map(|_| Some(b"source's".to_vec())));
	});
}

/// The core keys stored state by the id and drops chain caches by the
/// generation, so a stack that names neither cannot be told from another.
#[test]
fn a_stack_without_an_id_or_a_generation_is_refused() {
	let chains =
		StackChains { relay: None, asset_hub: None, people: [0xa1; 32], bulletin: [0xb2; 32] };
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

/// [`MemoryCoreStorage`] whose writes or clears under a stack's key fail.
#[derive(Default)]
struct FailingStorage {
	memory: MemoryCoreStorage,
	fail_writes: bool,
	fail_clears: bool,
}

#[async_trait::async_trait]
impl CoreStorage for FailingStorage {
	async fn read_core_storage(
		&self,
		key: StackStorageKey,
	) -> Result<Option<Vec<u8>>, GenericError> {
		self.memory.read_core_storage(key).await
	}

	async fn write_core_storage(
		&self,
		key: StackStorageKey,
		value: Vec<u8>,
	) -> Result<(), GenericError> {
		if self.fail_writes {
			return Err(GenericError { reason: "write failed".to_string() });
		}
		self.memory.write_core_storage(key, value).await
	}

	async fn clear_core_storage(&self, key: StackStorageKey) -> Result<(), GenericError> {
		if self.fail_clears {
			return Err(GenericError { reason: "clear failed".to_string() });
		}
		self.memory.clear_core_storage(key).await
	}

	async fn read_unscoped_core_storage(
		&self,
		slot: CoreStorageKey,
	) -> Result<Option<Vec<u8>>, GenericError> {
		self.memory.read_unscoped_core_storage(slot).await
	}

	async fn clear_unscoped_core_storage(&self, slot: CoreStorageKey) -> Result<(), GenericError> {
		self.memory.clear_unscoped_core_storage(slot).await
	}
}

fn test_stack(id: &str) -> Stack {
	let chains =
		StackChains { relay: None, asset_hub: None, people: [0xa1; 32], bulletin: [0xb2; 32] };
	let identity = StackIdentity { id: id.to_string(), generation: "generation".to_string() };
	Stack::new(identity, "paseo".into(), "paseo".into(), chains, false).expect("stack is valid")
}

/// A wallet that updates must keep its replay ledger, its device key and its
/// session, all of which a host stored before keys included a stack id. The
/// stack that reads a slot first takes the value, and the old entry is erased
/// so a secret is not left behind and no other stack takes it too.
#[test]
fn a_value_stored_with_no_stack_id_moves_to_the_stack_that_reads_it() {
	let host = MemoryCoreStorage::default();
	for slot in slots() {
		host.store_unscoped(slot, b"stored before".to_vec());
	}
	let (first, second) = (test_stack("first"), test_stack("second"));

	futures::executor::block_on(async {
		let stored = slots().map(|_| Some(b"stored before".to_vec()));
		assert_eq!(read_slots(&first.storage(&host)).await, stored);
		assert_eq!(read_slots(&first.storage(&host)).await, stored);
		assert_eq!(read_slots(&second.storage(&host)).await, [const { None }; 4]);
		for slot in slots() {
			assert_eq!(host.read_unscoped_core_storage(slot).await, Ok(None));
		}
	});
}

/// Signing out clears the session slot. If only the stack's copy went, the
/// value still held under the slot alone would sign the user back in on the
/// next read. A failed clear of the stack's copy must not leave that either.
#[test]
fn a_cleared_slot_stays_cleared() {
	let slot = CoreStorageKey::AuthSession;
	let stack = test_stack("stack");

	futures::executor::block_on(async {
		let host = MemoryCoreStorage::default();
		host.store_unscoped(slot.clone(), b"session".to_vec());
		stack.storage(&host).clear(slot.clone()).await.expect("memory storage clears");
		assert_eq!(stack.storage(&host).read(slot.clone()).await, Ok(None));

		let failing = FailingStorage { fail_clears: true, ..Default::default() };
		failing.memory.store_unscoped(slot.clone(), b"session".to_vec());
		let cleared = stack.storage(&failing).clear(slot.clone()).await;
		assert_eq!(
			(cleared.is_err(), failing.read_unscoped_core_storage(slot).await),
			(true, Ok(None))
		);
	});
}

/// When the host cannot write the stack's copy, the value is still the
/// user's: it is returned, and it stays where it was for the next read to
/// move.
#[test]
fn a_value_that_cannot_be_moved_is_still_read() {
	let slot = CoreStorageKey::DeviceEncryptionKey;
	let host = FailingStorage { fail_writes: true, ..Default::default() };
	host.memory.store_unscoped(slot.clone(), b"device key".to_vec());
	let stack = test_stack("stack");

	futures::executor::block_on(async {
		let read = stack.storage(&host).read(slot.clone()).await;
		assert_eq!(
			(read, host.read_unscoped_core_storage(slot).await),
			(Ok(Some(b"device key".to_vec())), Ok(Some(b"device key".to_vec())))
		);
	});
}

/// A kill between writing the stack's copy and clearing the old entry leaves
/// both. The stack's copy is the one the core has written to since.
#[test]
fn the_stacks_copy_is_read_before_one_stored_with_no_stack_id() {
	let slot = CoreStorageKey::AuthSession;
	let host = MemoryCoreStorage::default();
	host.store_unscoped(slot.clone(), b"older".to_vec());
	let stack = test_stack("stack");

	futures::executor::block_on(async {
		let storage = stack.storage(&host);
		storage
			.write(slot.clone(), b"newer".to_vec())
			.await
			.expect("memory storage writes");
		assert_eq!(storage.read(slot).await, Ok(Some(b"newer".to_vec())));
	});
}
