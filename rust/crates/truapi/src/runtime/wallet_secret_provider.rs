//! Wallet root access reserved for explicit activation.

use crate::latest::GenericError;

/// Reads the selected wallet without exposing root secrets to product services.
#[async_trait::async_trait]
pub trait WalletSecretProvider: Send + Sync {
    /// Missing, inaccessible or mismatched wallets are errors.
    async fn read_wallet_root_entropy(&self, wallet_id: String) -> Result<Vec<u8>, GenericError>;
}
