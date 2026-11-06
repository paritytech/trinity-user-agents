//! Wallet-local signing authority and incoming SSO handling.

mod local_activation;
pub mod ring_vrf;
mod sso_service;
mod wallet_account_holder;

use std::sync::{Arc, Mutex};

use wallet_account_holder::{allowance, allowance_renewal};

#[cfg(not(target_arch = "wasm32"))]
use allowance_renewal::StatementRenewalError;
pub use allowance_renewal::StatementRenewalTarget;
#[cfg(not(target_arch = "wasm32"))]
pub use allowance_renewal::TrackedStatementRenewalTarget;
pub use local_activation::LocalActivation;
pub use sso_service::SigningHostSsoService;

use super::{
	HostSession, RuntimeServices,
	authority::{
		AccountCaller, Allowance, AuthorityError, AuthoritySession, AutoSigningKey, GrantKeeping,
		StatementStoreAllowanceKey, authority_session_validation_id,
	},
	connected_session_ui_info,
	host_grants::HostGrantStore,
	product_consent::ProductConsent,
	ring_vrf_registry::RingVrfRegistryStore,
	validate_vrf_transcript,
};
use crate::{
	host_internal::sso_messages::{OnExistingAllowancePolicy, RingVrfError},
	host_logic::{
		product_account::{
			derivation_index_bytes, derive_full_person_ring_vrf_entropy, derive_identity_keypair,
			derive_lite_person_ring_vrf_entropy, derive_product_keypair,
			derive_product_subtree_keypair, derive_ring_vrf_domain_entropy,
			derive_root_keypair_from_entropy,
		},
		session::{SessionInfo, SessionState},
	},
	runtime::{
		auth_state::AuthStateMachine,
		sso_service::SsoWithdrawals,
		statement_allowance::{CollectionCandidate, collection::PersonhoodCollection},
	},
};
use ring_vrf::{ChainRingResolver, RingResolver};

/// The network suffix the unit tests configure their signing host for. `dot`
/// keeps the `peopl.dot` handles the RFC examples use meaningful; the
/// per-network behaviour has its own tests.
#[cfg(test)]
const TEST_NETWORK_SUFFIX: &str = "dot";

use crate::platform::{Platform, ProductContext, ProductId, SignVrfReview, UserConfirmationReview};
use truapi::{CallContext, CallError, latest, v01};
use zeroize::Zeroizing;

/// Wallet-local account authority for a signing host.
pub struct SigningHost {
	services: Arc<RuntimeServices>,
	platform: Arc<dyn Platform>,
	/// The dotNS TLD of the network this wallet serves, from
	/// [`crate::platform::SigningHostConfig::network_suffix`]. Every reserved
	/// RFC-0022 derivation (`uid.<suffix>`, `peopl.<suffix>`) ends in it.
	network_suffix: String,
	session_state: Arc<SessionState>,
	auth_state: AuthStateMachine,
	/// Account-access decisions and reviews for local and paired callers.
	consent: ProductConsent,
	ring_resolver: Arc<dyn RingResolver>,
	/// Unchecked grants and withheld resources for test-host suites. Compiled
	/// only into a build carrying `test-host`, which is off by default and which
	/// neither the production browser bundle nor a released native host
	/// enables, so a shipping host has no way to set it.
	#[cfg(feature = "test-host")]
	resource_controls: Arc<crate::runtime::test_resource_controls::TestResourceControls>,
	/// Keep preimage submissions in the core instead of the Bulletin chain,
	/// also implied by unchecked allowance grants.
	#[cfg(feature = "test-host")]
	submit_preimages_locally: core::sync::atomic::AtomicBool,
	/// Root BIP-39 entropy held only while a session is active.
	root_entropy: Mutex<Option<Zeroizing<Vec<u8>>>>,
	/// Generation of the installed wallet. Its mutex also makes session
	/// replacement and snapshot creation atomic with respect to generation
	/// changes.
	activation_generation: Mutex<u64>,
	/// Grants kept for this wallet activation.
	grants: HostGrantStore,
	/// Durable RFC-0024 registry, scoped by the active wallet root.
	ring_vrf_registry: Arc<RingVrfRegistryStore>,
	/// Paired-host requests the pairing host can still withdraw.
	sso_withdrawals: SsoWithdrawals,
	renewal: allowance_renewal::RenewalState,
}

