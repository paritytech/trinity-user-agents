//! Product account policy and retained grants, independent of account-holder transport.

use super::{
	ProductConnection,
	allowances::{AllowanceResource, current_unix_secs},
	authority::{
		AccountCaller, AccountGrant, AccountGrantOutcome, AccountHolder, AccountInvocation,
		Allowance, AuthorityError, AuthoritySession, AutoSigningKey, BulletinAllowanceKey,
		CreateTransactionAuthorityRequest, SignPayloadAuthorityRequest, SignRawAuthorityRequest,
		StatementStoreAllowanceKey, SubtreeResolution,
	},
	host_grants::{GrantBarrier, HostGrantGuard, HostGrantStore},
	product_consent::ProductConsent,
	ring_vrf_registry::{RingVrfRegistryStore, apply_ring_vrf_disclosure},
	services::RuntimeServices,
	signing_host::ring_vrf::{MemberCandidate, RingResolver, context_bytes, create_proof},
	sso_request_service::PairedSessionOwner,
	statement_allowance, vrf,
};
use crate::{
	host_internal::{
		extrinsic::{Sr25519Signer, build_signed_transaction, local_transaction_metadata},
		sso_messages::{OnExistingAllowancePolicy, RingVrfError},
		transaction::sign_extrinsic_payload,
	},
	host_logic::{
		product_account::{
			SR25519_SIGNING_CONTEXT, derivation_index_bytes,
			derive_product_keypair_from_subtree_secret, derive_product_public_key,
			derive_ring_vrf_entropy_from_domain,
		},
		raw_signing::raw_payload_bytes,
		session::{SessionInfo, SessionState},
	},
	platform::{
		CrossProductSignatureKind, PermissionAuthorizationStatus, ProductContext, ProductId,
		ProductSubtreeReview, ResourceAllocationReview, SignVrfReview,
		StatementStoreProductSignReview, UserConfirmationReview,
	},
};
use core::time::Duration;
use futures::{FutureExt, StreamExt, pin_mut};
use std::sync::Arc;
use truapi::{CallContext, CancellationReason, latest as api};
use zeroize::Zeroizing;

/// Receives, retains and uses product grants from one selected account holder.
pub struct HostAccounts<H: AccountHolder> {
	services: Arc<RuntimeServices>,
	holder: Arc<H>,
	session_state: Arc<SessionState>,
	grants: HostGrantStore,
	ring_resolver: Arc<dyn RingResolver>,
	ring_vrf_registry: Arc<RingVrfRegistryStore>,
	consent: Arc<ProductConsent>,
	#[cfg(feature = "test-host")]
	resource_controls: Arc<super::test_resource_controls::TestResourceControls>,
	/// Keep preimage submissions in the core instead of sending them to the
	/// Bulletin chain. A test host whose wallet answers allowances in-page
	/// holds no on-chain authorization to submit with.
	#[cfg(feature = "test-host")]
	submit_preimages_locally: core::sync::atomic::AtomicBool,
}

#[async_trait::async_trait]
impl<H: AccountHolder> PairedSessionOwner for HostAccounts<H> {
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

impl<H: AccountHolder> HostAccounts<H> {
	/// Compose one host's policy with its canonical session and grant owners.
	///
	/// Kept keys read their rings through `ring_resolver`, which should be the
	/// one `holder` reads them through.
	pub fn new(
		services: Arc<RuntimeServices>,
		holder: Arc<H>,
		session_state: Arc<SessionState>,
		ring_resolver: Arc<dyn RingResolver>,
		ring_vrf_registry: Arc<RingVrfRegistryStore>,
		consent: Arc<ProductConsent>,
		#[cfg(feature = "test-host")] resource_controls: Arc<
			super::test_resource_controls::TestResourceControls,
		>,
	) -> Self {
		Self {
			grants: HostGrantStore::new(
				services.platform.clone(),
				H::GRANT_KEEPING,
				session_state.clone(),
			),
			services,
			holder,
			session_state,
			ring_resolver,
			ring_vrf_registry,
			consent,
			#[cfg(feature = "test-host")]
			resource_controls,
			#[cfg(feature = "test-host")]
			submit_preimages_locally: core::sync::atomic::AtomicBool::new(false),
		}
	}

	/// This runtime's account-access decisions and operation reviews.
	pub fn consent(&self) -> &ProductConsent {
		&self.consent
	}

	/// Forget one product's grants and Allow once answers in memory, keeping
	/// the session and other products.
	pub fn forget_product(&self, product_id: &ProductId) {
		self.consent.forget_allowed_once_for(product_id);
		self.grants.lifecycle().revoke_product(product_id);
	}

	/// Clear one product's grants and Allow once answers, in memory and in
	/// storage, keeping the session and other products.
	pub async fn clear_product_state(&self, product_id: &ProductId) -> Result<(), String> {
		self.consent.forget_allowed_once_for(product_id);
		let session = {
			let mut lifecycle = self.grants.lifecycle();
			lifecycle.revoke_product(product_id);
			self.session_state.current()
		};
		self.grants
			.persistence()
			.await
			.clear_product(session.as_ref(), product_id)
			.await
	}

	/// Forget this activation's grants and Allow once answers, then make
	/// `change` before any grant can be used again.
	pub fn change_activation<T>(&self, change: impl FnOnce() -> T) -> T {
		let mut lifecycle = self.grants.lifecycle();
		lifecycle.clear_memory();
		self.consent.forget_allowed_once();
		change()
	}

