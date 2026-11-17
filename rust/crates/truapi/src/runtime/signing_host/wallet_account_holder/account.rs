//! Wallet account execution and access to other products' keys.

use super::{WalletAccountHolder, WalletKeys, allowance::AllowanceAllocationError};
use std::sync::Arc;

use crate::{
	host_internal::{
		extrinsic::{Sr25519Signer, build_signed_transaction, local_transaction_metadata},
		sso_messages::{OnExistingAllowancePolicy, RingVrfError},
		transaction::sign_extrinsic_payload,
	},
	host_logic::{
		product_account::{SR25519_SIGNING_CONTEXT, personhood_product_id},
		raw_signing::raw_payload_bytes,
		session::{SessionInfo, SessionState},
	},
	platform::{PermissionAuthorizationStatus, ProductId},
	runtime::{
		allowances::AllowanceResource,
		authority::{
			AccountGrant, AccountGrantOutcome, AccountHolder, AccountInvocation, Allowance,
			AuthorityError, AuthoritySession, AutoSigningKey, BulletinAllowanceKey,
			CreateTransactionAuthorityRequest, GrantKeeping, SignPayloadAuthorityRequest,
			SignRawAuthorityRequest, StatementStoreAllowanceKey, SubtreeResolution,
		},
		authority_cancellation_error,
		contacts::ContactHandles,
		product_manifest::{
			AuthorizedAccess, require_context_access, ring_vrf_key_access_granted,
			ring_vrf_key_access_granted_or_consent,
		},
		remote_authority_call, remote_authority_context,
		ring_vrf_registry::apply_ring_vrf_disclosure,
		signing_host::ring_vrf::{MemberCandidate, context_bytes, create_proof},
		statement_allowance::collection::PersonhoodCollection,
		until_cancelled,
		vrf::{self, Vrf},
	},
};
use futures::{
	StreamExt,
	stream::{self, BoxStream},
};
use truapi::{
	latest as api,
	latest::{
		DerivationIndex, HostAccountCreateProofRequest, HostAccountGetAliasRequest,
		HostAccountListRingVrfKeysRequest, HostAccountRegisterRingVrfKeyRequest,
		HostAccountRingVrfSignRequest, ProductAccountId, RingLocation, RingLocationJunction,
	},
};

impl WalletAccountHolder {
	fn caller_key_handle(
		invocation: &AccountInvocation<'_>,
		index: DerivationIndex,
	) -> Result<ProductAccountId, RingVrfError> {
		let owner = invocation.caller.product_id().ok_or(RingVrfError::NotAllowlisted)?;
		Ok(ProductAccountId { dot_ns_identifier: owner.clone(), derivation_index: index })
	}

	async fn derive_ring_vrf_member(
		&self,
		session: &AuthoritySession,
		handle: &ProductAccountId,
	) -> Result<[u8; 32], RingVrfError> {
		let vrf = vrf::load().await?;
		self.with_keys(session, |keys| vrf.member(&*keys.ring_vrf_entropy(handle)?))
	}

	async fn record_registration(
		&self,
		session: &AuthoritySession,
		handle: ProductAccountId,
		ring: RingLocation,
		public_key: [u8; 32],
	) -> Result<(), RingVrfError> {
		let mut update = self.ring_vrf_registry.prepare_update(session.public_key).await?;
		self.require_current_session(session)?;
		update.register(handle, ring, public_key)?;
		update.persist().await?;
		self.require_current_session(session)?;
		Ok(())
	}

	/// Derive the product's hard-subtree public key from the active session root.
	/// Returns `None` when no session is active.
	pub fn derive_subtree_public_key(
		&self,
		product_id: &ProductId,
	) -> Result<Option<[u8; 32]>, AuthorityError> {
		let Some(session) = self.current_session() else {
			return Ok(None);
		};
		self.with_keys(&session, |keys| {
			Ok(keys.product_subtree_keypair(product_id)?.public.to_bytes())
		})
		.map(Some)
	}