impl SigningHost {
	/// Build a locked signing host for the network's dotNS suffix.
	pub fn new(services: Arc<RuntimeServices>, network_suffix: String) -> Arc<Self> {
		let platform = services.platform.clone();
		let ring_resolver = ChainRingResolver::new(services.chain.clone());
		let session_state = SessionState::new();
		Arc::new(Self {
			services,
			platform: platform.clone(),
			network_suffix,
			#[cfg(feature = "test-host")]
			resource_controls: Arc::default(),
			#[cfg(feature = "test-host")]
			submit_preimages_locally: core::sync::atomic::AtomicBool::new(false),
			session_state: session_state.clone(),
			auth_state: AuthStateMachine::new(platform.clone()),
			consent: ProductConsent::new(platform.clone()),
			ring_resolver,
			root_entropy: Mutex::new(None),
			activation_generation: Mutex::new(0),
			grants: HostGrantStore::new(platform.clone(), GrantKeeping::Wallet, session_state),
			ring_vrf_registry: RingVrfRegistryStore::new(platform),
			sso_withdrawals: Default::default(),
			renewal: allowance_renewal::RenewalState::default(),
		})
	}

	/// Whether allocation is answered as granted without performing it.
	#[cfg(feature = "test-host")]
	pub fn grants_allowances_unchecked(&self) -> bool {
		self.resource_controls.grants_allowances_unchecked()
	}

	/// Keep preimage submissions in the core instead of the Bulletin chain.
	///
	/// For test hosts only, with the `test-host` feature enabled.
	#[cfg(feature = "test-host")]
	pub fn set_submit_preimages_locally(&self, local: bool) {
		self.submit_preimages_locally
			.store(local, core::sync::atomic::Ordering::Relaxed);
	}

	/// Answer resource allocation as granted without performing it in test hosts.
	#[cfg(feature = "test-host")]
	pub fn set_grant_allowances_unchecked(&self, granted: bool) {
		self.resource_controls.set_grant_allowances_unchecked(granted);
	}

	/// Replace refused resource tags; SmartContractAllowance covers every index.
	#[cfg(feature = "test-host")]
	pub(crate) fn set_withheld_resources(&self, tags: Vec<String>) {
		self.resource_controls.set_withheld_resources(tags);
	}

	/// The shared services this role was built over, for tests that also need
	/// to build a product runtime against the same platform and cache.
	#[cfg(test)]
	fn services(&self) -> Arc<RuntimeServices> {
		self.services.clone()
	}

	#[cfg(test)]
	fn new_with_ring_resolver(
		platform: Arc<dyn Platform>,
		ring_resolver: Arc<dyn RingResolver>,
	) -> Arc<Self> {
		Self::new_with_ring_resolver_on(platform, ring_resolver, TEST_NETWORK_SUFFIX)
	}

	#[cfg(test)]
	fn new_with_ring_resolver_on(
		platform: Arc<dyn Platform>,
		ring_resolver: Arc<dyn RingResolver>,
		network_suffix: &str,
	) -> Arc<Self> {
		let services = RuntimeServices::new(
			platform.clone(),
			crate::platform::HostInfo {
				name: "Polkadot Mobile".to_string(),
				icon: None,
				version: None,
				platform: truapi::latest::HostPlatform::Unknown,
			},
			[0; 32],
			[0xbb; 32],
			[0xcc; 32],
			crate::test_support::test_spawner(),
		);
		let session_state = SessionState::new();
		Arc::new(Self {
			services,
			platform: platform.clone(),
			network_suffix: network_suffix.to_string(),
			#[cfg(feature = "test-host")]
			resource_controls: Arc::default(),
			#[cfg(feature = "test-host")]
			submit_preimages_locally: core::sync::atomic::AtomicBool::new(false),
			session_state: session_state.clone(),
			auth_state: AuthStateMachine::new(platform.clone()),
			consent: ProductConsent::new(platform.clone()),
			ring_resolver,
			root_entropy: Mutex::new(None),
			activation_generation: Mutex::new(0),
			grants: HostGrantStore::new(platform.clone(), GrantKeeping::Wallet, session_state),
			ring_vrf_registry: RingVrfRegistryStore::new(platform),
			sso_withdrawals: Default::default(),
			renewal: allowance_renewal::RenewalState::default(),
		})
	}

