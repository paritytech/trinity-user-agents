//! Pairing role: product calls reach a wallet on a paired phone.

#[cfg(test)]
mod tests;

use std::sync::Arc;

use super::{
	AccountHolder, HostAccounts, HostSession, RemoteAccountHolder,
	host_grants::{GrantBarrier, HostGrantStore},
	product_consent::ProductConsent,
	ring_vrf_registry::RingVrfRegistryStore,
	services::RuntimeServices,
	signing_host::ring_vrf::ChainRingResolver,
	sso_request_service::{PairedSessionOwner, SsoRequestService},
};
use crate::{
	host_logic::session::{SessionInfo, SessionState},
	platform::{PairingHostConfig, ProductId},
};

/// The paired session's kept grants and Allow once answers, as the session
/// service changes them.
struct PairedGrants {
	grants: Arc<HostGrantStore>,
	consent: Arc<ProductConsent>,
}

#[async_trait::async_trait]
impl PairedSessionOwner for PairedGrants {
	async fn write_barrier(&self) -> GrantBarrier {
		self.grants.barrier().await
	}

	fn session_ended(&self, previous: Option<&SessionInfo>, revoked: bool) {
		self.grants.session_ended(previous, revoked);
		self.consent.forget_allowed_once();
	}

	fn begin_cleanup(&self, barrier: &GrantBarrier) -> bool {
		self.grants.persistence_under(barrier).begin_cleanup()
	}

	async fn finish_cleanup(&self, barrier: &GrantBarrier) -> Result<(), String> {
		self.grants.persistence_under(barrier).drain_cleanup().await
	}

	async fn prepare_session(
		&self,
		barrier: &GrantBarrier,
		previous: Option<&SessionInfo>,
		next: &SessionInfo,
	) {
		self.grants.persistence_under(barrier).prepare_session(previous, next).await;
	}
}

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
		let platform = services.platform.clone();
		let session_state = SessionState::new();
		let consent = Arc::new(ProductConsent::new(platform.clone()));
		let grants = Arc::new(HostGrantStore::new(
			platform.clone(),
			RemoteAccountHolder::GRANT_KEEPING,
			session_state.clone(),
		));
		let session = SsoRequestService::new(
			services.clone(),
			host_config,
			session_state,
			Arc::new(PairedGrants { grants: grants.clone(), consent: consent.clone() }),
		);
		let ring_vrf_registry = RingVrfRegistryStore::new(platform);
		let accounts = Arc::new(HostAccounts::new(
			services.clone(),
			Arc::new(RemoteAccountHolder::new(
				session.clone(),
				ring_vrf_registry.clone(),
				services.spawner.clone(),
			)),
			grants,
			ChainRingResolver::new(services.chain.clone()),
			ring_vrf_registry,
			consent,
			#[cfg(feature = "test-host")]
			Arc::default(),
		));
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
