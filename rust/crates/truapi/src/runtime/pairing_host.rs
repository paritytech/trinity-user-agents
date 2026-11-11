//! Pairing role: product calls reach a wallet on a paired phone.

#[cfg(test)]
mod tests;

use std::sync::{Arc, Weak};

use super::{
	HostAccounts, HostSession, RemoteAccountHolder,
	product_consent::ProductConsent,
	ring_vrf_registry::RingVrfRegistryStore,
	services::RuntimeServices,
	sso_request_service::{PairedSessionOwner, SsoRequestService},
};
use crate::platform::{PairingHostConfig, ProductId};

/// Builds a paired host's parts and serves the intents that span them.
pub struct PairingHost {
	accounts: Arc<HostAccounts<RemoteAccountHolder>>,
	/// The paired session, selected and served by [`SsoRequestService`].
	session: Arc<SsoRequestService>,
}

impl PairingHost {
	/// Bind account calls to a paired session whose changes reach the host
	/// accounts that keep its grants.
	pub fn new(services: Arc<RuntimeServices>, host_config: PairingHostConfig) -> Arc<Self> {
		if services.asset_hub_chain_genesis_hash().is_none() {
			// Said once at startup rather than inferred from every grant
			// refusing, matching the signing role. Cross-product refusals are
			// deliberately indistinguishable from a product that granted
			// nothing, so a pairing host with no Asset Hub resolves no manifest
			// and refuses every grant while looking exactly like one whose
			// publishers granted nothing. The hash itself now arrives by
			// construction, so there is nothing to install here, only to report.
			tracing::warn!(
				"no Asset Hub configured on the pairing role: no product manifest \
                 will resolve, so every cross-product grant is refused"
			);
		}
		let ring_vrf_registry = RingVrfRegistryStore::new(services.platform.clone());
		let mut session = None;
		let accounts = Arc::new_cyclic(|accounts: &Weak<HostAccounts<RemoteAccountHolder>>| {
			let owner: Weak<dyn PairedSessionOwner> = accounts.clone();
			let service = SsoRequestService::new(services.clone(), host_config, owner);
			session = Some(service.clone());
			HostAccounts::new(
				services.clone(),
				Arc::new(RemoteAccountHolder::new(
					service.clone(),
					ring_vrf_registry.clone(),
					services.spawner.clone(),
				)),
				service.session_state(),
				ring_vrf_registry,
				Arc::new(ProductConsent::new(services.platform.clone())),
				#[cfg(feature = "test-host")]
				Arc::default(),
			)
		});
		let session = session.expect("the session service is built with its owner");
		Arc::new(Self { accounts, session })
	}

	/// Account calls for products served by this host.
	pub fn accounts(&self) -> &Arc<HostAccounts<RemoteAccountHolder>> {
		&self.accounts
	}

	/// The paired session: login, replacement and disconnection.
	pub fn session(&self) -> &Arc<SsoRequestService> {
		&self.session
	}

	/// Disconnect and discard pairing bootstrap material so the next login
	/// generates a new device keypair and topic.
	pub async fn logout_and_reset_pairing(&self) -> Result<(), String> {
		self.session.disconnect().await;
		self.accounts.forget_auto_signing_keys().await.map_err(|reason| {
			format!("session disconnected, but AutoSigning reset failed: {reason}")
		})?;
		self.session.forget_pairing_identity().await
	}

	/// Clear one product's grants and Allow once answers, keeping the session and other products.
	pub async fn clear_product_state(&self, product_id: &ProductId) -> Result<(), String> {
		self.accounts.clear_product_state(product_id).await
	}
}