	/// Shared session holder for connection-status subscriptions.
	fn session_state(&self) -> Arc<SessionState> {
		self.session_state.clone()
	}

	/// The dotNS TLD of the network this wallet serves: the suffix of every
	/// reserved identity it derives.
	pub fn network_suffix(&self) -> &str {
		&self.network_suffix
	}

	fn product_subtree_secret(&self, product_id: &ProductId) -> Result<[u8; 64], AuthorityError> {
		let entropy = self.root_entropy()?;
		let root = derive_root_keypair_from_entropy(&entropy)?;
		derive_product_subtree_keypair(&root, product_id)
			.map(|keypair| keypair.secret.to_bytes())
			.map_err(Into::into)
	}

	/// Paired-host requests the pairing host can still withdraw.
	pub fn sso_withdrawals(&self) -> &SsoWithdrawals {
		&self.sso_withdrawals
	}

	/// The selected wallet session and the grant revision it is read under.
	fn grant_session(
		&self,
		session: &AuthoritySession,
	) -> Result<(SessionInfo, u64), AuthorityError> {
		let lifecycle = self.grants.lifecycle();
		let current = self.require_current_session(session)?;
		Ok((current, lifecycle.revision()))
	}

	async fn grant_auto_signing(
		&self,
		session: &AuthoritySession,
		grant_revision: u64,
		product_id: &ProductId,
	) -> Result<(), AuthorityError> {
		let current = self.require_current_session(session)?;
		let entropy = self.root_entropy()?;
		let root = derive_root_keypair_from_entropy(&entropy)?;
		if root.public.to_bytes() != session.public_key {
			return Err(AuthorityError::Disconnected);
		}
		let subtree = derive_product_subtree_keypair(&root, product_id)?;
		let ring_vrf_domain_entropy = derive_ring_vrf_domain_entropy(&entropy, product_id)?;
		self.grants
			.remember_auto_signing_key(
				&current,
				grant_revision,
				product_id,
				subtree.public.to_bytes(),
				AutoSigningKey::from_parts(subtree.secret.to_bytes(), ring_vrf_domain_entropy),
			)
			.await
	}

	async fn allocate_statement_store_allowance_key(
		&self,
		session: &AuthoritySession,
		grant_revision: u64,
		product_id: &ProductId,
		policy: OnExistingAllowancePolicy,
	) -> Result<StatementStoreAllowanceKey, allowance::AllowanceAllocationError> {
		let current = self.require_current_session(session)?;
		let allocation = allowance::allocate_statement_store_allowance(
			&self.services,
			self,
			session,
			product_id,
			policy,
		)
		.await?;
		let key = StatementStoreAllowanceKey::from_secret_bytes(allocation.secret)?;
		self.grants
			.retain_allowance(
				&current,
				grant_revision,
				product_id,
				&Allowance::StatementStore { key: key.clone(), period: Some(allocation.period) },
			)
			.await?;
		Ok(key)
	}

	async fn keeps_auto_signing_key(
		&self,
		session: &SessionInfo,
		calling_product_id: &ProductId,
		account_product_id: &ProductId,
	) -> bool {
		calling_product_id == account_product_id &&
			self.grants
				.auto_signing_key(session, calling_product_id)
				.await
				.is_ok_and(|key| key.is_some())
	}

