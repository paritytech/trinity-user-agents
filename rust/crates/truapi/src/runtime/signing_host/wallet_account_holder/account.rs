impl WalletAccountHolder {
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

impl WalletAccountHolder {
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
}

#[async_trait::async_trait]
impl AccountHolder for WalletAccountHolder {
	fn current_session(&self) -> Option<AuthoritySession> {
		PairingHost::current_session(self)
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

	async fn product_subtree_public_key(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		product_id: String,
	) -> Result<[u8; 32], AuthorityError> {
		let session = self.current_private_session(session)?;
		self.remote_product_subtree_public_key(cx, &session, product_id).await
	}

	async fn sign_vrf(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		calling_product_id: String,
		request: v01::HostAccountSignVrfRequest,
	) -> Result<v01::VrfSignature, AuthorityError> {
		self.sign_vrf_request(cx, session, calling_product_id, request, true).await
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

	async fn account_alias(
		&self,
		_cx: &CallContext,
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
				match super::account_access_authorization(
					self.services.platform.as_ref(),
					&requester,
					&owner,
				)
				.await
				{
					Ok(PermissionAuthorizationStatus::Authorized) => {},
					Ok(
						PermissionAuthorizationStatus::Denied |
						PermissionAuthorizationStatus::NotDetermined,
					) => return Err(RingVrfError::Rejected),
					Err(err) => {
						return Err(RingVrfError::Unknown { reason: err.to_string() });
					},
				}
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
		let context = development_context_bytes(&request.payload.context);
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
		let context = development_context_bytes(&request.payload.context);
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
		_cx: &CallContext,
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
			match super::account_access_authorization(
				self.services.platform.as_ref(),
				&caller,
				&owner,
			)
			.await
			{
				Ok(PermissionAuthorizationStatus::Authorized) => {},
				Ok(
					PermissionAuthorizationStatus::Denied |
					PermissionAuthorizationStatus::NotDetermined,
				) => return Err(RingVrfError::Rejected),
				Err(err) => {
					return Err(RingVrfError::Unknown { reason: err.to_string() });
				},
			}
		}

		self.register_builtin_personhood_keys_if_needed(session, &owner).await?;
		let mut entries = self.ring_vrf_registry.owner_entries(session.public_key, &owner).await?;
		self.require_current_session(session)?;
		if request.payload.disclosure == v01::RingVrfKeyDisclosure::Anonymized {
			for entry in &mut entries {
				entry.public_key = None;
			}
		}
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
		PairingHost::derive_entropy(self, session, product_id, context)
	}
}
