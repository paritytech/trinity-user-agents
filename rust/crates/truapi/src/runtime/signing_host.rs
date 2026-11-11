//! Native wallet lifecycle, retained grants and incoming SSO.
//!
//! Wallet replacement and retained grants use the same lifecycle guard.
mod local_activation;
pub mod ring_vrf;
mod sso_service;
mod wallet_account_holder;

use std::sync::Arc;

use super::native_chat::{NativeChatContext, NativeChatRegistry};
pub use crate::runtime::statement_allowance::inspection::WalletAllowanceSnapshot;
pub use local_activation::LocalActivation;
pub use sso_service::SigningHostSsoService;
#[cfg(not(target_arch = "wasm32"))]
use wallet_account_holder::StatementRenewalError;
#[cfg(any(test, not(target_arch = "wasm32")))]
pub use wallet_account_holder::StatementRenewalTarget;
#[cfg(not(target_arch = "wasm32"))]
pub use wallet_account_holder::TrackedStatementRenewalTarget;
pub use wallet_account_holder::{LocalIdentity, LocalIdentityContext, WalletAccountHolder};

use super::{
	HostSession, RuntimeServices,
	allowances::{AllowanceResource, current_unix_secs},
	authority::{
		AccountCaller, AccountGrant, AccountGrantOutcome, AccountHolder, AccountInvocation,
		Allowance, AuthorityError, AuthoritySession, AutoSigningGrant, BulletinAllowanceKey,
		CreateTransactionAuthorityRequest, PaymentTopUpAuthorityError, ProductAuthority,
		ProductDeviceChatAuthorityError, ProductDeviceChatAuthorityRequest,
		SignPayloadAuthorityRequest, SignRawAuthorityRequest, StatementStoreAllowanceKey,
	},
	connected_session_ui_info,
	host_grants::{HostGrantGuard, HostGrantStore},
	product_consent::ProductConsent,
	ring_vrf_registry::RingVrfRegistryStore,
	validate_vrf_transcript,
};
use crate::{
	host_internal::sso_messages::{
		OnExistingAllowancePolicy, PaymentTopUpRequest, ProductRequest, RingVrfError,
	},
	host_logic::session::{SessionInfo, SessionState},
	runtime::{auth_state::AuthStateMachine, sso_service::SsoWithdrawals, statement_allowance},
};
use futures::StreamExt;
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
use crate::platform::{ProductContext, ProductId, SignVrfReview, UserConfirmationReview};
use truapi::{CallContext, CallError, latest, v01};

/// Native host lifecycle and retained grants over a shared wallet.
pub struct SigningHost {
	services: Arc<RuntimeServices>,
	wallet: Arc<WalletAccountHolder>,
	auth_state: AuthStateMachine,
	/// Account-access decisions and reviews for local and paired callers.
	consent: Arc<ProductConsent>,
	/// Grants kept for this wallet activation.
	grants: Arc<HostGrantStore>,
	/// Durable RFC-0024 registry, scoped by the active wallet root.
	ring_vrf_registry: Arc<RingVrfRegistryStore>,
	/// Wallet/network engine and product-scoped Host-only transport devices.
	native_chat: NativeChatRegistry,
	coinage_instance_id: Option<u32>,
	/// Paired-host requests the pairing host can still withdraw.
	sso_withdrawals: SsoWithdrawals,
	/// Keep preimage submissions in the core instead of the Bulletin chain,
	/// also implied by unchecked allowance grants.
	#[cfg(feature = "test-host")]
	submit_preimages_locally: core::sync::atomic::AtomicBool,
	#[cfg(not(target_arch = "wasm32"))]
	renewal_loop_started: std::sync::atomic::AtomicBool,
}

impl Drop for SigningHost {
	fn drop(&mut self) {
		self.clear_local_session();
	}
}

