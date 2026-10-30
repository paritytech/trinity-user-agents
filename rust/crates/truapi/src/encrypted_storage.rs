//! Key-value storage the host keeps encrypted at rest, for secrets the core
//! database must not hold, and the trait the host provides it through.
//!
//! A feature keeps its public, queryable data in the core database and only
//! the secret here, under a key it derives from that data: it finds what it
//! needs in the database, then reads the secret by key. The store cannot list
//! its keys, so the database is also how a feature finds what to delete.

use async_trait::async_trait;
use truapi::latest::GenericError;
use zeroize::Zeroizing;

/// Values the host encrypts at rest, under raw byte keys. They are not backed
/// up or moved to another device, so whatever is stored here is lost with the
/// installation.
///
/// Each feature prefixes its keys with its own namespace, so features never
/// share a key.
#[async_trait]
pub trait EncryptedStorage: Send + Sync {
	/// The value stored under `key`, or `None` when nothing is. A store that
	/// cannot be read, or a value that no longer decrypts, is an error, never
	/// `None`, so a lost secret is never mistaken for an absent one.
	async fn read_encrypted(&self, key: &[u8]) -> Result<Option<Zeroizing<Vec<u8>>>, GenericError>;

	/// Stores `value` under `key`, replacing any earlier value.
	async fn write_encrypted(&self, key: &[u8], value: &[u8]) -> Result<(), GenericError>;

	/// Deletes the value stored under `key`. Deleting nothing succeeds.
	async fn delete_encrypted(&self, key: &[u8]) -> Result<(), GenericError>;
}

/// Encrypted storage in memory, for tests.
#[cfg(test)]
#[derive(Default)]
pub struct MemoryEncryptedStorage {
	values: std::sync::Mutex<std::collections::HashMap<Vec<u8>, Vec<u8>>>,
	calls: std::sync::atomic::AtomicUsize,
}

#[cfg(test)]
impl MemoryEncryptedStorage {
	/// Every stored value by key.
	pub fn stored(&self) -> std::collections::HashMap<Vec<u8>, Vec<u8>> {
		self.values.lock().expect("storage mutex poisoned").clone()
	}

	/// How many reads, writes and deletes the storage has served.
	pub fn calls(&self) -> usize {
		self.calls.load(std::sync::atomic::Ordering::Relaxed)
	}

	fn values(&self) -> std::sync::MutexGuard<'_, std::collections::HashMap<Vec<u8>, Vec<u8>>> {
		self.calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
		self.values.lock().expect("storage mutex poisoned")
	}
}

#[cfg(test)]
#[async_trait]
impl EncryptedStorage for MemoryEncryptedStorage {
	async fn read_encrypted(&self, key: &[u8]) -> Result<Option<Zeroizing<Vec<u8>>>, GenericError> {
		Ok(self.values().get(key).cloned().map(Zeroizing::new))
	}

	async fn write_encrypted(&self, key: &[u8], value: &[u8]) -> Result<(), GenericError> {
		self.values().insert(key.to_vec(), value.to_vec());
		Ok(())
	}

	async fn delete_encrypted(&self, key: &[u8]) -> Result<(), GenericError> {
		self.values().remove(key);
		Ok(())
	}
}
