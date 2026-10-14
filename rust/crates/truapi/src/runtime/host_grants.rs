#[derive(Clone, PartialEq, Eq, Hash, Encode, Decode)]
struct AutoSigningOwner {
	root_public_key: [u8; 32],
	authenticated_sso_identity: Option<[u8; 32]>,
}

impl AutoSigningOwner {
	fn from_session(session: &SessionInfo) -> Self {
		Self {
			root_public_key: session.public_key,
			authenticated_sso_identity: session.sso.as_ref().map(|sso| sso.identity_account_id),
		}
	}
}

#[derive(Encode, Decode, zeroize::ZeroizeOnDrop)]
struct PersistedAutoSigningKey {
	#[zeroize(skip)]
	owner: AutoSigningOwner,
	#[zeroize(skip)]
	product_id: String,
	#[zeroize(skip)]
	expected_product_subtree_public_key: [u8; 32],
	secret: [u8; 64],
	ring_vrf_domain_entropy: [u8; 32],
}

type AutoSigningCacheKey = (AutoSigningOwner, String);

fn decode_auto_signing_keys(blob: &[u8]) -> Result<Vec<PersistedAutoSigningKey>, AuthorityError> {
	let mut input = blob;
	let keys = Vec::<PersistedAutoSigningKey>::decode(&mut input).map_err(|_| {
		AuthorityError::Unavailable {
			reason: "persisted AutoSigning capabilities are invalid".to_string(),
		}
	})?;
	if !input.is_empty() {
		return Err(AuthorityError::Unavailable {
			reason: "persisted AutoSigning capabilities contain trailing bytes".to_string(),
		});
	}
	Ok(keys)
}

fn validate_auto_signing_key(
	secret: [u8; 64],
	expected_product_subtree_public_key: [u8; 32],
	ring_vrf_domain_entropy: [u8; 32],
) -> Result<AutoSigningKey, AuthorityError> {
	let secret = Zeroizing::new(secret);
	let ring_vrf_domain_entropy = Zeroizing::new(ring_vrf_domain_entropy);
	let secret_key =
		SecretKey::from_bytes(&secret[..]).map_err(|_| AuthorityError::Unavailable {
			reason: "AutoSigning capability contains an invalid subtree secret".to_string(),
		})?;
	if secret_key.to_public().to_bytes() != expected_product_subtree_public_key {
		return Err(AuthorityError::Unavailable {
			reason: "AutoSigning capability does not match the authenticated product subtree"
				.to_string(),
		});
	}
	Ok(AutoSigningKey::from_parts(*secret, *ring_vrf_domain_entropy))
}

impl HostGrantStore {
	fn session_secret_allocation_is_current(
		&self,
		session: &SessionInfo,
		lifecycle_epoch: u64,
	) -> bool {
		session.sso.as_ref().is_some_and(|sso| {
			self.current_sso_session_matches(SsoSessionKey::from_session(sso)) &&
				self.is_session_lifecycle_current(lifecycle_epoch)
		})
	}

	fn cache_auto_signing_key_if_current(
		&self,
		session: &SessionInfo,
		lifecycle_epoch: u64,
		cache_key: AutoSigningCacheKey,
		key: AutoSigningKey,
	) -> bool {
		let lifecycle = self.session_lifecycle.lock().expect("session lifecycle mutex poisoned");
		if lifecycle.epoch != lifecycle_epoch {
			return false;
		}
		let Some(sso) = session.sso.as_ref() else {
			return false;
		};
		if !self.current_sso_session_matches(SsoSessionKey::from_session(sso)) {
			return false;
		}
		self.auto_signing_keys
			.lock()
			.expect("AutoSigning key cache mutex poisoned")
			.insert(cache_key, key);
		true
	}

	/// Load a retained public subtree without contacting the account holder.
	pub async fn known_product_subtree(
		&self,
		session: &SessionInfo,
		cache_key: (SsoSessionKey, String),
	) -> Option<[u8; 32]> {
		let lifecycle_epoch = self.current_session_lifecycle_epoch();
		if let Some(public_key) = self
			.product_subtrees
			.lock()
			.expect("product subtree cache mutex poisoned")
			.get(&cache_key)
			.copied()
		{
			return Some(public_key);
		}
		self.stored_product_subtree(session, lifecycle_epoch, cache_key).await
	}

