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
/// First use is serialized across runtimes sharing the installation identity.
#[instrument(skip_all, fields(runtime.method = "device_key.read_or_create"))]
pub async fn read_or_create_device_encryption_secret(
    storage: &(impl SecretCoreStorage + ?Sized),
) -> Result<[u8; 32], String> {
    static INITIALIZATION: futures::lock::Mutex<()> = futures::lock::Mutex::new(());
    let _initialization = INITIALIZATION.lock().await;
    let stored = storage
        .read_secret_core_storage(SecretCoreStorageKey::DeviceEncryptionKey)
        .await
        .map_err(|err| format!("device encryption key read failed: {err:?}"))?;
    if let Some(stored) = stored {
        return <[u8; 32]>::try_from(stored.as_slice())
            .map_err(|_| "stored device encryption key must be 32 bytes".to_string());
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
    fn corrupt_secret_does_not_replace_the_identity_peers_use() {
        futures::executor::block_on(async {
            let storage = StubPlatform::default();
            let key = SecretCoreStorageKey::DeviceEncryptionKey;
            storage
                .write_secret_core_storage(key.clone(), vec![7; 31])
                .await
                .unwrap();

            assert!(
                read_or_create_device_encryption_secret(&storage)
                    .await
                    .is_err()
            );
            assert_eq!(
                storage.read_secret_core_storage(key).await.unwrap(),
                Some(vec![7; 31])
            );
        });
    }

    use crate::latest::GenericError;
    use core::task::Poll;

    #[derive(Default)]
    struct DelayedSecretStorage(StubPlatform);

    #[async_trait::async_trait]
    impl SecretCoreStorage for DelayedSecretStorage {
        async fn read_secret_core_storage(
            &self,
            key: SecretCoreStorageKey,
        ) -> Result<Option<Vec<u8>>, GenericError> {
            self.0.read_secret_core_storage(key).await
        }

        async fn write_secret_core_storage(
            &self,
            key: SecretCoreStorageKey,
            value: Vec<u8>,
        ) -> Result<(), GenericError> {
            let mut delayed = false;
            futures::future::poll_fn(|cx| {
                if delayed {
                    Poll::Ready(())
                } else {
                    delayed = true;
                    cx.waker().wake_by_ref();
                    Poll::Pending
                }
            })
            .await;
            self.0.write_secret_core_storage(key, value).await
        }

        async fn clear_secret_core_storage(
            &self,
            key: SecretCoreStorageKey,
        ) -> Result<(), GenericError> {
            self.0.clear_secret_core_storage(key).await
        }
    }

    #[test]
    fn concurrent_first_use_returns_only_the_persisted_device_identity() {
        futures::executor::block_on(async {
            let storage = DelayedSecretStorage::default();
            let (first, second) = futures::join!(
                read_or_create_device_encryption_secret(&storage),
                read_or_create_device_encryption_secret(&storage),
            );
            let persisted = read_or_create_device_encryption_secret(&storage)
                .await
                .unwrap();
            assert_eq!((first.unwrap(), second.unwrap()), (persisted, persisted));
        });
    }
}
