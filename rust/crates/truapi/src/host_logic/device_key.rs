//! This install's long-lived X25519 encryption identity.
//!
//! Peers address one device by this key, so it is random rather than derived
//! from the identity entropy: two devices restoring the same identity must not
//! collapse onto the same key. It is created on first use and then persisted,
//! because a regenerated key silently strands peers still addressing the old
//! one.

use crate::platform::{CoreStorage, CoreStorageKey, StackStorage};
use tracing::{instrument, warn};

use crate::host_logic::x25519_seal::X25519PrivateKey;

/// Read this device's persisted X25519 encryption secret, generating and
/// storing one on first use.
///
/// Callers that answer pairing must serialize this against themselves; two
/// concurrent generations would each persist and advertise a different key.
#[instrument(skip_all, fields(runtime.method = "device_key.read_or_create"))]
pub async fn read_or_create_device_encryption_secret(
	storage: &StackStorage<'_, impl CoreStorage + ?Sized>,
) -> Result<[u8; 32], String> {
	let stored = storage
		.read(CoreStorageKey::DeviceEncryptionKey)
		.await
		.map_err(|err| format!("device encryption key read failed: {err:?}"))?;
	if let Some(stored) = stored {
		match <[u8; 32]>::try_from(stored.as_slice()) {
			Ok(secret) => return Ok(secret),
			Err(_) => warn!("discarding malformed stored device encryption key"),
		}
	}

	let secret = X25519PrivateKey::generate()
		.map_err(|err| format!("device encryption key failed: {err}"))?;
	storage
		.write(CoreStorageKey::DeviceEncryptionKey, secret.as_bytes().to_vec())
		.await
		.map_err(|err| format!("device encryption key write failed: {err:?}"))?;
	Ok(*secret.as_bytes())
}

#[cfg(test)]
mod tests;
