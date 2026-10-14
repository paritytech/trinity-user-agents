impl<H: AccountHolder> HostAccounts<H> {
	/// Clear all capability material owned by one product while preserving the
	/// active session and unrelated products.
	pub async fn clear_product_state(&self, product_id: &str) -> Result<(), String> {
		let product_id =
			normalize_product_identifier(product_id).map_err(|error| error.to_string())?;
		let session = {
			let mut lifecycle =
				self.session_lifecycle.lock().expect("session lifecycle mutex poisoned");
			lifecycle.advance_preserving_source();
			self.session_state.current()
		};
		let _storage_guard = self.session_secret_storage.lock().await;

		self.statement_store_allowances
			.lock()
			.expect("statement-store allowance cache mutex poisoned")
			.retain(|key, _| !key.is_for_product(&product_id));
		self.bulletin_allowances
			.lock()
			.expect("bulletin allowance cache mutex poisoned")
			.retain(|key, _| !key.is_for_product(&product_id));
		self.product_subtrees
			.lock()
			.expect("product subtree cache mutex poisoned")
			.retain(|(_, cached_product_id), _| cached_product_id != &product_id);
		self.auto_signing_keys
			.lock()
			.expect("AutoSigning key cache mutex poisoned")
			.retain(|(_, cached_product_id), _| cached_product_id != &product_id);

		let mut first_error =
			self.clear_auto_signing_product_under_storage_guard(&product_id).await.err();
		if let Some(session) = session.as_ref() &&
			let Err(error) =
				allowances::clear_product_allowance_keys(&*self.platform, session, &product_id)
					.await && first_error.is_none()
		{
			first_error = Some(error.to_string());
		}
		match first_error {
			Some(error) => Err(error),
			None => Ok(()),
		}
	}

	/// Keep preimage submissions in the core instead of the Bulletin chain.
	///
	/// For test hosts only, with the `test-host` feature enabled.
	#[cfg(feature = "test-host")]
	pub fn set_submit_preimages_locally(&self, local: bool) {
		self.submit_preimages_locally
			.store(local, core::sync::atomic::Ordering::Relaxed);
	}

	#[cfg(feature = "test-host")]
	fn submits_preimages_locally(&self) -> bool {
		self.grant_allowances_unchecked.load(std::sync::atomic::Ordering::Relaxed) ||
			self.submit_preimages_locally.load(core::sync::atomic::Ordering::Relaxed)
	}

	/// The signing session captured before dispatching one request.
	pub fn current_session(&self) -> Option<AuthoritySession> {
		self.signing_host.current_session()
	}

	/// Resolve the canonical account owner this product may access.
	pub async fn authorized_product_account(
		&self,
		dot_ns_identifier: &str,
		cx: &CallContext,
	) -> Option<String> {
		let product_id = self.product_id();
		if crate::platform::is_localhost_product_identifier(&product_id) {
			return normalize_product_identifier(dot_ns_identifier).ok();
		}
		let cx = remote_authority_context(cx);
		self.bounded_cross_product_scope_target(dot_ns_identifier, Granted::Context, &cx)
			.await
	}

	async fn product_account_public_key(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		product_account_id: &latest::ProductAccountId,
	) -> Result<[u8; 32], AuthorityError> {
		let cx = remote_authority_context(cx);
		let subtree = remote_authority_call(
			&cx,
			self.authority.product_subtree_public_key(
				&cx,
				session,
				product_account_id.dot_ns_identifier.clone(),
			),
		)
		.await?;
		derive_product_public_key(
			subtree,
			derivation_index_bytes(&product_account_id.derivation_index),
		)
		.map_err(|err| AuthorityError::Unknown { reason: err.to_string() })
	}

	async fn product_subtree_public_key(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		product_id: String,
	) -> Result<[u8; 32], AuthorityError> {
		PairingHost::product_subtree_public_key(self, cx, session, product_id).await
	}

	async fn statement_store_allowance_key(
		&self,
		_cx: &CallContext,
		session: &AuthoritySession,
		product_id: String,
	) -> Result<StatementStoreAllowanceKey, AuthorityError> {
		let (_, activation_generation) = self.require_current_session(session)?;
		#[cfg(feature = "test-host")]
		self.refuse_withheld(&v01::AllocatableResource::StatementStoreAllowance)?;
		let period = statement_allowance::slot::current_period(
			sso_responder::current_unix_secs()
				.map_err(sso_responder::AllowanceAllocationError::into_authority_error)?,
		);
		if let Some(key) = self
			.local_grants
			.lock()
			.expect("local AutoSigning grant mutex poisoned")
			.statement_allowance_key(activation_generation, &product_id, period)?
		{
			return Ok(key.clone());
		}
		self.allocate_statement_store_allowance_key(
			session,
			&product_id,
			OnExistingAllowancePolicy::Ignore,
		)
		.await
		.map_err(sso_responder::AllowanceAllocationError::into_authority_error)
	}

	async fn bulletin_allowance_key(
		&self,
		_cx: &CallContext,
		session: &AuthoritySession,
		product_id: String,
	) -> Result<BulletinAllowanceKey, AuthorityError> {
		self.require_current_session(session)?;
		#[cfg(feature = "test-host")]
		self.refuse_withheld(&v01::AllocatableResource::BulletinAllowance)?;
		let secret = sso_responder::allocate_bulletin_allowance(
			&self.services,
			self,
			session,
			&product_id,
			OnExistingAllowancePolicy::Ignore,
		)
		.await
		.map_err(sso_responder::AllowanceAllocationError::into_authority_error)?;
		BulletinAllowanceKey::from_secret_bytes(secret)
	}

	async fn allocate_bulletin_allowance_key(
		&self,
		cx: &CallContext,
		session: &SessionInfo,
		lifecycle_epoch: u64,
		product_id: String,
		on_existing: OnExistingAllowancePolicy,
	) -> Result<BulletinAllowanceKey, AuthorityError> {
		match self
			.remote_allowance_slot(
				cx,
				session,
				&product_id,
				latest::AllocatableResource::BulletinAllowance,
				on_existing,
			)
			.await?
		{
			SsoAllocatedResource::BulletinAllowance { slot_account_key } => {
				self.cache_bulletin_allowance_key(
					session,
					lifecycle_epoch,
					&product_id,
					slot_account_key,
				)
				.await
			},
			other => Err(unexpected_resource("bulletin allowance", &other)),
		}
	}
}

