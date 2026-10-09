use super::{SigningHost, product_authority_error};
use crate::host_logic::product_account::{
    derive_identity_keypair, derive_root_keypair_from_entropy,
};
use crate::host_logic::session::SessionInfo;
use crate::host_logic::sso::pairing::derive_identity_chat_private_key;
use crate::runtime::authority::AuthorityError;
use crate::runtime::connected_session_ui_info;

use zeroize::Zeroizing;

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
        let secret = Zeroizing::new(secret);
        let root = derive_root_keypair_from_entropy(&secret).map_err(product_authority_error)?;
        let public_key = root.public.to_bytes();
        let identity_account_id = derive_identity_keypair(&secret, self.network_suffix())
            .map_err(product_authority_error)?
            .public
            .to_bytes();
        let identity_chat_private_key = derive_identity_chat_private_key(&secret);
        let session = SessionInfo {
            public_key,
            sso: None,
            root_entropy_source: None,
            identity_account_id: Some(identity_account_id),
            identity_chat_private_key: Some(identity_chat_private_key),
            // A local session has no answering remote device to address.
            device_enc_public_key: None,
            lite_username,
            full_username: None,
        };
        let ui_info = connected_session_ui_info(&session);
        self.install_local_session(secret, session);
        self.auth_state.connected(&ui_info);
        Ok(())
    }
}
