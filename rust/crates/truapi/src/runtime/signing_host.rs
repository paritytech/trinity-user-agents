//! Wallet-local signing authority and incoming SSO handling.

mod allowance;
mod allowance_renewal;
mod local_activation;
pub mod ring_vrf;
mod sso_service;

use std::{
	collections::{HashMap, HashSet},
	sync::{Arc, Mutex},
};
use truapi::latest::{
	DerivationIndex, HostAccountCreateProofRequest, HostAccountGetAliasRequest,
	HostAccountListRingVrfKeysRequest, HostAccountRegisterRingVrfKeyRequest,
	HostAccountRingVrfSignRequest, ProductAccountId, RingLocation, RingLocationJunction,
};

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
		AccountCaller, AuthorityError, AuthoritySession, AutoSigningGrant, BulletinAllowanceKey,
		CreateTransactionAuthorityRequest, ProductAuthority, SignPayloadAuthorityRequest,
		SignRawAuthorityRequest, StatementStoreAllowanceKey, authority_session_validation_id,
	},
	connected_session_ui_info,
	product_consent::ProductConsent,
	ring_vrf_registry::{RingVrfRegistryStore, apply_ring_vrf_disclosure},
	validate_vrf_transcript,
};
use crate::{
	host_internal::{
		extrinsic::build_local_transaction,
		sso_messages::{OnExistingAllowancePolicy, ProductRequest, RingVrfError},
		transaction::sign_extrinsic_payload,
	},
	host_logic::{
		entropy::derive_product_entropy,
		product_account::{
			SR25519_SIGNING_CONTEXT, derivation_index_bytes, derive_full_person_ring_vrf_entropy,
			derive_identity_keypair, derive_lite_person_ring_vrf_entropy, derive_product_keypair,
			derive_product_subtree_keypair, derive_ring_vrf_entropy,
			derive_root_keypair_from_entropy, personhood_product_id,
		},
		raw_signing::raw_payload_bytes,
		session::{SessionInfo, SessionState},
	},
	runtime::{
		auth_state::AuthStateMachine,
		sso_service::SsoWithdrawals,
		statement_allowance::{self, CollectionCandidate, collection::PersonhoodCollection},
		vrf::{self, Vrf},
	},
};
use ring_vrf::{ChainRingResolver, MemberCandidate, RingResolver, context_bytes, create_proof};

/// The network suffix the unit tests configure their signing host for. `dot`
/// keeps the `peopl.dot` handles the RFC examples use meaningful; the
/// per-network behaviour has its own tests.
#[cfg(test)]
const TEST_NETWORK_SUFFIX: &str = "dot";

use crate::platform::{
	PermissionAuthorizationStatus, Platform, ProductContext, ProductId, SignVrfReview,
	UserConfirmationReview,
};
use truapi::{CallContext, CallError, latest, v01};
use zeroize::Zeroizing;

#[derive(Default)]
struct LocalGrantState {
	activation_generation: u64,
	auto_signing_grants: HashSet<([u8; 32], ProductId)>,
	/// Per product, the period its statement-store allowance was last seen
	/// registered in, and its key.
	// TODO(#1159): persist in core.sqlite3 allowance_records.
	statement_allowance_keys: HashMap<ProductId, (u32, StatementStoreAllowanceKey)>,
}

impl LocalGrantState {
	fn advance_activation(&mut self) {
		self.activation_generation = self
			.activation_generation
			.checked_add(1)
			.expect("local activation generation exhausted");
		self.auto_signing_grants.clear();
		self.statement_allowance_keys.clear();
	}

	fn revoke_product(&mut self, product_id: &ProductId) {
		self.activation_generation = self
			.activation_generation
			.checked_add(1)
			.expect("local activation generation exhausted");
		self.auto_signing_grants
			.retain(|(_, granted_product_id)| granted_product_id != product_id);
		self.statement_allowance_keys.remove(product_id);
	}

	fn statement_allowance_key(
		&self,
		activation_generation: u64,
		product_id: &ProductId,
		period: u32,
	) -> Result<Option<&StatementStoreAllowanceKey>, AuthorityError> {
		if self.activation_generation != activation_generation {
			return Err(AuthorityError::Disconnected);
		}
		Ok(self
			.statement_allowance_keys
			.get(product_id)
			.filter(|(cached_period, _)| *cached_period == period)
			.map(|(_, key)| key))
	}

	fn forget_statement_allowance_key(&mut self, product_id: &ProductId, public_key: [u8; 32]) {
		if self
			.statement_allowance_keys
			.get(product_id)
			.is_some_and(|(_, key)| key.public_key == public_key)
		{
			self.statement_allowance_keys.remove(product_id);
		}
	}