impl SigningHost {
	/// Build a locked signing host for the network's dotNS suffix.
	pub fn new(
		services: Arc<RuntimeServices>,
		network_suffix: String,
		coinage_instance_id: Option<u32>,
	) -> Arc<Self> {
		let ring_resolver = ChainRingResolver::new(services.chain.clone());
		Self::with_ring_resolver(services, network_suffix, ring_resolver, coinage_instance_id)
	}

	fn with_ring_resolver(
		services: Arc<RuntimeServices>,
		network_suffix: String,
		ring_resolver: Arc<dyn RingResolver>,
		coinage_instance_id: Option<u32>,
	) -> Arc<Self> {
		let platform = services.platform.clone();
		let ring_vrf_registry = RingVrfRegistryStore::new(platform.clone());
		let consent = Arc::new(ProductConsent::new(platform.clone(), services.permissions.clone()));
		let wallet = Arc::new(WalletAccountHolder::new(
			services.clone(),
			network_suffix,
			consent.clone(),
			ring_resolver,
			ring_vrf_registry.clone(),
		));
		Arc::new(Self {
			services,
			auth_state: AuthStateMachine::new(platform.clone()),
			consent,
			grants: Arc::new(HostGrantStore::new(
				platform,
				WalletAccountHolder::GRANT_KEEPING,
				wallet.session_state(),
			)),
			wallet,
			ring_vrf_registry,
			native_chat: NativeChatRegistry::default(),
			coinage_instance_id,
			sso_withdrawals: Default::default(),
			#[cfg(feature = "test-host")]
			submit_preimages_locally: core::sync::atomic::AtomicBool::new(false),
			#[cfg(not(target_arch = "wasm32"))]
			renewal_loop_started: std::sync::atomic::AtomicBool::new(false),
		})
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
			[0; 32],
			[0xbb; 32],
			[0xcc; 32],
			crate::test_support::test_spawner(),
		);
		Self::with_ring_resolver(services, TEST_NETWORK_SUFFIX.to_string(), ring_resolver, None)
	}

	/// The wallet that answers this host's paired peers.
	pub fn wallet(&self) -> &Arc<WalletAccountHolder> {
		&self.wallet
	}

	pub fn local_identity_context(&self) -> Result<LocalIdentityContext, latest::GenericError> {
		self.wallet.local_identity_context()
	}

	pub fn check_identity_activation(
		&self,
		activation_id: &str,
	) -> Result<(), latest::GenericError> {
		self.wallet.check_identity_activation(activation_id)
	}

	pub fn local_identity_auth_proof(
		&self,
		activation_id: &str,
		challenge: &[u8],
	) -> Result<Vec<u8>, latest::GenericError> {
		self.wallet.local_identity_auth_proof(activation_id, challenge)
	}

	pub async fn local_lite_registration_body(
		&self,
		activation_id: &str,
		username_base: &str,
		verifier: [u8; 32],
	) -> Result<String, latest::GenericError> {
		self.wallet
			.local_lite_registration_body(activation_id, username_base, verifier)
			.await
	}

	pub async fn refresh_local_identity(
		&self,
		activation_id: &str,
	) -> Result<LocalIdentity, latest::GenericError> {
		let identity = self.wallet.refresh_local_identity(activation_id).await?;
		if let Some(session) = self.wallet.session_state().current() {
			self.auth_state.connected(&connected_session_ui_info(&session));
		}
		Ok(identity)
	}

	pub async fn get_wallet_allowance_snapshot(
		&self,
		activation_id: &str,
		product_ids: Vec<String>,
	) -> Result<WalletAllowanceSnapshot, latest::GenericError> {
		self.wallet.get_wallet_allowance_snapshot(activation_id, product_ids).await
	}

	/// Paired-host requests the pairing host can still withdraw.
	pub fn sso_withdrawals(&self) -> &SsoWithdrawals {
		&self.sso_withdrawals
	}

	/// Clear one product's grants and Allow once answers.
	///
	/// The wallet session and other products' grants are kept.
	pub async fn clear_product_state(&self, product_id: &ProductId) -> Result<(), AuthorityError> {
		{
			let _permissions = self.services.permissions.invalidate_pending(Some(product_id)).await;
			self.consent.forget_allowed_once_for(product_id);
			self.grants.lifecycle().revoke_product(product_id);
			self.wallet.revoke_product(product_id);
		}
		if let Some(session) = self.wallet.current_session() {
			let context = self.native_chat_context(&session).map_err(|error| {
				AuthorityError::Unavailable {
					reason: format!("native Chat product state unavailable: {error:?}"),
				}
			})?;
			self.native_chat.forget_product(&context, product_id).await.map_err(|_| {
				AuthorityError::Unavailable {
					reason: "native Chat product state could not be cleared".into(),
				}
			})?;
		}
		Ok(())
	}

	/// The product's hard-subtree public key under the active wallet, or
	/// `None` while the wallet is locked.
	pub fn product_subtree_public_key(
		&self,
		product_id: &ProductId,
	) -> Result<Option<[u8; 32]>, AuthorityError> {
		self.wallet.derive_subtree_public_key(product_id)
	}

	/// Forget this activation's grants and Allow once answers, then make
	/// `change` before any grant can be used again.
	fn change_activation<T>(&self, change: impl FnOnce() -> T) -> T {
		let mut lifecycle = self.grants.lifecycle();
		lifecycle.clear_memory();
		self.consent.forget_allowed_once();
		self.services.contacts_session_changed();
		let changed = change();
		drop(lifecycle);
		self.native_chat.release();
		if let Some(session) = self.wallet.current_session() &&
			let Ok(context) = self.native_chat_context(&session)
		{
			self.native_chat.resume_wallet_recovery(context);
		}
		changed
	}

	fn clear_local_session(&self) {
		self.change_activation(|| self.wallet.clear());
	}

	fn hold_session(
		&self,
		session: &AuthoritySession,
	) -> Result<HostGrantGuard<'_>, AuthorityError> {
		let lifecycle = self.grants.lifecycle();
		self.wallet.require_current_session(session)?;
		Ok(lifecycle)
	}

	/// The selected wallet session and the grant revision it is read under.
	fn grant_session(
		&self,
		session: &AuthoritySession,
	) -> Result<(SessionInfo, u64), AuthorityError> {
		let lifecycle = self.hold_session(session)?;
		let session = self.wallet.session_state().current().ok_or(AuthorityError::Disconnected)?;
		Ok((session, lifecycle.revision()))
	}

	fn native_chat_context(
		&self,
		session: &AuthoritySession,
	) -> Result<NativeChatContext, ProductDeviceChatAuthorityError> {
		let entropy = self.wallet.native_chat_entropy(session)?;
		let wallet = Arc::downgrade(&self.wallet);
		let selected = session.clone();
		let session_valid = Arc::new(move || {
			wallet
				.upgrade()
				.is_some_and(|wallet| wallet.require_current_session(&selected).is_ok())
		});
		let grants = Arc::downgrade(&self.grants);
		let owner = session.public_key;
		let chat_session_granted = Arc::new(move |product: &ProductId| {
			grants
				.upgrade()
				.is_some_and(|grants| grants.lifecycle().chat_session_granted(owner, product))
		});
		Ok(NativeChatContext {
			services: self.services.clone(),
			session: session.clone(),
			entropy,
			session_valid,
			chat_session_granted,
			network_suffix: self.wallet.network_suffix().to_string(),
			genesis_hash: self.services.people_chain_genesis_hash,
			coinage_instance_id: self.coinage_instance_id,
		})
	}

	fn native_chat_product_context(
		&self,
		session: &AuthoritySession,
		product_id: &ProductId,
	) -> Result<NativeChatContext, ProductDeviceChatAuthorityError> {
		let revision = self.hold_session(session)?.product_revision(product_id);
		let mut context = self.native_chat_context(session)?;
		let session_valid = context.session_valid.clone();
		let grants = Arc::downgrade(&self.grants);
		let product_id = product_id.clone();
		context.session_valid = Arc::new(move || {
			session_valid() &&
				grants.upgrade().is_some_and(|grants| {
					grants.lifecycle().product_revision(&product_id) == revision
				})
		});
		Ok(context)
	}

	/// Read the current wallet's authenticated native Chat roster for the host shell.
	pub async fn get_native_chat_contacts(
		&self,
	) -> Result<super::NativeChatContactsSnapshot, v01::GenericError> {
		let session = self.wallet.current_session().ok_or_else(|| v01::GenericError {
			reason: "native Chat contacts require a local signing session".into(),
		})?;
		let context = self.native_chat_context(&session).map_err(|error| v01::GenericError {
			reason: format!("native Chat contacts unavailable: {error:?}"),
		})?;
		let snapshot = self.native_chat.contacts(&context).await.map_err(|error| {
			v01::GenericError { reason: format!("native Chat contacts unavailable: {error:?}") }
		})?;
		self.wallet
			.require_current_session(&session)
			.map_err(|error| v01::GenericError { reason: error.to_string() })?;
		Ok(snapshot)
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

	/// Retain one issued grant for the product, under the revision read before its review.
	async fn retain_grant(
		&self,
		authority_session: &AuthoritySession,
		revision: u64,
		product_id: &ProductId,
		grant: AccountGrant,
	) -> Result<(), AuthorityError> {
		let (session, _) = self.grant_session(authority_session)?;
		match grant {
			AccountGrant::Allowance(allowance) => {
				self.grants.retain_allowance(&session, revision, product_id, &allowance).await
			},
			AccountGrant::AutoSigning(key) => {
				let expected = self
					.wallet
					.derive_subtree_public_key(product_id)?
					.ok_or(AuthorityError::Disconnected)?;
				self.grants
					.remember_auto_signing_key(&session, revision, product_id, expected, key)
					.await
			},
			AccountGrant::SmartContract | AccountGrant::ProductStatementStore => self
				.hold_session(authority_session)?
				.require_product_revision(revision, product_id),
		}
	}

	/// Ask the wallet for one allowance on behalf of this host's own product.
	async fn local_allowance(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		product_id: &ProductId,
		resource: AllowanceResource,
		policy: OnExistingAllowancePolicy,
	) -> Result<AccountGrant, AuthorityError> {
		let product = ProductContext::new(product_id.clone());
		self.wallet
			.ensure_allowance(local_invocation(cx, session, &product), resource, policy)
			.await
	}

	async fn bulletin_grant(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		product_id: &ProductId,
		policy: OnExistingAllowancePolicy,
	) -> Result<BulletinAllowanceKey, AuthorityError> {
		self.wallet.require_current_session(session)?;
		#[cfg(feature = "test-host")]
		self.wallet
			.resource_controls()
			.refuse_withheld(&v01::AllocatableResource::BulletinAllowance)?;
		match self
			.local_allowance(cx, session, product_id, AllowanceResource::Bulletin, policy)
			.await?
		{
			AccountGrant::Allowance(Allowance::Bulletin(key)) => Ok(key),
			_ => Err(AuthorityError::Unknown {
				reason: "Unexpected Bulletin allowance response resource".to_string(),
			}),
		}
	}

	/// Import an existing iOS Lite identity without replacing a registered key.
	pub(crate) async fn ensure_legacy_lite_person_provider(
		&self,
		people_chain_id: [u8; 32],
		owner: &ProductId,
	) -> Result<(), RingVrfError> {
		self.wallet.ensure_legacy_lite_person_provider(people_chain_id, owner).await
	}

	/// Fund a product account only when it does not already hold a full PGAS claim.
	pub(crate) async fn prewarm_smart_contract_allowance(
		&self,
		product_id: &ProductId,
		derivation_index: v01::DerivationIndex,
	) -> Result<(), AuthorityError> {
		let session = self.wallet.current_session().ok_or(AuthorityError::Disconnected)?;
		self.wallet
			.allocate_smart_contract_allowance(
				&session,
				product_id,
				derivation_index,
				OnExistingAllowancePolicy::Ignore,
			)
			.await
			.map_err(|error| AuthorityError::Unknown { reason: error.to_string() })?;
		self.wallet.require_current_session(&session)
	}

	pub async fn ring_vrf_providers(
		&self,
		ring: &v01::RingLocation,
	) -> Result<Vec<v01::ProductAccountId>, RingVrfError> {
		let session = self
			.wallet
			.current_session()
			.ok_or(RingVrfError::Unknown { reason: "no active session".to_string() })?;
		self.ring_vrf_registry.providers(session.public_key, ring).await
	}

	pub async fn selected_ring_vrf_provider(
		&self,
		ring: &v01::RingLocation,
	) -> Result<Option<v01::ProductAccountId>, RingVrfError> {
		let session = self
			.wallet
			.current_session()
			.ok_or(RingVrfError::Unknown { reason: "no active session".to_string() })?;
		self.ring_vrf_registry.selected_provider(session.public_key, ring).await
	}

	pub async fn select_ring_vrf_provider(
		&self,
		ring: v01::RingLocation,
		handle: v01::ProductAccountId,
	) -> Result<(), RingVrfError> {
		let session = self
			.wallet
			.current_session()
			.ok_or(RingVrfError::Unknown { reason: "no active session".to_string() })?;
		self.ring_vrf_registry.select_provider(session.public_key, ring, handle).await
	}

	async fn sign_vrf_request(
		&self,
		invocation: AccountInvocation<'_>,
		request: v01::HostAccountSignVrfRequest,
	) -> Result<v01::VrfSignature, AuthorityError> {
		let (current, _) = self.grant_session(invocation.session)?;
		validate_vrf_transcript(&request).map_err(|reason| AuthorityError::Unknown { reason })?;
		let calling_product_id = invocation.caller.product_id().ok_or(AuthorityError::Rejected)?;
		let owner = &request.account.dot_ns_identifier;
		let auto_signing = matches!(invocation.caller, AccountCaller::Local { .. }) &&
			self.keeps_auto_signing_key(&current, calling_product_id, owner).await;
		if !auto_signing {
			let review = UserConfirmationReview::SignVrf(SignVrfReview {
				calling_product_id: calling_product_id.clone(),
				request: request.clone(),
			});
			self.consent
				.review(
					invocation.call,
					invocation.caller.clone(),
					self.services.platform.as_ref(),
					review,
				)
				.await?;
		}
		self.wallet.sign_vrf(invocation, request).await
	}
}