	/// Drop the kept AutoSigning keys, as a paired host's logout requires.
	pub async fn forget_auto_signing_keys(&self) -> Result<(), String> {
		self.grants.persistence().await.clear_auto_signing_keys().await
	}

	/// Keep preimage submissions in the core instead of the Bulletin chain.
	///
	/// For test hosts only, with the `test-host` feature enabled.
	#[cfg(feature = "test-host")]
	pub fn set_submit_preimages_locally(&self, local: bool) {
		self.submit_preimages_locally
			.store(local, core::sync::atomic::Ordering::Relaxed);
	}

	/// Whether a preimage submission is kept in the core instead of being sent
	/// to the Bulletin chain.
	///
	/// True for a test host that answers allowances as granted without
	/// performing them: the Bulletin allowance it hands out was never
	/// authorized on chain, so a real `store` would always be refused.
	#[cfg(feature = "test-host")]
	pub fn submits_preimages_locally(&self) -> bool {
		self.resource_controls.grants_allowances_unchecked() ||
			self.submit_preimages_locally.load(core::sync::atomic::Ordering::Relaxed)
	}

	/// Selected account holder identity.
	pub fn current_session(&self) -> Option<AuthoritySession> {
		self.holder.current_session()
	}

	fn hold_session(
		&self,
		session: &AuthoritySession,
	) -> Result<HostGrantGuard<'_>, AuthorityError> {
		let lifecycle = self.grants.lifecycle();
		self.holder.require_current_session(session)?;
		Ok(lifecycle)
	}

