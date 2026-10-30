//! The secrets of item keys the host did not derive, such as the keys of a
//! deposited cheque, kept in encrypted storage so their moves can be signed
//! again after a restart.

use std::sync::Arc;

use truapi::latest::GenericError;
use zeroize::Zeroizing;

use super::keys::{ImportedItemKey, ItemKey, ItemSecret};
use crate::encrypted_storage::EncryptedStorage;

/// Prefix of the encrypted storage key that holds an imported item secret.
const SECRET_KEY_PREFIX: &[u8] = b"nft/item-secret/";

/// The secrets of imported item keys, kept in encrypted storage so each move
/// that spends one can be signed again after a restart. Entries are never
/// removed.
#[derive(Clone)]
pub struct ImportedSecretStore {
	encrypted: Arc<dyn EncryptedStorage>,
}

impl ImportedSecretStore {
	/// A store over `encrypted`.
	pub fn new(encrypted: Arc<dyn EncryptedStorage>) -> Self {
		Self { encrypted }
	}

	/// Stores the secret of each of `keys` under that key.
	pub async fn store_secrets(&self, keys: &[ImportedItemKey]) -> Result<(), GenericError> {
		for imported in keys {
			let storage_key = imported_secret_storage_key(&imported.key());
			self.encrypted
				.write_encrypted(&storage_key, imported.secret().as_bytes())
				.await?;
		}
		Ok(())
	}

	/// The secret stored for `key`, or `None` when none is. A stored value that
	/// is not a 64-byte secret is an error.
	pub async fn secret_of(&self, key: &ItemKey) -> Result<Option<ItemSecret>, GenericError> {
		let storage_key = imported_secret_storage_key(key);
		let stored = self.encrypted.read_encrypted(&storage_key).await?;
		stored.map(|bytes| secret_from_stored(key, &bytes)).transpose()
	}
}

/// The encrypted storage key under which the secret of `key` is kept.
fn imported_secret_storage_key(key: &ItemKey) -> Vec<u8> {
	[SECRET_KEY_PREFIX, &key.0].concat()
}

/// `bytes`, read back for `key`, as an item secret.
fn secret_from_stored(key: &ItemKey, bytes: &[u8]) -> Result<ItemSecret, GenericError> {
	let secret = <[u8; 64]>::try_from(bytes).map_err(|_| GenericError {
		reason: format!(
			"the stored secret of item key {} has {} bytes, expected 64",
			hex::encode(key.0),
			bytes.len()
		),
	})?;
	Ok(ItemSecret::from_bytes(Zeroizing::new(secret)))
}

#[cfg(test)]
mod tests {
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
}