/// A local product's call into the wallet, under the session it selected.
fn local_invocation<'a>(
	cx: &'a CallContext,
	session: &'a AuthoritySession,
	product: &'a ProductContext,
) -> AccountInvocation<'a> {
	AccountInvocation { call: cx, session, caller: AccountCaller::Local { product } }
}

/// A product this host bound, or an SSO relay when the call names none.
fn relayed_or_local_invocation<'a>(
	cx: &'a CallContext,
	session: &'a AuthoritySession,
	product: Option<&'a ProductContext>,
) -> AccountInvocation<'a> {
	match product {
		Some(product) => local_invocation(cx, session, product),
		None => AccountInvocation {
			call: cx,
			session,
			caller: AccountCaller::Remote { product_id: None },
		},
	}
}

#[async_trait::async_trait]
impl ProductAuthority for SigningHost {
	fn chat_session_granted(&self, session: &AuthoritySession, product_id: &ProductId) -> bool {
		self.hold_session(session)
			.is_ok_and(|grants| grants.chat_session_granted(session.public_key, product_id))
	}

	fn current_session(&self) -> Option<AuthoritySession> {
		self.wallet.current_session()
	}

	fn consent(&self) -> &ProductConsent {
		&self.consent
	}

	fn grant_revision(&self) -> u64 {
		self.grants.lifecycle().revision()
	}

