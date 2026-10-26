//! Key-value storage the host keeps encrypted at rest, for secrets the core
//! database must not hold, and the trait the host provides it through.

use async_trait::async_trait;
use truapi::latest::GenericError;
use zeroize::Zeroizing;

/// Values the host encrypts at rest, under raw byte keys. They are not backed
/// up or moved to another device, so whatever is stored here is lost with the
/// installation.
#[async_trait]
pub trait EncryptedStorage: Send + Sync {
	/// The value stored under `key`, or `None` when nothing is. A store that
	/// cannot be read is an error, never `None`.
	async fn read_encrypted(&self, key: &[u8]) -> Result<Option<Zeroizing<Vec<u8>>>, GenericError>;

	/// Stores `value` under `key`, replacing any earlier value.
	async fn write_encrypted(&self, key: &[u8], value: &[u8]) -> Result<(), GenericError>;
}

/// Encrypted storage in memory, for tests.
#[cfg(test)]
#[derive(Default)]
pub struct MemoryEncryptedStorage {
	values: std::sync::Mutex<std::collections::HashMap<Vec<u8>, Vec<u8>>>,
}

#[cfg(test)]
impl MemoryEncryptedStorage {
	/// Every stored value by key.
	pub fn stored(&self) -> std::collections::HashMap<Vec<u8>, Vec<u8>> {
		self.values.lock().expect("storage mutex poisoned").clone()
	}
}

#[cfg(test)]
#[async_trait]
impl EncryptedStorage for MemoryEncryptedStorage {
	async fn read_encrypted(&self, key: &[u8]) -> Result<Option<Zeroizing<Vec<u8>>>, GenericError> {
		let values = self.values.lock().expect("storage mutex poisoned");
		Ok(values.get(key).cloned().map(Zeroizing::new))
	}

	async fn write_encrypted(&self, key: &[u8], value: &[u8]) -> Result<(), GenericError> {
		self.values
			.lock()
			.expect("storage mutex poisoned")
			.insert(key.to_vec(), value.to_vec());
		Ok(())
	}
}