	/// Restore a public subtree only into the selected live session.
	pub async fn stored_product_subtree(
		&self,
		session: &SessionInfo,
		lifecycle_epoch: u64,
		cache_key: (SsoSessionKey, String),
	) -> Option<[u8; 32]> {
		let public_key =
			match product_subtree::read_product_subtree(&*self.platform, session, &cache_key.1)
				.await
			{
				Ok(public_key) => public_key?,
				Err(error) => {
					warn!(reason = %error, "stored product subtree read failed");
					return None;
				},
			};
		// A stored key still has to belong to the live pairing before it is
		// served, exactly as a freshly fetched one does.
		self.cache_product_subtree_if_current(session, lifecycle_epoch, cache_key, public_key)
			.then_some(public_key)
	}

	/// Persist and memory-cache a freshly fetched product subtree public key.
	///
	/// Persistence is an optimisation: a storage failure still serves the key
	/// and leaves the next launch to re-ask the wallet, which is what happens
	/// today. Losing the pairing mid-write is not, so the storage entry is
	/// rolled back rather than left addressing a session that has gone.
	pub async fn persist_product_subtree_if_current(
		&self,
		session: &SessionInfo,
		lifecycle_epoch: u64,
		cache_key: (SsoSessionKey, String),
		public_key: [u8; 32],
	) -> bool {
		let product_id = cache_key.1.clone();
		let _storage_guard = self.session_secret_storage.lock().await;
		if !self.session_secret_allocation_is_current(session, lifecycle_epoch) {
			return false;
		}
		let persisted = match product_subtree::write_product_subtree(
			&*self.platform,
			session,
			&product_id,
			public_key,
		)
		.await
		{
			Ok(()) => true,
			Err(error) => {
				warn!(reason = %error, "product subtree persist failed");
				false
			},
		};
		if self.cache_product_subtree_if_current(session, lifecycle_epoch, cache_key, public_key) {
			return true;
		}
		if persisted {
			let _ = product_subtree::remove_product_subtree(&*self.platform, session, &product_id)
				.await;
		}
		false
	}

	/// Cache a public subtree only for the selected live session.
	pub fn cache_product_subtree_if_current(
		&self,
		session: &SessionInfo,
		lifecycle_epoch: u64,
		cache_key: (SsoSessionKey, String),
		public_key: [u8; 32],
	) -> bool {
		let lifecycle = self.session_lifecycle.lock().expect("session lifecycle mutex poisoned");
		if lifecycle.epoch != lifecycle_epoch {
			return false;
		}
		let Some(sso) = session.sso.as_ref() else {
			return false;
		};
		if !self.current_sso_session_matches(SsoSessionKey::from_session(sso)) {
			return false;
		}
		self.product_subtrees
			.lock()
			.expect("product subtree cache mutex poisoned")
			.insert(cache_key, public_key);
		true
	}

	/// Drop memory-cached statement-store allowance keys, scoped to `session`
	/// when given, otherwise all.
	pub fn clear_statement_store_allowance_keys(&self, session: Option<&SessionInfo>) {
		let mut allowances = self
			.statement_store_allowances
			.lock()
			.expect("statement-store allowance cache mutex poisoned");
		let Some(session) = session else {
			allowances.clear();
			return;
		};
		let Some(sso) = session.sso.as_ref() else {
			return;
		};
		let session_key = SsoSessionKey::from_session(sso);
		allowances.retain(|key, _| !key.is_for_session(session_key));
	}

	/// Drop memory-cached Bulletin allowance keys, scoped to `session` when
	/// given, otherwise all.
	pub fn clear_bulletin_allowance_keys(&self, session: Option<&SessionInfo>) {
		let mut allowances = self
			.bulletin_allowances
			.lock()
			.expect("bulletin allowance cache mutex poisoned");
		let Some(session) = session else {
			allowances.clear();
			return;
		};
		let Some(sso) = session.sso.as_ref() else {
			return;
		};
		let session_key = SsoSessionKey::from_session(sso);
		allowances.retain(|key, _| !key.is_for_session(session_key));
	}