	fn transaction_keypair(
		keys: &WalletKeys,
		request: &CreateTransactionAuthorityRequest,
	) -> Result<schnorrkel::Keypair, AuthorityError> {
		match request {
			// The product account is authoritative and caller-scoping is
			// enforced upstream, so the derived key defines the signer.
			CreateTransactionAuthorityRequest::Product(payload) => {
				keys.product_keypair(&payload.signer)
			},
			CreateTransactionAuthorityRequest::LegacyAccount { product_account, request } => {
				let keypair = keys.product_keypair(product_account)?;
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
				Ok(keypair)
			},
			CreateTransactionAuthorityRequest::IdentityAccount(request) => {
				keys.identity_keypair_for(request.signer)
			},
		}
	}

	async fn register_builtin_personhood_keys_if_needed(
		&self,
		session: &AuthoritySession,
		owner: &ProductId,
	) -> Result<(), RingVrfError> {
		if owner != &personhood_product_id(self.services.stack().tld()) {
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
			self.require_current_session(session)?;
			return Ok(());
		}
		let pallet_index = self.ring_resolver.members_pallet_index(&chain_id).await?;
		let vrf = vrf::load().await?;
		for (collection, index) in missing {
			let handle = ProductAccountId {
				dot_ns_identifier: owner.clone(),
				derivation_index: DerivationIndex::Index(index),
			};
			let public_key =
				self.with_keys(session, |keys| vrf.member(&*keys.ring_vrf_entropy(&handle)?))?;
			let ring = RingLocation {
				chain_id,
				junctions: vec![
					RingLocationJunction::PalletInstance(pallet_index),
					RingLocationJunction::CollectionId(collection.identifier().to_vec()),
				],
			};
			self.record_registration(session, handle, ring, public_key).await?;
		}
		Ok(())
	}

	async fn registered_ring_vrf_entry(
		&self,
		session: &AuthoritySession,
		handle: &ProductAccountId,
	) -> Result<Option<api::RegisteredRingVrfKey>, RingVrfError> {
		self.require_current_session(session)?;
		let entry = self.ring_vrf_registry.entry(session.public_key, handle).await?;
		self.require_current_session(session)?;
		Ok(entry)
	}

	async fn registered_ring_vrf_entry_for_ring(
		&self,
		session: &AuthoritySession,
		handle: &ProductAccountId,
		ring: &RingLocation,
	) -> Result<api::RegisteredRingVrfKey, RingVrfError> {
		let entry = self
			.registered_ring_vrf_entry(session, handle)
			.await?
			.ok_or(RingVrfError::KeyNotRegistered)?;
		if !entry.rings.contains(ring) {
			return Err(RingVrfError::KeyNotInRing);
		}
		Ok(entry)
	}