	fn hold_grant(
		&self,
		session: &AuthoritySession,
		revision: u64,
	) -> Result<HostGrantGuard<'_>, AuthorityError> {
		let lifecycle = self.hold_session(session)?;
		lifecycle.require_revision(revision)?;
		Ok(lifecycle)
	}

	/// The selected session and the grant revision it is read under.
	fn grant_session(
		&self,
		session: &AuthoritySession,
	) -> Result<(SessionInfo, u64), AuthorityError> {
		let lifecycle = self.hold_session(session)?;
		let session = self.session_state.current().ok_or(AuthorityError::Disconnected)?;
		Ok((session, lifecycle.revision()))
	}

	/// Disclose an account only after the calling product may access it.
	///
	/// Fails `Disconnected` while signed out, `Rejected` when the product may
	/// not use the owner's accounts, and `ConfirmationFailed` when that
	/// decision can be neither read nor asked.
	pub async fn get_account(
		&self,
		cx: &CallContext,
		connection: &ProductConnection,
		account: api::ProductAccountId,
	) -> Result<[u8; 32], AuthorityError> {
		let session = self.current_session().ok_or(AuthorityError::Disconnected)?;
		let product = &connection.product;
		let owner = &account.dot_ns_identifier;
		if *owner != product.product_id {
			let access = self.consent.account_access(
				&product.product_id,
				owner,
				connection.platform.as_ref(),
			);
			match super::until_cancelled(cx, access).await?.map_err(|err| {
				AuthorityError::ConfirmationFailed(api::GenericError { reason: err.to_string() })
			})? {
				PermissionAuthorizationStatus::Authorized => {},
				PermissionAuthorizationStatus::Denied |
				PermissionAuthorizationStatus::NotDetermined => return Err(AuthorityError::Rejected),
			}
		}
		let confirm = (*owner == product.product_id).then(|| {
			let review = UserConfirmationReview::ProductSubtree(ProductSubtreeReview {
				product_id: owner.clone(),
			});
			(connection, review)
		});
		let subtree = self.resolve_product_subtree(&session, cx, product, owner, confirm).await?;
		Self::derive_account(subtree, &account)
	}

	/// Derive an account beneath a subtree selected by an already authorized operation.
	pub async fn product_account_public_key(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		product: &ProductContext,
		account: &api::ProductAccountId,
	) -> Result<[u8; 32], AuthorityError> {
		let subtree = self
			.resolve_product_subtree(session, cx, product, &account.dot_ns_identifier, None)
			.await?;
		Self::derive_account(subtree, account)
	}

	fn derive_account(
		subtree: [u8; 32],
		account: &api::ProductAccountId,
	) -> Result<[u8; 32], AuthorityError> {
		derive_product_public_key(subtree, derivation_index_bytes(&account.derivation_index))
			.map_err(|err| AuthorityError::Unknown { reason: err.to_string() })
	}

	/// `product_id`'s hard-subtree public key under the current session, for
	/// host administration, or `None` while signed out.
	///
	/// The deadline is enforced by dropping the call rather than awaiting it
	/// after cancelling. Only the SSO response wait observes the cancellation
	/// token; the statement-store setup before it does not, so a call parked
	/// there ignores a cancel and would outlive any deadline that waited for it
	/// to finish.
	pub async fn product_subtree_public_key_within(
		&self,
		product_id: &ProductId,
		timeout: Duration,
	) -> Result<Option<[u8; 32]>, AuthorityError> {
		let Some(session) = self.current_session() else {
			return Ok(None);
		};
		let mut cx = CallContext::default();
		cx.set_timeout(timeout);
		let product = ProductContext::new(product_id.clone());
		let call = self.resolve_product_subtree(&session, &cx, &product, product_id, None).fuse();
		let deadline = futures_timer::Delay::new(timeout).fuse();
		pin_mut!(call, deadline);
		futures::select! {
			result = call => result.map(Some),
			() = deadline => {
				let reason = CancellationReason::TimedOut { timeout };
				// Best effort for anything already watching the token. The call is
				// dropped on return either way, which is what actually ends it.
				cx.cancel().cancel_with_reason(reason.clone());
				Err(AuthorityError::Unavailable {
					reason: format!("Product subtree request {reason}"),
				})
			}
		}
	}

	/// Resolve `product_id`'s subtree for `product` from the kept grants, or
	/// else from the account holder.
	///
	/// When the holder has to ask for it, the user first approves the review
	/// in `confirm` on its connection; a kept subtree, or one the holder
	/// derives itself, needs no review.
	async fn resolve_product_subtree(
		&self,
		session: &AuthoritySession,
		cx: &CallContext,
		product: &ProductContext,
		product_id: &ProductId,
		confirm: Option<(&ProductConnection, UserConfirmationReview)>,
	) -> Result<[u8; 32], AuthorityError> {
		let (current, revision) = self.grant_session(session)?;
		if let Some(key) = self.grants.known_product_subtree(&current, product_id).await {
			self.holder.require_current_session(session)?;
			return Ok(key);
		}
		let cx = super::remote_authority_context(cx);
		let resolution = self
			.holder
			.product_subtree_public_key(local_invocation(&cx, session, product), product_id)
			.await?;
		if let Some((connection, review)) = confirm &&
			matches!(resolution, SubtreeResolution::Ask(_))
		{
			self.confirm(&cx, session, connection, review).await?;
		}
		self.keep_product_subtree(&cx, session, &current, revision, product_id, resolution)
			.await
	}

	async fn keep_product_subtree(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		current: &SessionInfo,
		revision: u64,
		product_id: &ProductId,
		resolution: SubtreeResolution<'_>,
	) -> Result<[u8; 32], AuthorityError> {
		super::remote_authority_call(cx, async {
			let key = resolution.resolve().await?;
			self.holder.require_current_session(session)?;
			if !self
				.grants
				.persist_product_subtree_if_current(current, revision, product_id, key)
				.await
			{
				return Err(AuthorityError::Disconnected);
			}
			Ok(key)
		})
		.await
	}

	/// Confirm one allocation with the user, then retain each issued grant for
	/// the selected session.
	///
	/// A declined review answers `Rejected` for every resource.
	pub async fn allocate_resources(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		connection: &ProductConnection,
		request: api::HostRequestResourceAllocationRequest,
	) -> Result<api::HostRequestResourceAllocationResponse, AuthorityError> {
		let revision = self.hold_session(session)?.revision();
		let product = &connection.product;
		let review = UserConfirmationReview::ResourceAllocation(ResourceAllocationReview {
			calling_product_id: product.product_id.clone(),
			resources: request.resources.clone(),
		});
		match self.confirm(cx, session, connection, review).await {
			Ok(()) => {},
			Err(AuthorityError::Rejected) => {
				return Ok(api::HostRequestResourceAllocationResponse {
					outcomes: vec![api::AllocationOutcome::Rejected; request.resources.len()],
				});
			},
			Err(error) => return Err(error),
		}
		let cx = super::remote_authority_context_with_default(
			cx,
			super::RESOURCE_ALLOCATION_REMOTE_AUTHORITY_RESPONSE_TIMEOUT,
		);
		super::remote_authority_call(&cx, async {
			self.hold_grant(session, revision)?;
			#[cfg(feature = "test-host")]
			if self.resource_controls.grants_allowances_unchecked() {
				// Nothing is allocated and no proof is built: a suite in this mode
				// learns that its product handles a grant, not that a host would
				// have given one. A withheld tag is still refused here, so the one
				// resource a suite wants to prove its product lives without stays
				// refused while the rest are granted.
				return Ok(api::HostRequestResourceAllocationResponse {
					outcomes: request
						.resources
						.iter()
						.map(|resource| {
							if self.resource_controls.withholds(resource) {
								api::AllocationOutcome::Rejected
							} else {
								api::AllocationOutcome::Allocated
							}
						})
						.collect(),
				});
			}
			let mut grants = self
				.holder
				.allocate_grants(
					local_invocation(&cx, session, product),
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
						match self.retain_grant(&cx, session, revision, product, grant).await {
							Ok(()) => api::AllocationOutcome::Allocated,
							Err(reason) => {
								tracing::warn!(product_id = %product.product_id, %reason, "direct resource allocation item failed");
								api::AllocationOutcome::NotAvailable
							},
						}
					},
					AccountGrantOutcome::Rejected => api::AllocationOutcome::Rejected,
					AccountGrantOutcome::NotAvailable { reason } => {
						if let Some(reason) = reason {
							tracing::warn!(product_id = %product.product_id, %reason, "direct resource allocation item failed");
						}
						api::AllocationOutcome::NotAvailable
					},
				});
			}
			Ok(api::HostRequestResourceAllocationResponse { outcomes })
		})
		.await
	}

	/// Retain one issued grant for the product, under the revision read before its review.
	async fn retain_grant(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		revision: u64,
		product: &ProductContext,
		grant: AccountGrant,
	) -> Result<(), AuthorityError> {
		let (current, _) = self.grant_session(session)?;
		match grant {
			AccountGrant::Allowance(allowance) => {
				self.grants
					.retain_allowance(&current, revision, &product.product_id, &allowance)
					.await
			},
			AccountGrant::AutoSigning(key) => {
				let expected = self
					.resolve_product_subtree(session, cx, product, &product.product_id, None)
					.await?;
				self.grants
					.remember_auto_signing_key(
						&current,
						revision,
						&product.product_id,
						expected,
						key,
					)
					.await
			},
			AccountGrant::SmartContract => Ok(()),
		}
	}

	/// The product's kept Statement Store key, or one the account holder issues.
	pub async fn statement_store_allowance_key(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		product: &ProductContext,
	) -> Result<StatementStoreAllowanceKey, AuthorityError> {
		let (current, revision) = self.grant_session(session)?;
		#[cfg(feature = "test-host")]
		self.resource_controls
			.refuse_withheld(&api::AllocatableResource::StatementStoreAllowance)?;
		if let Some(Allowance::StatementStore { key, period }) = self
			.grants
			.cached_allowance(
				&current,
				revision,
				&product.product_id,
				AllowanceResource::StatementStore,
			)
			.await?
		{
			// A key issued for one allowance period is asked for again in the
			// next; a key without one stays sponsored by the wallet that issued it.
			let current_period = match period {
				Some(period) => {
					period == statement_allowance::slot::current_period(current_unix_secs()?)
				},
				None => true,
			};
			if current_period {
				return Ok(key);
			}
		}
		let grant = self
			.holder
			.ensure_allowance(
				local_invocation(cx, session, product),
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
		self.grants
			.retain_allowance(&current, revision, &product.product_id, allowance)
			.await?;
		Ok(key.clone())
	}

	/// Forget the product's kept Statement Store key only if it is still
	/// `public_key`, preserving any replacement.
	pub fn forget_statement_store_allowance_key(
		&self,
		product_id: &ProductId,
		public_key: [u8; 32],
	) {
		self.grants.forget_statement_store_allowance_key(product_id, public_key);
	}

	/// The product's kept Bulletin key, or one the account holder issues.
	pub async fn bulletin_allowance_key(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		product: &ProductContext,
	) -> Result<BulletinAllowanceKey, AuthorityError> {
		let (current, revision) = self.grant_session(session)?;
		#[cfg(feature = "test-host")]
		self.resource_controls
			.refuse_withheld(&api::AllocatableResource::BulletinAllowance)?;
		if let Some(Allowance::Bulletin(key)) = self
			.grants
			.cached_allowance(&current, revision, &product.product_id, AllowanceResource::Bulletin)
			.await?
		{
			return Ok(key);
		}
		self.allocate_bulletin_allowance_key(
			cx,
			session,
			product,
			OnExistingAllowancePolicy::Ignore,
		)
		.await
	}

	/// Increase or replace the product's Bulletin allowance after a submission
	/// was refused. The kept key stays until its replacement is retained.
	pub async fn refresh_bulletin_allowance_key(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		product: &ProductContext,
	) -> Result<BulletinAllowanceKey, AuthorityError> {
		#[cfg(feature = "test-host")]
		self.resource_controls
			.refuse_withheld(&api::AllocatableResource::BulletinAllowance)?;
		self.allocate_bulletin_allowance_key(
			cx,
			session,
			product,
			OnExistingAllowancePolicy::Increase,
		)
		.await
	}

	async fn allocate_bulletin_allowance_key(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		product: &ProductContext,
		policy: OnExistingAllowancePolicy,
	) -> Result<BulletinAllowanceKey, AuthorityError> {
		let (current, revision) = self.grant_session(session)?;
		let grant = self
			.holder
			.ensure_allowance(
				local_invocation(cx, session, product),
				AllowanceResource::Bulletin,
				policy,
			)
			.await?;
		let AccountGrant::Allowance(allowance @ Allowance::Bulletin(key)) = &grant else {
			return Err(AuthorityError::Unknown {
				reason: "Unexpected Bulletin allowance response resource".to_string(),
			});
		};
		self.grants
			.retain_allowance(&current, revision, &product.product_id, allowance)
			.await?;
		Ok(key.clone())
	}

	/// Derive entropy scoped to `product_id` under the selected session.
	pub fn derive_entropy(
		&self,
		session: &AuthoritySession,
		product_id: &ProductId,
		context: &[u8],
	) -> Result<[u8; 32], AuthorityError> {
		self.holder.derive_entropy(session, product_id, context)
	}

	/// Contact handles under the selected account holder.
	pub fn contact_handles(
		&self,
		session: &AuthoritySession,
	) -> Result<super::contacts::ContactHandles, AuthorityError> {
		self.holder.contact_handles(session)
	}

	/// Revision of the kept grants, for tests that pin grant work to it.
	#[cfg(test)]
	pub fn grant_revision(&self) -> u64 {
		self.grants.lifecycle().revision()
	}

	/// Kept grants, for tests that seed and inspect them.
	#[cfg(test)]
	pub fn grants_for_tests(&self) -> &HostGrantStore {
		&self.grants
	}

	/// Register fixture material in the registry this host reads.
	#[cfg(test)]
	pub async fn register_ring_vrf_key_for_tests(
		&self,
		root_public_key: [u8; 32],
		handle: api::ProductAccountId,
		ring: api::RingLocation,
		public_key: [u8; 32],
	) -> Result<(), RingVrfError> {
		self.ring_vrf_registry.register(root_public_key, handle, ring, public_key).await
	}

	/// Seed a subtree without an account-holder exchange.
	#[cfg(test)]
	pub fn cache_product_subtree_for_test(
		&self,
		session: &SessionInfo,
		product_id: &ProductId,
		public_key: [u8; 32],
	) {
		self.grants.cache_product_subtree_for_test(session, product_id, public_key);
	}

	/// Confirm with the user before the account holder is asked to act.
	async fn confirm(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		connection: &ProductConnection,
		review: UserConfirmationReview,
	) -> Result<(), AuthorityError> {
		self.holder.require_current_session(session)?;
		let caller = AccountCaller::Local { product: &connection.product };
		self.consent.review(cx, caller, connection.platform.as_ref(), review).await
	}

	/// Confirm a signature with one of `owner`'s accounts, once per kind for
	/// this execution when another product owns it.
	async fn confirm_signature(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		connection: &ProductConnection,
		kind: CrossProductSignatureKind,
		owner: &ProductId,
		review: UserConfirmationReview,
	) -> Result<(), AuthorityError> {
		self.holder.require_current_session(session)?;
		self.consent.review_signature(cx, connection, kind, owner, review).await
	}

	/// The kept AutoSigning key that serves `account` for `product`.
	///
	/// Only the product's own accounts are served: a kept key covers its
	/// owner's subtree, not the subtree of another product it was granted.
	async fn product_signing_grant(
		&self,
		session: &AuthoritySession,
		product: &ProductContext,
		account: &api::ProductAccountId,
	) -> Result<Option<AutoSigningKey>, AuthorityError> {
		if product.product_id != account.dot_ns_identifier {
			return Ok(None);
		}
		let (current, _) = self.grant_session(session)?;
		let grant = self.grants.auto_signing_key(&current, &product.product_id).await?;
		self.holder.require_current_session(session)?;
		Ok(grant)
	}

	fn grant_keypair(
		grant: &AutoSigningKey,
		account: &api::ProductAccountId,
	) -> Result<schnorrkel::Keypair, AuthorityError> {
		derive_product_keypair_from_subtree_secret(
			*grant.as_secret_bytes(),
			derivation_index_bytes(&account.derivation_index),
		)
		.map_err(|error| AuthorityError::Unknown { reason: error.to_string() })
	}

	/// Sign with a kept product key, or confirm with the user and ask the holder.
	pub async fn sign_payload(
		&self,
		session: &AuthoritySession,
		cx: &CallContext,
		connection: &ProductConnection,
		request: SignPayloadAuthorityRequest,
	) -> Result<api::HostSignPayloadResponse, AuthorityError> {
		let revision = self.grants.lifecycle().revision();
		let product = &connection.product;
		if let SignPayloadAuthorityRequest::Product(payload) = &request &&
			let Some(grant) =
				self.product_signing_grant(session, product, &payload.account).await?
		{
			let cx = super::remote_authority_context(cx);
			return super::remote_authority_call(&cx, async {
				let _lifecycle = self.hold_grant(session, revision)?;
				let keypair = Self::grant_keypair(&grant, &payload.account)?;
				Ok(sign_extrinsic_payload(&keypair, payload.payload.clone())?)
			})
			.await;
		}
		let review = request.review(&AccountCaller::Local { product });
		match &request {
			SignPayloadAuthorityRequest::Product(payload) => {
				self.confirm_signature(
					cx,
					session,
					connection,
					CrossProductSignatureKind::Payload,
					&payload.account.dot_ns_identifier,
					review,
				)
				.await?
			},
			SignPayloadAuthorityRequest::LegacyAccount { .. } => {
				self.confirm(cx, session, connection, review).await?
			},
		}
		let cx = super::remote_authority_context(cx);
		super::remote_authority_call(
			&cx,
			self.holder.sign_payload(local_invocation(&cx, session, product), request),
		)
		.await
	}

	/// Sign raw bytes, confirming a legacy request even when a kept product
	/// key signs it.
	pub async fn sign_raw(
		&self,
		session: &AuthoritySession,
		cx: &CallContext,
		connection: &ProductConnection,
		request: SignRawAuthorityRequest,
		watermarked: bool,
	) -> Result<api::HostSignPayloadResponse, AuthorityError> {
		let revision = self.grants.lifecycle().revision();
		let product = &connection.product;
		let account = match &request {
			SignRawAuthorityRequest::Product(payload) => Some(&payload.account),
			SignRawAuthorityRequest::LegacyAccount { product_account, .. } => Some(product_account),
			SignRawAuthorityRequest::IdentityAccount { .. } => None,
		};
		// The unwatermarked API is never grant-covered, so a local signature
		// would skip a prompt the review deliberately raises.
		let grant = match account {
			Some(account) if watermarked => {
				self.product_signing_grant(session, product, account).await?
			},
			_ => None,
		};
		if grant.is_none() || !matches!(request, SignRawAuthorityRequest::Product(_)) {
			let review = request.review(&AccountCaller::Local { product }, watermarked);
			match &request {
				SignRawAuthorityRequest::Product(payload) if watermarked => {
					self.confirm_signature(
						cx,
						session,
						connection,
						CrossProductSignatureKind::Raw,
						&payload.account.dot_ns_identifier,
						review,
					)
					.await?
				},
				_ => self.confirm(cx, session, connection, review).await?,
			}
		}
		let cx = super::remote_authority_context(cx);
		if let (Some(grant), Some(account)) = (grant, account) {
			let payload = match &request {
				SignRawAuthorityRequest::Product(request) => &request.payload,
				SignRawAuthorityRequest::LegacyAccount { request, .. } |
				SignRawAuthorityRequest::IdentityAccount { request, .. } => &request.payload,
			};
			let message = raw_payload_bytes(payload.clone(), watermarked)?;
			return super::remote_authority_call(&cx, async {
				let _lifecycle = self.hold_grant(session, revision)?;
				let keypair = Self::grant_keypair(&grant, account)?;
				Ok(api::HostSignPayloadResponse {
					signature: keypair
						.secret
						.sign_simple(SR25519_SIGNING_CONTEXT, &message, &keypair.public)
						.to_bytes()
						.to_vec(),
					signed_transaction: None,
				})
			})
			.await;
		}
		super::remote_authority_call(
			&cx,
			self.holder
				.sign_raw(local_invocation(&cx, session, product), request, watermarked),
		)
		.await
	}

	/// Build a transaction with a kept product key, or confirm with the user
	/// and ask the holder.
	pub async fn create_transaction(
		&self,
		session: &AuthoritySession,
		cx: &CallContext,
		connection: &ProductConnection,
		request: CreateTransactionAuthorityRequest,
	) -> Result<api::HostCreateTransactionResponse, AuthorityError> {
		let revision = self.grants.lifecycle().revision();
		let product = &connection.product;
		let caller = AccountCaller::Local { product };
		if let CreateTransactionAuthorityRequest::Product(payload) = &request &&
			let Some(grant) =
				self.product_signing_grant(session, product, &payload.signer).await?
		{
			if !payload.contacts.is_empty() {
				self.confirm(cx, session, connection, request.review(&caller)).await?;
			}
			let cx = super::remote_authority_context(cx);
			return super::remote_authority_call(&cx, async {
				// A payload this host cannot assemble is an error, not a
				// fall-through to the account holder, which would raise a
				// prompt the caller was told is not coming. Assembling needs the
				// genesis's runtime metadata; an unreachable genesis holds the
				// read until its deadline.
				let metadata =
					local_transaction_metadata(&self.services.chain, payload.genesis_hash).await?;
				let _lifecycle = self.hold_grant(session, revision)?;
				let keypair = Self::grant_keypair(&grant, &payload.signer)?;
				Ok(api::HostCreateTransactionResponse {
					transaction: build_signed_transaction(
						&Sr25519Signer::from_keypair(&keypair),
						payload.genesis_hash,
						&payload.call_data,
						&payload.extensions,
						payload.tx_ext_version,
						metadata,
					)?,
				})
			})
			.await;
		}
		let review = request.review(&caller);
		match &request {
			CreateTransactionAuthorityRequest::Product(payload) if payload.contacts.is_empty() => {
				self.confirm_signature(
					cx,
					session,
					connection,
					CrossProductSignatureKind::Transaction,
					&payload.signer.dot_ns_identifier,
					review,
				)
				.await?
			},
			_ => self.confirm(cx, session, connection, review).await?,
		}
		let cx = super::remote_authority_context(cx);
		super::remote_authority_call(
			&cx,
			self.holder.create_transaction(local_invocation(&cx, session, product), request),
		)
		.await
	}

	/// Sign a VRF transcript with a kept product key, or confirm with the user
	/// and ask the holder.
	pub async fn sign_vrf(
		&self,
		session: &AuthoritySession,
		cx: &CallContext,
		connection: &ProductConnection,
		request: api::HostAccountSignVrfRequest,
	) -> Result<api::VrfSignature, AuthorityError> {
		let revision = self.grants.lifecycle().revision();
		let product = &connection.product;
		if let Some(grant) = self.product_signing_grant(session, product, &request.account).await? {
			let _lifecycle = self.hold_grant(session, revision)?;
			let keypair = Self::grant_keypair(&grant, &request.account)?;
			let (pre_output, proof) = crate::dynamic_vrf::sign_dynamic_vrf(
				&keypair,
				&request.transcript_label,
				request.items.iter().map(|item| (item.label.as_slice(), item.value.as_slice())),
			);
			return Ok(api::VrfSignature { pre_output, proof });
		}
		let review = UserConfirmationReview::SignVrf(SignVrfReview {
			calling_product_id: product.product_id.clone(),
			request: request.clone(),
		});
		self.confirm(cx, session, connection, review).await?;
		self.holder.sign_vrf(local_invocation(cx, session, product), request).await
	}

	/// Sign exact statement proof bytes with a kept product key, or confirm
	/// with the user and ask the holder.
	pub async fn sign_statement_store_product_payload(
		&self,
		session: &AuthoritySession,
		cx: &CallContext,
		connection: &ProductConnection,
		account: api::ProductAccountId,
		payload: Vec<u8>,
	) -> Result<[u8; 64], AuthorityError> {
		let revision = self.grants.lifecycle().revision();
		let product = &connection.product;
		if let Some(grant) = self.product_signing_grant(session, product, &account).await? {
			let cx = super::remote_authority_context(cx);
			return super::remote_authority_call(&cx, async {
				let _lifecycle = self.hold_grant(session, revision)?;
				let keypair = Self::grant_keypair(&grant, &account)?;
				Ok(keypair
					.secret
					.sign_simple(SR25519_SIGNING_CONTEXT, &payload, &keypair.public)
					.to_bytes())
			})
			.await;
		}
		let review =
			UserConfirmationReview::StatementStoreProductSign(StatementStoreProductSignReview {
				calling_product_id: Some(product.product_id.clone()),
				account: account.clone(),
				payload: payload.clone(),
			});
		self.confirm_signature(
			cx,
			session,
			connection,
			CrossProductSignatureKind::StatementProof,
			&account.dot_ns_identifier,
			review,
		)
		.await?;
		let cx = super::remote_authority_context(cx);
		super::remote_authority_call(
			&cx,
			self.holder.sign_statement_store_product_payload(
				local_invocation(&cx, session, product),
				account,
				payload,
			),
		)
		.await
	}

	/// Providers registered for `ring` under the selected session.
	pub async fn ring_vrf_providers(
		&self,
		ring: &api::RingLocation,
	) -> Result<Vec<api::ProductAccountId>, RingVrfError> {
		let session = self.current_session().ok_or(AuthorityError::Disconnected)?;
		self.ring_vrf_registry.providers(session.public_key, ring).await
	}

	/// Provider selected for `ring` under the selected session.
	pub async fn selected_ring_vrf_provider(
		&self,
		ring: &api::RingLocation,
	) -> Result<Option<api::ProductAccountId>, RingVrfError> {
		let session = self.current_session().ok_or(AuthorityError::Disconnected)?;
		self.ring_vrf_registry.selected_provider(session.public_key, ring).await
	}

	/// Persist the provider choice under the selected session.
	pub async fn select_ring_vrf_provider(
		&self,
		ring: api::RingLocation,
		handle: api::ProductAccountId,
	) -> Result<(), RingVrfError> {
		let session = self.current_session().ok_or(AuthorityError::Disconnected)?;
		self.ring_vrf_registry.select_provider(session.public_key, ring, handle).await
	}

	/// Whether `product` may act on `handle`'s ring-VRF key, by its owner's
	/// published manifest rather than the request. Every refusal, including
	/// a lookup that outlives the call, answers `NotAllowlisted`.
	async fn require_ring_vrf_key_access(
		&self,
		cx: &CallContext,
		product: &ProductContext,
		handle: &api::ProductAccountId,
	) -> Result<(api::ProductAccountId, super::product_manifest::AuthorizedAccess), RingVrfError> {
		let lookup = super::product_manifest::ring_vrf_key_access_granted_within(
			&self.services,
			self.services.platform.as_ref(),
			product.product_id.clone(),
			handle,
			cx.timeout(),
		);
		let Ok(Ok(access)) = super::until_cancelled(cx, lookup).await else {
			return Err(RingVrfError::NotAllowlisted);
		};
		Ok((
			api::ProductAccountId {
				dot_ns_identifier: access.owner.clone(),
				derivation_index: handle.derivation_index.clone(),
			},
			access,
		))
	}

	/// Ring-VRF entropy from the kept AutoSigning key of `handle`'s owner, when
	/// this host keeps one and `handle` is registered under it in `ring`.
	async fn local_ring_vrf_entropy(
		&self,
		session: &AuthoritySession,
		handle: &api::ProductAccountId,
		ring: Option<&api::RingLocation>,
	) -> Result<Option<(Zeroizing<[u8; 32]>, vrf::Vrf)>, RingVrfError> {
		let (current, revision) = self.grant_session(session)?;
		let Some(grant) = self.grants.auto_signing_key(&current, &handle.dot_ns_identifier).await?
		else {
			return Ok(None);
		};
		let entry = self
			.ring_vrf_registry
			.entry(current.public_key, handle)
			.await?
			.ok_or(RingVrfError::KeyNotRegistered)?;
		if ring.is_some_and(|ring| !entry.rings.contains(ring)) {
			return Err(RingVrfError::KeyNotInRing);
		}
		let vrf = vrf::load().await?;
		let _lifecycle = self.hold_grant(session, revision)?;
		let entropy = Zeroizing::new(derive_ring_vrf_entropy_from_domain(
			grant.ring_vrf_domain_entropy(),
			&handle.derivation_index,
		));
		if entry.public_key != Some(vrf.member(&entropy)?) {
			return Err(RingVrfError::Unknown {
				reason: "registered ring-VRF public key does not match the AutoSigning capability"
					.to_string(),
			});
		}
		Ok(Some((entropy, vrf)))
	}

	/// Derive an alias with the product's own kept key, or ask the holder.
	pub async fn account_alias(
		&self,
		session: &AuthoritySession,
		cx: &CallContext,
		product: &ProductContext,
		request: api::HostAccountGetAliasRequest,
	) -> Result<api::ContextualAlias, RingVrfError> {
		let revision = self.grants.lifecycle().revision();
		if product.product_id == request.key_handle.dot_ns_identifier &&
			let Some((entropy, vrf)) = self
				.local_ring_vrf_entropy(session, &request.key_handle, Some(&request.ring_location))
				.await?
		{
			self.ring_resolver.validate(&request.ring_location).await?;
			let _lifecycle = self.hold_grant(session, revision)?;
			let context = context_bytes(&request.context);
			return Ok(api::ContextualAlias {
				alias: vrf.alias(&entropy, &context)?.to_vec(),
				context,
			});
		}
		self.holder.account_alias(local_invocation(cx, session, product), request).await
	}

	/// Create a ring proof with a kept key, or ask the holder.
	///
	/// The manifest check runs before the account-holder deadline, so a lookup
	/// that outlives the call answers the uniform refusal.
	pub async fn create_proof(
		&self,
		session: &AuthoritySession,
		cx: &CallContext,
		product: &ProductContext,
		request: api::HostAccountCreateProofRequest,
	) -> Result<api::HostAccountCreateProofResponse, RingVrfError> {
		let revision = self.grants.lifecycle().revision();
		let (handle, access) =
			self.require_ring_vrf_key_access(cx, product, &request.key_handle).await?;
		super::remote_authority_call(cx, async {
			// A grant cannot expose the owner's alias in a third product's context
			// unless that product's manifest grants the caller `context`.
			super::product_manifest::require_context_access(
				&self.services,
				&access,
				&request.context,
			)
			.await?;
			if let Some((entropy, vrf)) = self
				.local_ring_vrf_entropy(session, &handle, Some(&request.ring_location))
				.await?
			{
				let member = vrf.member(&entropy)?;
				let resolved = self
					.ring_resolver
					.resolve(&request.ring_location, &[MemberCandidate { member }])
					.await?;
				let context = context_bytes(&request.context);
				// Proved outside the grant lock, which would otherwise stall every
				// grant use, lock and wallet switch for the whole proof.
				let (proof, alias) =
					create_proof(&vrf, &entropy, &resolved, &context, &request.message)?;
				self.hold_grant(session, revision)?;
				return Ok(api::HostAccountCreateProofResponse {
					proof,
					contextual_alias: api::ContextualAlias { context, alias: alias.to_vec() },
					ring_index: resolved.ring_index,
					ring_revision: resolved.ring_revision,
				});
			}
			self.holder.create_proof(local_invocation(cx, session, product), request).await
		})
		.await
	}

	/// Register with the product's kept AutoSigning key when this host keeps
	/// one; the account holder records the key either way.
	pub async fn register_ring_vrf_key(
		&self,
		session: &AuthoritySession,
		cx: &CallContext,
		product: &ProductContext,
		request: api::HostAccountRegisterRingVrfKeyRequest,
	) -> Result<[u8; 32], RingVrfError> {
		let (current, revision) = self.grant_session(session)?;
		let invocation = local_invocation(cx, session, product);
		let Some(grant) = self.grants.auto_signing_key(&current, &product.product_id).await? else {
			return self.holder.register_ring_vrf_key(invocation, request).await;
		};
		self.ring_resolver.validate(&request.ring).await?;
		let vrf = vrf::load().await?;
		let public_key = {
			let _lifecycle = self.hold_grant(session, revision)?;
			let entropy = Zeroizing::new(derive_ring_vrf_entropy_from_domain(
				grant.ring_vrf_domain_entropy(),
				&request.index,
			));
			vrf.member(&entropy)?
		};
		self.holder.record_ring_vrf_key(invocation, request, public_key).await?;
		Ok(public_key)
	}

	/// Answer the product's own complete listing locally; otherwise the
	/// account holder lists and keeps the registry current.
	pub async fn list_ring_vrf_keys(
		&self,
		session: &AuthoritySession,
		cx: &CallContext,
		product: &ProductContext,
		request: api::HostAccountListRingVrfKeysRequest,
	) -> Result<Vec<api::RegisteredRingVrfKey>, RingVrfError> {
		self.holder.require_current_session(session)?;
		if product.product_id == request.owner &&
			let Some(mut entries) = self
				.ring_vrf_registry
				.complete_owner_entries(session.public_key, &request.owner)
				.await?
		{
			self.holder.require_current_session(session)?;
			apply_ring_vrf_disclosure(&mut entries, request.disclosure);
			return Ok(entries);
		}
		self.holder
			.list_ring_vrf_keys(local_invocation(cx, session, product), request)
			.await
	}

	/// Sign with a registered key from a kept AutoSigning key, or ask the holder.
	///
	/// The manifest check runs before the account-holder deadline, as for
	/// [`Self::create_proof`].
	pub async fn ring_vrf_sign(
		&self,
		session: &AuthoritySession,
		cx: &CallContext,
		product: &ProductContext,
		request: api::HostAccountRingVrfSignRequest,
	) -> Result<Vec<u8>, RingVrfError> {
		let revision = self.grants.lifecycle().revision();
		let (handle, _) =
			self.require_ring_vrf_key_access(cx, product, &request.key_handle).await?;
		super::remote_authority_call(cx, async {
			if let Some((entropy, vrf)) =
				self.local_ring_vrf_entropy(session, &handle, None).await?
			{
				let _lifecycle = self.hold_grant(session, revision)?;
				return vrf.sign(&entropy, &request.message);
			}
			self.holder.ring_vrf_sign(local_invocation(cx, session, product), request).await
		})
		.await
	}
}

/// A call from `product` this host bound, under the session it selected.
fn local_invocation<'a>(
	cx: &'a CallContext,
	session: &'a AuthoritySession,
	product: &'a ProductContext,
) -> AccountInvocation<'a> {
	AccountInvocation { call: cx, session, caller: AccountCaller::Local { product } }
}
