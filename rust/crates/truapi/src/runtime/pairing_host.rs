//! Pairing role: product calls reach a wallet on a paired phone.

mod remote_calls;

use std::sync::{Arc, Weak};
use truapi::latest::{
	HostAccountCreateProofRequest, HostAccountGetAliasRequest, HostAccountListRingVrfKeysRequest,
	HostAccountRegisterRingVrfKeyRequest, HostAccountRingVrfSignRequest,
};

use super::{
	HostSession,
	allowances::GrantScope,
	authority::{
		AccountCaller, AuthorityError, AuthoritySession, AutoSigningGrant, BulletinAllowanceKey,
		CreateTransactionAuthorityRequest, GrantKeeping, ProductAuthority,
		SignPayloadAuthorityRequest, SignRawAuthorityRequest, StatementStoreAllowanceKey,
		authority_session, require_current_session,
	},
	host_grants::HostGrantStore,
	product_consent::ProductConsent,
	services::RuntimeServices,
	sso_remote::sso_message_id,
	sso_request_service::{PairedSessionOwner, SsoRequestService},
};
use crate::{
	chain_runtime::ChainRuntime,
	host_internal::{
		extrinsic::build_local_transaction,
		sso_messages::{ProductRequest, RingVrfError},
		transaction::sign_extrinsic_payload,
	},
	host_logic::{
		entropy::derive_product_entropy_from_source,
		product_account::{
			SR25519_SIGNING_CONTEXT, derivation_index_bytes,
			derive_product_keypair_from_subtree_secret, derive_ring_vrf_entropy_from_domain,
		},
		raw_signing::raw_payload_bytes,
		session::{SessionInfo, SessionState},
	},
	runtime::vrf,
	subscription::Spawner,
};

use crate::platform::{
	PairingHostConfig, Platform, ProductContext, SignVrfReview, UserConfirmationReview,
	normalize_product_identifier,
};
use tracing::warn;
use truapi::{CallContext, latest};
use zeroize::Zeroizing;

use super::{
	ring_vrf_registry::{RingVrfRegistryStore, apply_ring_vrf_disclosure, validate_owner_listing},
	signing_host::ring_vrf::{
		ChainRingResolver, MemberCandidate, RingResolver, context_bytes, create_proof,
	},
};

/// Remote account authority for a pairing host.
pub struct PairingHost {
	/// Shared runtime services. Held, not just borrowed at construction, so
	/// this role can resolve a product manifest for itself when it adjudicates
	/// a cross-product grant. `RuntimeServices` does not hold the pairing host
	/// back (`host_core` owns both), so this is not a cycle.
	services: Arc<RuntimeServices>,
	/// Host platform backing all syscalls.
	pub platform: Arc<dyn Platform>,
	/// Shared chain runtime, used to assemble granted transactions locally.
	pub chain: ChainRuntime,
	/// The paired session, selected and served by [`SsoRequestService`].
	session: Arc<SsoRequestService>,
	/// Active inter-host session with a signing host.
	session_state: Arc<SessionState>,
	/// Account-access decisions and reviews for this runtime's products.
	consent: ProductConsent,
	/// Grants kept for the paired session.
	grants: HostGrantStore,
	ring_resolver: Arc<dyn RingResolver>,
	ring_vrf_registry: Arc<RingVrfRegistryStore>,
	/// Keep preimage submissions in the core instead of sending them to the
	/// Bulletin chain. A test host whose wallet answers allowances in-page
	/// holds no on-chain authorization to submit with.
	#[cfg(feature = "test-host")]
	submit_preimages_locally: core::sync::atomic::AtomicBool,
	/// Self-reference captured by spawned registration mirrors.
	weak_self: Weak<PairingHost>,
	/// Task spawner for background work.
	pub spawner: Spawner,
}

impl PairingHost {
	/// Keep preimage submissions in the core instead of the Bulletin chain.
	///
	/// For test hosts only, with the `test-host` feature enabled.
	#[cfg(feature = "test-host")]
	pub fn set_submit_preimages_locally(&self, local: bool) {
		self.submit_preimages_locally
			.store(local, core::sync::atomic::Ordering::Relaxed);
	}