	fn require_matching_registered_public_key(
		vrf: &Vrf,
		entry: &api::RegisteredRingVrfKey,
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

	/// Whether the invocation's caller may act on `handle`'s ring-VRF key.
	///
	/// Delegates to [`ring_vrf_key_access_granted`], which resolves the owner's
	/// manifest here rather than trusting the request: on this role the
	/// request can have arrived over the pairing wire.
	async fn require_ring_vrf_key_access(
		&self,
		invocation: &AccountInvocation<'_>,
		handle: &ProductAccountId,
	) -> Result<(ProductAccountId, AuthorizedAccess), RingVrfError> {
		let access = ring_vrf_key_access_granted(
			&self.services,
			self.services.platform.as_ref(),
			invocation.caller.product_id().ok_or(RingVrfError::Rejected)?.clone(),
			handle,
		)
		.await?;
		Ok((
			ProductAccountId {
				dot_ns_identifier: access.owner.clone(),
				derivation_index: handle.derivation_index.clone(),
			},
			access,
		))
	}

	async fn require_account_access(
		&self,
		invocation: &AccountInvocation<'_>,
		requester: &ProductId,
		owner: &ProductId,
	) -> Result<(), RingVrfError> {
		let access = self.consent.account_access(requester, owner, self.services.platform.as_ref());
		let status = until_cancelled(invocation.call, access)
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
impl AccountHolder for WalletAccountHolder {
	const GRANT_KEEPING: GrantKeeping = GrantKeeping::Wallet;

	fn current_session(&self) -> Option<AuthoritySession> {
		let state = self.lifecycle.lock().expect("wallet lifecycle mutex poisoned");
		let session = self.session_state.current()?;
		state.keys.as_ref()?;
		Some(state.session(&session))
	}

	fn session_state(&self) -> Arc<SessionState> {
		WalletAccountHolder::session_state(self)
	}

	fn require_current_session(
		&self,
		session: &AuthoritySession,
	) -> Result<SessionInfo, AuthorityError> {
		self.lifecycle
			.lock()
			.expect("wallet lifecycle mutex poisoned")
			.require_session(self.session_state.current(), session)
	}

	async fn allocate_grants<'a>(
		&'a self,
		invocation: AccountInvocation<'a>,
		request: api::HostRequestResourceAllocationRequest,
		policy: OnExistingAllowancePolicy,
	) -> Result<BoxStream<'a, Result<AccountGrantOutcome, AuthorityError>>, AuthorityError> {
		let product_id = invocation.caller.product_id().ok_or(AuthorityError::Rejected)?.clone();
		self.require_current_session(invocation.session)?;
		Ok(stream::unfold(
			(request.resources.into_iter(), product_id, invocation),
			move |(mut resources, product_id, invocation)| async move {
				let resource = resources.next()?;
				let outcome = async {
					self.require_current_session(invocation.session)?;
					if let Some(reason) = invocation.call.cancel().reason() {
						return Err(authority_cancellation_error(invocation.call, reason).into());
					}
					// Checked before the work, not after: withholding is the suite
					// saying this resource is refused, so performing the allocation
					// and then reporting a refusal would leave the two disagreeing.
					#[cfg(feature = "test-host")]
					self.resource_controls.refuse_withheld(&resource)?;
					let grant = match resource {
						api::AllocatableResource::StatementStoreAllowance => {
							let allocation = self
								.allocate_statement_store_allowance(
									invocation.session,
									&product_id,
									policy,
								)
								.await?;
							AccountGrant::Allowance(Allowance::StatementStore {
								key: StatementStoreAllowanceKey::from_secret_bytes(
									allocation.secret,
								)?,
								period: Some(allocation.period),
							})
						},
						api::AllocatableResource::BulletinAllowance => AccountGrant::Allowance(
							Allowance::Bulletin(BulletinAllowanceKey::from_secret_bytes(
								self.allocate_bulletin_allowance(
									invocation.session,
									&product_id,
									policy,
								)
								.await?,
							)?),
						),
						api::AllocatableResource::SmartContractAllowance(index) => {
							self.allocate_smart_contract_allowance(
								invocation.session,
								&product_id,
								index,
								policy,
							)
							.await?;
							AccountGrant::SmartContract
						},
						api::AllocatableResource::AutoSigning => self
							.with_keys::<_, AuthorityError>(invocation.session, |keys| {
								Ok(AccountGrant::AutoSigning(AutoSigningKey::from_parts(
									keys.product_subtree_keypair(&product_id)?.secret.to_bytes(),
									keys.ring_vrf_domain_entropy(&product_id)?,
								)))
							})?,
					};
					Ok(grant)
				}
				.await;
				let outcome = self
					.require_current_session(invocation.session)
					.map_err(AllowanceAllocationError::from)
					.and(outcome);
				let outcome = match outcome {
					Ok(grant) => Ok(AccountGrantOutcome::Allocated(grant)),
					Err(AllowanceAllocationError::Authority(
						error @ (AuthorityError::Disconnected | AuthorityError::Cancelled(_)),
					)) => Err(error),
					Err(AllowanceAllocationError::Authority(AuthorityError::Rejected)) => {
						Ok(AccountGrantOutcome::Rejected)
					},
					Err(error) => {
						Ok(AccountGrantOutcome::NotAvailable { reason: Some(error.to_string()) })
					},
				};
				Some((outcome, (resources, product_id, invocation)))
			},
		)
		.boxed())
	}

	async fn ensure_allowance(
		&self,
		invocation: AccountInvocation<'_>,
		resource: AllowanceResource,
		policy: OnExistingAllowancePolicy,
	) -> Result<AccountGrant, AuthorityError> {
		let resource = match resource {
			AllowanceResource::StatementStore => api::AllocatableResource::StatementStoreAllowance,
			AllowanceResource::Bulletin => api::AllocatableResource::BulletinAllowance,
		};
		let mut grants = self
			.allocate_grants(
				invocation,
				api::HostRequestResourceAllocationRequest { resources: vec![resource] },
				policy,
			)
			.await?;
		match grants.next().await.transpose()? {
			Some(AccountGrantOutcome::Allocated(grant)) => Ok(grant),
			Some(AccountGrantOutcome::NotAvailable { reason }) => {
				Err(AuthorityError::Unavailable {
					reason: reason.unwrap_or_else(|| "Allowance unavailable".to_string()),
				})
			},
			Some(AccountGrantOutcome::Rejected) | None => Err(AuthorityError::Rejected),
		}
	}

	async fn product_subtree_public_key<'a>(
		&'a self,
		invocation: AccountInvocation<'a>,
		product_id: &'a ProductId,
	) -> Result<SubtreeResolution<'a>, AuthorityError> {
		self.with_keys(invocation.session, |keys| {
			Ok(SubtreeResolution::Known(
				keys.product_subtree_keypair(product_id)?.public.to_bytes(),
			))
		})
	}

	async fn sign_vrf(
		&self,
		invocation: AccountInvocation<'_>,
		request: api::HostAccountSignVrfRequest,
	) -> Result<api::VrfSignature, AuthorityError> {
		self.with_keys(invocation.session, |keys| {
			let keypair = keys.product_keypair(&request.account)?;
			let (pre_output, proof) = crate::dynamic_vrf::sign_dynamic_vrf(
				&keypair,
				&request.transcript_label,
				request.items.iter().map(|item| (item.label.as_slice(), item.value.as_slice())),
			);
			Ok(api::VrfSignature { pre_output, proof })
		})
	}

	async fn sign_payload(
		&self,
		invocation: AccountInvocation<'_>,
		request: SignPayloadAuthorityRequest,
	) -> Result<api::HostSignPayloadResponse, AuthorityError> {
		self.require_current_session(invocation.session)?;
		let cx = remote_authority_context(invocation.call);
		remote_authority_call(&cx, async {
			self.with_keys(invocation.session, |keys| {
				let (account, payload) = match request {
					SignPayloadAuthorityRequest::Product(request) => {
						(request.account, request.payload)
					},
					SignPayloadAuthorityRequest::LegacyAccount { product_account, request } => {
						(product_account, request.payload)
					},
				};
				Ok(sign_extrinsic_payload(&keys.product_keypair(&account)?, payload)?)
			})
		})
		.await
	}

	async fn sign_raw(
		&self,
		invocation: AccountInvocation<'_>,
		request: SignRawAuthorityRequest,
		watermarked: bool,
	) -> Result<api::HostSignPayloadResponse, AuthorityError> {
		self.require_current_session(invocation.session)?;
		let cx = remote_authority_context(invocation.call);
		remote_authority_call(&cx, async {
			self.with_keys(invocation.session, |keys| {
				let (keypair, payload) = match request {
					SignRawAuthorityRequest::Product(request) => {
						(keys.product_keypair(&request.account)?, request.payload)
					},
					SignRawAuthorityRequest::LegacyAccount { product_account, request } => {
						(keys.product_keypair(&product_account)?, request.payload)
					},
					SignRawAuthorityRequest::IdentityAccount { account, request } => {
						(keys.identity_keypair_for(account)?, request.payload)
					},
				};
				let message = raw_payload_bytes(payload, watermarked)?;
				let signature = keypair
					.secret
					.sign_simple(SR25519_SIGNING_CONTEXT, &message, &keypair.public)
					.to_bytes();
				Ok(api::HostSignPayloadResponse {
					signature: signature.to_vec(),
					signed_transaction: None,
				})
			})
		})
		.await
	}

	async fn create_transaction(
		&self,
		invocation: AccountInvocation<'_>,
		request: CreateTransactionAuthorityRequest,
	) -> Result<api::HostCreateTransactionResponse, AuthorityError> {
		self.require_current_session(invocation.session)?;
		let cx = remote_authority_context(invocation.call);
		remote_authority_call(&cx, async {
			self.with_keys(invocation.session, |keys| {
				Self::transaction_keypair(keys, &request).map(|_| ())
			})?;
			let (genesis_hash, call_data, extensions, tx_ext_version) = match &request {
				CreateTransactionAuthorityRequest::Product(payload) => (
					payload.genesis_hash,
					&payload.call_data,
					&payload.extensions,
					payload.tx_ext_version,
				),
				CreateTransactionAuthorityRequest::LegacyAccount { request, .. } |
				CreateTransactionAuthorityRequest::IdentityAccount(request) => (
					request.genesis_hash,
					&request.call_data,
					&request.extensions,
					request.tx_ext_version,
				),
			};
			let metadata = local_transaction_metadata(&self.services.chain, genesis_hash).await?;
			self.with_keys(invocation.session, |keys| {
				let keypair = Self::transaction_keypair(keys, &request)?;
				Ok(api::HostCreateTransactionResponse {
					transaction: build_signed_transaction(
						&Sr25519Signer::from_keypair(&keypair),
						genesis_hash,
						call_data,
						extensions,
						tx_ext_version,
						metadata,
					)?,
				})
			})
		})
		.await
	}

	async fn account_alias(
		&self,
		invocation: AccountInvocation<'_>,
		request: HostAccountGetAliasRequest,
	) -> Result<api::ContextualAlias, RingVrfError> {
		let session = invocation.session;
		self.require_current_session(session)?;
		let requester = invocation.caller.product_id().ok_or(RingVrfError::NotAllowlisted)?;
		// Aliases expose the same identity as `create_proof`, so both must enforce
		// the same RFC-0024 grant and context restrictions.
		let key_access = ring_vrf_key_access_granted_or_consent(
			&self.services,
			self.services.platform.as_ref(),
			requester.clone(),
			&request.key_handle,
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
				require_context_access(&self.services, &access, &request.context).await?;
				ProductAccountId {
					dot_ns_identifier: access.owner,
					derivation_index: request.key_handle.derivation_index.clone(),
				}
			},
			None => {
				// Permission lookups and prompts must use the same canonical identifiers.
				let owner = request.key_handle.dot_ns_identifier.clone();
				self.require_account_access(&invocation, requester, &owner).await?;
				ProductAccountId {
					dot_ns_identifier: owner,
					derivation_index: request.key_handle.derivation_index.clone(),
				}
			},
		};
		let vrf = vrf::load().await?;
		let entry = self
			.registered_ring_vrf_entry_for_ring(session, &key_handle, &request.ring_location)
			.await?;
		self.ring_resolver.validate(&request.ring_location).await?;
		let context = context_bytes(&request.context);
		self.with_keys(session, |keys| {
			let entropy = keys.ring_vrf_entropy(&key_handle)?;
			Self::require_matching_registered_public_key(&vrf, &entry, &entropy)?;
			let alias = vrf.alias(&entropy, &context)?;
			Ok(api::ContextualAlias { context, alias: alias.to_vec() })
		})
	}

	async fn create_proof(
		&self,
		invocation: AccountInvocation<'_>,
		request: HostAccountCreateProofRequest,
	) -> Result<api::HostAccountCreateProofResponse, RingVrfError> {
		let session = invocation.session;
		self.require_current_session(session)?;
		let (key_handle, access) =
			self.require_ring_vrf_key_access(&invocation, &request.key_handle).await?;
		// A grant cannot expose the owner's alias in a third product's context unless
		// that product's manifest grants the caller `context`.
		require_context_access(&self.services, &access, &request.context).await?;
		let vrf = vrf::load().await?;
		let entry = self
			.registered_ring_vrf_entry_for_ring(session, &key_handle, &request.ring_location)
			.await?;
		let candidate = self.with_keys(session, |keys| {
			let entropy = keys.ring_vrf_entropy(&key_handle)?;
			Self::require_matching_registered_public_key(&vrf, &entry, &entropy)?;
			Ok::<_, RingVrfError>(MemberCandidate { member: vrf.member(&entropy)? })
		})?;
		let resolved = self.ring_resolver.resolve(&request.ring_location, &[candidate]).await?;
		// Proved outside the wallet lock, which would otherwise stall every
		// signing call, lock and wallet switch for the whole proof.
		let entropy = self.with_keys(session, |keys| keys.ring_vrf_entropy(&key_handle))?;
		let context = context_bytes(&request.context);
		let (proof, alias) = create_proof(&vrf, &entropy, &resolved, &context, &request.message)?;
		self.require_current_session(session)?;
		Ok(api::HostAccountCreateProofResponse {
			proof,
			contextual_alias: api::ContextualAlias { context, alias: alias.to_vec() },
			ring_index: resolved.ring_index,
			ring_revision: resolved.ring_revision,
		})
	}

	async fn register_ring_vrf_key(
		&self,
		invocation: AccountInvocation<'_>,
		request: HostAccountRegisterRingVrfKeyRequest,
	) -> Result<[u8; 32], RingVrfError> {
		self.require_current_session(invocation.session)?;
		self.ring_resolver.validate(&request.ring).await?;
		let handle = Self::caller_key_handle(&invocation, request.index)?;
		let public_key = self.derive_ring_vrf_member(invocation.session, &handle).await?;
		self.record_registration(invocation.session, handle, request.ring, public_key)
			.await?;
		Ok(public_key)
	}

	async fn record_ring_vrf_key(
		&self,
		invocation: AccountInvocation<'_>,
		request: HostAccountRegisterRingVrfKeyRequest,
		public_key: [u8; 32],
	) -> Result<(), RingVrfError> {
		self.require_current_session(invocation.session)?;
		let handle = Self::caller_key_handle(&invocation, request.index)?;
		if self.derive_ring_vrf_member(invocation.session, &handle).await? != public_key {
			return Err(RingVrfError::Unknown {
				reason: "the kept AutoSigning key derives a different ring-VRF key".to_string(),
			});
		}
		self.record_registration(invocation.session, handle, request.ring, public_key)
			.await
	}

	async fn list_ring_vrf_keys(
		&self,
		invocation: AccountInvocation<'_>,
		request: HostAccountListRingVrfKeysRequest,
	) -> Result<Vec<api::RegisteredRingVrfKey>, RingVrfError> {
		let session = invocation.session;
		self.require_current_session(session)?;
		let owner = &request.owner;
		let caller = invocation.caller.product_id().ok_or(RingVrfError::NotAllowlisted)?;
		if caller != owner {
			self.require_account_access(&invocation, caller, owner).await?;
		}

		self.register_builtin_personhood_keys_if_needed(session, owner).await?;
		let mut entries = self.ring_vrf_registry.owner_entries(session.public_key, owner).await?;
		self.require_current_session(session)?;
		apply_ring_vrf_disclosure(&mut entries, request.disclosure);
		Ok(entries)
	}

	async fn ring_vrf_sign(
		&self,
		invocation: AccountInvocation<'_>,
		request: HostAccountRingVrfSignRequest,
	) -> Result<Vec<u8>, RingVrfError> {
		let session = invocation.session;
		self.require_current_session(session)?;
		let (key_handle, _access) =
			self.require_ring_vrf_key_access(&invocation, &request.key_handle).await?;
		let vrf = vrf::load().await?;
		let entry = self
			.registered_ring_vrf_entry(session, &key_handle)
			.await?
			.ok_or(RingVrfError::KeyNotRegistered)?;
		self.with_keys(session, |keys| {
			let entropy = keys.ring_vrf_entropy(&key_handle)?;
			Self::require_matching_registered_public_key(&vrf, &entry, &entropy)?;
			vrf.sign(&entropy, &request.message)
		})
	}

	async fn sign_statement_store_product_payload(
		&self,
		invocation: AccountInvocation<'_>,
		account: ProductAccountId,
		payload: Vec<u8>,
	) -> Result<[u8; 64], AuthorityError> {
		let cx = remote_authority_context(invocation.call);
		remote_authority_call(&cx, async {
			self.with_keys(invocation.session, |keys| {
				let keypair = keys.product_keypair(&account)?;
				Ok(keypair
					.secret
					.sign_simple(SR25519_SIGNING_CONTEXT, &payload, &keypair.public)
					.to_bytes())
			})
		})
		.await
	}

	fn derive_entropy(
		&self,
		session: &AuthoritySession,
		product_id: &ProductId,
		context: &[u8],
	) -> Result<[u8; 32], AuthorityError> {
		self.with_keys(session, |keys| keys.derive_entropy(product_id, context))
	}

	fn contact_handles(
		&self,
		session: &AuthoritySession,
	) -> Result<ContactHandles, AuthorityError> {
		self.with_keys(session, |keys| Ok(keys.contact_handles()))
	}
}
