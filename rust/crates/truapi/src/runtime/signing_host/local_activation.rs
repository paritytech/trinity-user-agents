use super::SigningHost;
use super::wallet_account_holder::PreparedWalletActivation;
use crate::runtime::authority::AuthorityError;
use crate::runtime::{WalletSecretProvider, connected_session_ui_info};

/// Raw entropy activation available only to test hosts.
#[cfg(any(test, feature = "test-host"))]
#[async_trait::async_trait]
pub trait LocalActivation: Send + Sync {
    /// Activate a local session from raw BIP-39 entropy, deriving the root
    /// public key and marking the session connected.
    async fn activate_local_session(&self, secret: Vec<u8>) -> Result<(), AuthorityError>;

    /// Activate a local session and attach known identity metadata from the
    /// host's signer/account store.
    async fn activate_local_session_with_identity(
        &self,
        secret: Vec<u8>,
        lite_username: Option<String>,
    ) -> Result<(), AuthorityError>;
}

#[cfg(any(test, feature = "test-host"))]
#[async_trait::async_trait]
impl LocalActivation for SigningHost {
    async fn activate_local_session(&self, secret: Vec<u8>) -> Result<(), AuthorityError> {
        self.activate_local_session_with_identity(secret, None)
            .await
    }

    async fn activate_local_session_with_identity(
        &self,
        secret: Vec<u8>,
        lite_username: Option<String>,
    ) -> Result<(), AuthorityError> {
        let activation = self.wallet.prepare_activation(secret, lite_username)?;
        self.install_wallet(activation)
    }
}

impl SigningHost {
    /// Activate one selected wallet without granting product access to its provider.
    pub async fn activate_wallet(
        &self,
        provider: &dyn WalletSecretProvider,
        wallet_id: String,
        lite_username: Option<String>,
    ) -> Result<(), AuthorityError> {
        let activation = self
            .wallet
            .prepare_wallet_activation(provider, wallet_id, lite_username)
            .await?;
        self.install_wallet(activation)
    }

    fn install_wallet(&self, activation: PreparedWalletActivation) -> Result<(), AuthorityError> {
        let session = {
            let mut state = self.grants.lifecycle();
            let session = self.wallet.install(activation)?;
            state.clear_memory();
            session
        };
        self.auth_state
            .connected(&connected_session_ui_info(&session));
        Ok(())
    }
}