	/// Build a pairing host over the shared runtime services.
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
		Arc::new_cyclic(|weak_self: &Weak<PairingHost>| {
			let owner: Weak<dyn PairedSessionOwner> = weak_self.clone();
			let session = SsoRequestService::new(services.clone(), host_config, owner);
			Self {
				services: services.clone(),
				platform: services.platform.clone(),
				chain: services.chain.clone(),
				session_state: session.session_state(),
				session,
				consent: ProductConsent::new(services.platform.clone()),
				grants: HostGrantStore::new(services.platform.clone(), GrantKeeping::Paired),
				ring_resolver: ChainRingResolver::new(services.chain.clone()),
				ring_vrf_registry: RingVrfRegistryStore::new(services.platform.clone()),
				#[cfg(feature = "test-host")]
				submit_preimages_locally: core::sync::atomic::AtomicBool::new(false),
				weak_self: weak_self.clone(),
				spawner: services.spawner.clone(),
			}
		})
	}

	/// The paired session: login, replacement and disconnection.
	pub fn session(&self) -> &Arc<SsoRequestService> {
		&self.session
	}

	fn current_session(&self) -> Option<AuthoritySession> {
		self.session_state.current().as_ref().map(authority_session)
	}

	/// Providers registered for this ring under the active account.
	pub async fn ring_vrf_providers(
		&self,
		ring: &latest::RingLocation,
	) -> Result<Vec<latest::ProductAccountId>, RingVrfError> {
		let session = self
			.session_state
			.current()
			.ok_or(RingVrfError::Unknown { reason: "no active session".to_string() })?;
		self.ring_vrf_registry.providers(session.public_key, ring).await
	}

	/// Provider selected for this ring under the active account.
	pub async fn selected_ring_vrf_provider(
		&self,
		ring: &latest::RingLocation,
	) -> Result<Option<latest::ProductAccountId>, RingVrfError> {
		let session = self
			.session_state
			.current()
			.ok_or(RingVrfError::Unknown { reason: "no active session".to_string() })?;
		self.ring_vrf_registry.selected_provider(session.public_key, ring).await
	}

	/// Persist the provider choice under the active account.
	pub async fn select_ring_vrf_provider(
		&self,
		ring: latest::RingLocation,
		handle: latest::ProductAccountId,
	) -> Result<(), RingVrfError> {
		let session = self
			.session_state
			.current()
			.ok_or(RingVrfError::Unknown { reason: "no active session".to_string() })?;
		self.ring_vrf_registry.select_provider(session.public_key, ring, handle).await
	}

	/// Disconnect and discard pairing bootstrap material so the next login
	/// generates a new device keypair and topic.
	pub async fn logout_and_reset_pairing(&self) -> Result<(), String> {
		self.session.disconnect().await;
		self.grants
			.persistence()
			.await
			.clear_auto_signing_keys()
			.await
			.map_err(|reason| {
				format!("session disconnected, but AutoSigning reset failed: {reason}")
			})?;
		self.session.forget_pairing_identity().await
	}

	/// Clear one product's grants and Allow once answers, keeping the session and other products.
	pub async fn clear_product_state(&self, product_id: &str) -> Result<(), String> {
		let product_id =
			normalize_product_identifier(product_id).map_err(|error| error.to_string())?;
		self.consent.forget_allowed_once_for(&product_id);
		let session = {
			let mut lifecycle = self.grants.lifecycle();
			lifecycle.revoke_product(&product_id);
			self.session_state.current()
		};
		self.grants
			.persistence()
			.await
			.clear_product(session.as_ref(), &product_id)
			.await
	}

	/// Retained grants, exposed so tests can seed and inspect them.
	#[cfg(test)]
	pub fn grants_for_tests(&self) -> &HostGrantStore {
		&self.grants
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

	/// Whether resolving `product_id`'s subtree would reach the Account Holder,
	/// i.e. neither the memory cache nor the persisted slot already holds it.
	///
	/// A stale or missing session resolves as `false` so a broken state falls
	/// through to the resolution's own error rather than a spurious prompt.
	pub async fn subtree_reaches_account_holder(
		&self,
		session: &AuthoritySession,
		product_id: &str,
	) -> bool {
		let Ok(session) = self.current_private_session(session) else {
			return false;
		};
		if session.sso.is_none() {
			return false;
		}
		let cache_key = (GrantScope::from_session(&session), product_id.to_string());
		self.grants
			.known_product_subtree(&self.session_state, &session, cache_key)
			.await
			.is_none()
	}

	fn current_private_session(
		&self,
		session: &AuthoritySession,
	) -> Result<SessionInfo, AuthorityError> {
		require_current_session(&self.session_state, session)
	}

	/// Whether `calling_product_id` may act on `handle`'s ring-VRF key.
	///
	/// Delegates to [`crate::runtime::ring_vrf_key_access_granted`], which
	/// resolves the owner's manifest here rather than trusting the request: on
	/// this role the request can have arrived over the pairing wire.
	async fn require_ring_vrf_key_access(
		&self,
		calling_product_id: &str,
		handle: &latest::ProductAccountId,
	) -> Result<
		(latest::ProductAccountId, crate::runtime::product_manifest::AuthorizedAccess),
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
			latest::ProductAccountId {
				dot_ns_identifier: access.owner.clone(),
				derivation_index: handle.derivation_index.clone(),
			},
			access,
		))
	}

	async fn local_ring_vrf_entropy(
		&self,
		session: &SessionInfo,
		handle: &latest::ProductAccountId,
	) -> Result<Option<Zeroizing<[u8; 32]>>, RingVrfError> {
		let Some(auto_signing) = self
			.grants
			.auto_signing_key(session, &handle.dot_ns_identifier)
			.await
			.map_err(RingVrfError::from)?
		else {
			return Ok(None);
		};
		let entry = self
			.ring_vrf_registry
			.entry(session.public_key, handle)
			.await?
			.ok_or(RingVrfError::KeyNotRegistered)?;
		let entropy = Zeroizing::new(derive_ring_vrf_entropy_from_domain(
			auto_signing.ring_vrf_domain_entropy(),
			&handle.derivation_index,
		));
		let vrf = vrf::load().await?;
		if entry.public_key != Some(vrf.member(&entropy)?) {
			return Err(RingVrfError::Unknown {
				reason: "registered ring-VRF public key does not match the AutoSigning capability"
					.to_string(),
			});
		}
		Ok(Some(entropy))
	}

	async fn local_ring_vrf_entropy_for_ring(
		&self,
		session: &SessionInfo,
		handle: &latest::ProductAccountId,
		ring: &latest::RingLocation,
	) -> Result<Option<Zeroizing<[u8; 32]>>, RingVrfError> {
		let Some(entropy) = self.local_ring_vrf_entropy(session, handle).await? else {
			return Ok(None);
		};
		let entry = self
			.ring_vrf_registry
			.entry(session.public_key, handle)
			.await?
			.ok_or(RingVrfError::KeyNotRegistered)?;
		if !entry.rings.contains(ring) {
			return Err(RingVrfError::KeyNotInRing);
		}
		Ok(Some(entropy))
	}

	fn mirror_ring_vrf_registration(
		&self,
		session: SessionInfo,
		request: ProductRequest<HostAccountRegisterRingVrfKeyRequest>,
	) {
		let weak_self = self.weak_self.clone();
		(self.spawner)(Box::pin(async move {
			let Some(host) = weak_self.upgrade() else {
				return;
			};
			let cx = CallContext::with_request_id(format!(
				"ring-vrf-registration-mirror:{}",
				sso_message_id()
			));
			if let Err(error) = host.remote_register_ring_vrf_key(&cx, &session, request).await {
				warn!(?error, "ring-VRF registration mirror failed");
			}
		}));
	}

	async fn product_subtree_public_key(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		product_id: String,
	) -> Result<[u8; 32], AuthorityError> {
		let session = self.current_private_session(session)?;
		self.remote_product_subtree_public_key(cx, &session, product_id).await
	}

	/// The keypair that can serve `account` locally under an AutoSigning
	/// capability, when this host holds one.
	///
	/// The single source of truth for local serviceability: the grant
	/// predicate and every local fast path call it, so a request can never be
	/// told "no prompt" and then relayed to a signing host that will prompt
	/// for it.
	///
	/// A caller may name an account belonging to another product —
	/// `authorized_product_account` admits that for a `localhost` caller, and
	/// for one the owner granted `context` — and the grant covers the owning
	/// product's subtree, not the caller's. Serving one locally would sign with a key the caller
	/// was never granted and skip the confirmation the signing host raises for a
	/// relayed request, so the binding is checked here rather than only at the
	/// gate.
	async fn local_product_signing_key(
		&self,
		session: &SessionInfo,
		calling_product_id: Option<&str>,
		account: &latest::ProductAccountId,
	) -> Result<Option<schnorrkel::Keypair>, AuthorityError> {
		if calling_product_id != Some(account.dot_ns_identifier.as_str()) {
			return Ok(None);
		}
		let Some(key) = self.grants.auto_signing_key(session, &account.dot_ns_identifier).await?
		else {
			return Ok(None);
		};
		derive_product_keypair_from_subtree_secret(
			*key.as_secret_bytes(),
			derivation_index_bytes(&account.derivation_index),
		)
		.map(Some)
		.map_err(|err| AuthorityError::Unknown { reason: err.to_string() })
	}

	/// Whether an AutoSigning capability lets this host serve `account`
	/// locally for `calling_product_id`.
	///
	/// A capability this host cannot trust is an error, not a fall-through:
	/// the lookup erases the slot as it rejects it, and prompting afterwards
	/// would ask the user to approve a signature the host just refused to make.
	async fn auto_signing_status(
		&self,
		session: &AuthoritySession,
		calling_product_id: &str,
		account: &latest::ProductAccountId,
	) -> Result<AutoSigningGrant, AuthorityError> {
		let session = self.current_private_session(session)?;
		Ok(
			match self
				.local_product_signing_key(&session, Some(calling_product_id), account)
				.await?
			{
				Some(_) => AutoSigningGrant::Active,
				None => AutoSigningGrant::Absent,
			},
		)
	}

	async fn sign_vrf(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		calling_product_id: String,
		request: latest::HostAccountSignVrfRequest,
	) -> Result<latest::VrfSignature, AuthorityError> {
		let session = self.current_private_session(session)?;
		if calling_product_id == request.account.dot_ns_identifier &&
			let Some(auto_signing_key) = self
				.grants
				.auto_signing_key(&session, &request.account.dot_ns_identifier)
				.await?
		{
			let keypair = derive_product_keypair_from_subtree_secret(
				*auto_signing_key.as_secret_bytes(),
				derivation_index_bytes(&request.account.derivation_index),
			)
			.map_err(|err| AuthorityError::Unknown { reason: err.to_string() })?;
			let (pre_output, proof) = crate::dynamic_vrf::sign_dynamic_vrf(
				&keypair,
				&request.transcript_label,
				request.items.iter().map(|item| (item.label.as_slice(), item.value.as_slice())),
			);
			return Ok(latest::VrfSignature { pre_output, proof });
		}
		let product = ProductContext::new(calling_product_id.clone())
			.map_err(|error| AuthorityError::Unknown { reason: error.to_string() })?;
		let review = UserConfirmationReview::SignVrf(SignVrfReview {
			calling_product_id: calling_product_id.clone(),
			request: request.clone(),
		});
		self.consent
			.review(cx, AccountCaller::Local { product: &product }, review)
			.await?;
		self.remote_sign_vrf(cx, &session, calling_product_id, request).await
	}

	async fn sign_payload(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		calling_product_id: Option<&str>,
		request: SignPayloadAuthorityRequest,
	) -> Result<latest::HostSignPayloadResponse, AuthorityError> {
		let session = self.current_private_session(session)?;
		if let SignPayloadAuthorityRequest::Product(payload) = &request &&
			let Some(keypair) = self
				.local_product_signing_key(&session, calling_product_id, &payload.account)
				.await?
		{
			return Ok(sign_extrinsic_payload(&keypair, payload.payload.clone())?);
		}
		self.remote_sign_payload(cx, &session, request).await
	}

	async fn sign_raw(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		calling_product_id: Option<&str>,
		request: SignRawAuthorityRequest,
		watermarked: bool,
	) -> Result<latest::HostSignPayloadResponse, AuthorityError> {
		let session = self.current_private_session(session)?;
		// The unwatermarked API is never grant-covered, so a local signature
		// here would skip a prompt the gate deliberately raised.
		if watermarked &&
			let SignRawAuthorityRequest::Product(payload) = &request &&
			let Some(keypair) = self
				.local_product_signing_key(&session, calling_product_id, &payload.account)
				.await?
		{
			let message = raw_payload_bytes(payload.payload.clone(), watermarked)?;
			let signature = keypair
				.secret
				.sign_simple(SR25519_SIGNING_CONTEXT, &message, &keypair.public)
				.to_bytes();
			return Ok(latest::HostSignPayloadResponse {
				signature: signature.to_vec(),
				signed_transaction: None,
			});
		}
		self.remote_sign_raw(cx, &session, request, watermarked).await
	}

	async fn create_transaction(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		calling_product_id: Option<&str>,
		request: CreateTransactionAuthorityRequest,
	) -> Result<latest::HostCreateTransactionResponse, AuthorityError> {
		let session = self.current_private_session(session)?;
		if let CreateTransactionAuthorityRequest::Product(payload) = &request &&
			let Some(keypair) = self
				.local_product_signing_key(&session, calling_product_id, &payload.signer)
				.await?
		{
			// A payload this host cannot assemble is an error, not a
			// fall-through to the relay: the gate already told the caller no
			// prompt was coming, and relaying would raise one on the signing
			// host after a chain timeout.
			//
			// Assembling needs runtime metadata, which a granted product
			// moves from the signing host to this one. A pairing host is
			// assumed to reach any genesis a product it has granted signs
			// against; where it cannot, the call fails rather than producing
			// the prompt-and-signature an ungranted product would have got.
			//
			// The failure is not prompt. An unreachable genesis leaves the
			// metadata read waiting on the chain, so the caller can sit on the
			// authority request timeout before it sees anything — the cost of
			// assembling locally is paid before the grant can be found wanting.
			return Ok(build_local_transaction(
				&self.chain,
				&keypair,
				payload.genesis_hash,
				&payload.call_data,
				&payload.extensions,
				payload.tx_ext_version,
			)
			.await?);
		}
		self.remote_create_transaction(cx, &session, request).await
	}

	async fn account_alias(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		request: ProductRequest<HostAccountGetAliasRequest>,
	) -> Result<latest::ContextualAlias, RingVrfError> {
		let private_session = self.current_private_session(session)?;
		if request.calling_product_id == request.payload.key_handle.dot_ns_identifier &&
			let Some(entropy) = self
				.local_ring_vrf_entropy_for_ring(
					&private_session,
					&request.payload.key_handle,
					&request.payload.ring_location,
				)
				.await?
		{
			self.ring_resolver.validate(&request.payload.ring_location).await?;
			let vrf = vrf::load().await?;
			self.current_private_session(session)?;
			let context = context_bytes(&request.payload.context);
			let alias = vrf.alias(&entropy, &context)?;
			return Ok(latest::ContextualAlias { context, alias: alias.to_vec() });
		}
		self.remote_account_alias(cx, &private_session, request).await
	}

	async fn create_proof(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		request: ProductRequest<HostAccountCreateProofRequest>,
	) -> Result<latest::HostAccountCreateProofResponse, RingVrfError> {
		let (key_handle, access) = self
			.require_ring_vrf_key_access(&request.calling_product_id, &request.payload.key_handle)
			.await?;
		// A grant cannot expose the owner's alias in a third product's context unless
		// that product's manifest grants the caller `context`.
		crate::runtime::product_manifest::require_context_access(
			&self.services,
			&access,
			&request.payload.context,
		)
		.await?;
		let private_session = self.current_private_session(session)?;
		if let Some(entropy) = self
			.local_ring_vrf_entropy_for_ring(
				&private_session,
				&key_handle,
				&request.payload.ring_location,
			)
			.await?
		{
			let vrf = vrf::load().await?;
			let member = vrf.member(&entropy)?;
			let resolved = self
				.ring_resolver
				.resolve(&request.payload.ring_location, &[MemberCandidate { member }])
				.await?;
			self.current_private_session(session)?;
			let context = context_bytes(&request.payload.context);
			let (proof, alias) =
				create_proof(&vrf, &entropy, &resolved, &context, &request.payload.message)?;
			return Ok(latest::HostAccountCreateProofResponse {
				proof,
				contextual_alias: latest::ContextualAlias { context, alias: alias.to_vec() },
				ring_index: resolved.ring_index,
				ring_revision: resolved.ring_revision,
			});
		}
		self.remote_create_proof(cx, &private_session, request).await
	}

	async fn register_ring_vrf_key(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		request: ProductRequest<HostAccountRegisterRingVrfKeyRequest>,
	) -> Result<[u8; 32], RingVrfError> {
		let private_session = self.current_private_session(session)?;
		let handle = latest::ProductAccountId {
			dot_ns_identifier: normalize_product_identifier(&request.calling_product_id)
				.map_err(|error| RingVrfError::Unknown { reason: error.to_string() })?,
			derivation_index: request.payload.index.clone(),
		};
		if let Some(auto_signing) = self
			.grants
			.auto_signing_key(&private_session, &request.calling_product_id)
			.await
			.map_err(RingVrfError::from)?
		{
			self.ring_resolver.validate(&request.payload.ring).await?;
			let vrf = vrf::load().await?;
			self.current_private_session(session)?;
			let entropy = Zeroizing::new(derive_ring_vrf_entropy_from_domain(
				auto_signing.ring_vrf_domain_entropy(),
				&request.payload.index,
			));
			let public_key = vrf.member(&entropy)?;
			self.ring_vrf_registry
				.register(
					private_session.public_key,
					handle,
					request.payload.ring.clone(),
					public_key,
				)
				.await?;
			self.current_private_session(session)?;
			self.mirror_ring_vrf_registration(private_session, request);
			return Ok(public_key);
		}
		let public_key =
			self.remote_register_ring_vrf_key(cx, &private_session, request.clone()).await?;
		self.ring_vrf_registry
			.register(private_session.public_key, handle, request.payload.ring, public_key)
			.await?;
		self.current_private_session(session)?;
		Ok(public_key)
	}

	async fn list_ring_vrf_keys(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		request: ProductRequest<HostAccountListRingVrfKeysRequest>,
	) -> Result<Vec<latest::RegisteredRingVrfKey>, RingVrfError> {
		let private_session = self.current_private_session(session)?;
		let owner = normalize_product_identifier(&request.payload.owner)
			.map_err(|error| RingVrfError::Unknown { reason: error.to_string() })?;
		if request.calling_product_id == owner &&
			let Some(mut entries) = self
				.ring_vrf_registry
				.complete_owner_entries(private_session.public_key, &owner)
				.await?
		{
			self.current_private_session(session)?;
			apply_ring_vrf_disclosure(&mut entries, request.payload.disclosure);
			return Ok(entries);
		}
		let requested_disclosure = request.payload.disclosure;
		let mut remote_request = request;
		if remote_request.calling_product_id == owner {
			remote_request.payload.disclosure = latest::RingVrfKeyDisclosure::PublicKey;
		}
		let mut entries =
			self.remote_list_ring_vrf_keys(cx, &private_session, remote_request).await?;
		validate_owner_listing(&owner, &entries)?;
		if entries.iter().all(|entry| entry.public_key.is_some()) {
			entries = self
				.ring_vrf_registry
				.reconcile_owner(private_session.public_key, &owner, entries)
				.await?;
		}
		self.current_private_session(session)?;
		apply_ring_vrf_disclosure(&mut entries, requested_disclosure);
		Ok(entries)
	}

	async fn ring_vrf_sign(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		request: ProductRequest<HostAccountRingVrfSignRequest>,
	) -> Result<Vec<u8>, RingVrfError> {
		let (key_handle, _access) = self
			.require_ring_vrf_key_access(&request.calling_product_id, &request.payload.key_handle)
			.await?;
		let private_session = self.current_private_session(session)?;
		if let Some(entropy) = self.local_ring_vrf_entropy(&private_session, &key_handle).await? {
			let vrf = vrf::load().await?;
			self.current_private_session(session)?;
			return vrf.sign(&entropy, &request.payload.message);
		}
		self.remote_ring_vrf_sign(cx, &private_session, request).await
	}

	async fn allocate_resources(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		grant_revision: u64,
		product_id: String,
		request: latest::HostRequestResourceAllocationRequest,
	) -> Result<latest::HostRequestResourceAllocationResponse, AuthorityError> {
		let session = self.current_private_session(session)?;
		self.remote_allocate_resources(cx, &session, grant_revision, product_id, request)
			.await
	}

	async fn statement_store_allowance_key(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		product_id: String,
	) -> Result<StatementStoreAllowanceKey, AuthorityError> {
		let session = self.current_private_session(session)?;
		self.remote_statement_store_allowance_key(cx, &session, product_id).await
	}

	async fn bulletin_allowance_key(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		product_id: String,
	) -> Result<BulletinAllowanceKey, AuthorityError> {
		let session = self.current_private_session(session)?;
		self.remote_bulletin_allowance_key(cx, &session, product_id).await
	}

	async fn refresh_bulletin_allowance_key(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		product_id: String,
	) -> Result<BulletinAllowanceKey, AuthorityError> {
		let session = self.current_private_session(session)?;
		self.remote_refresh_bulletin_allowance_key(cx, &session, product_id).await
	}

	async fn sign_statement_store_product_payload(
		&self,
		_cx: &CallContext,
		session: &AuthoritySession,
		calling_product_id: Option<&str>,
		account: latest::ProductAccountId,
		payload: Vec<u8>,
	) -> Result<[u8; 64], AuthorityError> {
		let session = self.current_private_session(session)?;
		// The SSO raw-signing protocol cannot carry an exact, unwatermarked
		// payload, so the capability's own key is the only way this role signs
		// one. Statement proofs raise no confirmation on either role, so this
		// unlocks the operation rather than waiving a prompt.
		let Some(keypair) =
			self.local_product_signing_key(&session, calling_product_id, &account).await?
		else {
			return Err(AuthorityError::Unavailable {
				reason: "pairing host: exact statement proof signing needs an AutoSigning \
                         capability; the current SSO raw-signing protocol cannot carry it"
					.to_string(),
			});
		};
		Ok(keypair
			.secret
			.sign_simple(SR25519_SIGNING_CONTEXT, &payload, &keypair.public)
			.to_bytes())
	}

	fn derive_entropy(
		&self,
		session: &AuthoritySession,
		product_id: &str,
		context: &[u8],
	) -> Result<[u8; 32], AuthorityError> {
		let session = self.current_private_session(session)?;
		if session.sso.is_none() {
			return Err(AuthorityError::Disconnected);
		}
		let root_entropy_source = session.root_entropy_source.ok_or_else(|| {
			AuthorityError::Unavailable { reason: "Session secret missing".to_string() }
		})?;
		derive_product_entropy_from_source(&root_entropy_source, product_id, context)
			.map_err(|err| AuthorityError::Unknown { reason: err.to_string() })
	}

	/// Contact-handle key from the wallet-provided root entropy source.
	///
	/// Unlike `derive_entropy` this does not require a live SSO channel: it
	/// needs the key material the session already carries, not a round trip to
	/// the wallet.
	fn contacts_handle_key(&self, session: &AuthoritySession) -> Result<[u8; 32], AuthorityError> {
		let session = self.current_private_session(session)?;
		let root_entropy_source = session.root_entropy_source.ok_or_else(|| {
			AuthorityError::Unavailable { reason: "Session secret missing".to_string() }
		})?;
		Ok(crate::runtime::contacts::handle_key_from_root_source(&root_entropy_source))
	}
}

