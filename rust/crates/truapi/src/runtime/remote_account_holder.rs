#[async_trait::async_trait]
impl AccountHolder for RemoteAccountHolder {
	fn current_session(&self) -> Option<AuthoritySession> {
		self.current_local_session()
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
		PairingHost::ring_vrf_sign(self, cx, session, request).await
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
}

fn remote_authority_error(reason: impl Into<SsoRemoteResponseError>) -> AuthorityError {
	match reason.into() {
		SsoRemoteResponseError::Cancelled(err) => AuthorityError::Cancelled(
			AuthorityCancelError::new(err.remote_message_id(), err.reason()),
		),
		SsoRemoteResponseError::LocalDisconnected | SsoRemoteResponseError::PeerDisconnected => {
			AuthorityError::Disconnected
		},
		SsoRemoteResponseError::Failure(reason) => match reason.as_str() {
			"Rejected" | "User rejected" => AuthorityError::Rejected,
			SSO_LOCAL_DISCONNECT_REASON | SSO_PEER_DISCONNECT_REASON => {
				AuthorityError::Disconnected
			},
			_ => AuthorityError::Unknown { reason },
		},
	}
}

/// Reason for an allocation that came back as a different resource kind; names
/// only the kind so no key material reaches logs.
fn unexpected_resource(label: &str, resource: &SsoAllocatedResource) -> AuthorityError {
	AuthorityError::Unknown {
		reason: format!("Unexpected {label} response resource: {}", resource.kind()),
	}
}
