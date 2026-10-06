//! Protected host records, separate from public core state.

use parity_scale_codec::{Decode, Encode};

use super::async_trait;
use crate::latest::GenericError;

/// Protected host slots; root wallet entropy has a separate owner.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum SecretCoreStorageKey {
    /// Authenticated paired-session state.
    #[codec(index = 0)]
    AuthSession,
    /// Signing and encryption identity used while pairing.
    #[codec(index = 1)]
    PairingDeviceIdentity,
    /// Installation identity shared with device messaging, independent of logout.
    #[codec(index = 2)]
    DeviceEncryptionKey,
    /// Retained allowance keys for one authenticated pairing.
    #[codec(index = 3)]
    AllowanceKeys {
        /// Canonical pair of SSO session identifiers.
        session_id: String,
    },
    /// Wallet-bound delegated product signing capabilities.
    #[codec(index = 4)]
    AutoSigningKeys,
    /// Native resource grants bound to stable wallet ownership.
    #[codec(index = 5)]
    NativeAllowanceKeys,
    /// Installation key protecting native product and core database values.
    #[codec(index = 6)]
    StorageEncryptionKey,
}

impl SecretCoreStorageKey {
    /// Opaque persistent identifier whose encoding belongs to Rust.
    pub fn storage_key(&self) -> String {
        format!("truapi:secret:{}", hex::encode(self.encode()))
    }
}

/// Protected persistence whose errors never masquerade as absent secrets.
#[async_trait]
pub trait SecretCoreStorage: Send + Sync {
    /// Only a missing record returns `None`.
    async fn read_secret_core_storage(
        &self,
        key: SecretCoreStorageKey,
    ) -> Result<Option<Vec<u8>>, GenericError>;

    /// A successful write has completed persistent storage, including protection.
    async fn write_secret_core_storage(
        &self,
        key: SecretCoreStorageKey,
        value: Vec<u8>,
    ) -> Result<(), GenericError>;

    /// Completion orders removal after earlier writes, including cancelled waits.
    async fn clear_secret_core_storage(
        &self,
        key: SecretCoreStorageKey,
    ) -> Result<(), GenericError>;
}