	fn remember_statement_allowance_key(
		&mut self,
		activation_generation: u64,
		product_id: ProductId,
		period: u32,
		key: StatementStoreAllowanceKey,
	) -> Result<(), AuthorityError> {
		if self.activation_generation != activation_generation {
			return Err(AuthorityError::Disconnected);
		}
		self.statement_allowance_keys.insert(product_id, (period, key));
		Ok(())
	}
}

/// Wallet-local account authority for a signing host.
pub struct SigningHost {
	services: Arc<RuntimeServices>,
	platform: Arc<dyn Platform>,
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
	/// In-memory grants and the activation generation that owns them. The
	/// lifecycle mutex also makes session replacement and snapshot creation
	/// atomic with respect to generation changes.
	local_grants: Mutex<LocalGrantState>,
	/// Durable RFC-0024 registry, scoped by the active wallet root.
	ring_vrf_registry: Arc<RingVrfRegistryStore>,
	/// Paired-host requests the pairing host can still withdraw.
	sso_withdrawals: SsoWithdrawals,
	renewal: allowance_renewal::RenewalState,
}

impl SigningHost {
	/// Build a locked signing host for the stack `services` runs.
	pub fn new(services: Arc<RuntimeServices>) -> Arc<Self> {
		let platform = services.platform.clone();
		let ring_resolver = ChainRingResolver::new(services.chain.clone());
		Arc::new(Self {
			services,
			platform: platform.clone(),
			#[cfg(feature = "test-host")]
			resource_controls: Arc::default(),
			#[cfg(feature = "test-host")]
			submit_preimages_locally: core::sync::atomic::AtomicBool::new(false),
			session_state: SessionState::new(),
			auth_state: AuthStateMachine::new(platform.clone()),
			consent: ProductConsent::new(platform.clone()),
			ring_resolver,
			root_entropy: Mutex::new(None),
			local_grants: Mutex::new(LocalGrantState::default()),
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
			crate::test_support::test_stack(network_suffix, [0; 32], [0xbb; 32], Some([0xcc; 32])),
			crate::test_support::test_spawner(),
		);
		Arc::new(Self {
			services,
			platform: platform.clone(),
			#[cfg(feature = "test-host")]
			resource_controls: Arc::default(),
			#[cfg(feature = "test-host")]
			submit_preimages_locally: core::sync::atomic::AtomicBool::new(false),
			session_state: SessionState::new(),
			auth_state: AuthStateMachine::new(platform.clone()),
			consent: ProductConsent::new(platform.clone()),
			ring_resolver,
			root_entropy: Mutex::new(None),
			local_grants: Mutex::new(LocalGrantState::default()),
			ring_vrf_registry: RingVrfRegistryStore::new(platform),
			sso_withdrawals: Default::default(),
			renewal: allowance_renewal::RenewalState::default(),
		})
	}

	/// Shared session holder for connection-status subscriptions.
	fn session_state(&self) -> Arc<SessionState> {
		self.session_state.clone()
	}

	/// The dotNS TLD of the stack this wallet serves: the suffix of every
	/// reserved identity it derives.
	pub fn network_suffix(&self) -> &str {
		self.services.stack().tld()
	}

	/// Current root entropy, or [`AuthorityError::Disconnected`] when no local
	/// session is active.
	pub fn root_entropy(&self) -> Result<Zeroizing<Vec<u8>>, AuthorityError> {
		self.root_entropy
			.lock()
			.expect("signing host entropy mutex poisoned")
			.clone()
			.ok_or(AuthorityError::Disconnected)
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

	fn grant_auto_signing(
		&self,
		session: &AuthoritySession,
		product_id: &ProductId,
	) -> Result<(), AuthorityError> {
		let (_, activation_generation) = self.require_current_session(session)?;
		let entropy = self.root_entropy()?;
		let root = derive_root_keypair_from_entropy(&entropy)?;
		let owner = root.public.to_bytes();
		if owner != session.public_key {
			return Err(AuthorityError::Disconnected);
		}
		derive_product_subtree_keypair(&root, product_id)?;

		let mut state = self.local_grants.lock().expect("local AutoSigning grant mutex poisoned");
		if state.activation_generation != activation_generation {
			return Err(AuthorityError::Disconnected);
		}
		state.auto_signing_grants.insert((owner, product_id.clone()));
		Ok(())
	}

	async fn allocate_statement_store_allowance_key(
		&self,
		session: &AuthoritySession,
		product_id: &ProductId,
		policy: OnExistingAllowancePolicy,
	) -> Result<StatementStoreAllowanceKey, allowance::AllowanceAllocationError> {
		let (_, activation_generation) = self.require_current_session(session)?;
		let allocation = allowance::allocate_statement_store_allowance(
			&self.services,
			self,
			session,
			product_id,
			policy,
		)
		.await?;
		let key = StatementStoreAllowanceKey::from_secret_bytes(allocation.secret)?;
		self.local_grants
			.lock()
			.expect("local AutoSigning grant mutex poisoned")
			.remember_statement_allowance_key(
				activation_generation,
				product_id.clone(),
				allocation.period,
				key.clone(),
			)?;
		Ok(key)
	}

	fn has_auto_signing_grant(
		&self,
		activation_generation: u64,
		owner: [u8; 32],
		calling_product_id: &ProductId,
		account_product_id: &ProductId,
	) -> bool {
		if account_product_id != calling_product_id {
			return false;
		}

		let state = self.local_grants.lock().expect("local AutoSigning grant mutex poisoned");
		state.activation_generation == activation_generation &&
			state.auto_signing_grants.contains(&(owner, calling_product_id.clone()))
	}

	/// Fence in-flight grant work and revoke this product's grants from the
	/// current local activation while preserving unrelated products.
	pub fn clear_product_state(&self, product_id: &ProductId) {
		self.consent.forget_allowed_once_for(product_id);
		self.local_grants
			.lock()
			.expect("local AutoSigning grant mutex poisoned")
			.revoke_product(product_id);
	}

	/// Derive the product's hard-subtree public key from the active session root.
	/// Returns `None` when no session is active.
	pub fn derive_subtree_public_key(
		&self,
		product_id: &ProductId,
	) -> Result<Option<[u8; 32]>, AuthorityError> {
		let Ok(entropy) = self.root_entropy() else {
			return Ok(None);
		};
		let root = derive_root_keypair_from_entropy(&entropy)?;
		let subtree = derive_product_subtree_keypair(&root, product_id)?;
		Ok(Some(subtree.public.to_bytes()))
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
		derive_identity_keypair(&entropy, self.network_suffix()).map_err(Into::into)
	}

	fn install_local_session(&self, secret: Zeroizing<Vec<u8>>, session: SessionInfo) {
		let mut state = self.local_grants.lock().expect("local AutoSigning grant mutex poisoned");
		state.advance_activation();
		self.consent.forget_allowed_once();
		*self.root_entropy.lock().expect("signing host entropy mutex poisoned") = Some(secret);
		self.session_state.set_session(session);
	}

	fn clear_local_session(&self) {
		let mut state = self.local_grants.lock().expect("local AutoSigning grant mutex poisoned");
		state.advance_activation();
		self.consent.forget_allowed_once();
		self.root_entropy.lock().expect("signing host entropy mutex poisoned").take();
		self.session_state.clear_session();
	}

	fn current_local_session(&self) -> Option<AuthoritySession> {
		let state = self.local_grants.lock().expect("local AutoSigning grant mutex poisoned");
		let session = self.session_state.current()?;
		Some(AuthoritySession::from_session_info(
			&session,
			local_session_validation_id(&session, state.activation_generation),
		))
	}

	fn require_current_session(
		&self,
		session: &AuthoritySession,
	) -> Result<(SessionInfo, u64), AuthorityError> {
		let state = self.local_grants.lock().expect("local AutoSigning grant mutex poisoned");
		let current = self.session_state.current().ok_or(AuthorityError::Disconnected)?;
		if local_session_validation_id(&current, state.activation_generation) !=
			session.validation_id
		{
			return Err(AuthorityError::Disconnected);
		}
		Ok((current, state.activation_generation))
	}

	fn ring_vrf_entropy(
		&self,
		session: &AuthoritySession,
		handle: &v01::ProductAccountId,
	) -> Result<Zeroizing<[u8; 32]>, RingVrfError> {
		self.require_current_session(session)?;
		let root = self.root_entropy()?;
		derive_ring_vrf_entropy(&root, &handle.dot_ns_identifier, &handle.derivation_index)
			.map(Zeroizing::new)
			.map_err(|err| RingVrfError::Unknown { reason: err.to_string() })
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
				entropy: derive_full_person_ring_vrf_entropy(&root, self.network_suffix()),
			},
			CollectionCandidate {
				collection: PersonhoodCollection::LitePeople,
				entropy: derive_lite_person_ring_vrf_entropy(&root, self.network_suffix()),
			},
		])
	}

	async fn register_builtin_personhood_keys_if_needed(
		&self,
		session: &AuthoritySession,
		owner: &ProductId,
	) -> Result<(), RingVrfError> {
		if owner != &personhood_product_id(self.network_suffix()) {
			return Ok(());
		}
		let chain_id = self.services.stack().chains().people;
		let entries = self.ring_vrf_registry.owner_entries(session.public_key, owner).await?;
		let missing = [(PersonhoodCollection::People, 0), (PersonhoodCollection::LitePeople, 1)]
			.into_iter()
			.filter(|(collection, index)| {
				!entries.iter().any(|entry| {
					entry.handle.derivation_index == DerivationIndex::Index(*index) &&
						entry.rings.iter().any(|ring| {
							ring.chain_id == chain_id &&
								matches!(
									ring.junctions.as_slice(),
									[RingLocationJunction::PalletInstance(_), RingLocationJunction::CollectionId(identifier)]
										if identifier.as_slice() == collection.identifier()
								)
						})
				})
			})
			.collect::<Vec<_>>();
		if missing.is_empty() {
			return Ok(());
		}
		let pallet_index = self.ring_resolver.members_pallet_index(&chain_id).await?;
		let vrf = vrf::load().await?;
		for (collection, index) in missing {
			let handle = ProductAccountId {
				dot_ns_identifier: owner.clone(),
				derivation_index: DerivationIndex::Index(index),
			};
			let entropy = self.ring_vrf_entropy(session, &handle)?;
			let public_key = vrf.member(&entropy)?;
			let ring = RingLocation {
				chain_id,
				junctions: vec![
					RingLocationJunction::PalletInstance(pallet_index),
					RingLocationJunction::CollectionId(collection.identifier().to_vec()),
				],
			};
			self.ring_vrf_registry
				.register(session.public_key, handle, ring, public_key)
				.await?;
		}
		Ok(())
	}

	async fn registered_ring_vrf_entry(
		&self,
		session: &AuthoritySession,
		handle: &v01::ProductAccountId,
	) -> Result<Option<v01::RegisteredRingVrfKey>, RingVrfError> {
		self.require_current_session(session)?;
		self.ring_vrf_registry.entry(session.public_key, handle).await
	}

	async fn resolve_ring_vrf_key_for_ring(
		&self,
		vrf: &Vrf,
		session: &AuthoritySession,
		handle: &v01::ProductAccountId,
		ring: &v01::RingLocation,
	) -> Result<Zeroizing<[u8; 32]>, RingVrfError> {
		let entry = self
			.registered_ring_vrf_entry(session, handle)
			.await?
			.ok_or(RingVrfError::KeyNotRegistered)?;
		if !entry.rings.contains(ring) {
			return Err(RingVrfError::KeyNotInRing);
		}
		let entropy = self.ring_vrf_entropy(session, handle)?;
		Self::require_matching_registered_public_key(vrf, &entry, &entropy)?;
		Ok(entropy)
	}

	async fn resolve_registered_ring_vrf_key(
		&self,
		vrf: &Vrf,
		session: &AuthoritySession,
		handle: &v01::ProductAccountId,
	) -> Result<Zeroizing<[u8; 32]>, RingVrfError> {
		let entry = self
			.registered_ring_vrf_entry(session, handle)
			.await?
			.ok_or(RingVrfError::KeyNotRegistered)?;
		let entropy = self.ring_vrf_entropy(session, handle)?;
		Self::require_matching_registered_public_key(vrf, &entry, &entropy)?;
		Ok(entropy)
	}

	fn require_matching_registered_public_key(
		vrf: &Vrf,
		entry: &v01::RegisteredRingVrfKey,
		entropy: &[u8; 32],
	) -> Result<(), RingVrfError> {
		if entry.public_key != Some(vrf.member(entropy)?) {
			return Err(RingVrfError::Unknown {
				reason: "registered ring-VRF public key does not match the active wallet"
					.to_string(),
			});
		}
		Ok(())
	}

	fn ring_vrf_member_candidate(
		&self,
		vrf: &Vrf,
		entropy: &[u8; 32],
	) -> Result<MemberCandidate, RingVrfError> {
		Ok(MemberCandidate { member: vrf.member(entropy)? })
	}

	/// Whether `calling_product_id` may act on `handle`'s ring-VRF key.
	///
	/// Delegates to [`crate::runtime::ring_vrf_key_access_granted`], which
	/// resolves the owner's manifest here rather than trusting the request: on
	/// this role the request can have arrived over the pairing wire.
	async fn require_ring_vrf_key_access(
		&self,
		calling_product_id: ProductId,
		handle: &v01::ProductAccountId,
	) -> Result<
		(v01::ProductAccountId, crate::runtime::product_manifest::AuthorizedAccess),
		RingVrfError,
	> {
		let access = crate::runtime::product_manifest::ring_vrf_key_access_granted(
			&self.services,
			self.platform.as_ref(),
			calling_product_id,
			handle,
		)
		.await?;
		Ok((
			v01::ProductAccountId {
				dot_ns_identifier: access.owner.clone(),
				derivation_index: handle.derivation_index.clone(),
			},
			access,
		))
	}

	async fn require_account_access(
		&self,
		cx: &CallContext,
		requester: &ProductId,
		owner: &ProductId,
	) -> Result<(), RingVrfError> {
		let access = self.consent.account_access(requester, owner, self.platform.as_ref());
		let status = super::until_cancelled(cx, access)
			.await?
			.map_err(|error| RingVrfError::Unknown { reason: error.to_string() })?;
		if status == PermissionAuthorizationStatus::Authorized {
			Ok(())
		} else {
			Err(RingVrfError::Rejected)
		}
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
		let (current, activation_generation) = self.require_current_session(session)?;
		let calling_product_id = caller.product_id().ok_or(AuthorityError::Rejected)?;
		let owner = &request.account.dot_ns_identifier;
		let auto_signing = matches!(caller, AccountCaller::Local { .. }) &&
			self.has_auto_signing_grant(
				activation_generation,
				current.public_key,
				calling_product_id,
				owner,
			);
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

#[async_trait::async_trait]
impl ProductAuthority for SigningHost {
	fn current_session(&self) -> Option<AuthoritySession> {
		self.current_local_session()
	}

	fn consent(&self) -> &ProductConsent {
		&self.consent
	}

	async fn product_subtree_public_key(
		&self,
		_cx: &CallContext,
		session: &AuthoritySession,
		product_id: &ProductId,
	) -> Result<[u8; 32], AuthorityError> {
		self.require_current_session(session)?;
		let entropy = self.root_entropy()?;
		let root = derive_root_keypair_from_entropy(&entropy)?;
		derive_product_subtree_keypair(&root, product_id)
			.map(|keypair| keypair.public.to_bytes())
			.map_err(Into::into)
	}

	async fn subtree_resolution_reaches_account_holder(
		&self,
		_session: &AuthoritySession,
		_product_id: &ProductId,
	) -> bool {
		// A signing host derives the subtree locally from root entropy, so
		// resolution never reaches a remote Account Holder and never prompts.
		false
	}

	async fn auto_signing_status(
		&self,
		session: &AuthoritySession,
		calling_product_id: &ProductId,
		account: &v01::ProductAccountId,
	) -> Result<AutoSigningGrant, AuthorityError> {
		// A stale session is not a grant, and is answered here rather than
		// raising a prompt against a session that no longer exists.
		// `grant_auto_signing` refuses to record a grant whose owner is not
		// the session's own key, so the session carries the owner a grant can
		// be keyed on and no root derivation is needed to answer this.
		let (current, activation_generation) = self.require_current_session(session)?;
		if super::authority::is_blessed_owner(calling_product_id, &account.dot_ns_identifier) ||
			self.has_auto_signing_grant(
				activation_generation,
				current.public_key,
				calling_product_id,
				&account.dot_ns_identifier,
			) {
			Ok(AutoSigningGrant::Active)
		} else {
			Ok(AutoSigningGrant::Absent)
		}
	}

	async fn sign_vrf(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		calling_product_id: &ProductId,
		request: v01::HostAccountSignVrfRequest,
	) -> Result<v01::VrfSignature, AuthorityError> {
		let product = ProductContext::new(calling_product_id.clone());
		self.sign_vrf_request(cx, session, AccountCaller::Local { product: &product }, request)
			.await
	}

	async fn sign_payload(
		&self,
		_cx: &CallContext,
		session: &AuthoritySession,
		_calling_product_id: Option<&ProductId>,
		request: SignPayloadAuthorityRequest,
	) -> Result<v01::HostSignPayloadResponse, AuthorityError> {
		self.require_current_session(session)?;
		let (keypair, payload) = match request {
			SignPayloadAuthorityRequest::Product(request) => {
				(self.product_keypair(&request.account)?, request.payload)
			},
			SignPayloadAuthorityRequest::LegacyAccount { product_account, request } => {
				(self.product_keypair(&product_account)?, request.payload)
			},
		};
		Ok(sign_extrinsic_payload(&keypair, payload)?)
	}

	async fn sign_raw(
		&self,
		_cx: &CallContext,
		session: &AuthoritySession,
		_calling_product_id: Option<&ProductId>,
		request: SignRawAuthorityRequest,
		watermarked: bool,
	) -> Result<v01::HostSignPayloadResponse, AuthorityError> {
		let (keypair, payload) = match request {
			SignRawAuthorityRequest::Product(request) => {
				(self.product_keypair(&request.account)?, request.payload)
			},
			SignRawAuthorityRequest::LegacyAccount { account, request } => {
				let keypair = self.identity_keypair()?;
				if keypair.public.to_bytes() != account {
					return Err(AuthorityError::Unavailable {
						reason: "signing host: the requested legacy account is not available in \
                                 this CLI wallet"
							.to_string(),
					});
				}
				(keypair, request.payload)
			},
		};
		self.require_current_session(session)?;
		let message = raw_payload_bytes(payload, watermarked)?;
		let signature = keypair
			.secret
			.sign_simple(SR25519_SIGNING_CONTEXT, &message, &keypair.public)
			.to_bytes();
		Ok(v01::HostSignPayloadResponse { signature: signature.to_vec(), signed_transaction: None })
	}

	async fn create_transaction(
		&self,
		_cx: &CallContext,
		session: &AuthoritySession,
		_calling_product_id: Option<&ProductId>,
		request: CreateTransactionAuthorityRequest,
	) -> Result<v01::HostCreateTransactionResponse, AuthorityError> {
		self.require_current_session(session)?;
		match request {
			CreateTransactionAuthorityRequest::Product(payload) => {
				// The product account is authoritative and caller-scoping is
				// enforced upstream, so the derived key defines the signer.
				let keypair = self.product_keypair(&payload.signer)?;
				build_local_transaction(
					&self.services.chain,
					&keypair,
					payload.genesis_hash,
					&payload.call_data,
					&payload.extensions,
					payload.tx_ext_version,
				)
				.await
				.map_err(AuthorityError::from)
			},
			CreateTransactionAuthorityRequest::LegacyAccount { product_account, request } => {
				let keypair = self.product_keypair(&product_account)?;
				// Defense-in-depth: the slot-zero key must match the legacy
				// signer the caller asked for (also validated upstream). Never
				// sign with a diverging key.
				if keypair.public.to_bytes() != request.signer {
					return Err(AuthorityError::Unknown {
						reason: "signing host: legacy signer does not match the product \
                                 slot-zero account"
							.to_string(),
					});
				}
				build_local_transaction(
					&self.services.chain,
					&keypair,
					request.genesis_hash,
					&request.call_data,
					&request.extensions,
					request.tx_ext_version,
				)
				.await
				.map_err(AuthorityError::from)
			},
			CreateTransactionAuthorityRequest::IdentityAccount(request) => {
				let keypair = self.identity_keypair()?;
				if keypair.public.to_bytes() != request.signer {
					return Err(AuthorityError::Unavailable {
						reason: "signing host: the requested identity account is not available in \
                                 this CLI wallet"
							.to_string(),
					});
				}
				build_local_transaction(
					&self.services.chain,
					&keypair,
					request.genesis_hash,
					&request.call_data,
					&request.extensions,
					request.tx_ext_version,
				)
				.await
				.map_err(AuthorityError::from)
			},
		}
	}

	async fn account_alias(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		request: ProductRequest<HostAccountGetAliasRequest>,
	) -> Result<v01::ContextualAlias, RingVrfError> {
		self.require_current_session(session)?;
		// Aliases expose the same identity as `create_proof`, so both must enforce
		// the same RFC-0024 grant and context restrictions.
		let key_access = crate::runtime::product_manifest::ring_vrf_key_access_granted_or_consent(
			&self.services,
			self.platform.as_ref(),
			request.calling_product_id.clone(),
			&request.payload.key_handle,
		)
		.await;
		let granted = match key_access {
			Ok(access) => Some(access),
			Err(RingVrfError::NotAllowlisted) => None,
			Err(err) => return Err(err),
		};
		// The context is held to the same rule as on `create_proof`: the alias and
		// the proof come from one VRF evaluation.
		let key_handle = match granted {
			Some(access) => {
				crate::runtime::product_manifest::require_context_access(
					&self.services,
					&access,
					&request.payload.context,
				)
				.await?;
				v01::ProductAccountId {
					dot_ns_identifier: access.owner,
					derivation_index: request.payload.key_handle.derivation_index.clone(),
				}
			},
			None => {
				// Permission lookups and prompts must use the same canonical identifiers.
				let requester = &request.calling_product_id;
				let owner = request.payload.key_handle.dot_ns_identifier.clone();
				self.require_account_access(cx, requester, &owner).await?;
				v01::ProductAccountId {
					dot_ns_identifier: owner,
					derivation_index: request.payload.key_handle.derivation_index.clone(),
				}
			},
		};
		let vrf = vrf::load().await?;
		let entropy = self
			.resolve_ring_vrf_key_for_ring(
				&vrf,
				session,
				&key_handle,
				&request.payload.ring_location,
			)
			.await?;
		self.ring_resolver.validate(&request.payload.ring_location).await?;
		let context = context_bytes(&request.payload.context);
		let alias = vrf.alias(&entropy, &context)?;
		Ok(v01::ContextualAlias { context, alias: alias.to_vec() })
	}

	async fn create_proof(
		&self,
		_cx: &CallContext,
		session: &AuthoritySession,
		request: ProductRequest<HostAccountCreateProofRequest>,
	) -> Result<v01::HostAccountCreateProofResponse, RingVrfError> {
		self.require_current_session(session)?;
		let (key_handle, access) = self
			.require_ring_vrf_key_access(request.calling_product_id, &request.payload.key_handle)
			.await?;
		// A grant cannot expose the owner's alias in a third product's context unless
		// that product's manifest grants the caller `context`.
		crate::runtime::product_manifest::require_context_access(
			&self.services,
			&access,
			&request.payload.context,
		)
		.await?;
		let vrf = vrf::load().await?;
		let entropy = self
			.resolve_ring_vrf_key_for_ring(
				&vrf,
				session,
				&key_handle,
				&request.payload.ring_location,
			)
			.await?;
		let candidate = self.ring_vrf_member_candidate(&vrf, &entropy)?;
		let resolved =
			self.ring_resolver.resolve(&request.payload.ring_location, &[candidate]).await?;
		// Reject a stale request if the local session disconnected or changed
		// while its chain snapshot was being resolved.
		self.require_current_session(session)?;
		let context = context_bytes(&request.payload.context);
		let (proof, alias) =
			create_proof(&vrf, &entropy, &resolved, &context, &request.payload.message)?;
		Ok(v01::HostAccountCreateProofResponse {
			proof,
			contextual_alias: v01::ContextualAlias { context, alias: alias.to_vec() },
			ring_index: resolved.ring_index,
			ring_revision: resolved.ring_revision,
		})
	}

	async fn register_ring_vrf_key(
		&self,
		_cx: &CallContext,
		session: &AuthoritySession,
		request: ProductRequest<HostAccountRegisterRingVrfKeyRequest>,
	) -> Result<[u8; 32], RingVrfError> {
		self.require_current_session(session)?;
		self.ring_resolver.validate(&request.payload.ring).await?;

		let handle = v01::ProductAccountId {
			dot_ns_identifier: request.calling_product_id,
			derivation_index: request.payload.index,
		};
		let entropy = self.ring_vrf_entropy(session, &handle)?;
		let public_key = vrf::load().await?.member(&entropy)?;
		self.ring_vrf_registry
			.register(session.public_key, handle, request.payload.ring, public_key)
			.await?;
		Ok(public_key)
	}

	async fn list_ring_vrf_keys(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		request: ProductRequest<HostAccountListRingVrfKeysRequest>,
	) -> Result<Vec<v01::RegisteredRingVrfKey>, RingVrfError> {
		self.require_current_session(session)?;
		let owner = &request.payload.owner;
		let caller = &request.calling_product_id;
		if caller != owner {
			self.require_account_access(cx, caller, owner).await?;
		}

		self.register_builtin_personhood_keys_if_needed(session, owner).await?;
		let mut entries = self.ring_vrf_registry.owner_entries(session.public_key, owner).await?;
		self.require_current_session(session)?;
		apply_ring_vrf_disclosure(&mut entries, request.payload.disclosure);
		Ok(entries)
	}

	async fn ring_vrf_sign(
		&self,
		_cx: &CallContext,
		session: &AuthoritySession,
		request: ProductRequest<HostAccountRingVrfSignRequest>,
	) -> Result<Vec<u8>, RingVrfError> {
		self.require_current_session(session)?;
		let (key_handle, _access) = self
			.require_ring_vrf_key_access(request.calling_product_id, &request.payload.key_handle)
			.await?;
		let vrf = vrf::load().await?;
		let entropy = self.resolve_registered_ring_vrf_key(&vrf, session, &key_handle).await?;
		vrf.sign(&entropy, &request.payload.message)
	}

	async fn allocate_resources(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		product_id: &ProductId,
		request: v01::HostRequestResourceAllocationRequest,
	) -> Result<v01::HostRequestResourceAllocationResponse, AuthorityError> {
		self.require_current_session(session)?;
		#[cfg(feature = "test-host")]
		if self.resource_controls.grants_allowances_unchecked() {
			// Nothing is allocated and no proof is built: a suite in this mode
			// learns that its product handles a grant, not that a host would
			// have given one. A withheld tag is still refused here, so the one
			// resource a suite wants to prove its product lives without stays
			// refused while the rest are granted.
			return Ok(v01::HostRequestResourceAllocationResponse {
				outcomes: request
					.resources
					.iter()
					.map(|resource| {
						if self.resource_controls.withholds(resource) {
							v01::AllocationOutcome::Rejected
						} else {
							v01::AllocationOutcome::Allocated
						}
					})
					.collect(),
			});
		}
		let mut outcomes = Vec::with_capacity(request.resources.len());
		for resource in request.resources {
			if let Some(reason) = cx.cancel().reason() {
				return Err(super::authority_cancellation_error(cx, reason));
			}
			// Checked before the work, not after: withholding is the suite
			// saying this resource is refused, so performing the allocation and
			// then reporting a refusal would leave the two disagreeing.
			#[cfg(feature = "test-host")]
			if self.resource_controls.withholds(&resource) {
				outcomes.push(v01::AllocationOutcome::Rejected);
				continue;
			}
			let outcome = match resource {
				v01::AllocatableResource::StatementStoreAllowance => self
					.allocate_statement_store_allowance_key(
						session,
						product_id,
						OnExistingAllowancePolicy::Increase,
					)
					.await
					.map(|_| v01::AllocationOutcome::Allocated),
				v01::AllocatableResource::BulletinAllowance => {
					allowance::allocate_bulletin_allowance(
						&self.services,
						self,
						session,
						product_id,
						OnExistingAllowancePolicy::Increase,
					)
					.await
					.map(|_| v01::AllocationOutcome::Allocated)
				},
				v01::AllocatableResource::SmartContractAllowance(index) => {
					allowance::allocate_smart_contract_allowance(
						&self.services,
						self,
						session,
						product_id,
						index,
						OnExistingAllowancePolicy::Increase,
					)
					.await
					.map(|()| v01::AllocationOutcome::Allocated)
				},
				v01::AllocatableResource::AutoSigning => self
					.grant_auto_signing(session, product_id)
					.map(|_| v01::AllocationOutcome::Allocated)
					.map_err(allowance::AllowanceAllocationError::Authority),
			};
			match outcome {
				Ok(outcome) => outcomes.push(outcome),
				Err(reason) => {
					tracing::warn!(%product_id, %reason, "direct resource allocation item failed");
					outcomes.push(v01::AllocationOutcome::NotAvailable);
				},
			}
		}
		Ok(v01::HostRequestResourceAllocationResponse { outcomes })
	}

	async fn statement_store_allowance_key(
		&self,
		_cx: &CallContext,
		session: &AuthoritySession,
		product_id: &ProductId,
	) -> Result<StatementStoreAllowanceKey, AuthorityError> {
		let (_, activation_generation) = self.require_current_session(session)?;
		#[cfg(feature = "test-host")]
		self.resource_controls
			.refuse_withheld(&v01::AllocatableResource::StatementStoreAllowance)?;
		let period = statement_allowance::slot::current_period(allowance::current_unix_secs()?);
		if let Some(key) = self
			.local_grants
			.lock()
			.expect("local AutoSigning grant mutex poisoned")
			.statement_allowance_key(activation_generation, product_id, period)?
		{
			return Ok(key.clone());
		}
		self.allocate_statement_store_allowance_key(
			session,
			product_id,
			OnExistingAllowancePolicy::Ignore,
		)
		.await
		.map_err(Into::into)
	}

	#[cfg(feature = "test-host")]
	fn submits_preimages_locally(&self) -> bool {
		self.resource_controls.grants_allowances_unchecked() ||
			self.submit_preimages_locally.load(core::sync::atomic::Ordering::Relaxed)
	}

	fn forget_statement_store_allowance_key(&self, product_id: &ProductId, public_key: [u8; 32]) {
		self.local_grants
			.lock()
			.expect("local AutoSigning grant mutex poisoned")
			.forget_statement_allowance_key(product_id, public_key);
	}

	async fn bulletin_allowance_key(
		&self,
		_cx: &CallContext,
		session: &AuthoritySession,
		product_id: &ProductId,
	) -> Result<BulletinAllowanceKey, AuthorityError> {
		self.require_current_session(session)?;
		#[cfg(feature = "test-host")]
		self.resource_controls
			.refuse_withheld(&v01::AllocatableResource::BulletinAllowance)?;
		let secret = allowance::allocate_bulletin_allowance(
			&self.services,
			self,
			session,
			product_id,
			OnExistingAllowancePolicy::Ignore,
		)
		.await?;
		BulletinAllowanceKey::from_secret_bytes(secret)
	}

	async fn refresh_bulletin_allowance_key(
		&self,
		_cx: &CallContext,
		session: &AuthoritySession,
		product_id: &ProductId,
	) -> Result<BulletinAllowanceKey, AuthorityError> {
		self.require_current_session(session)?;
		#[cfg(feature = "test-host")]
		self.resource_controls
			.refuse_withheld(&v01::AllocatableResource::BulletinAllowance)?;
		let secret = allowance::allocate_bulletin_allowance(
			&self.services,
			self,
			session,
			product_id,
			OnExistingAllowancePolicy::Increase,
		)
		.await?;
		BulletinAllowanceKey::from_secret_bytes(secret)
	}

	async fn sign_statement_store_product_payload(
		&self,
		_cx: &CallContext,
		session: &AuthoritySession,
		_calling_product_id: Option<&ProductId>,
		account: v01::ProductAccountId,
		payload: Vec<u8>,
	) -> Result<[u8; 64], AuthorityError> {
		self.require_current_session(session)?;
		let keypair = self.product_keypair(&account)?;
		Ok(keypair
			.secret
			.sign_simple(SR25519_SIGNING_CONTEXT, &payload, &keypair.public)
			.to_bytes())
	}

	fn derive_entropy(
		&self,
		session: &AuthoritySession,
		product_id: &ProductId,
		context: &[u8],
	) -> Result<[u8; 32], AuthorityError> {
		self.require_current_session(session)?;
		let entropy = self.root_entropy()?;
		derive_product_entropy(&entropy, product_id, context)
			.map_err(|err| AuthorityError::Unknown { reason: err.to_string() })
	}

	fn contacts_handle_key(&self, session: &AuthoritySession) -> Result<[u8; 32], AuthorityError> {
		self.require_current_session(session)?;
		// The same 32 bytes a pairing host receives from the wallet, so one
		// contact hashes alike whichever role the user is running.
		let root_entropy_source =
			crate::host_logic::entropy::root_entropy_source(&self.root_entropy()?);
		Ok(crate::runtime::contacts::handle_key_from_root_source(&root_entropy_source))
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