#[async_trait::async_trait]
impl PairedSessionOwner for PairingHost {
	fn session_ended(&self, previous: Option<&SessionInfo>, revoked: bool) {
		self.grants.session_ended(previous, revoked);
		self.consent.forget_allowed_once();
	}

	async fn session_cleared(&self) {
		let persistence = self.grants.persistence().await;
		persistence.begin_cleanup();
		if let Err(reason) = persistence.drain_cleanup().await {
			warn!(%reason, "grant cleanup remains pending after disconnect");
		}
	}

	async fn prepare_session(&self, previous: Option<&SessionInfo>, next: &SessionInfo) {
		let persistence = self.grants.persistence().await;
		if persistence.begin_cleanup() &&
			let Err(reason) = persistence.drain_cleanup().await
		{
			warn!(%reason, "grant cleanup remains pending");
		}
		persistence.prepare_session(previous, next).await;
	}
}

#[async_trait::async_trait]
impl ProductAuthority for PairingHost {
	fn current_session(&self) -> Option<AuthoritySession> {
		PairingHost::current_session(self)
	}

	fn consent(&self) -> &ProductConsent {
		&self.consent
	}

	fn grant_revision(&self) -> u64 {
		self.grants.lifecycle().revision()
	}

	#[cfg(feature = "test-host")]
	fn submits_preimages_locally(&self) -> bool {
		self.submit_preimages_locally.load(core::sync::atomic::Ordering::Relaxed)
	}

