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
