//! The pairing role: product calls reach a wallet on a paired phone.

use std::sync::{Arc, Weak};

use super::host_grants::HostGrantStore;
use super::product_consent::ProductConsent;
use super::ring_vrf_registry::RingVrfRegistryStore;
use super::services::RuntimeServices;
use super::sso_request_service::{PairedSessionOwner, SsoRequestService};
use super::{HostAccounts, HostSession, SsoAccountHolderClient};
use crate::platform::PairingHostConfig;

/// Builds a paired host's parts and serves the intents that span them.
#[derive(Clone)]
pub struct PairingHost {
    accounts: Arc<HostAccounts<SsoAccountHolderClient>>,
    session: Arc<SsoRequestService>,
}

impl PairingHost {
    /// Bind account calls to a paired session whose changes reach the host that keeps its grants.
    pub fn new(services: Arc<RuntimeServices>, config: PairingHostConfig) -> Self {
        let mut session = None;
        let accounts = Arc::new_cyclic(|accounts: &Weak<HostAccounts<SsoAccountHolderClient>>| {
            let owner: Weak<dyn PairedSessionOwner> = accounts.clone();
            let service = SsoRequestService::new(services.clone(), config, owner);
            session = Some(service.clone());
            HostAccounts::new(
                services.clone(),
                Arc::new(SsoAccountHolderClient::new(service.clone())),
                service.session_state(),
                Arc::new(HostGrantStore::new(services.platform.clone())),
                RingVrfRegistryStore::new(services.platform.clone()),
                Arc::new(ProductConsent::new(services.platform.clone())),
                #[cfg(feature = "test-host")]
                Arc::default(),
            )
        });
        Self {
            accounts,
            session: session.expect("the session service is built with its owner"),
        }
    }

    /// Account calls for products served by this host.
    pub fn accounts(&self) -> &Arc<HostAccounts<SsoAccountHolderClient>> {
        &self.accounts
    }

    /// The paired session: login, replacement and disconnection.
    pub fn session(&self) -> &Arc<SsoRequestService> {
        &self.session
    }

    /// End the paired session, drop its kept AutoSigning keys and forget the
    /// pairing identity, so the next login pairs anew.
    pub async fn logout(&self) -> Result<(), String> {
        self.session.disconnect().await;
        self.accounts
            .forget_auto_signing_keys()
            .await
            .map_err(|reason| {
                format!("session disconnected, but AutoSigning reset failed: {reason}")
            })?;
        self.session.forget_pairing_identity().await
    }
}
