//! Concrete host lifecycle composition kept outside the wallet domain contract.

use std::sync::Arc;

use crate::host_logic::session::SessionState;
use crate::platform::ProductContext;
use crate::runtime::authority::{AccountHolder, AuthorityError, AuthoritySession};
use crate::runtime::{SsoAccountHolderClient, WalletAccountHolder};
use truapi::CallError;
use truapi::versioned::account::{HostRequestLoginError, HostRequestLoginResponse};

/// Composition selected by the native and paired runtime facades.
pub enum HostLifecycle {
    /// Wallet activation is provided by the embedding application.
    Native(Arc<WalletAccountHolder>),
    /// Pairing and peer transport are provided by the SSO client.
    Paired(Arc<SsoAccountHolderClient>),
}

impl HostLifecycle {
    /// Shared connection-status source for product subscriptions.
    pub fn session_state(&self) -> Arc<SessionState> {
        match self {
            Self::Native(wallet) => wallet.session_state(),
            Self::Paired(client) => client.session_state(),
        }
    }

    /// Connect a product through its composing host.
    pub async fn request_login(&self, product: &ProductContext) -> Result<HostRequestLoginResponse, CallError<HostRequestLoginError>> {
        match self {
            Self::Native(wallet) => Ok(HostRequestLoginResponse::V1(if wallet.current_session().is_some() {
                truapi::latest::HostRequestLoginResponse::AlreadyConnected
            } else {
                truapi::latest::HostRequestLoginResponse::Rejected
            })),
            Self::Paired(client) => client.request_login(product).await,
        }
    }

    /// Stop account work; native wallet grants remain in protected storage.
    pub async fn disconnect(&self) -> Result<(), AuthorityError> {
        match self {
            Self::Native(wallet) => wallet.lock().await,
            Self::Paired(client) => client.disconnect().await,
        }
    }

    /// Refresh identity through the host's existing account connection.
    pub async fn refresh_session_identity(&self) -> Result<Option<AuthoritySession>, AuthorityError> {
        match self {
            Self::Native(wallet) => Ok(wallet.current_session()),
            Self::Paired(client) => client.refresh_current_session_identity().await,
        }
    }
}