	async fn remember_auto_signing_key(
		&self,
		session: &SessionInfo,
		lifecycle_epoch: u64,
		product_id: &str,
		expected_product_subtree_public_key: [u8; 32],
		secret: [u8; 64],
		ring_vrf_domain_entropy: [u8; 32],
	) -> Result<(), AuthorityError> {
		let key = validate_auto_signing_key(
			secret,
			expected_product_subtree_public_key,
			ring_vrf_domain_entropy,
		)?;
		let owner = AutoSigningOwner::from_session(session);
		let cache_key = (owner.clone(), product_id.to_string());
		let _storage_guard = self.session_secret_storage.lock().await;
		if !self.session_secret_allocation_is_current(session, lifecycle_epoch) {
			return Err(AuthorityError::Disconnected);
		}
		self.clear_legacy_auto_signing_key(product_id).await?;
		let mut keys =
			match self.platform.read_core_storage(CoreStorageKey::AutoSigningKeys).await.map_err(
				|err| AuthorityError::Unknown {
					reason: format!("failed to read AutoSigning capabilities: {}", err.reason),
				},
			)? {
				Some(mut blob) => {
					let decoded = decode_auto_signing_keys(&blob).unwrap_or_default();
					blob.zeroize();
					decoded
				},
				None => Vec::new(),
			};
		keys.retain(|persisted| persisted.owner == owner && persisted.product_id != product_id);
		keys.push(PersistedAutoSigningKey {
			owner,
			product_id: product_id.to_string(),
			expected_product_subtree_public_key,
			secret,
			ring_vrf_domain_entropy,
		});
		if !self.session_secret_allocation_is_current(session, lifecycle_epoch) {
			return Err(AuthorityError::Disconnected);
		}
		self.platform
			.write_core_storage(CoreStorageKey::AutoSigningKeys, keys.encode())
			.await
			.map_err(|err| AuthorityError::Unknown {
				reason: format!("failed to persist AutoSigning capability: {}", err.reason),
			})?;
		if !self.cache_auto_signing_key_if_current(session, lifecycle_epoch, cache_key, key) {
			let _ = self.clear_auto_signing_product_under_storage_guard(product_id).await;
			return Err(AuthorityError::Disconnected);
		}
		Ok(())
	}

	async fn auto_signing_key(
		&self,
		session: &SessionInfo,
		product_id: &str,
	) -> Result<Option<AutoSigningKey>, AuthorityError> {
		let owner = AutoSigningOwner::from_session(session);
		let cache_key = (owner.clone(), product_id.to_string());
		if let Some(key) = self
			.auto_signing_keys
			.lock()
			.expect("AutoSigning key cache mutex poisoned")
			.get(&cache_key)
			.cloned()
		{
			return Ok(Some(key));
		}

		let _storage_guard = self.session_secret_storage.lock().await;
		if let Some(key) = self
			.auto_signing_keys
			.lock()
			.expect("AutoSigning key cache mutex poisoned")
			.get(&cache_key)
			.cloned()
		{
			return Ok(Some(key));
		}
		let legacy_present = self.clear_legacy_auto_signing_key(product_id).await?;
		let Some(mut blob) =
			self.platform.read_core_storage(CoreStorageKey::AutoSigningKeys).await.map_err(
				|err| AuthorityError::Unknown {
					reason: format!("failed to read AutoSigning capabilities: {}", err.reason),
				},
			)?
		else {
			return if legacy_present {
				Err(AuthorityError::Unavailable {
					reason: "legacy unscoped AutoSigning capability was rejected".to_string(),
				})
			} else {
				Ok(None)
			};
		};
		let decoded = decode_auto_signing_keys(&blob);
		blob.zeroize();
		let keys = match decoded {
			Ok(keys) => keys,
			Err(err) => {
				self.auto_signing_keys
					.lock()
					.expect("AutoSigning key cache mutex poisoned")
					.clear();
				let _ = self.platform.clear_core_storage(CoreStorageKey::AutoSigningKeys).await;
				return Err(err);
			},
		};
		if keys.iter().any(|persisted| persisted.owner != owner) {
			self.auto_signing_keys
				.lock()
				.expect("AutoSigning key cache mutex poisoned")
				.clear();
			let _ = self.platform.clear_core_storage(CoreStorageKey::AutoSigningKeys).await;
			return Ok(None);
		}
		let Some(persisted) = keys.iter().find(|persisted| persisted.product_id == product_id)
		else {
			return if legacy_present {
				Err(AuthorityError::Unavailable {
					reason: "legacy unscoped AutoSigning capability was rejected".to_string(),
				})
			} else {
				Ok(None)
			};
		};
		let current_expected_subtree = session.sso.as_ref().and_then(|sso| {
			self.product_subtrees
				.lock()
				.expect("product subtree cache mutex poisoned")
				.get(&(SsoSessionKey::from_session(sso), product_id.to_string()))
				.copied()
		});
		if current_expected_subtree
			.is_some_and(|expected| expected != persisted.expected_product_subtree_public_key)
		{
			self.auto_signing_keys
				.lock()
				.expect("AutoSigning key cache mutex poisoned")
				.clear();
			let _ = self.platform.clear_core_storage(CoreStorageKey::AutoSigningKeys).await;
			return Err(AuthorityError::Unavailable {
				reason: "AutoSigning capability is not for the current product subtree".to_string(),
			});
		}
		let key = match validate_auto_signing_key(
			persisted.secret,
			persisted.expected_product_subtree_public_key,
			persisted.ring_vrf_domain_entropy,
		) {
			Ok(key) => key,
			Err(err) => {
				self.auto_signing_keys
					.lock()
					.expect("AutoSigning key cache mutex poisoned")
					.clear();
				let _ = self.platform.clear_core_storage(CoreStorageKey::AutoSigningKeys).await;
				return Err(err);
			},
		};
		self.auto_signing_keys
			.lock()
			.expect("AutoSigning key cache mutex poisoned")
			.insert(cache_key, key.clone());
		Ok(Some(key))
	}

