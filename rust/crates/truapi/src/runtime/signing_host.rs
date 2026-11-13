//! Native wallet lifecycle, shared host accounts and incoming SSO.
//!
//! Wallet replacement and retained grants use the same lifecycle guard.

mod local_activation;
pub mod ring_vrf;
mod wallet_account_holder;

use std::sync::Arc;

pub use local_activation::LocalActivation;
#[cfg(not(target_arch = "wasm32"))]
use wallet_account_holder::StatementRenewalError;
#[cfg(not(target_arch = "wasm32"))]
pub use wallet_account_holder::TrackedStatementRenewalTarget;
pub use wallet_account_holder::{StatementRenewalTarget, WalletAccountHolder};

#[cfg(not(target_arch = "wasm32"))]
use super::allowances::current_unix_secs;
use super::{
	AccountHolder, HostAccounts, HostSession, RuntimeServices, authority::AuthorityError,
	connected_session_ui_info, host_grants::HostGrantStore, product_consent::ProductConsent,
	ring_vrf_registry::RingVrfRegistryStore, sso_responder_service::SsoResponderService,
};
#[cfg(not(target_arch = "wasm32"))]
use crate::runtime::statement_allowance;
use crate::{host_logic::session::SessionState, runtime::auth_state::AuthStateMachine};
use ring_vrf::{ChainRingResolver, RingResolver};

/// The network suffix the unit tests configure their signing host for. `dot`
/// keeps the `peopl.dot` handles the RFC examples use meaningful; the
/// per-network behaviour has its own tests.
#[cfg(test)]
const TEST_NETWORK_SUFFIX: &str = "dot";

/// Fallback renewal tick delay when the system clock is unusable.
#[cfg(not(target_arch = "wasm32"))]
const CLOCK_FAILURE_TICK_DELAY: core::time::Duration = core::time::Duration::from_secs(3_600);

#[cfg(test)]
use crate::platform::Platform;
use crate::platform::{ProductContext, ProductId};
use truapi::{CallError, latest};

/// Native host lifecycle and retained grants over a shared wallet.
pub struct SigningHost {
	services: Arc<RuntimeServices>,
	wallet: Arc<WalletAccountHolder>,
	auth_state: AuthStateMachine,
	/// Product account policy and the grants kept for this wallet activation.
	accounts: Arc<HostAccounts<WalletAccountHolder>>,
	sso_responder: Arc<SsoResponderService>,
	#[cfg(not(target_arch = "wasm32"))]
	renewal_loop_started: std::sync::atomic::AtomicBool,
}

impl SigningHost {
	/// Build a locked signing host for the stack `services` runs.
	pub fn new(services: Arc<RuntimeServices>) -> Arc<Self> {
		let ring_resolver = ChainRingResolver::new(services.chain.clone());
		Self::with_ring_resolver(services, ring_resolver)
	}

	fn with_ring_resolver(
		services: Arc<RuntimeServices>,
		ring_resolver: Arc<dyn RingResolver>,
	) -> Arc<Self> {
		let platform = services.platform.clone();
		let ring_vrf_registry = RingVrfRegistryStore::new(platform.clone());
		let consent = Arc::new(ProductConsent::new(platform.clone()));
		let wallet = Arc::new(WalletAccountHolder::new(
			services.clone(),
			consent.clone(),
			ring_resolver.clone(),
			ring_vrf_registry.clone(),
		));
		let accounts = Arc::new(HostAccounts::new(
			services.clone(),
			wallet.clone(),
			Arc::new(HostGrantStore::new(
				platform.clone(),
				WalletAccountHolder::GRANT_KEEPING,
				wallet.session_state(),
			)),
			ring_resolver,
			ring_vrf_registry,
			consent.clone(),
			#[cfg(feature = "test-host")]
			wallet.resource_controls().clone(),
		));
		Arc::new(Self {
			services: services.clone(),
			sso_responder: SsoResponderService::new(services, wallet.clone(), consent),
			auth_state: AuthStateMachine::new(platform),
			accounts,
			wallet,
			#[cfg(not(target_arch = "wasm32"))]
			renewal_loop_started: std::sync::atomic::AtomicBool::new(false),
		})
	}

	/// Product account policy and grants shared by this host's connections.
	pub fn accounts(&self) -> &Arc<HostAccounts<WalletAccountHolder>> {
		&self.accounts
	}

	/// Answer resource allocation as granted without performing it in test hosts.
	#[cfg(feature = "test-host")]
	pub fn set_grant_allowances_unchecked(&self, granted: bool) {
		self.wallet.resource_controls().set_grant_allowances_unchecked(granted);
	}

	/// Replace resource tags refused by this test host.
	#[cfg(feature = "test-host")]
	pub fn set_withheld_resources(&self, tags: Vec<String>) {
		self.wallet.resource_controls().set_withheld_resources(tags);
	}

	#[cfg(test)]
	fn new_with_ring_resolver(
		platform: Arc<dyn Platform>,
		ring_resolver: Arc<dyn RingResolver>,
	) -> Arc<Self> {
		let services = RuntimeServices::new(
			platform,
			crate::platform::HostInfo {
				name: "Polkadot Mobile".to_string(),
				icon: None,
				version: None,
				platform: truapi::latest::HostPlatform::Unknown,
			},
			crate::test_support::test_stack(
				TEST_NETWORK_SUFFIX,
				[0; 32],
				[0xbb; 32],
				Some([0xcc; 32]),
			),
			crate::Spawner::for_test(),
		);
		Self::with_ring_resolver(services, ring_resolver)
	}

	/// Incoming SSO for this host's wallet, answered by its runtime.
	pub fn sso_responder(&self) -> Arc<SsoResponderService> {
		self.sso_responder.clone()
	}

