use super::*;
use crate::test_support::{StubPlatform, paseo_test_stack, test_services, test_services_on};

fn handle(owner: &str, index: u32) -> ProductAccountId {
	ProductAccountId {
		dot_ns_identifier: ProductId::for_test(owner),
		derivation_index: truapi::v01::DerivationIndex::Index(index),
	}
}

/// A ring on the People chain of the [`paseo_test_stack`].
fn ring(byte: u8) -> RingLocation {
	RingLocation {
		chain_id: [0; 32],
		junctions: vec![truapi::v01::RingLocationJunction::PalletInstance(byte)],
	}
}

#[test]
fn registry_is_wallet_scoped_durable_and_registration_is_idempotent() {
	let platform = Arc::new(StubPlatform::default());
	let store = RingVrfRegistryStore::new(test_services(platform.clone()));
	let first_root = [1; 32];
	let second_root = [2; 32];
	let key = handle("owner.dot", 7);
	futures::executor::block_on(store.register(first_root, key.clone(), ring(1), [9; 32])).unwrap();
	futures::executor::block_on(store.register(first_root, key.clone(), ring(1), [9; 32])).unwrap();
	futures::executor::block_on(store.register(first_root, key.clone(), ring(2), [9; 32])).unwrap();

	let entry = futures::executor::block_on(store.entry(first_root, &key)).unwrap().unwrap();
	assert_eq!(entry.rings, vec![ring(1), ring(2)]);
	assert!(futures::executor::block_on(store.entry(second_root, &key)).unwrap().is_none());

	let reloaded = RingVrfRegistryStore::new(test_services(platform));
	assert_eq!(futures::executor::block_on(reloaded.entry(first_root, &key)).unwrap(), Some(entry));
}

/// The registry is the wallet's own record of its registrations, kept as an
/// install stored it, so a start on the same chains loses none of it. A wipe
/// replaces the People chain and the rings on it: an entry and a selection on
/// one of those is dropped, and the owner's listing is read again, while an
/// entry on a chain the stack still runs is kept.
#[test]
fn a_registry_keeps_what_is_on_the_chains_the_stack_still_runs() {
	let platform = Arc::new(StubPlatform::default());
	let (root, owner) = ([1; 32], ProductId::for_test("owner.dot"));
	let (on_people, on_bulletin) = (handle("owner.dot", 1), handle("owner.dot", 2));
	let people_ring = RingLocation { chain_id: [0; 32], ..ring(1) };
	let bulletin_ring = RingLocation { chain_id: [0xbb; 32], ..ring(2) };
	let stored = RegistrySnapshot {
		entries: vec![
			RegisteredRingVrfKey {
				handle: on_people.clone(),
				rings: vec![people_ring.clone()],
				public_key: Some([9; 32]),
			},
			RegisteredRingVrfKey {
				handle: on_bulletin.clone(),
				rings: vec![bulletin_ring.clone()],
				public_key: Some([8; 32]),
			},
		],
		complete_owners: vec![owner.clone()],
		selected_providers: vec![
			SelectedProvider { ring: people_ring.clone(), handle: on_people.clone() },
			SelectedProvider { ring: bulletin_ring, handle: on_bulletin.clone() },
		],
	};
	futures::executor::block_on(
		crate::test_support::test_storage(platform.as_ref())
			.write(CoreStorageKey::RingVrfRegistry { root_public_key: root }, stored.encode()),
	)
	.expect("stub core storage writes");

	let read = |stack| {
		let store = RingVrfRegistryStore::new(test_services_on(platform.clone(), stack));
		futures::executor::block_on(async {
			(
				store.entry(root, &on_people).await.unwrap().is_some(),
				store.entry(root, &on_bulletin).await.unwrap().is_some(),
				store.selected_provider(root, &people_ring).await.unwrap(),
				store.complete_owner_entries(root, &owner).await.unwrap().is_some(),
			)
		})
	};

	let wiped = crate::test_support::test_stack("paseo", [0x77; 32], [0xbb; 32], [0xcc; 32]);
	assert_eq!(
		[read(paseo_test_stack()), read(wiped)],
		[(true, true, Some(on_people), true), (false, true, None, false)]
	);
}

#[test]
fn first_registrar_remains_selected_until_explicitly_changed() {
	let store = RingVrfRegistryStore::new(test_services(Arc::new(StubPlatform::default())));
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
	let store = RingVrfRegistryStore::new(test_services(Arc::new(StubPlatform::default())));
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
	let store = RingVrfRegistryStore::new(test_services(Arc::new(StubPlatform::default())));
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
	let store = RingVrfRegistryStore::new(test_services(Arc::new(StubPlatform::default())));
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