	#[cfg(test)]
	fn cache_product_subtree_for_test(
		&self,
		session: &SessionInfo,
		product_id: &str,
		public_key: [u8; 32],
	) {
		self.grants.cache_product_subtree_for_test(session, product_id, public_key);
	}

	async fn product_subtree_public_key(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		product_id: String,
	) -> Result<[u8; 32], AuthorityError> {
		PairingHost::product_subtree_public_key(self, cx, session, product_id).await
	}

	async fn subtree_resolution_reaches_account_holder(
		&self,
		session: &AuthoritySession,
		product_id: &str,
	) -> bool {
		PairingHost::subtree_reaches_account_holder(self, session, product_id).await
	}

	async fn auto_signing_status(
		&self,
		session: &AuthoritySession,
		calling_product_id: &str,
		account: &latest::ProductAccountId,
	) -> Result<AutoSigningGrant, AuthorityError> {
		PairingHost::auto_signing_status(self, session, calling_product_id, account).await
	}

	async fn sign_vrf(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		calling_product_id: String,
		request: latest::HostAccountSignVrfRequest,
	) -> Result<latest::VrfSignature, AuthorityError> {
		PairingHost::sign_vrf(self, cx, session, calling_product_id, request).await
	}

	async fn sign_payload(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		calling_product_id: Option<&str>,
		request: SignPayloadAuthorityRequest,
	) -> Result<latest::HostSignPayloadResponse, AuthorityError> {
		PairingHost::sign_payload(self, cx, session, calling_product_id, request).await
	}

