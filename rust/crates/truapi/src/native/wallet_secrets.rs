//! Protected wallet reads used only during explicit native activation.

use super::HostRejection;
use crate::latest::GenericError;
use crate::runtime::WalletSecretProvider;

/// Host-owned protected root access, independent of product callbacks.
#[uniffi::export(rust, foreign)]
#[async_trait::async_trait]
pub trait NativeWalletSecretProvider: Send + Sync {
    /// Read the exact selected wallet or report an unavailable root.
    async fn read_wallet_root_entropy(&self, wallet_id: String) -> Result<Vec<u8>, HostRejection>;
}

/// Error conversion at the native wallet boundary.
pub struct WalletSecretCallback {
    /// Provider retained for the runtime's activation lifetime.
    pub provider: std::sync::Arc<dyn NativeWalletSecretProvider>,
}

#[async_trait::async_trait]
impl WalletSecretProvider for WalletSecretCallback {
    async fn read_wallet_root_entropy(&self, wallet_id: String) -> Result<Vec<u8>, GenericError> {
        self.provider
            .read_wallet_root_entropy(wallet_id)
            .await
            .map_err(GenericError::from)
    }
}
