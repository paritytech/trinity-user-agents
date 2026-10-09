use super::SigningHost;
use crate::runtime::authority::AuthorityError;
use crate::runtime::connected_session_ui_info;

/// Activate the wallet from entropy supplied by the embedding host.
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
        let session = {
            let mut state = self
                .local_grants
                .lock()
                .expect("local AutoSigning grant mutex poisoned");
            state.clear_grants();
            self.consent.forget_allowed_once();
            self.wallet.install(activation)
        };
        self.auth_state
            .connected(&connected_session_ui_info(&session));
        Ok(())
    }
}
