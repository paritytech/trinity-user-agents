//! Wallet account execution and access to other products' keys.

use super::allowance;
use truapi::latest::{
	ChainIdentifier, DerivationIndex, HostAccountCreateProofRequest, HostAccountGetAliasRequest,
	HostAccountListRingVrfKeysRequest, HostAccountRegisterRingVrfKeyRequest,
	HostAccountRingVrfSignRequest, ProductAccountId, RingLocation, RingLocationJunction,
};

use crate::{
	host_internal::{
		extrinsic::build_local_transaction,
		sso_messages::{OnExistingAllowancePolicy, ProductRequest, RingVrfError},
		transaction::sign_extrinsic_payload,
	},
	host_logic::{
		entropy::derive_product_entropy,
		features::genesis_for,
		product_account::{
			SR25519_SIGNING_CONTEXT, derive_product_subtree_keypair,
			derive_root_keypair_from_entropy, personhood_product_id,
		},
		raw_signing::raw_payload_bytes,
	},
	platform::{PermissionAuthorizationStatus, ProductContext, normalize_product_identifier},
	runtime::{
		allowances::AllowanceResource,
		authority::{
			AccountCaller, AccountGrant, AuthorityError, AuthoritySession, AutoSigningGrant,
			BulletinAllowanceKey, CreateTransactionAuthorityRequest, ProductAuthority,
			SignPayloadAuthorityRequest, SignRawAuthorityRequest, StatementStoreAllowanceKey,
		},
		product_consent::ProductConsent,
		ring_vrf_registry::apply_ring_vrf_disclosure,
		signing_host::{
			SigningHost, product_authority_error,
			ring_vrf::{MemberCandidate, context_bytes, create_proof},
		},
		statement_allowance::{self, collection::PersonhoodCollection},
		until_cancelled,
		vrf::{self, Vrf},
	},
};
use truapi::{CallContext, v01};
use zeroize::Zeroizing;

impl SigningHost {
	/// Derive the product's hard-subtree public key from the active session root.
	/// Returns `None` when no session is active.
	pub fn derive_subtree_public_key(
		&self,
		product_id: &str,
	) -> Result<Option<[u8; 32]>, AuthorityError> {
		let product_id = normalize_product_identifier(product_id)
			.map_err(|err| AuthorityError::Unavailable { reason: err.to_string() })?;
		let Ok(entropy) = self.root_entropy() else {
			return Ok(None);
		};
		let root = derive_root_keypair_from_entropy(&entropy).map_err(product_authority_error)?;
		let subtree =
			derive_product_subtree_keypair(&root, &product_id).map_err(product_authority_error)?;
		Ok(Some(subtree.public.to_bytes()))
	}
}