	async fn sign_raw(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		calling_product_id: Option<&str>,
		request: SignRawAuthorityRequest,
		watermarked: bool,
	) -> Result<latest::HostSignPayloadResponse, AuthorityError> {
		PairingHost::sign_raw(self, cx, session, calling_product_id, request, watermarked).await
	}

	async fn create_transaction(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		calling_product_id: Option<&str>,
		request: CreateTransactionAuthorityRequest,
	) -> Result<latest::HostCreateTransactionResponse, AuthorityError> {
		PairingHost::create_transaction(self, cx, session, calling_product_id, request).await
	}

	async fn account_alias(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		request: ProductRequest<HostAccountGetAliasRequest>,
	) -> Result<latest::ContextualAlias, RingVrfError> {
		PairingHost::account_alias(self, cx, session, request).await
	}

	async fn create_proof(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		request: ProductRequest<HostAccountCreateProofRequest>,
	) -> Result<latest::HostAccountCreateProofResponse, RingVrfError> {
		PairingHost::create_proof(self, cx, session, request).await
	}

	async fn register_ring_vrf_key(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		request: ProductRequest<HostAccountRegisterRingVrfKeyRequest>,
	) -> Result<[u8; 32], RingVrfError> {
		PairingHost::register_ring_vrf_key(self, cx, session, request).await
	}

