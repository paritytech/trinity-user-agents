mod sso_channel;

struct LoginInFlight {
	waiters: Vec<oneshot::Sender<Result<(), String>>>,
}

struct LoginInFlightOwner<'a> {
	host: &'a PairingHost,
	active: bool,
}

impl<'a> LoginInFlightOwner<'a> {
	fn new(host: &'a PairingHost) -> Self {
		Self { host, active: true }
	}

	fn finish(&mut self, result: Result<(), String>) {
		if self.active {
			self.active = false;
			self.host.finish_login_in_flight(result);
		}
	}
}

impl Drop for LoginInFlightOwner<'_> {
	fn drop(&mut self) {
		if self.active {
			self.host.finish_login_in_flight(Err("login request aborted".to_string()));
		}
	}
}

#[derive(Debug, derive_more::Display)]
enum StoredSessionActivationError {
	#[display("stored auth session is absent")]
	Missing,
	#[display("invalid stored auth session: {_0}")]
	Invalid(String),
	#[display("failed to read stored auth session: {_0}")]
	Read(String),
	#[display("stored auth session changed during activation")]
	Changed,
}

/// State carried across the reconciles of one session store sync task.
#[derive(Default)]
struct SessionStoreSync {
	// Storage clears can notify this subscription; clear at most once per read-error streak.
	cleared_after_read_error: bool,
}

impl SessionStoreSync {
	/// Re-read the persisted auth session and reconcile the in-memory one.
	async fn reconcile(&mut self, pairing_host: &PairingHost) {
		self.cleared_after_read_error = matches!(
			pairing_host
				.reconcile_stored_session(!self.cleared_after_read_error, true)
				.await,
			Err(StoredSessionActivationError::Read(_))
		);
	}
}

impl SsoRequestService {
	/// Signal that the persisted auth session may have changed; the sync task
	/// re-reads it.
	pub fn notify_session_store_changed(&self) {
		self.advance_session_lifecycle();
		self.session_store_changes.notify();
	}

	fn advance_session_lifecycle(&self) -> u64 {
		let mut lifecycle =
			self.session_lifecycle.lock().expect("session lifecycle mutex poisoned");
		lifecycle.advance()
	}

	fn current_session(&self) -> Option<AuthoritySession> {
		self.session_state.current().as_ref().map(authority_session)
	}

	/// Validate, resolve, and install an externally persisted canonical
	/// session blob without copying it into core storage. Reports the resulting
	/// auth state to the host, including when the blob failed to decode and the
	/// active session was therefore left alone.
	pub async fn activate_external_session(&self, blob: &[u8]) -> Result<(), String> {
		let installed = self.install_external_session(blob).await;
		self.auth_state.announce_current();
		installed
	}

	async fn install_external_session(&self, blob: &[u8]) -> Result<(), String> {
		let _activation = self.session_store_activation.lock().await;
		let session = crate::host_logic::session::decode_persisted_session(blob)?;
		let activation_epoch = self.advance_session_lifecycle();
		let resolved = resolve_session_identity_with_chain(
			&self.chain,
			self.host_config.asset_hub_chain_genesis_hash,
			session,
		)
		.await;
		#[cfg(test)]
		self.wait_at_external_session_activation_pause().await;
		self.set_connected_session_if_current(resolved, activation_epoch, true).await;
		Ok(())
	}

	/// Restore the persisted session and report the resulting connection state.
	pub async fn activate_stored_session(&self) -> Result<(), String> {
		let restored = self
			.reconcile_stored_session(true, false)
			.await
			.map_err(|error| error.to_string());
		self.auth_state.announce_current();
		restored
	}

