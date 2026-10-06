//! This install's long-lived X25519 encryption identity.
//!
//! Peers address one device by this key, so it is random rather than derived
//! from the identity entropy: two devices restoring the same identity must not
//! collapse onto the same key. It is created on first use and then persisted,
//! because a regenerated key silently strands peers still addressing the old
//! one.

use crate::platform::{SecretCoreStorage, SecretCoreStorageKey};
use tracing::instrument;

use crate::host_logic::sso::pairing::generate_x25519_keypair;

/// Read this device's persisted X25519 encryption secret, generating and
/// storing one on first use.
///
/// Callers that answer pairing must serialize this against themselves; two
/// concurrent generations would each persist and advertise a different key.
#[instrument(skip_all, fields(runtime.method = "device_key.read_or_create"))]
pub async fn read_or_create_device_encryption_secret(
    storage: &(impl SecretCoreStorage + ?Sized),
) -> Result<[u8; 32], String> {
    let stored = storage
        .read_secret_core_storage(SecretCoreStorageKey::DeviceEncryptionKey)
        .await
        .map_err(|err| format!("device encryption key read failed: {err:?}"))?;
    if let Some(stored) = stored {
        return stored
            .as_slice()
            .try_into()
            .map_err(|_| "stored device encryption key has invalid length".to_string());
    }

    let (secret, _) =
        generate_x25519_keypair().map_err(|err| format!("device encryption key failed: {err}"))?;
    storage
        .write_secret_core_storage(SecretCoreStorageKey::DeviceEncryptionKey, secret.to_vec())
        .await
        .map_err(|err| format!("device encryption key write failed: {err:?}"))?;
    Ok(secret)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::Platform;
    use crate::test_support::StubPlatform;
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
    #[test]
    fn corrupt_device_identity_is_not_replaced() {
        futures::executor::block_on(async {
            let storage = StubPlatform::default();
            let malformed = vec![7; 31];
            storage
                .write_secret_core_storage(
                    SecretCoreStorageKey::DeviceEncryptionKey,
                    malformed.clone(),
                )
                .await
                .unwrap();

            let result = read_or_create_device_encryption_secret(&storage).await;
            assert_eq!(
                (
                    result.is_err(),
                    storage
                        .read_secret_core_storage(SecretCoreStorageKey::DeviceEncryptionKey)
                        .await
                        .unwrap(),
                ),
                (true, Some(malformed)),
            );
        });
    }
}