	fn clear_product_subtrees(&self, session: Option<&SessionInfo>) {
		let mut subtrees =
			self.product_subtrees.lock().expect("product subtree cache mutex poisoned");
		let Some(session) = session else {
			subtrees.clear();
			return;
		};
		let Some(sso) = session.sso.as_ref() else {
			return;
		};
		let session_key = SsoSessionKey::from_session(sso);
		subtrees.retain(|(key, _), _| *key != session_key);
	}
}

impl HostGrantGuard<'_> {
	fn advance(&mut self) -> u64 {
		self.epoch = self.epoch.checked_add(1).expect("session lifecycle epoch exhausted");
		self.external_session_active = false;
		self.epoch
	}
}

impl HostGrantPersistence<'_> {
	async fn clear_auto_signing_keys(&self) -> Result<(), String> {
		let _storage_guard = self.session_secret_storage.lock().await;
		self.clear_auto_signing_keys_under_storage_guard().await
	}

	async fn clear_auto_signing_keys_for_other_owner(
		&self,
		session: &SessionInfo,
	) -> Result<(), String> {
		let owner = AutoSigningOwner::from_session(session);
		let _storage_guard = self.session_secret_storage.lock().await;
		let Some(mut blob) = self
			.platform
			.read_core_storage(CoreStorageKey::AutoSigningKeys)
			.await
			.map_err(|err| err.reason)?
		else {
			return Ok(());
		};
		let decoded = decode_auto_signing_keys(&blob);
		blob.zeroize();
		let should_clear = decoded
			.as_ref()
			.map(|keys| keys.iter().any(|key| key.owner != owner))
			.unwrap_or(true);
		if !should_clear {
			return Ok(());
		}
		self.auto_signing_keys
			.lock()
			.expect("AutoSigning key cache mutex poisoned")
			.clear();
		self.platform
			.clear_core_storage(CoreStorageKey::AutoSigningKeys)
			.await
			.map_err(|err| err.reason)
	}

	async fn clear_legacy_auto_signing_key(
		&self,
		product_id: &str,
	) -> Result<bool, AuthorityError> {
		let storage_key = CoreStorageKey::AutoSigningKey { product_id: product_id.to_string() };
		let legacy = self.platform.read_core_storage(storage_key.clone()).await.map_err(|err| {
			AuthorityError::Unknown {
				reason: format!("failed to inspect legacy AutoSigning key: {}", err.reason),
			}
		})?;
		let present = if let Some(mut secret) = legacy {
			secret.zeroize();
			true
		} else {
			false
		};
		if present {
			self.platform.clear_core_storage(storage_key).await.map_err(|err| {
				AuthorityError::Unknown {
					reason: format!("failed to clear legacy AutoSigning key: {}", err.reason),
				}
			})?;
		}
		Ok(present)
	}
}
