//! This install's long-lived X25519 encryption identity.
//!
//! Peers address one device by this key, so it is random rather than derived
//! from the identity entropy: two devices restoring the same identity must not
//! collapse onto the same key. It is created on first use and then persisted,
//! because a regenerated key silently strands peers still addressing the old
//! one.

use crate::platform::{CoreStorage, CoreStorageKey};
use tracing::{instrument, warn};

use crate::host_logic::sso::pairing::generate_x25519_keypair;

/// Read this device's persisted X25519 encryption secret, generating and
/// storing one on first use.
///
/// Callers that answer pairing must serialize this against themselves; two
/// concurrent generations would each persist and advertise a different key.
#[instrument(skip_all, fields(runtime.method = "device_key.read_or_create"))]
pub async fn read_or_create_device_encryption_secret(
	storage: &(impl CoreStorage + ?Sized),
) -> Result<[u8; 32], String> {
	let stored = storage
		.read_core_storage(CoreStorageKey::DeviceEncryptionKey)
		.await
		.map_err(|err| format!("device encryption key read failed: {err:?}"))?;
	if let Some(stored) = stored {
		match <[u8; 32]>::try_from(stored.as_slice()) {
			Ok(secret) => return Ok(secret),
			Err(_) => warn!("discarding malformed stored device encryption key"),
		}
	}

	let (secret, _) =
		generate_x25519_keypair().map_err(|err| format!("device encryption key failed: {err}"))?;
	storage
		.write_core_storage(CoreStorageKey::DeviceEncryptionKey, secret.to_vec())
		.await
		.map_err(|err| format!("device encryption key write failed: {err:?}"))?;
	Ok(secret)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{platform::Platform, test_support::StubPlatform};
	use std::sync::Arc;

	#[test]
	fn secret_is_generated_once_and_then_reused() {
		let storage: Arc<dyn Platform> = Arc::new(StubPlatform::default());

		let first =
			futures::executor::block_on(read_or_create_device_encryption_secret(storage.as_ref()))
				.unwrap();
		let second =
			futures::executor::block_on(read_or_create_device_encryption_secret(storage.as_ref()))
				.unwrap();

		// Peers address this device by the matching public key, so regenerating
		// it would strand everyone still holding the old one.
		assert_eq!(first, second);
	}
}
