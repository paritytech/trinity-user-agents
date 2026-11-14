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
mod tests;