	/// Clear one product's grants and Allow once answers.
	///
	/// The wallet session and other products' grants are kept.
	pub fn clear_product_state(&self, product_id: &ProductId) {
		self.consent.forget_allowed_once_for(product_id);
		self.grants.lifecycle().revoke_product(product_id);
	}

	/// Derive the product-account keypair for `account` from the root entropy.
	///
	/// The root keypair is recomputed per call (PBKDF2, 2048 rounds, via
	/// `substrate-bip39`) rather than cached: the signing host holds only the
	/// raw, zeroizable entropy, never an expanded secret key.
	fn product_keypair_with_owner(
		&self,
		account: &v01::ProductAccountId,
	) -> Result<([u8; 32], schnorrkel::Keypair), AuthorityError> {
		let entropy = self.root_entropy()?;
		let root = derive_root_keypair_from_entropy(&entropy)?;
		let owner = root.public.to_bytes();
		derive_product_keypair(
			&root,
			&account.dot_ns_identifier,
			derivation_index_bytes(&account.derivation_index),
		)
		.map(|keypair| (owner, keypair))
		.map_err(Into::into)
	}

	fn product_keypair(
		&self,
		account: &v01::ProductAccountId,
	) -> Result<schnorrkel::Keypair, AuthorityError> {
		self.product_keypair_with_owner(account).map(|(_, keypair)| keypair)
	}

	fn identity_keypair(&self) -> Result<schnorrkel::Keypair, AuthorityError> {
		let entropy = self.root_entropy()?;
		derive_identity_keypair(&entropy, &self.network_suffix).map_err(Into::into)
	}