	/// Clear one product's grants and Allow once answers.
	///
	/// The wallet session and other products' grants are kept.
	pub fn clear_product_state(&self, product_id: &ProductId) {
		self.accounts.forget_product(product_id);
	}

	/// The product's hard-subtree public key under the active wallet, or
	/// `None` while the wallet is locked.
	pub fn product_subtree_public_key(
		&self,
		product_id: &ProductId,
	) -> Result<Option<[u8; 32]>, AuthorityError> {
		self.wallet.derive_subtree_public_key(product_id)
	}

	fn clear_local_session(&self) {
		self.accounts.change_activation(|| self.wallet.clear());
	}
}

#[cfg(not(target_arch = "wasm32"))]
impl SigningHost {
	/// Start the periodic statement-store renewal loop. Idempotent.
	///
	/// The loop holds only a weak reference, so it exits when the host is dropped.
	pub fn start_statement_allowance_renewal(self: &Arc<Self>) {
		if self.renewal_loop_started.swap(true, std::sync::atomic::Ordering::SeqCst) {
			return;
		}
		let weak_host = Arc::downgrade(self);
		self.services.spawner.spawn(async move {
			loop {
				{
					let Some(host) = weak_host.upgrade() else {
						return;
					};
					host.wallet.renewal_tick().await;
				}
				let delay = match current_unix_secs() {
					Ok(now) => statement_allowance::renewal::next_tick_delay(now),
					Err(_) => CLOCK_FAILURE_TICK_DELAY,
				};
				futures_timer::Delay::new(delay).await;
			}
		});
	}

	/// Record statement accounts to keep renewed under the active wallet.
	pub async fn track_statement_renewal_targets(
		&self,
		targets: Vec<StatementRenewalTarget>,
	) -> Result<(), StatementRenewalError> {
		self.wallet.track_statement_renewal_targets(targets).await
	}

	/// Every statement account the renewal ledger tracks.
	pub async fn statement_renewal_targets(
		&self,
	) -> Result<Vec<TrackedStatementRenewalTarget>, StatementRenewalError> {
		self.wallet.statement_renewal_targets().await
	}

	/// Root public key the active wallet records its ledger entries under.
	pub fn statement_renewal_owner_key(&self) -> Result<[u8; 32], StatementRenewalError> {
		self.wallet.statement_renewal_owner_key()
	}

	/// Stop renewing one statement account.
	pub async fn untrack_statement_renewal_account(
		&self,
		account_id: &[u8; 32],
	) -> Result<bool, StatementRenewalError> {
		self.wallet.untrack_statement_renewal_account(account_id).await
	}

	/// Run one renewal pass now.
	pub async fn renew_statement_allowances(
		&self,
	) -> Result<statement_allowance::renewal::StatementRenewalReport, StatementRenewalError> {
		self.wallet.renew_statement_allowances().await
	}

	/// The most recent pass the renewal loop ran.
	///
	/// A direct call to [`Self::renew_statement_allowances`] returns its own
	/// report, so only the loop needs somewhere to leave one.
	pub fn last_statement_renewal_report(
		&self,
	) -> Option<statement_allowance::renewal::StatementRenewalReport> {
		self.wallet.last_statement_renewal_report()
	}
}

#[async_trait::async_trait]
impl HostSession for SigningHost {
	/// Shared session holder for connection-status subscriptions.
	fn session_state(&self) -> Arc<SessionState> {
		self.wallet.session_state()
	}

	async fn request_login(
		&self,
		_product: &ProductContext,
	) -> Result<latest::HostRequestLoginResponse, CallError<latest::HostRequestLoginError>> {
		if let Some(session) = self.wallet.session_state().current() {
			self.auth_state.connected(&connected_session_ui_info(&session));
			Ok(latest::HostRequestLoginResponse::AlreadyConnected)
		} else {
			// Wallet unlock and session activation are platform-owned.
			Ok(latest::HostRequestLoginResponse::Rejected)
		}
	}

	async fn disconnect(&self) {
		self.clear_local_session();
		self.auth_state.store_disconnected();
	}

	async fn primary_username(&self) -> Option<String> {
		self.wallet.current_session()?.primary_username().map(str::to_string)
	}
}

#[cfg(not(target_arch = "wasm32"))]
impl crate::nft::ItemKeyDeriver for SigningHost {
	fn derive_item_keys(
		&self,
		slots: &[crate::nft::ItemSlot],
	) -> Result<Vec<crate::nft::ItemKey>, AuthorityError> {
		self.wallet.item_keys(slots)
	}
}

#[cfg(not(target_arch = "wasm32"))]
impl crate::nft::ItemSigner for SigningHost {
	fn item_signer(
		&self,
		slot: crate::nft::ItemSlot,
	) -> Result<crate::host_internal::extrinsic::Sr25519Signer, AuthorityError> {
		self.wallet.item_signer(slot)
	}
}

#[cfg(not(target_arch = "wasm32"))]
impl crate::nft::ItemSecrets for SigningHost {
	fn derive_item_secrets(
		&self,
		slots: &[crate::nft::ItemSlot],
	) -> Result<Vec<crate::nft::ItemSecret>, AuthorityError> {
		self.wallet.item_secrets(slots)
	}
}

/// The user's chat key, until the team settles which key products publish
/// for `Static` cheques.
#[cfg(not(target_arch = "wasm32"))]
impl crate::nft_purse::StaticReceiveSecret for SigningHost {
	fn static_receive_secret(
		&self,
	) -> Result<crate::host_logic::x25519_seal::X25519PrivateKey, AuthorityError> {
		Ok(crate::host_logic::x25519_seal::X25519PrivateKey::from(
			self.wallet.identity_chat_private_key()?,
		))
	}
}

#[cfg(test)]
mod tests;
