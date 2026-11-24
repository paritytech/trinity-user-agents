use super::*;
use crate::{
	platform::CoreStorage,
	test_support::{StubPlatform, paseo_test_stack, test_stack},
};

fn handle(owner: &str, index: u32) -> ProductAccountId {
	ProductAccountId {
		dot_ns_identifier: ProductId::for_test(owner),
		derivation_index: truapi::v01::DerivationIndex::Index(index),
	}
}

fn ring(byte: u8) -> RingLocation {
	RingLocation {
		chain_id: [byte; 32],
		junctions: vec![truapi::v01::RingLocationJunction::PalletInstance(byte)],
	}
}

#[test]
fn registry_is_wallet_scoped_durable_and_registration_is_idempotent() {
	let platform = Arc::new(StubPlatform::default());
	let store = RingVrfRegistryStore::new(platform.clone(), &paseo_test_stack());
	let first_root = [1; 32];
	let second_root = [2; 32];
	let key = handle("owner.dot", 7);
	futures::executor::block_on(store.register(first_root, key.clone(), ring(1), [9; 32])).unwrap();
	futures::executor::block_on(store.register(first_root, key.clone(), ring(1), [9; 32])).unwrap();
	futures::executor::block_on(store.register(first_root, key.clone(), ring(2), [9; 32])).unwrap();

	let entry = futures::executor::block_on(store.entry(first_root, &key)).unwrap().unwrap();
	assert_eq!(entry.rings, vec![ring(1), ring(2)]);
	assert!(futures::executor::block_on(store.entry(second_root, &key)).unwrap().is_none());

	let reloaded = RingVrfRegistryStore::new(platform, &paseo_test_stack());
	assert_eq!(futures::executor::block_on(reloaded.entry(first_root, &key)).unwrap(), Some(entry));
}

/// One key registered in a ring on each of `chains`, each ring selected, with
/// the owner's listing complete.
fn registered_on(chains: [[u8; 32]; 3]) -> (RegistrySnapshot, [ProductAccountId; 3]) {
	let handles = [1, 2, 3].map(|index| handle("owner.dot", index));
	let rings = chains.map(|chain_id| RingLocation { chain_id, ..ring(1) });
	let snapshot = RegistrySnapshot {
		entries: (handles.iter().zip(&rings))
			.map(|(handle, ring)| RegisteredRingVrfKey {
				handle: handle.clone(),
				rings: vec![ring.clone()],
				public_key: Some([9; 32]),
			})
			.collect(),
		complete_owners: vec![ProductId::for_test("owner.dot")],
		selected_providers: (handles.iter().zip(rings))
			.map(|(handle, ring)| SelectedProvider { ring, handle: handle.clone() })
			.collect(),
	};
	(snapshot, handles)
}

/// Which of `handles` a registry on `stack` still lists after it read `stored`,
/// and whether it still takes the owner's listing for complete.
fn listed(stored: Vec<u8>, stack: &Stack, handles: &[ProductAccountId; 3]) -> ([bool; 3], bool) {
	let platform = Arc::new(StubPlatform::default());
	let (root, owner) = ([1; 32], ProductId::for_test("owner.dot"));
	let key = CoreStorageKey::RingVrfRegistry { root_public_key: root };
	futures::executor::block_on(platform.write_core_storage(key, stored))
		.expect("stub core storage writes");
	let store = RingVrfRegistryStore::new(platform, stack);
	futures::executor::block_on(async {
		let mut entries = [false; 3];
		for (entry, handle) in entries.iter_mut().zip(handles) {
			*entry = store.entry(root, handle).await.unwrap().is_some();
		}
		(entries, store.complete_owner_entries(root, &owner).await.unwrap().is_some())
	})
}

/// The registry is the wallet's own record of its registrations, so a start
/// on the same chains loses none of it. A wipe replaces the People chain and
/// the rings on it: the entry there is dropped and the owner's listing is
/// read again. An entry on a chain the stack still runs is kept, and so is
/// one on a chain the stack never listed, which no wipe of the stack touched.
#[test]
fn a_registry_loses_only_the_rings_on_chains_the_stack_no_longer_runs() {
	let (people, bulletin, elsewhere) = ([0; 32], [0xbb; 32], [0xee; 32]);
	let (registry, handles) = registered_on([people, bulletin, elsewhere]);
	let stored = paseo_test_stack().run().stamp(&registry.encode());
	let wiped = test_stack("paseo", [0x77; 32], bulletin, [0xcc; 32]);

	assert_eq!(
		[listed(stored.clone(), &paseo_test_stack(), &handles), listed(stored, &wiped, &handles)],
		[([true, true, true], true), ([false, true, true], false)]
	);
}

/// A registry stored before runs were recorded does not say which chains it
/// was written on, so only the rings on the stack's chains can be told from
/// ones a wipe removed, and the rest are dropped. It is stored again with the
/// run, so this happens once and the next start reads what this one kept.
#[test]
fn a_registry_stored_without_a_run_keeps_the_rings_on_the_stacks_chains_once() {
	let (people, elsewhere) = ([0; 32], [0xee; 32]);
	let (registry, handles) = registered_on([people, [0xbb; 32], elsewhere]);
	let platform = Arc::new(StubPlatform::default());
	let key = CoreStorageKey::RingVrfRegistry { root_public_key: [1; 32] };
	futures::executor::block_on(platform.write_core_storage(key.clone(), registry.encode()))
		.expect("stub core storage writes");

	let store = RingVrfRegistryStore::new(platform.clone(), &paseo_test_stack());
	let first = futures::executor::block_on(store.entry([1; 32], &handles[2])).unwrap();
	let stored = futures::executor::block_on(platform.read_core_storage(key))
		.expect("stub core storage reads")
		.expect("the registry is stored");

	let kept = registry.without_rings_on(|chain| chain == elsewhere);
	assert_eq!(
		(first, stored.clone(), listed(stored, &paseo_test_stack(), &handles)),
		(None, paseo_test_stack().run().stamp(&kept.encode()), ([true, true, false], false))
	);
}