	/// Start a new wallet activation, holding its lock until the session changes.
	fn advance_activation(&self) -> std::sync::MutexGuard<'_, u64> {
		let mut generation =
			self.activation_generation.lock().expect("activation generation mutex poisoned");
		*generation = generation.checked_add(1).expect("local activation generation exhausted");
		generation
	}

	fn install_local_session(&self, secret: Zeroizing<Vec<u8>>, session: SessionInfo) {
		let mut lifecycle = self.grants.lifecycle();
		lifecycle.clear_memory();
		let _activation = self.advance_activation();
		self.consent.forget_allowed_once();
		*self.root_entropy.lock().expect("signing host entropy mutex poisoned") = Some(secret);
		self.session_state.set_session(session);
	}

	fn clear_local_session(&self) {
		let mut lifecycle = self.grants.lifecycle();
		lifecycle.clear_memory();
		let _activation = self.advance_activation();
		self.consent.forget_allowed_once();
		self.root_entropy.lock().expect("signing host entropy mutex poisoned").take();
		self.session_state.clear_session();
	}

	fn current_local_session(&self) -> Option<AuthoritySession> {
		let generation =
			self.activation_generation.lock().expect("activation generation mutex poisoned");
		let session = self.session_state.current()?;
		Some(AuthoritySession::from_session_info(
			&session,
			local_session_validation_id(&session, *generation),
		))
	}

	fn require_current_session(
		&self,
		session: &AuthoritySession,
	) -> Result<SessionInfo, AuthorityError> {
		let generation =
			self.activation_generation.lock().expect("activation generation mutex poisoned");
		let current = self.session_state.current().ok_or(AuthorityError::Disconnected)?;
		if local_session_validation_id(&current, *generation) != session.validation_id {
			return Err(AuthorityError::Disconnected);
		}
		Ok(current)
	}

	/// Every personhood collection this wallet can derive allowance aliases for,
	/// widest slot budget first.
	///
	/// Wallet-internal allowance proofs use the reserved `peopl.<suffix>` keys
	/// the mobile hosts derive on the same network. Product-facing RFC-0024
	/// operations resolve registered handles, including the built-in keys
	/// registered when the personhood owner is listed.
	///
	/// Both entropies are always returned; which collections the person is
	/// actually a member of is settled on chain by looking for a ring that
	/// includes each member key, not by local state. That keeps the two hosts
	/// from disagreeing about personhood.
	fn reserved_person_collection_candidates(
		&self,
		session: &AuthoritySession,
	) -> Result<Vec<CollectionCandidate>, AuthorityError> {
		self.require_current_session(session)?;
		let root = self.root_entropy()?;
		Ok(vec![
			CollectionCandidate {
				collection: PersonhoodCollection::People,
				entropy: derive_full_person_ring_vrf_entropy(&root, &self.network_suffix),
			},
			CollectionCandidate {
				collection: PersonhoodCollection::LitePeople,
				entropy: derive_lite_person_ring_vrf_entropy(&root, &self.network_suffix),
			},
		])
	}

	pub async fn ring_vrf_providers(
		&self,
		ring: &v01::RingLocation,
	) -> Result<Vec<v01::ProductAccountId>, RingVrfError> {
		let session = self
			.current_local_session()
			.ok_or(RingVrfError::Unknown { reason: "no active session".to_string() })?;
		self.ring_vrf_registry.providers(session.public_key, ring).await
	}

	pub async fn selected_ring_vrf_provider(
		&self,
		ring: &v01::RingLocation,
	) -> Result<Option<v01::ProductAccountId>, RingVrfError> {
		let session = self
			.current_local_session()
			.ok_or(RingVrfError::Unknown { reason: "no active session".to_string() })?;
		self.ring_vrf_registry.selected_provider(session.public_key, ring).await
	}

	pub async fn select_ring_vrf_provider(
		&self,
		ring: v01::RingLocation,
		handle: v01::ProductAccountId,
	) -> Result<(), RingVrfError> {
		let session = self
			.current_local_session()
			.ok_or(RingVrfError::Unknown { reason: "no active session".to_string() })?;
		self.ring_vrf_registry.select_provider(session.public_key, ring, handle).await
	}

	async fn sign_vrf_request(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		caller: AccountCaller<'_>,
		request: v01::HostAccountSignVrfRequest,
	) -> Result<v01::VrfSignature, AuthorityError> {
		self.require_current_session(session)?;
		validate_vrf_transcript(&request).map_err(|reason| AuthorityError::Unknown { reason })?;
		let keypair = self.product_keypair(&request.account)?;
		let current = self.require_current_session(session)?;
		let calling_product_id = caller.product_id().ok_or(AuthorityError::Rejected)?;
		let owner = &request.account.dot_ns_identifier;
		let auto_signing = matches!(caller, AccountCaller::Local { .. }) &&
			self.keeps_auto_signing_key(&current, calling_product_id, owner).await;
		if !auto_signing {
			let review = UserConfirmationReview::SignVrf(SignVrfReview {
				calling_product_id: calling_product_id.clone(),
				request: request.clone(),
			});
			self.consent.review(cx, caller, self.platform.as_ref(), review).await?;
		}
		let (pre_output, proof) = crate::dynamic_vrf::sign_dynamic_vrf(
			&keypair,
			&request.transcript_label,
			request.items.iter().map(|item| (item.label.as_slice(), item.value.as_slice())),
		);
		Ok(v01::VrfSignature { pre_output, proof })
	}
}

#[cfg(not(target_arch = "wasm32"))]
impl SigningHost {
	/// Record statement-store accounts to keep renewed across periods.
	pub async fn track_statement_renewal_targets(
		&self,
		targets: Vec<StatementRenewalTarget>,
	) -> Result<(), StatementRenewalError> {
		allowance_renewal::track(self, targets).await
	}

	/// Every statement account the ledger currently tracks.
	pub async fn statement_renewal_targets(
		&self,
	) -> Result<Vec<TrackedStatementRenewalTarget>, StatementRenewalError> {
		allowance_renewal::list(self).await
	}

	/// Root public key the active identity records its fixed entries under.
	pub fn statement_renewal_owner_key(&self) -> Result<[u8; 32], StatementRenewalError> {
		allowance_renewal::active_owner_key(self)
	}