	async fn reconcile_stored_session(
		&self,
		clear_after_read_error: bool,
		preserve_external_session: bool,
	) -> Result<(), StoredSessionActivationError> {
		let _activation = self.session_store_activation.lock().await;
		if preserve_external_session &&
			self.session_lifecycle
				.lock()
				.expect("session lifecycle mutex poisoned")
				.external_session_active
		{
			return Ok(());
		}
		let blob = match self.platform.read_core_storage(CoreStorageKey::AuthSession).await {
			Ok(Some(blob)) => blob,
			Ok(None) => {
				self.clear_disconnected_session(false).await;
				return Err(StoredSessionActivationError::Missing);
			},
			Err(error) => {
				self.clear_disconnected_session(false).await;
				if clear_after_read_error {
					let _ = self.platform.clear_core_storage(CoreStorageKey::AuthSession).await;
				}
				return Err(StoredSessionActivationError::Read(error.reason));
			},
		};
		let session = match crate::host_logic::session::decode_persisted_session(&blob) {
			Ok(session) => session,
			Err(error) => {
				self.clear_disconnected_session(true).await;
				return Err(StoredSessionActivationError::Invalid(error));
			},
		};
		let resolved = resolve_session_identity_with_chain(
			&self.chain,
			self.host_config.asset_hub_chain_genesis_hash,
			session,
		)
		.await;

		// Identity resolution can await chain I/O. Re-read the slot before
		// installation so an older activation cannot overwrite or expose a
		// session replaced while that lookup was in flight.
		let latest = match self.platform.read_core_storage(CoreStorageKey::AuthSession).await {
			Ok(latest) => latest,
			Err(error) => {
				self.clear_disconnected_session(false).await;
				if clear_after_read_error {
					let _ = self.platform.clear_core_storage(CoreStorageKey::AuthSession).await;
				}
				return Err(StoredSessionActivationError::Read(error.reason));
			},
		};
		if latest.as_deref() != Some(blob.as_slice()) {
			self.clear_disconnected_session(false).await;
			return Err(StoredSessionActivationError::Changed);
		}

		let resolved_blob = encode_persisted_session(&resolved);
		if resolved_blob != blob {
			let _ = self
				.platform
				.write_core_storage(CoreStorageKey::AuthSession, resolved_blob)
				.await;
		}
		self.set_connected_session(resolved).await;
		Ok(())
	}

	/// Reconcile at boot and after storage notifications, reporting auth state.
	#[instrument(skip_all, fields(runtime.method = "session_store.sync"))]
	pub fn start_session_store_sync(self: Arc<Self>, spawner: Spawner) {
		let pairing_host = Arc::downgrade(&self);
		drop(self);
		spawner(Box::pin(async move {
			let Some(booting) = pairing_host.upgrade() else {
				return;
			};
			let mut ticks = booting.session_store_changes.subscribe();
			let mut sync = SessionStoreSync::default();
			sync.reconcile(&booting).await;
			booting.auth_state.announce_current();
			drop(booting);
			while ticks.next().await.is_some() {
				let Some(pairing_host) = pairing_host.upgrade() else {
					break;
				};
				sync.reconcile(&pairing_host).await;
				#[cfg(test)]
				pairing_host.session_store_change_ticks.fetch_add(1, Ordering::SeqCst);
			}
		}));
	}

	/// Clear the canonical local session and all session capabilities without
	/// sending a peer-disconnect statement. Reports the resulting auth state to
	/// the host, including when there was no session to clear.
	pub async fn reset_session_state(&self) {
		self.cancel_login();
		self.clear_disconnected_session(true).await;
		let _storage_guard = self.session_secret_storage.lock().await;
		self.clear_statement_store_allowance_keys(None);
		self.clear_bulletin_allowance_keys(None);
		self.clear_product_subtrees(None);
		self.auth_state.announce_current();
	}

	/// Invalidate in-flight login attempts and emit the cancelled auth state.
	#[instrument(skip_all, fields(runtime.method = "account.cancel_login"))]
	pub fn cancel_login(&self) {
		self.invalidate_login_attempts();
		self.auth_state.login_cancelled();
	}

	fn begin_login_attempt(&self) -> u64 {
		let mut generation = self.login_generation.lock().expect("login generation mutex poisoned");
		*generation = generation.wrapping_add(1);
		*generation
	}

	fn invalidate_login_attempts(&self) {
		let mut generation = self.login_generation.lock().expect("login generation mutex poisoned");
		*generation = generation.wrapping_add(1);
	}

	fn is_current_login_attempt(&self, generation: u64) -> bool {
		*self.login_generation.lock().expect("login generation mutex poisoned") == generation
	}

