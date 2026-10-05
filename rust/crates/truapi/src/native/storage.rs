use std::sync::{Arc, Mutex};

use crate::latest::GenericError;
use crate::platform::{SecretCoreStorage, SecretCoreStorageKey, WalletSecretProvider, async_trait};
use crate::store::RuntimeStore;

use super::callbacks::{HostCallbacks, NativeWalletSecretProvider};

/// Protected host storage, separate from the wallet root provider.
pub struct NativeSecrets(pub Arc<dyn HostCallbacks>);

#[async_trait]
impl SecretCoreStorage for NativeSecrets {
    async fn read_secret_core_storage(
        &self,
        key: SecretCoreStorageKey,
    ) -> Result<Option<Vec<u8>>, GenericError> {
        self.0
            .read_secret_core_storage(key)
            .await
            .map_err(Into::into)
    }

    async fn write_secret_core_storage(
        &self,
        key: SecretCoreStorageKey,
        value: Vec<u8>,
    ) -> Result<(), GenericError> {
        self.0
            .write_secret_core_storage(key, value)
            .await
            .map_err(Into::into)
    }

    async fn clear_secret_core_storage(
        &self,
        key: SecretCoreStorageKey,
    ) -> Result<(), GenericError> {
        self.0
            .clear_secret_core_storage(key)
            .await
            .map_err(Into::into)
    }
}

/// Root storage is reachable only during explicit wallet activation.
pub struct WalletSecrets(pub Arc<dyn NativeWalletSecretProvider>);

#[async_trait]
impl WalletSecretProvider for WalletSecrets {
    async fn read_wallet_root_entropy(&self, wallet_id: &str) -> Result<Vec<u8>, GenericError> {
        self.0
            .read_wallet_root_entropy(wallet_id.to_string())
            .await
            .map_err(Into::into)
    }
}

/// Repositories for one native runtime's active owner.
#[derive(Default)]
pub struct NativeStorage {
    current: Mutex<Option<Arc<RuntimeStore>>>,
}

impl NativeStorage {
    /// Inactive wallets cannot read or mutate their runtime records.
    pub fn current(&self) -> Result<Arc<RuntimeStore>, GenericError> {
        self.current
            .lock()
            .expect("native storage mutex poisoned")
            .clone()
            .ok_or_else(|| GenericError {
                reason: "wallet storage is locked".to_string(),
            })
    }

    /// Bind product work to its current lifetime, remaining inert during removal.
    pub fn snapshot_for_product(&self, product: &str) -> Arc<Self> {
        let current = self
            .current()
            .ok()
            .and_then(|store| store.for_product(product).ok())
            .map(Arc::new);
        Arc::new(Self {
            current: Mutex::new(current),
        })
    }

    /// Install a fully opened account database before publishing activation.
    pub fn install(&self, store: Arc<RuntimeStore>) {
        *self.current.lock().expect("native storage mutex poisoned") = Some(store);
    }

    /// Stop accepting operations and drain writes already holding the store.
    pub async fn lock(&self) -> Result<(), String> {
        let previous = self
            .current
            .lock()
            .expect("native storage mutex poisoned")
            .take();
        if let Some(previous) = previous {
            previous
                .deactivate()
                .await
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }
}
