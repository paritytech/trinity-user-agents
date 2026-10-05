use super::{WalletAccountHolder, product_authority_error};
use crate::host_logic::product_account::{
    derive_identity_keypair, derive_root_keypair_from_entropy,
};
use crate::host_logic::session::SessionInfo;
use crate::host_logic::sso::pairing::derive_identity_chat_private_key;
use crate::platform::WalletSecretProvider;
use crate::runtime::authority::AuthorityError;
use crate::runtime::connected_session_ui_info;

use zeroize::Zeroizing;

/// Establish a wallet-local session from host-held secret material.
///
/// A signing host owns the user's keys, so it establishes sessions directly
/// rather than through the SSO pairing flow. Only [`WalletAccountHolder`] implements
/// this; pairing hosts have no local secret to activate.
#[cfg(any(test, feature = "test-host"))]
#[async_trait::async_trait]
pub trait LocalActivation: Send + Sync {
    /// Activate a local session from raw BIP-39 entropy, deriving the root
    /// public key and marking the session connected.
    #[cfg(any(test, feature = "test-host"))]
    async fn activate_local_session(&self, secret: Vec<u8>) -> Result<(), AuthorityError>;

    /// Activate a local session and attach known identity metadata from the
    /// host's signer/account store.
    #[cfg(any(test, feature = "test-host"))]
    async fn activate_local_session_with_identity(
        &self,
        secret: Vec<u8>,
        lite_username: Option<String>,
    ) -> Result<(), AuthorityError>;
}

#[cfg(any(test, feature = "test-host"))]
#[async_trait::async_trait]
impl LocalActivation for WalletAccountHolder {
    #[cfg(any(test, feature = "test-host"))]
    async fn activate_local_session(&self, secret: Vec<u8>) -> Result<(), AuthorityError> {
        self.activate_local_session_with_identity(secret, None)
            .await
    }

    #[cfg(any(test, feature = "test-host"))]
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
        let _persistence = self.persistence.lock().await;
        self.install_local_session(secret, session);
        self.auth_state.connected(&ui_info);
        Ok(())
    }
}

/// Prepared wallet secrets are private until the owner's repositories are selected.
pub struct PreparedWalletActivation {
    secret: Zeroizing<Vec<u8>>,
    session: SessionInfo,
    generation: u64,
}

impl PreparedWalletActivation {
    /// Stable owner used to select the account database.
    pub fn owner_public_key(&self) -> [u8; 32] {
        self.session.public_key
    }
}

impl WalletAccountHolder {
    /// Load protected wallet entropy without starting account work.
    pub async fn prepare_wallet(
        &self,
        provider: &dyn WalletSecretProvider,
        wallet_id: &str,
        lite_username: Option<String>,
    ) -> Result<PreparedWalletActivation, AuthorityError> {
        self.clear_local_session();
        let generation = self
            .activation
            .lock()
            .expect("wallet activation mutex poisoned")
            .generation;
        let secret = Zeroizing::new(provider.read_wallet_root_entropy(wallet_id).await.map_err(
            |error| AuthorityError::Unavailable {
                reason: error.reason,
            },
        )?);
        let root = derive_root_keypair_from_entropy(&secret).map_err(product_authority_error)?;
        let identity = derive_identity_keypair(&secret, self.network_suffix())
            .map_err(product_authority_error)?;
        let session = SessionInfo {
            public_key: root.public.to_bytes(),
            sso: None,
            root_entropy_source: None,
            identity_account_id: Some(identity.public.to_bytes()),
            identity_chat_private_key: Some(derive_identity_chat_private_key(&secret)),
            device_enc_public_key: None,
            lite_username,
            full_username: None,
        };
        if self
            .activation
            .lock()
            .expect("wallet activation mutex poisoned")
            .generation
            != generation
        {
            return Err(AuthorityError::Disconnected);
        }
        Ok(PreparedWalletActivation {
            secret,
            session,
            generation,
        })
    }

    /// Publish the prepared wallet after its repositories have been selected.
    pub async fn activate_wallet(
        &self,
        prepared: PreparedWalletActivation,
    ) -> Result<(), AuthorityError> {
        let _persistence = self.persistence.lock().await;
        let mut activation = self
            .activation
            .lock()
            .expect("wallet activation mutex poisoned");
        if activation.generation != prepared.generation {
            return Err(AuthorityError::Disconnected);
        }
        activation.advance_activation();
        let ui_info = connected_session_ui_info(&prepared.session);
        *self
            .root_entropy
            .lock()
            .expect("wallet entropy mutex poisoned") = Some(prepared.secret);
        self.session_state.set_session(prepared.session);
        drop(activation);
        self.auth_state.connected(&ui_info);
        Ok(())
    }
}
