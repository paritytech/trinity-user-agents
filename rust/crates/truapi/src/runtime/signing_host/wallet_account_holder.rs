// Allocation uses `track`; the renewal loop around it is driven by native
// entry points only, so on wasm the rest of the module is not reached yet.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
mod allowance_renewal;

/// RFC-0022 domain for the responder's persistent SSO X25519 key.
const SSO_ENCRYPTION_DOMAIN: &[u8] = b"sso";

impl WalletAccountHolder {
	fn session_state(&self) -> Arc<SessionState> {
		PairingHost::session_state(self)
	}
}

impl WalletKeys {
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

	fn product_subtree_secret(&self, product_id: &str) -> Result<[u8; 64], AuthorityError> {
		let entropy = self.root_entropy()?;
		let root = derive_root_keypair_from_entropy(&entropy).map_err(product_authority_error)?;
		let product_id = normalize_product_identifier(product_id)
			.map_err(|err| AuthorityError::Unavailable { reason: err.to_string() })?;
		derive_product_subtree_keypair(&root, &product_id)
			.map(|keypair| keypair.secret.to_bytes())
			.map_err(product_authority_error)
	}

	fn product_keypair(
		&self,
		account: &v01::ProductAccountId,
	) -> Result<schnorrkel::Keypair, AuthorityError> {
		self.product_keypair_with_owner(account).map(|(_, keypair)| keypair)
	}

	fn identity_keypair(&self) -> Result<schnorrkel::Keypair, AuthorityError> {
		let entropy = self.root_entropy()?;
		derive_identity_keypair(&entropy, &self.network_suffix).map_err(product_authority_error)
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
}

fn product_authority_error(err: ProductAccountError) -> AuthorityError {
	AuthorityError::Unavailable { reason: err.to_string() }
}
