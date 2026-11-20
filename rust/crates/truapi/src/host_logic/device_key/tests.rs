use super::*;
use crate::{
	platform::Platform,
	test_support::{MemoryCoreStorage, StubPlatform, paseo_test_stack, test_storage},
};
use std::sync::Arc;

#[test]
fn secret_is_generated_once_and_then_reused() {
	let storage: Arc<dyn Platform> = Arc::new(StubPlatform::default());

	let first = futures::executor::block_on(read_or_create_device_encryption_secret(
		&test_storage(storage.as_ref()),
	))
	.unwrap();
	let second = futures::executor::block_on(read_or_create_device_encryption_secret(
		&test_storage(storage.as_ref()),
	))
	.unwrap();

	// Peers address this device by the matching public key, so regenerating
	// it would strand everyone still holding the old one.
	assert_eq!(first, second);
}

/// Peers hold the public half of the key a device advertised. A device that
/// stored its key before keys included a stack id has to keep using it, or
/// everyone paired with it is addressing a key it no longer has.
#[test]
fn a_secret_stored_with_no_stack_id_is_kept() {
	let host = MemoryCoreStorage::default();
	host.store_unscoped(CoreStorageKey::DeviceEncryptionKey, vec![7; 32]);

	let stack = paseo_test_stack();

	let secret =
		futures::executor::block_on(read_or_create_device_encryption_secret(&stack.storage(&host)));

	assert_eq!(secret, Ok([7; 32]));
}

#[test]
fn a_stored_secret_replaces_the_generated_one() {
	let storage: Arc<dyn Platform> = Arc::new(StubPlatform::default());
	futures::executor::block_on(read_or_create_device_encryption_secret(&test_storage(
		storage.as_ref(),
	)))
	.unwrap();

	futures::executor::block_on(store_device_encryption_secret(
		&test_storage(storage.as_ref()),
		[7; 32],
	))
	.unwrap();

	// A host that announced a key of its own before the core kept one hands
	// it over; reading back anything else strands the peers addressing it.
	assert_eq!(
		futures::executor::block_on(read_or_create_device_encryption_secret(&test_storage(
			storage.as_ref(),
		))),
		Ok([7; 32])
	);
}