#[test]
fn first_registrar_remains_selected_until_explicitly_changed() {
	let store = RingVrfRegistryStore::new(Arc::new(StubPlatform::default()), &paseo_test_stack());
	let root = [1; 32];
	let location = ring(1);
	let first = handle("first.dot", 0);
	let second = handle("second.dot", 0);
	futures::executor::block_on(store.register(root, first.clone(), location.clone(), [1; 32]))
		.unwrap();
	futures::executor::block_on(store.register(root, second.clone(), location.clone(), [2; 32]))
		.unwrap();
	assert_eq!(
		futures::executor::block_on(store.selected_provider(root, &location)).unwrap(),
		Some(first.clone())
	);
	assert_eq!(
		futures::executor::block_on(store.providers(root, &location)).unwrap(),
		vec![first.clone(), second.clone()]
	);
	futures::executor::block_on(store.select_provider(root, location.clone(), second.clone()))
		.unwrap();
	assert_eq!(
		futures::executor::block_on(store.selected_provider(root, &location)).unwrap(),
		Some(second)
	);
}

#[test]
fn registration_rejects_a_conflicting_public_key_for_the_same_handle() {
	let store = RingVrfRegistryStore::new(Arc::new(StubPlatform::default()), &paseo_test_stack());
	let root = [1; 32];
	let key = handle("owner.dot", 7);
	futures::executor::block_on(store.register(root, key.clone(), ring(1), [1; 32])).unwrap();

	let error = futures::executor::block_on(store.register(root, key.clone(), ring(2), [2; 32]))
		.unwrap_err();

	assert!(matches!(
		error,
		RingVrfError::Unknown { reason }
			if reason.contains("conflicting public key")
	));
	assert_eq!(
		futures::executor::block_on(store.entry(root, &key)).unwrap().unwrap().rings,
		vec![ring(1)]
	);
}

#[test]
fn stale_owner_snapshot_preserves_a_local_registration() {
	let store = RingVrfRegistryStore::new(Arc::new(StubPlatform::default()), &paseo_test_stack());
	let root = [1; 32];
	let local = handle("owner.dot", 7);
	let remote = RegisteredRingVrfKey {
		handle: handle("owner.dot", 8),
		rings: vec![ring(2)],
		public_key: Some([2; 32]),
	};
	futures::executor::block_on(store.register(root, local.clone(), ring(1), [1; 32])).unwrap();

	let reconciled = futures::executor::block_on(store.reconcile_owner(
		root,
		&ProductId::for_test("owner.dot"),
		vec![remote.clone()],
	))
	.unwrap();

	assert_eq!(
		reconciled,
		vec![
			RegisteredRingVrfKey { handle: local, rings: vec![ring(1)], public_key: Some([1; 32]) },
			remote,
		]
	);
	assert_eq!(
		futures::executor::block_on(
			store.complete_owner_entries(root, &ProductId::for_test("owner.dot"))
		)
		.unwrap(),
		Some(reconciled)
	);
}

#[test]
fn owner_snapshot_rejects_a_conflicting_local_registration() {
	let store = RingVrfRegistryStore::new(Arc::new(StubPlatform::default()), &paseo_test_stack());
	let root = [1; 32];
	let key = handle("owner.dot", 7);
	futures::executor::block_on(store.register(root, key.clone(), ring(1), [1; 32])).unwrap();

	let error = futures::executor::block_on(store.reconcile_owner(
		root,
		&ProductId::for_test("owner.dot"),
		vec![RegisteredRingVrfKey {
			handle: key.clone(),
			rings: vec![ring(1)],
			public_key: Some([2; 32]),
		}],
	))
	.unwrap_err();

	assert!(matches!(
		error,
		RingVrfError::Unknown { reason } if reason.contains("conflicts with a locally registered public key")
	));
	assert_eq!(
		futures::executor::block_on(store.entry(root, &key))
			.unwrap()
			.unwrap()
			.public_key,
		Some([1; 32])
	);
}

#[test]
fn persisted_entries_must_declare_at_least_one_ring() {
	let snapshot = RegistrySnapshot {
		entries: vec![RegisteredRingVrfKey {
			handle: handle("owner.dot", 0),
			rings: vec![],
			public_key: Some([1; 32]),
		}],
		..RegistrySnapshot::default()
	};

	assert!(matches!(
		decode_snapshot(&snapshot.encode()),
		Err(RingVrfError::Unknown { reason }) if reason.contains("no declared rings")
	));
}

#[test]
fn owner_listing_rejects_foreign_and_duplicate_entries() {
	let owner = ProductId::for_test("owner.dot");
	let entry = RegisteredRingVrfKey {
		handle: handle(owner.as_str(), 0),
		rings: vec![ring(1)],
		public_key: None,
	};
	assert!(validate_owner_listing(&owner, std::slice::from_ref(&entry)).is_ok());

	let mut foreign = entry.clone();
	foreign.handle = handle("foreign.dot", 0);
	assert!(matches!(
		validate_owner_listing(&owner, &[foreign]),
		Err(RingVrfError::Unknown { reason }) if reason.contains("foreign key handle")
	));
	assert!(matches!(
		validate_owner_listing(&owner, &[entry.clone(), entry]),
		Err(RingVrfError::Unknown { reason }) if reason.contains("duplicate key handle")
	));
}