	async fn list_ring_vrf_keys(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		request: ProductRequest<HostAccountListRingVrfKeysRequest>,
	) -> Result<Vec<latest::RegisteredRingVrfKey>, RingVrfError> {
		PairingHost::list_ring_vrf_keys(self, cx, session, request).await
	}

	async fn ring_vrf_sign(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		request: ProductRequest<HostAccountRingVrfSignRequest>,
	) -> Result<Vec<u8>, RingVrfError> {
		PairingHost::ring_vrf_sign(self, cx, session, request).await
	}

	async fn allocate_resources(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		grant_revision: u64,
		product_id: String,
		request: latest::HostRequestResourceAllocationRequest,
	) -> Result<latest::HostRequestResourceAllocationResponse, AuthorityError> {
		PairingHost::allocate_resources(self, cx, session, grant_revision, product_id, request)
			.await
	}

	async fn statement_store_allowance_key(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		product_id: String,
	) -> Result<StatementStoreAllowanceKey, AuthorityError> {
		PairingHost::statement_store_allowance_key(self, cx, session, product_id).await
	}

	async fn bulletin_allowance_key(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		product_id: String,
	) -> Result<BulletinAllowanceKey, AuthorityError> {
		PairingHost::bulletin_allowance_key(self, cx, session, product_id).await
	}

	async fn refresh_bulletin_allowance_key(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		product_id: String,
	) -> Result<BulletinAllowanceKey, AuthorityError> {
		PairingHost::refresh_bulletin_allowance_key(self, cx, session, product_id).await
	}

	async fn sign_statement_store_product_payload(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		calling_product_id: Option<&str>,
		account: latest::ProductAccountId,
		payload: Vec<u8>,
	) -> Result<[u8; 64], AuthorityError> {
		PairingHost::sign_statement_store_product_payload(
			self,
			cx,
			session,
			calling_product_id,
			account,
			payload,
		)
		.await
	}

	fn derive_entropy(
		&self,
		session: &AuthoritySession,
		product_id: &str,
		context: &[u8],
	) -> Result<[u8; 32], AuthorityError> {
		PairingHost::derive_entropy(self, session, product_id, context)
	}

	fn contacts_handle_key(&self, session: &AuthoritySession) -> Result<[u8; 32], AuthorityError> {
		PairingHost::contacts_handle_key(self, session)
	}
}
