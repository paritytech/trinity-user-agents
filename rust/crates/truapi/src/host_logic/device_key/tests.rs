use super::*;
use crate::{
	platform::Platform,
	test_support::{MemoryCoreStorage, StubPlatform, test_storage},
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

	let secret =
		futures::executor::block_on(read_or_create_device_encryption_secret(&test_storage(&host)));

	assert_eq!(secret, Ok([7; 32]));
}