impl<H: AccountHolder> HostAccounts<H> {
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

	async fn sign_vrf(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		calling_product_id: String,
		request: latest::HostAccountSignVrfRequest,
	) -> Result<latest::VrfSignature, AuthorityError> {
		let session = self.current_private_session(session)?;
		if calling_product_id == request.account.dot_ns_identifier &&
			let Some(auto_signing_key) =
				self.auto_signing_key(&session, &request.account.dot_ns_identifier).await?
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
		if !super::authority::is_blessed_owner(
			&calling_product_id,
			&request.account.dot_ns_identifier,
		) {
			let confirmed = super::until_cancelled(
				cx,
				self.platform
					.confirm_user_action(UserConfirmationReview::SignVrf(SignVrfReview {
						calling_product_id: calling_product_id.clone(),
						request: request.clone(),
					})),
			)
			.await?
			.map_err(|err| AuthorityError::Unknown {
				reason: format!("VRF signing confirmation failed: {err:?}"),
			})?;
			if !confirmed {
				return Err(AuthorityError::Rejected);
			}
		}
		self.remote_sign_vrf(cx, &session, calling_product_id, request).await
	}
}

impl<H: AccountHolder + 'static> HostAccounts<H> {
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
			let context = development_context_bytes(&request.payload.context);
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
			let context = development_context_bytes(&request.payload.context);
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
}