	fn login_waiter(&self) -> Option<oneshot::Receiver<Result<(), String>>> {
		let mut in_flight = self.login_in_flight.lock().expect("login in-flight mutex poisoned");
		if let Some(in_flight) = in_flight.as_mut() {
			let (tx, rx) = oneshot::channel();
			in_flight.waiters.push(tx);
			Some(rx)
		} else {
			*in_flight = Some(LoginInFlight { waiters: Vec::new() });
			None
		}
	}

	fn finish_login_in_flight(&self, result: Result<(), String>) {
		let waiters = self
			.login_in_flight
			.lock()
			.expect("login in-flight mutex poisoned")
			.take()
			.map(|in_flight| in_flight.waiters)
			.unwrap_or_default();
		for waiter in waiters {
			let _ = waiter.send(result.clone());
		}
	}

	#[instrument(skip_all, fields(runtime.method = "session_store.clear_disconnected"))]
	async fn clear_disconnected_session(&self, clear_auth_session: bool) {
		let previous = {
			let mut lifecycle =
				self.session_lifecycle.lock().expect("session lifecycle mutex poisoned");
			lifecycle.advance();
			let previous = self.session_state.current();
			self.session_state.clear_session();
			previous
		};
		self.stop_session_channel(previous.as_ref());
		if clear_auth_session {
			let _ = self.platform.clear_core_storage(CoreStorageKey::AuthSession).await;
		}
		let _storage_guard = self.session_secret_storage.lock().await;
		if let Some(session) = previous.as_ref() {
			self.clear_statement_store_allowance_keys(Some(session));
			self.clear_bulletin_allowance_keys(Some(session));
			if let Err(reason) =
				allowances::clear_session_allowance_keys(&*self.platform, session).await
			{
				warn!(%reason, "allowance capability clear failed during disconnect");
			}
			self.clear_stored_product_subtrees(session).await;
		}
		self.clear_product_subtrees(previous.as_ref());
		if let Err(reason) = self.clear_auto_signing_keys_under_storage_guard().await {
			warn!(%reason, "AutoSigning capability clear failed during disconnect");
		}
		self.auth_state.store_disconnected();
	}

	/// Single funnel for peer-initiated disconnects. Every detection source
	/// (monitor task, request-path error) must route here: it wakes in-flight
	/// waiters for `key`, then clears the session when `key` is still current,
	/// so stale notifications for replaced sessions only wake their own
	/// waiters.
	async fn handle_signing_host_disconnected(&self, key: SsoSessionKey) {
		self.session_disconnects.notify_key(key, SSO_PEER_DISCONNECT_REASON);
		if !self.current_sso_session_matches(key) {
			return;
		}

		self.clear_disconnected_session(true).await;
	}

	fn current_sso_session_matches(&self, key: SsoSessionKey) -> bool {
		sso_channel::session_matches_key(&self.session_state, key)
	}

	async fn refresh_current_session_identity(&self) -> Option<AuthoritySession> {
		let current = self.session_state.current()?;
		if current.has_username() || self.host_config.asset_hub_chain_genesis_hash == [0; 32] {
			return Some(authority_session(&current));
		}

		let resolved = resolve_session_identity_with_chain(
			&self.chain,
			self.host_config.asset_hub_chain_genesis_hash,
			current.clone(),
		)
		.await;
		if !resolved.has_username() || resolved == current {
			return self.current_session();
		}

		if !self.session_state.replace_session_if_current(&current, resolved.clone()) {
			return self.current_session();
		}
		self.auth_state.connected(&connected_session_ui_info(&resolved));

		if let Err(err) = self
			.platform
			.write_core_storage(CoreStorageKey::AuthSession, encode_persisted_session(&resolved))
			.await
		{
			warn!(reason = %err.reason, "refreshed session identity persist failed");
		}

		match self.session_state.current() {
			Some(live) if live != resolved => {
				if let Err(err) = self
					.platform
					.write_core_storage(
						CoreStorageKey::AuthSession,
						encode_persisted_session(&live),
					)
					.await
				{
					warn!(reason = %err.reason, "live session identity persist repair failed");
				}
				Some(authority_session(&live))
			},
			None => {
				if let Err(err) =
					self.platform.clear_core_storage(CoreStorageKey::AuthSession).await
				{
					warn!(reason = %err.reason, "cleared session identity persist repair failed");
				}
				None
			},
			_ => Some(authority_session(&resolved)),
		}
	}
}