impl SigningHost {
	async fn register_builtin_personhood_keys_if_needed(
		&self,
		session: &AuthoritySession,
		owner: &str,
	) -> Result<(), RingVrfError> {
		if owner != personhood_product_id(&self.network_suffix) {
			return Ok(());
		}
		let chains = self
			.platform
			.supported_chains()
			.await
			.map_err(|error| RingVrfError::Unknown { reason: error.reason })?;
		let chain_id =
			genesis_for(&chains, ChainIdentifier::People).ok_or(RingVrfError::RingNotFound)?;
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
				dot_ns_identifier: owner.to_string(),
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
		calling_product_id: &str,
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
		requester: &str,
		owner: &str,
	) -> Result<(), RingVrfError> {
		let status = until_cancelled(cx, self.consent.account_access(requester, owner))
			.await?
			.map_err(|error| RingVrfError::Unknown { reason: error.to_string() })?;
		if status == PermissionAuthorizationStatus::Authorized {
			Ok(())
		} else {
			Err(RingVrfError::Rejected)
		}
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

	fn grant_revision(&self) -> u64 {
		self.grants.lifecycle().revision()
	}

	async fn product_subtree_public_key(
		&self,
		_cx: &CallContext,
		session: &AuthoritySession,
		product_id: String,
	) -> Result<[u8; 32], AuthorityError> {
		self.require_current_session(session)?;
		let product_id = normalize_product_identifier(&product_id)
			.map_err(|err| AuthorityError::Unavailable { reason: err.to_string() })?;
		let entropy = self.root_entropy()?;
		let root = derive_root_keypair_from_entropy(&entropy).map_err(product_authority_error)?;
		derive_product_subtree_keypair(&root, &product_id)
			.map(|keypair| keypair.public.to_bytes())
			.map_err(product_authority_error)
	}

	async fn subtree_resolution_reaches_account_holder(
		&self,
		_session: &AuthoritySession,
		_product_id: &str,
	) -> bool {
		// A signing host derives the subtree locally from root entropy, so
		// resolution never reaches a remote Account Holder and never prompts.
		false
	}

	async fn auto_signing_status(
		&self,
		session: &AuthoritySession,
		calling_product_id: &str,
		account: &v01::ProductAccountId,
	) -> Result<AutoSigningGrant, AuthorityError> {
		// A stale session is not a grant, and is answered here rather than
		// raising a prompt against a session that no longer exists.
		// `grant_auto_signing` refuses to keep a key whose owner is not the
		// session's own key, so the session carries the owner a key is kept
		// under and no root derivation is needed to answer this.
		let (current, _) = self.require_current_session(session)?;
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
		calling_product_id: String,
		request: v01::HostAccountSignVrfRequest,
	) -> Result<v01::VrfSignature, AuthorityError> {
		let product = ProductContext::new(calling_product_id)
			.map_err(|error| AuthorityError::Unknown { reason: error.to_string() })?;
		self.sign_vrf_request(cx, session, AccountCaller::Local { product: &product }, request)
			.await
	}

	async fn sign_payload(
		&self,
		_cx: &CallContext,
		session: &AuthoritySession,
		_calling_product_id: Option<&str>,
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
		_calling_product_id: Option<&str>,
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
		_calling_product_id: Option<&str>,
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
		let granted = match self
			.require_ring_vrf_key_access(&request.calling_product_id, &request.payload.key_handle)
			.await
		{
			Ok(granted) => Some(granted),
			Err(RingVrfError::NotAllowlisted) => None,
			Err(err) => return Err(err),
		};
		// The context is held to the same rule as on `create_proof`: the alias and
		// the proof come from one VRF evaluation.
		let key_handle = match granted {
			Some((key_handle, access)) => {
				crate::runtime::product_manifest::require_context_access(
					&self.services,
					&access,
					&request.payload.context,
				)
				.await?;
				key_handle
			},
			None => {
				// Permission lookups and prompts must use the same canonical identifiers.
				let requester = normalize_product_identifier(&request.calling_product_id)
					.map_err(|_| RingVrfError::NotAllowlisted)?;
				let owner =
					normalize_product_identifier(&request.payload.key_handle.dot_ns_identifier)
						.map_err(|_| RingVrfError::NotAllowlisted)?;
				self.require_account_access(cx, &requester, &owner).await?;
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
			dot_ns_identifier: normalize_product_identifier(&request.calling_product_id)
				.map_err(|err| RingVrfError::Unknown { reason: err.to_string() })?,
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
		let owner = normalize_product_identifier(&request.payload.owner)
			.map_err(|err| RingVrfError::Unknown { reason: err.to_string() })?;
		// Ownership checks and permission decisions must use canonical identifiers.
		let caller = normalize_product_identifier(&request.calling_product_id)
			.map_err(|_| RingVrfError::NotAllowlisted)?;
		if caller != owner {
			self.require_account_access(cx, &caller, &owner).await?;
		}

		self.register_builtin_personhood_keys_if_needed(session, &owner).await?;
		let mut entries = self.ring_vrf_registry.owner_entries(session.public_key, &owner).await?;
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
			.require_ring_vrf_key_access(&request.calling_product_id, &request.payload.key_handle)
			.await?;
		let vrf = vrf::load().await?;
		let entropy = self.resolve_registered_ring_vrf_key(&vrf, session, &key_handle).await?;
		vrf.sign(&entropy, &request.payload.message)
	}

	async fn allocate_resources(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		grant_revision: u64,
		product_id: String,
		request: v01::HostRequestResourceAllocationRequest,
	) -> Result<v01::HostRequestResourceAllocationResponse, AuthorityError> {
		self.require_current_session(session)?;
		self.grants.lifecycle().require_revision(grant_revision)?;
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
				return Err(crate::runtime::authority_cancellation_error(cx, reason));
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
						grant_revision,
						&product_id,
						OnExistingAllowancePolicy::Increase,
					)
					.await
					.map(|_| v01::AllocationOutcome::Allocated),
				v01::AllocatableResource::BulletinAllowance => {
					allowance::allocate_bulletin_allowance(
						&self.services,
						self,
						session,
						&product_id,
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
						&product_id,
						index,
						OnExistingAllowancePolicy::Increase,
					)
					.await
					.map(|()| v01::AllocationOutcome::Allocated)
				},
				v01::AllocatableResource::AutoSigning => self
					.grant_auto_signing(session, grant_revision, &product_id)
					.await
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
		product_id: String,
	) -> Result<StatementStoreAllowanceKey, AuthorityError> {
		let (current, grant_revision) = self.grant_session(session)?;
		#[cfg(feature = "test-host")]
		self.resource_controls
			.refuse_withheld(&v01::AllocatableResource::StatementStoreAllowance)?;
		let period = statement_allowance::slot::current_period(
			allowance::current_unix_secs()
				.map_err(allowance::AllowanceAllocationError::into_authority_error)?,
		);
		if let Some(AccountGrant::StatementStore { key, period: Some(kept_period) }) = self
			.grants
			.cached_allowance(
				&self.session_state,
				&current,
				grant_revision,
				&product_id,
				AllowanceResource::StatementStore,
			)
			.await? && kept_period == period
		{
			return Ok(key);
		}
		self.allocate_statement_store_allowance_key(
			session,
			grant_revision,
			&product_id,
			OnExistingAllowancePolicy::Ignore,
		)
		.await
		.map_err(allowance::AllowanceAllocationError::into_authority_error)
	}

	#[cfg(feature = "test-host")]
	fn submits_preimages_locally(&self) -> bool {
		self.resource_controls.grants_allowances_unchecked() ||
			self.submit_preimages_locally.load(core::sync::atomic::Ordering::Relaxed)
	}

	fn forget_statement_store_allowance_key(&self, product_id: &str, public_key: [u8; 32]) {
		self.grants.forget_statement_store_allowance_key(product_id, public_key);
	}

	async fn bulletin_allowance_key(
		&self,
		_cx: &CallContext,
		session: &AuthoritySession,
		product_id: String,
	) -> Result<BulletinAllowanceKey, AuthorityError> {
		self.require_current_session(session)?;
		#[cfg(feature = "test-host")]
		self.resource_controls
			.refuse_withheld(&v01::AllocatableResource::BulletinAllowance)?;
		let secret = allowance::allocate_bulletin_allowance(
			&self.services,
			self,
			session,
			&product_id,
			OnExistingAllowancePolicy::Ignore,
		)
		.await
		.map_err(allowance::AllowanceAllocationError::into_authority_error)?;
		BulletinAllowanceKey::from_secret_bytes(secret)
	}

	async fn refresh_bulletin_allowance_key(
		&self,
		_cx: &CallContext,
		session: &AuthoritySession,
		product_id: String,
	) -> Result<BulletinAllowanceKey, AuthorityError> {
		self.require_current_session(session)?;
		#[cfg(feature = "test-host")]
		self.resource_controls
			.refuse_withheld(&v01::AllocatableResource::BulletinAllowance)?;
		let secret = allowance::allocate_bulletin_allowance(
			&self.services,
			self,
			session,
			&product_id,
			OnExistingAllowancePolicy::Increase,
		)
		.await
		.map_err(allowance::AllowanceAllocationError::into_authority_error)?;
		BulletinAllowanceKey::from_secret_bytes(secret)
	}

	async fn sign_statement_store_product_payload(
		&self,
		_cx: &CallContext,
		session: &AuthoritySession,
		_calling_product_id: Option<&str>,
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
		product_id: &str,
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
