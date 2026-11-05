use futures::executor::block_on;

use super::*;
use crate::{encrypted_storage::MemoryEncryptedStorage, nft::testing::imported_test_key};

const KEY: ItemKey = ItemKey([0x05; 32]);

/// A move is signed again after a restart, from the secret read back.
#[test]
fn a_stored_secret_is_read_back_for_its_key() {
	let store = ImportedSecretStore::new(Arc::new(MemoryEncryptedStorage::default()));
	let imported = imported_test_key(0);
	let expected = *imported.secret().as_bytes();

	block_on(store.store_secrets(&[imported_test_key(0)])).unwrap();
	let read = block_on(store.secret_of(&imported.key())).unwrap();
	let other = block_on(store.secret_of(&KEY)).unwrap();

	assert_eq!(read.map(|secret| *secret.as_bytes()), Some(expected));
	assert!(other.is_none());
}

/// A value that is not a secret must not be signed with, nor be taken for
/// a missing one.
#[test]
fn a_stored_value_of_the_wrong_length_is_an_error() {
	let encrypted = Arc::new(MemoryEncryptedStorage::default());
	let storage_key = imported_secret_storage_key(&KEY);
	block_on(encrypted.write_encrypted(&storage_key, &[0x09; 32])).unwrap();
	let store = ImportedSecretStore::new(encrypted);

	let read = block_on(store.secret_of(&KEY));

	assert!(read.is_err());
}