#[crate::platform::async_trait]
impl HostSession for SsoRequestService {
	/// Shared session holder for connection-status subscriptions.
	pub fn session_state(&self) -> Arc<SessionState> {
		self.session_state.clone()
	}

	#[instrument(skip_all, fields(runtime.method = "account.request_login", product = %product.product_id))]
	async fn request_login(
		&self,
		product: &ProductContext,
	) -> Result<HostRequestLoginResponse, CallError<HostRequestLoginError>> {
		let _ = product;
		if let Some(session) = self.session_state.current() {
			self.auth_state.connected(&connected_session_ui_info(&session));
			return Ok(HostRequestLoginResponse::V1(
				latest::HostRequestLoginResponse::AlreadyConnected,
			));
		}

		if let Some(waiter) = self.login_waiter() {
			match waiter.await {
				Ok(Ok(())) => {
					return Ok(HostRequestLoginResponse::V1(
						if self.session_state.current().is_some() {
							latest::HostRequestLoginResponse::AlreadyConnected
						} else {
							latest::HostRequestLoginResponse::Rejected
						},
					));
				},
				Ok(Err(reason)) => {
					return Err(CallError::Domain(HostRequestLoginError::V1(
						latest::HostRequestLoginError::Unknown { reason },
					)));
				},
				Err(_) => {
					return Err(CallError::Domain(HostRequestLoginError::V1(
						latest::HostRequestLoginError::Unknown {
							reason: "login waiter dropped".to_string(),
						},
					)));
				},
			}
		}

		let mut login_owner = LoginInFlightOwner::new(self);
		let login_generation = self.begin_login_attempt();
		let outcome = match SsoPairingFlow::new(self).request_session().await {
			Ok(outcome) => outcome,
			Err(err) => {
				login_owner.finish(Err(login_error_reason(&err)));
				return Err(err);
			},
		};
		match outcome {
			SsoPairingOutcome::Cancelled => {
				login_owner.finish(Ok(()));
				if self.session_state.current().is_some() {
					Ok(HostRequestLoginResponse::V1(
						latest::HostRequestLoginResponse::AlreadyConnected,
					))
				} else {
					Ok(HostRequestLoginResponse::V1(latest::HostRequestLoginResponse::Rejected))
				}
			},
			SsoPairingOutcome::Success(session) => {
				if !self.is_current_login_attempt(login_generation) {
					let _ = self.platform.clear_core_storage(CoreStorageKey::AuthSession).await;
					login_owner.finish(Ok(()));
					return Ok(HostRequestLoginResponse::V1(
						latest::HostRequestLoginResponse::Rejected,
					));
				}
				self.set_connected_session(*session).await;
				login_owner.finish(Ok(()));
				Ok(HostRequestLoginResponse::V1(latest::HostRequestLoginResponse::Success))
			},
		}
	}

	#[instrument(skip_all, fields(runtime.method = "account.disconnect"))]
	async fn disconnect(&self) {
		self.cancel_login();
		let session = self.session_state.current();
		self.clear_disconnected_session(true).await;
		if let Some(session) = session {
			let weak_self = self.weak_self.clone();
			(self.spawner)(Box::pin(async move {
				if let Some(host) = weak_self.upgrade() {
					let _ = host.submit_disconnected_message(&session).await;
				}
			}));
		}
	}
}

fn login_error_reason(err: &CallError<HostRequestLoginError>) -> String {
	match err {
		CallError::Domain(HostRequestLoginError::V1(latest::HostRequestLoginError::Unknown {
			reason,
		})) |
		CallError::HostFailure { reason } => reason.clone(),
		CallError::Unsupported => "login unsupported".to_string(),
		CallError::Denied => "login denied".to_string(),
		CallError::MalformedFrame { reason } => reason.clone(),
		CallError::Cancelled => "login cancelled".to_string(),
	}
}
