use super::*;
use crate::test_support::{StubPlatform, paseo_test_stack};

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
	let store = RingVrfRegistryStore::new(platform.clone(), paseo_test_stack());
	let first_root = [1; 32];
	let second_root = [2; 32];
	let key = handle("owner.dot", 7);
	futures::executor::block_on(store.register(first_root, key.clone(), ring(1), [9; 32])).unwrap();
	futures::executor::block_on(store.register(first_root, key.clone(), ring(1), [9; 32])).unwrap();
	futures::executor::block_on(store.register(first_root, key.clone(), ring(2), [9; 32])).unwrap();

	let entry = futures::executor::block_on(store.entry(first_root, &key)).unwrap().unwrap();
	assert_eq!(entry.rings, vec![ring(1), ring(2)]);
	assert!(futures::executor::block_on(store.entry(second_root, &key)).unwrap().is_none());

	let reloaded = RingVrfRegistryStore::new(platform, paseo_test_stack());
	assert_eq!(futures::executor::block_on(reloaded.entry(first_root, &key)).unwrap(), Some(entry));
}

#[test]
fn first_registrar_remains_selected_until_explicitly_changed() {
	let store = RingVrfRegistryStore::new(Arc::new(StubPlatform::default()), paseo_test_stack());
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
	let store = RingVrfRegistryStore::new(Arc::new(StubPlatform::default()), paseo_test_stack());
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
	let store = RingVrfRegistryStore::new(Arc::new(StubPlatform::default()), paseo_test_stack());
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
	let store = RingVrfRegistryStore::new(Arc::new(StubPlatform::default()), paseo_test_stack());
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