	/// Stop renewing one fixed statement account.
	pub async fn untrack_statement_renewal_account(
		&self,
		account_id: &[u8; 32],
	) -> Result<bool, StatementRenewalError> {
		allowance_renewal::untrack_account_for_signing_host(self, account_id).await
	}

	/// Run one statement-store renewal pass over the tracked targets.
	pub async fn renew_statement_allowances(
		&self,
	) -> Result<
		crate::runtime::statement_allowance::renewal::StatementRenewalReport,
		StatementRenewalError,
	> {
		allowance_renewal::renew_now(&self.services, self).await
	}

	/// The most recent pass the in-process loop ran.
	///
	/// A direct call to [`Self::renew_statement_allowances`] returns its own
	/// report, so only the loop needs somewhere to leave one.
	pub fn last_statement_renewal_report(
		&self,
	) -> Option<crate::runtime::statement_allowance::renewal::StatementRenewalReport> {
		self.renewal.last_report()
	}

	/// Start the periodic statement-store renewal loop. Idempotent.
	pub fn start_statement_allowance_renewal(self: &Arc<Self>) {
		allowance_renewal::start_renewal_loop(&self.services, self);
	}
}

#[async_trait::async_trait]
impl HostSession for SigningHost {
	/// Shared session holder for connection-status subscriptions.
	fn session_state(&self) -> Arc<SessionState> {
		SigningHost::session_state(self)
	}

	async fn request_login(
		&self,
		_product: &ProductContext,
	) -> Result<latest::HostRequestLoginResponse, CallError<latest::HostRequestLoginError>> {
		if let Some(session) = self.session_state.current() {
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
		self.current_local_session()?.primary_username().map(str::to_string)
	}
}

fn local_session_validation_id(session: &SessionInfo, activation_generation: u64) -> Vec<u8> {
	let mut id = authority_session_validation_id(session);
	id.extend_from_slice(b":activation:");
	id.extend_from_slice(&activation_generation.to_le_bytes());
	id
}

#[cfg(not(target_arch = "wasm32"))]
impl crate::nft::ItemKeyDeriver for SigningHost {
	fn derive_item_keys(
		&self,
		slots: &[crate::nft::ItemSlot],
	) -> Result<Vec<crate::nft::ItemKey>, AuthorityError> {
		let entropy = self.root_entropy()?;
		crate::nft::item_keys_from_entropy(&entropy, slots).map_err(AuthorityError::from)
	}
}

#[cfg(not(target_arch = "wasm32"))]
impl crate::nft::ItemSigner for SigningHost {
	fn item_signer(
		&self,
		slot: crate::nft::ItemSlot,
	) -> Result<crate::host_internal::extrinsic::Sr25519Signer, AuthorityError> {
		let entropy = self.root_entropy()?;
		crate::nft::item_signer_from_entropy(&entropy, slot).map_err(AuthorityError::from)
	}
}

#[cfg(not(target_arch = "wasm32"))]
impl crate::nft::ItemSecrets for SigningHost {
	fn derive_item_secrets(
		&self,
		slots: &[crate::nft::ItemSlot],
	) -> Result<Vec<crate::nft::ItemSecret>, AuthorityError> {
		let entropy = self.root_entropy()?;
		crate::nft::item_secrets_from_entropy(&entropy, slots).map_err(AuthorityError::from)
	}
}

/// The user's chat key, until the team settles which key products publish
/// for `Static` cheques.
#[cfg(not(target_arch = "wasm32"))]
impl crate::nft_purse::StaticReceiveSecret for SigningHost {
	fn static_receive_secret(
		&self,
	) -> Result<crate::host_logic::x25519_seal::X25519PrivateKey, AuthorityError> {
		let entropy = self.root_entropy()?;
		let chat_key = crate::host_logic::sso::pairing::derive_identity_chat_private_key(&entropy);
		Ok(crate::host_logic::x25519_seal::X25519PrivateKey::from(chat_key))
	}
}

#[cfg(test)]
mod tests;