	fn grant_is_current(
		&self,
		session: &AuthoritySession,
		revision: u64,
		product_id: &ProductId,
	) -> bool {
		self.hold_session(session)
			.is_ok_and(|guard| guard.require_product_revision(revision, product_id).is_ok())
	}

	async fn refresh_session_identity(&self) -> Result<Option<AuthoritySession>, String> {
		let Ok(context) = self.local_identity_context() else {
			return Ok(None);
		};
		self.refresh_local_identity(&context.activation_id)
			.await
			.map_err(|error| error.reason)?;
		self.check_identity_activation(&context.activation_id)
			.map_err(|error| error.reason)?;
		Ok(self.wallet.current_session())
	}

	async fn product_device_chat(
		&self,
		_cx: &CallContext,
		session: &AuthoritySession,
		request: ProductDeviceChatAuthorityRequest,
	) -> Result<latest::HostProductDeviceChatResponse, ProductDeviceChatAuthorityError> {
		let context = self.native_chat_product_context(session, &request.calling_product_id)?;
		if request.session_consent {
			self.hold_session(session)?
				.grant_chat_session(session.public_key, &request.calling_product_id);
		}
		self.native_chat
			.execute(context, request.calling_product_id, request.operation)
			.await
			.map_err(ProductDeviceChatAuthorityError::Domain)
	}

