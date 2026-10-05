use parity_scale_codec::{Decode, Encode};

use super::async_trait;
use crate::latest::GenericError;

/// Host secrets whose bytes must be protected by the platform.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum SecretCoreStorageKey {
    /// Secret-bearing authentication state for this host.
    AuthSession,
    /// Pairing device signing and encryption material.
    PairingDeviceIdentity,
    /// Shared installation device encryption identity, including chat.
    DeviceEncryptionKey,
    /// Installation encryption key for native runtime database payloads.
    StorageEncryptionKey,
    /// Retained host grants scoped to a native owner or authenticated pairing.
    HostAccountGrants {
        /// Stable root public key of the owning wallet.
        root_public_key: [u8; 32],
        /// Authenticated pairing binding, absent for native grants.
        session_id: Option<String>,
    },
}

impl SecretCoreStorageKey {
    /// Opaque stable adapter key; its encoding and scope belong to Rust.
    pub fn storage_key(&self) -> String {
        format!("truapi:secret:{}", hex::encode(self.encode()))
    }
}

/// Persistent protected bytes; root wallet entropy uses a separate provider.
#[async_trait]
pub trait SecretCoreStorage: Send + Sync {
    /// Only `None` means absent. Inaccessible or corrupt storage is an error.
    async fn read_secret_core_storage(
        &self,
        key: SecretCoreStorageKey,
    ) -> Result<Option<Vec<u8>>, GenericError>;

    /// Success means the protected write has durably completed.
    async fn write_secret_core_storage(
        &self,
        key: SecretCoreStorageKey,
        value: Vec<u8>,
    ) -> Result<(), GenericError>;

    /// Success means persistent removal completed; an absent key is harmless.
    async fn clear_secret_core_storage(
        &self,
        key: SecretCoreStorageKey,
    ) -> Result<(), GenericError>;
}

/// Wallet-only access to the existing protected root store under native unlock rules.
#[async_trait]
pub trait WalletSecretProvider: Send + Sync {
    /// Resolve the selected stable wallet identifier without copying its root store.
    async fn read_wallet_root_entropy(&self, wallet_id: &str) -> Result<Vec<u8>, GenericError>;
}
