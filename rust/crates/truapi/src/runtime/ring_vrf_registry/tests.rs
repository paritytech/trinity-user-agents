use super::*;
use crate::{
	platform::{CoreStorage, StackChains, StackIdentity},
	test_support::{StubPlatform, paseo_test_stack},
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

/// One key of `owner.dot` registered in a ring on each of `chains`, each ring
/// selected, with the owner's listing complete.
fn registered_on(chains: &[[u8; 32]]) -> RegistrySnapshot {
	let registrations = chains.iter().zip(1..).map(|(&chain_id, index)| {
		(handle("owner.dot", index), RingLocation { chain_id, ..ring(1) })
	});
	RegistrySnapshot {
		entries: registrations
			.clone()
			.map(|(handle, ring)| RegisteredRingVrfKey {
				handle,
				rings: vec![ring],
				public_key: Some([9; 32]),
			})
			.collect(),
		complete_owners: vec![ProductId::for_test("owner.dot")],
		selected_providers: registrations
			.map(|(handle, ring)| SelectedProvider { ring, handle })
			.collect(),
	}
}

/// What a registry on `stack` lists once it read `stored`: for each
/// registration of `registry`, whether its entry and the selection of its ring
/// are still there, and whether the owner's listing still counts as complete.
fn listed(
	stored: Vec<u8>,
	stack: &Stack,
	registry: &RegistrySnapshot,
) -> (Vec<(bool, bool)>, bool) {
	let platform = Arc::new(StubPlatform::default());
	let (root, owner) = ([1; 32], ProductId::for_test("owner.dot"));
	let key = CoreStorageKey::RingVrfRegistry { root_public_key: root };
	futures::executor::block_on(platform.write_core_storage(key, stored))
		.expect("stub core storage writes");
	let store = RingVrfRegistryStore::new(platform, stack);
	futures::executor::block_on(async {
		let mut registrations = Vec::new();
		for selection in &registry.selected_providers {
			registrations.push((
				store.entry(root, &selection.handle).await.unwrap().is_some(),
				store.selected_provider(root, &selection.ring).await.unwrap().is_some(),
			));
		}
		(registrations, store.complete_owner_entries(root, &owner).await.unwrap().is_some())
	})
}

/// A stack with this `generation` over People `people` and the Bulletin and
/// Asset Hub of the [`paseo_test_stack`].
fn stack_in(generation: &str, people: [u8; 32]) -> Stack {
	let paseo = paseo_test_stack();
	let identity = StackIdentity { id: paseo.id().to_string(), generation: generation.to_string() };
	let chains = StackChains { people, ..paseo.chains().clone() };
	Stack::new(identity, "paseo".to_string(), "paseo".to_string(), chains, true)
		.expect("test stack is valid")
}

/// The registry is the wallet's own record of its registrations, so a start
/// in the same run loses none of it. A wipe replaces chains and the rings on
/// them, and those registrations go, with their selections, and the owner's
/// listing is read again. A wipe that replaced only People leaves the ring on
/// Bulletin. One that the operator marked with a new `generation` restarted
/// every chain of the stack, also where a genesis hash stayed. A ring on a
/// chain the stack never listed is no wipe's to remove.
#[test]
fn a_registry_loses_only_the_rings_on_chains_a_wipe_replaced() {
	let paseo = paseo_test_stack();
	let (people, bulletin, elsewhere) =
		(paseo.chains().people, paseo.chains().bulletin, [0xee; 32]);
	let registry = registered_on(&[people, bulletin, elsewhere]);
	let stored = paseo.run().stamp(&registry.encode());
	let listed_on = |stack: &Stack| listed(stored.clone(), stack, &registry);
	let (kept, gone) = ((true, true), (false, false));

	assert_eq!(
		[
			listed_on(&paseo),
			listed_on(&stack_in(paseo.generation(), [0x77; 32])),
			listed_on(&stack_in("next", people)),
		],
		[
			(vec![kept, kept, kept], true),
			(vec![gone, kept, kept], false),
			(vec![gone, gone, kept], false),
		]
	);
}

/// A registry stored without a run does not say which chains it was written
/// on, so only the rings on this run's chains can be told from ones a wipe
/// removed, and the rest are dropped. It is stored again with the run, so
/// this happens once and the next start reads what this one kept.
#[test]
fn a_registry_stored_without_a_run_keeps_the_rings_on_this_runs_chains_once() {
	let paseo = paseo_test_stack();
	let (people, elsewhere) = (paseo.chains().people, [0xee; 32]);
	let registry = registered_on(&[people, elsewhere]);
	let platform = Arc::new(StubPlatform::default());
	let key = CoreStorageKey::RingVrfRegistry { root_public_key: [1; 32] };
	futures::executor::block_on(platform.write_core_storage(key.clone(), registry.encode()))
		.expect("stub core storage writes");

	let store = RingVrfRegistryStore::new(platform.clone(), &paseo);
	let dropped = &registry.selected_providers[1].handle;
	let first = futures::executor::block_on(store.entry([1; 32], dropped)).unwrap();
	let stored = futures::executor::block_on(platform.read_core_storage(key))
		.expect("stub core storage reads")
		.expect("the registry is stored");

	let mut on_people = registered_on(&[people]);
	on_people.complete_owners.clear();
	assert_eq!(
		(first, stored.clone(), listed(stored, &paseo, &registry)),
		(None, paseo.run().stamp(&on_people.encode()), (vec![(true, true), (false, false)], false))
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