	async fn payment_top_up(
		&self,
		_cx: &CallContext,
		session: &AuthoritySession,
		request: PaymentTopUpRequest,
	) -> Result<(), PaymentTopUpAuthorityError> {
		let product = request.calling_product_id.clone();
		let context =
			self.native_chat_product_context(session, &product)
				.map_err(|error| match error {
					ProductDeviceChatAuthorityError::Disconnected => AuthorityError::Disconnected,
					_ => AuthorityError::Unavailable {
						reason: "Wallet payment context unavailable".to_string(),
					},
				})?;
		let (_, payload) = request.into_parts();
		self.native_chat
			.top_up(context, product, payload)
			.await
			.map_err(PaymentTopUpAuthorityError::Domain)
	}

	fn profile_disclosure_changed(&self, session: &AuthoritySession) {
		if let Ok(context) = self.native_chat_context(session) {
			self.native_chat.relay_profile_disclosure(context);
		}
	}

	async fn contact_username(
		&self,
		session: &AuthoritySession,
		product_id: &ProductId,
		peer_identity: [u8; 32],
	) -> Option<String> {
		let context = self.native_chat_product_context(session, product_id).ok()?;
		self.native_chat.contact_username(&context, product_id, peer_identity).await
	}

	async fn product_subtree_public_key(
		&self,
		_cx: &CallContext,
		session: &AuthoritySession,
		product_id: &ProductId,
	) -> Result<[u8; 32], AuthorityError> {
		self.wallet.require_current_session(session)?;
		self.wallet
			.derive_subtree_public_key(product_id)?
			.ok_or(AuthorityError::Disconnected)
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
		let (current, _) = self.grant_session(session)?;
		if crate::runtime::authority::is_blessed_owner(
			calling_product_id,
			&account.dot_ns_identifier,
		) || self
			.keeps_auto_signing_key(&current, calling_product_id, &account.dot_ns_identifier)
			.await
		{
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
		self.sign_vrf_request(local_invocation(cx, session, &product), request).await
	}

	async fn sign_payload(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		calling_product_id: Option<&ProductId>,
		request: SignPayloadAuthorityRequest,
	) -> Result<v01::HostSignPayloadResponse, AuthorityError> {
		let product = calling_product_id.cloned().map(ProductContext::new);
		self.wallet
			.sign_payload(relayed_or_local_invocation(cx, session, product.as_ref()), request)
			.await
	}

	async fn sign_raw(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		calling_product_id: Option<&ProductId>,
		request: SignRawAuthorityRequest,
		watermarked: bool,
	) -> Result<v01::HostSignPayloadResponse, AuthorityError> {
		let product = calling_product_id.cloned().map(ProductContext::new);
		self.wallet
			.sign_raw(
				relayed_or_local_invocation(cx, session, product.as_ref()),
				request,
				watermarked,
			)
			.await
	}

	async fn create_transaction(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		calling_product_id: Option<&ProductId>,
		request: CreateTransactionAuthorityRequest,
	) -> Result<v01::HostCreateTransactionResponse, AuthorityError> {
		let product = calling_product_id.cloned().map(ProductContext::new);
		self.wallet
			.create_transaction(relayed_or_local_invocation(cx, session, product.as_ref()), request)
			.await
	}
	async fn account_alias(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		request: ProductRequest<latest::HostAccountGetAliasRequest>,
	) -> Result<v01::ContextualAlias, RingVrfError> {
		let product = ProductContext::new(request.calling_product_id);
		self.wallet
			.account_alias(local_invocation(cx, session, &product), request.payload)
			.await
	}

	async fn create_proof(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		request: ProductRequest<latest::HostAccountCreateProofRequest>,
	) -> Result<v01::HostAccountCreateProofResponse, RingVrfError> {
		let product = ProductContext::new(request.calling_product_id);
		self.wallet
			.create_proof(local_invocation(cx, session, &product), request.payload)
			.await
	}

	async fn register_ring_vrf_key(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		request: ProductRequest<latest::HostAccountRegisterRingVrfKeyRequest>,
	) -> Result<[u8; 32], RingVrfError> {
		let product = ProductContext::new(request.calling_product_id);
		self.wallet
			.register_ring_vrf_key(local_invocation(cx, session, &product), request.payload)
			.await
	}

	async fn list_ring_vrf_keys(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		request: ProductRequest<latest::HostAccountListRingVrfKeysRequest>,
	) -> Result<Vec<v01::RegisteredRingVrfKey>, RingVrfError> {
		let product = ProductContext::new(request.calling_product_id);
		self.wallet
			.list_ring_vrf_keys(local_invocation(cx, session, &product), request.payload)
			.await
	}

	async fn ring_vrf_sign(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		request: ProductRequest<latest::HostAccountRingVrfSignRequest>,
	) -> Result<Vec<u8>, RingVrfError> {
		let product = ProductContext::new(request.calling_product_id);
		self.wallet
			.ring_vrf_sign(local_invocation(cx, session, &product), request.payload)
			.await
	}

	async fn allocate_resources(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		grant_revision: u64,
		product_id: &ProductId,
		request: v01::HostRequestResourceAllocationRequest,
	) -> Result<v01::HostRequestResourceAllocationResponse, AuthorityError> {
		self.hold_session(session)?
			.require_product_revision(grant_revision, product_id)?;
		#[cfg(feature = "test-host")]
		if self.wallet.resource_controls().grants_allowances_unchecked() {
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
						if self.wallet.resource_controls().withholds(resource) {
							v01::AllocationOutcome::Rejected
						} else {
							v01::AllocationOutcome::Allocated
						}
					})
					.collect(),
			});
		}
		let product = ProductContext::new(product_id.clone());
		let mut grants = self
			.wallet
			.allocate_grants(
				local_invocation(cx, session, &product),
				request,
				OnExistingAllowancePolicy::Increase,
			)
			.await?;
		let mut outcomes = Vec::new();
		while let Some(outcome) = grants.next().await {
			// A lock partway through reports the rest unavailable rather than
			// failing the call, so the product still learns what it was granted.
			let outcome = match outcome {
				Err(error @ AuthorityError::Disconnected) => {
					AccountGrantOutcome::NotAvailable { reason: Some(error.to_string()) }
				},
				outcome => outcome?,
			};
			outcomes.push(match outcome {
				AccountGrantOutcome::Allocated(grant) => {
					match self.retain_grant(session, grant_revision, product_id, grant).await {
						Ok(()) => v01::AllocationOutcome::Allocated,
						Err(reason) => {
							tracing::warn!(%product_id, %reason, "direct resource allocation item failed");
							v01::AllocationOutcome::NotAvailable
						},
					}
				},
				AccountGrantOutcome::Rejected => v01::AllocationOutcome::Rejected,
				AccountGrantOutcome::NotAvailable { reason } => {
					if let Some(reason) = reason {
						tracing::warn!(%product_id, %reason, "direct resource allocation item failed");
					}
					v01::AllocationOutcome::NotAvailable
				},
			});
		}
		Ok(v01::HostRequestResourceAllocationResponse { outcomes })
	}

	async fn statement_store_allowance_key(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		product_id: &ProductId,
	) -> Result<StatementStoreAllowanceKey, AuthorityError> {
		let (current, grant_revision) = self.grant_session(session)?;
		#[cfg(feature = "test-host")]
		self.wallet
			.resource_controls()
			.refuse_withheld(&v01::AllocatableResource::StatementStoreAllowance)?;
		let period = statement_allowance::slot::current_period(current_unix_secs()?);
		if let Some(Allowance::StatementStore { key, period: Some(kept_period) }) = self
			.grants
			.cached_allowance(
				&current,
				grant_revision,
				product_id,
				AllowanceResource::StatementStore,
			)
			.await? && kept_period == period
		{
			return Ok(key);
		}
		let grant = self
			.local_allowance(
				cx,
				session,
				product_id,
				AllowanceResource::StatementStore,
				OnExistingAllowancePolicy::Ignore,
			)
			.await?;
		let AccountGrant::Allowance(allowance @ Allowance::StatementStore { key, .. }) = &grant
		else {
			return Err(AuthorityError::Unknown {
				reason: "Unexpected statement-store allowance response resource".to_string(),
			});
		};
		let key = key.clone();
		self.grants
			.retain_allowance(&current, grant_revision, product_id, allowance)
			.await?;
		Ok(key)
	}

	#[cfg(feature = "test-host")]
	fn submits_preimages_locally(&self) -> bool {
		self.wallet.resource_controls().grants_allowances_unchecked() ||
			self.submit_preimages_locally.load(core::sync::atomic::Ordering::Relaxed)
	}

	fn forget_statement_store_allowance_key(&self, product_id: &ProductId, public_key: [u8; 32]) {
		self.grants.forget_statement_store_allowance_key(product_id, public_key);
	}

	async fn bulletin_allowance_key(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		product_id: &ProductId,
	) -> Result<BulletinAllowanceKey, AuthorityError> {
		self.bulletin_grant(cx, session, product_id, OnExistingAllowancePolicy::Ignore)
			.await
	}

	async fn refresh_bulletin_allowance_key(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		product_id: &ProductId,
	) -> Result<BulletinAllowanceKey, AuthorityError> {
		self.bulletin_grant(cx, session, product_id, OnExistingAllowancePolicy::Increase)
			.await
	}

	async fn sign_statement_store_product_payload(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		calling_product_id: Option<&ProductId>,
		account: v01::ProductAccountId,
		payload: Vec<u8>,
	) -> Result<[u8; 64], AuthorityError> {
		let product = calling_product_id.cloned().map(ProductContext::new);
		self.wallet
			.sign_statement_store_product_payload(
				relayed_or_local_invocation(cx, session, product.as_ref()),
				account,
				payload,
			)
			.await
	}

	fn derive_entropy(
		&self,
		session: &AuthoritySession,
		product_id: &ProductId,
		context: &[u8],
	) -> Result<[u8; 32], AuthorityError> {
		self.wallet.derive_entropy(session, product_id, context)
	}

	fn contacts_handle_key(&self, session: &AuthoritySession) -> Result<[u8; 32], AuthorityError> {
		self.wallet.contact_handles(session).map(|handles| handles.handle_key())
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
		(self.services.spawner)(Box::pin(async move {
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
		}));
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
		{
			let _permissions = self.services.permissions.invalidate_pending(None).await;
			self.clear_local_session();
		}
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
