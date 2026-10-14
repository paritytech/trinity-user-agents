//! The pairing role: product calls reach a wallet on a paired phone.

use std::sync::{Arc, Weak};

use super::{
	HostAccounts, HostSession, RemoteAccountHolder,
	product_consent::ProductConsent,
	ring_vrf_registry::RingVrfRegistryStore,
	services::RuntimeServices,
	sso_request_service::{PairedSessionOwner, SsoRequestService},
};
use crate::platform::PairingHostConfig;

#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};

/// Builds a paired host's parts and serves the intents that span them.
#[derive(Clone)]
pub struct PairingHost {
	accounts: Arc<HostAccounts<RemoteAccountHolder>>,
	session: Arc<SsoRequestService>,
}

impl PairingHost {
	/// Bind account calls to a paired session whose changes reach the host that keeps its grants.
	pub fn new(services: Arc<RuntimeServices>, config: PairingHostConfig) -> Self {
		let registry = RingVrfRegistryStore::new(services.platform.clone());
		let mut session = None;
		let accounts = Arc::new_cyclic(|accounts: &Weak<HostAccounts<RemoteAccountHolder>>| {
			let owner: Weak<dyn PairedSessionOwner> = accounts.clone();
			let service = SsoRequestService::new(services.clone(), config, owner);
			session = Some(service.clone());
			HostAccounts::new(
				services.clone(),
				Arc::new(RemoteAccountHolder::new(
					service.clone(),
					registry.clone(),
					services.spawner.clone(),
				)),
				service.session_state(),
				registry,
				Arc::new(ProductConsent::new(services.platform.clone())),
				#[cfg(feature = "test-host")]
				Arc::default(),
			)
		});
		Self { accounts, session: session.expect("the session service is built with its owner") }
	}

	/// Test hook for [`Self::start_session_store_sync`].
	#[cfg(test)]
	pub fn start_session_store_sync_for_tests(self: Arc<Self>, spawner: Spawner) {
		self.start_session_store_sync(spawner);
	}

	/// `message_id` of the request the session's request channel carries.
	#[cfg(test)]
	pub fn newest_request_for_tests(&self) -> Option<String> {
		self.newest_request.lock().expect("newest request mutex poisoned").clone()
	}

	/// Change notifications the sync task has finished reconciling.
	#[cfg(test)]
	pub fn session_store_change_ticks_for_tests(&self) -> usize {
		self.session_store_change_ticks.load(Ordering::SeqCst)
	}

	/// Test alias for [`Self::start_remote_monitor_for_current_session`].
	#[cfg(test)]
	pub fn start_session_supervision_for_current_session(&self) {
		self.start_remote_monitor_for_current_session();
	}

	/// Pause replacement before the guarded installation for race tests.
	#[cfg(test)]
	pub fn pause_external_session_activation_for_tests(
		&self,
	) -> (oneshot::Receiver<()>, oneshot::Sender<()>) {
		let (entered_tx, entered_rx) = oneshot::channel();
		let (resume_tx, resume_rx) = oneshot::channel();
		*self
			.external_session_activation_pause
			.lock()
			.expect("external activation pause mutex poisoned") = Some((entered_tx, resume_rx));
		(entered_rx, resume_tx)
	}

	#[cfg(test)]
	async fn wait_at_external_session_activation_pause(&self) {
		let pause = self
			.external_session_activation_pause
			.lock()
			.expect("external activation pause mutex poisoned")
			.take();
		if let Some((entered, resume)) = pause {
			let _ = entered.send(());
			let _ = resume.await;
		}
	}

	/// Start the disconnect monitor when a session is already active.
	#[cfg(test)]
	pub fn start_remote_monitor_for_current_session(&self) {
		if let Some(session) = self.session_state.current() {
			self.start_disconnect_monitor(&session);
		}
	}

	/// Install a selected session through the real lifecycle.
	#[cfg(test)]
	pub async fn set_connected_session_for_tests(&self, session: SessionInfo) {
		self.set_connected_session(session).await;
	}

	#[cfg(test)]
	pub async fn has_auto_signing_key_for_tests(
		&self,
		session: &SessionInfo,
		product_id: &str,
	) -> Result<bool, AuthorityError> {
		self.auto_signing_key(session, product_id).await.map(|key| key.is_some())
	}

	#[cfg(test)]
	pub async fn remember_auto_signing_key_for_tests(
		&self,
		session: &SessionInfo,
		lifecycle_epoch: u64,
		product_id: &str,
		expected_product_subtree_public_key: [u8; 32],
		secret: [u8; 64],
		ring_vrf_domain_entropy: [u8; 32],
	) -> Result<(), AuthorityError> {
		self.remember_auto_signing_key(
			session,
			lifecycle_epoch,
			product_id,
			expected_product_subtree_public_key,
			secret,
			ring_vrf_domain_entropy,
		)
		.await
	}

	/// Register fixture material in the actual durable registry.
	#[cfg(test)]
	pub async fn register_ring_vrf_key_for_tests(
		&self,
		session: &SessionInfo,
		handle: latest::ProductAccountId,
		ring: latest::RingLocation,
		public_key: [u8; 32],
	) -> Result<(), RingVrfError> {
		self.ring_vrf_registry
			.register(session.public_key, handle, ring, public_key)
			.await
	}

	/// Cache sizes for lifecycle regression checks.
	#[cfg(test)]
	pub fn capability_cache_sizes_for_tests(&self) -> (usize, usize, usize, usize) {
		(
			self.statement_store_allowances
				.lock()
				.expect("statement-store allowance cache mutex poisoned")
				.len(),
			self.bulletin_allowances
				.lock()
				.expect("bulletin allowance cache mutex poisoned")
				.len(),
			self.product_subtrees
				.lock()
				.expect("product subtree cache mutex poisoned")
				.len(),
			self.auto_signing_keys
				.lock()
				.expect("AutoSigning key cache mutex poisoned")
				.len(),
		)
	}

	/// Account calls for products served by this host.
	pub fn accounts(&self) -> &Arc<HostAccounts<RemoteAccountHolder>> {
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
		self.accounts.forget_auto_signing_keys().await.map_err(|reason| {
			format!("session disconnected, but AutoSigning reset failed: {reason}")
		})?;
		self.session.forget_pairing_identity().await
	}
}
