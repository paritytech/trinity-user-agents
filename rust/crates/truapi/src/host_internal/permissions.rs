//! Permission authorization state machine (ask -> authorized | denied), backed
//! by the platform [`CoreStorage`] trait with typed [`CoreStorageKey`] slots.
//!
//! Device permissions (camera, mic, NFC, ...) are separate from remote
//! permissions (domain access, chain submit, ...), so this module exposes two
//! `check_or_prompt` entrypoints that route to the matching platform callback.
//! The cache layer is shared but keys are typed so a device grant cannot
//! authorize a remote operation by accident. Keys are also scoped by product id
//! so one product's authorization never grants another product's request.
//! Identity disclosure is also represented as a product-scoped authorization;
//! its richer user-confirmation prompt is coordinated here so local and remote
//! account-authority paths share one decision state machine.
//!
//! Domain grants (`RemotePermission::Remote`) are the one request that does not
//! occupy a single slot. A product may ask for several domains at once, while
//! outbound-request authorization checks one host at a time. So a grant is
//! stored as one authorization per domain pattern, and a lookup for a concrete
//! host resolves through the
//! RFC 0002 candidate list ([`remote_domain_candidates`]), letting the most
//! specific stored decision win.
//!
//! A denial is not symmetric with that. "No" to a set of domains is not "no" to
//! each of them — the user was never asked the narrower question — so a
//! multi-domain denial is recorded against the exact set that was asked about
//! instead of fanning out. The bundle stays answered, so the same request does
//! not re-prompt, while a later request naming one of those domains on its own
//! still gets a prompt. A one-domain prompt has no narrower question left, and
//! its set-shaped key is that domain's key, so it persists per-domain with no
//! special case.
//!
//! Remote permissions, identity disclosure and account access have one exception.
//! A product listed in [`crate::platform::REMOTE_PERMISSION_TRUSTED_LABELS`]
//! is authorized without a prompt when no explicit denial is stored. An
//! administrative denial overrides that default. Device permissions, Chat authority
//! and profile disclosure are never covered.

use std::{
	collections::{HashMap, HashSet},
	sync::{Arc, Weak},
};

use crate::platform::{
	BLESSED_REMOTE_DOMAINS, ChatAuthorityReview, CoreStorage, CoreStorageKey,
	CrossProductSignatureKind, DevicePermissionStatus, IdentityDisclosureReview,
	PermissionAuthorizationRequest, PermissionAuthorizationStatus, PermissionDecision,
	PermissionStatusHost, Permissions, ProductContext, ProductId, ProfileDisclosureReview,
	UserConfirmation, UserConfirmationReview, is_valid_remote_domain_pattern,
	normalize_remote_domain, remote_domain_candidates,
};
use parity_scale_codec::{Decode, Encode};
use truapi::latest::{
	GenericError, HostDevicePermissionRequest, RemotePermission, RemotePermissionRequest,
};

/// Persisted answer for a single permission request. Keep `Authorized` at
/// discriminant 0 and `Denied` at 1 to preserve the existing two-variant cache
/// encoding.
#[derive(Clone, PartialEq, Eq, Encode, Decode)]
enum StoredAuthorizationStatus {
	/// User authorized the permission.
	Authorized,
	/// User denied the permission.
	Denied,
	/// Explicit reset: ask again, but never re-import an obsolete legacy grant.
	NotDetermined,
}

impl From<StoredAuthorizationStatus> for PermissionAuthorizationStatus {
	fn from(status: StoredAuthorizationStatus) -> Self {
		match status {
			StoredAuthorizationStatus::Authorized => PermissionAuthorizationStatus::Authorized,
			StoredAuthorizationStatus::Denied => PermissionAuthorizationStatus::Denied,
			StoredAuthorizationStatus::NotDetermined => {
				PermissionAuthorizationStatus::NotDetermined
			},
		}
	}
}

/// Domain patterns a remote request covers, or `None` when the request is not a
/// domain grant and so occupies a single slot of its own.
fn requested_domains(request: &RemotePermissionRequest) -> Option<&[String]> {
	match &request.permission {
		RemotePermission::Remote { domains } => Some(domains),
		_ => None,
	}
}

/// A domain bundle as a request, for the key that answers it as a whole.
fn remote_bundle_request(domains: &[String]) -> RemotePermissionRequest {
	RemotePermissionRequest { permission: RemotePermission::Remote { domains: domains.to_vec() } }
}

/// What the stored per-domain decisions alone say about a bundle.
enum BundleResolution {
	/// Every domain in the bundle has a stored grant.
	Authorized,
	/// At least one domain has a stored denial, which denies the bundle: the
	/// product asked to reach all of them.
	Denied,
	/// Nothing is denied, and these domains have no decision of their own —
	/// exactly the set a prompt would put to the user.
	Undecided(Vec<String>),
}

/// Scope an approval like the manifest grant, across the product's executables.
pub fn cross_product_signature_key(
	product_id: &ProductId,
	target_product_id: &ProductId,
	kind: CrossProductSignatureKind,
) -> CoreStorageKey {
	CoreStorageKey::product_signature_authorization(product_id, target_product_id, kind)
}

/// How a product's Chat authority is granted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatAuthorityConsent {
	/// A stored user decision authorizes Chat.
	Persisted,
	/// The user allowed Chat for this session only; nothing is stored.
	Session,
	/// Chat is not authorized, or the prompt was dismissed.
	Refused,
}

/// Runtime-owned serialization and cancellation for permission decisions.
/// Prompts never hold the mutation lock; administration can cancel them while
/// serializing its check/write with every decision that might otherwise win late.
#[derive(Default)]
pub struct PermissionAuthority {
	scopes: parking_lot::Mutex<HashMap<String, Arc<PermissionScope>>>,
	mutation: Arc<futures::lock::Mutex<()>>,
}

impl PermissionAuthority {
	pub(crate) fn scope(&self, product: &str) -> Arc<PermissionScope> {
		self.scopes
			.lock()
			.entry(product.to_owned())
			.or_insert_with(|| {
				Arc::new(PermissionScope {
					mutation: self.mutation.clone(),
					state: Default::default(),
				})
			})
			.clone()
	}

	/// Withdraw pending decisions while serializing the caller's authority change.
	///
	/// Keep the returned guard until the account or product has been invalidated.
	/// Completed, stored decisions are unchanged; later prompt commits must still
	/// match their scope revision.
	pub(crate) async fn invalidate_pending(
		&self,
		product: Option<&ProductId>,
	) -> futures::lock::MutexGuard<'_, ()> {
		let mutation = self.mutation.lock().await;
		for (owner, scope) in self.scopes.lock().iter() {
			if product.is_none_or(|product| owner == product.as_str()) {
				scope.invalidate();
			}
		}
		mutation
	}

	fn invalidate(&self, product: &str, request: &PermissionAuthorizationRequest) {
		let account_access =
			matches!(request, PermissionAuthorizationRequest::AccountAccess { .. });
		for (owner, scope) in self.scopes.lock().iter() {
			if owner == product ||
				(account_access &&
					permission_product_label(owner) == permission_product_label(product))
			{
				scope.invalidate();
			}
		}
	}
}

#[derive(Default)]
pub(crate) struct PermissionScope {
	pub(crate) mutation: Arc<futures::lock::Mutex<()>>,
	state: parking_lot::Mutex<PermissionScopeState>,
}

#[derive(Default)]
struct PermissionScopeState {
	revision: u64,
	prompts: Vec<Weak<futures::future::AbortHandle>>,
	temporary: Vec<Weak<TemporaryPermissions>>,
}

impl PermissionScope {
	pub(crate) fn revision(&self) -> u64 {
		self.state.lock().revision
	}

	pub(crate) fn require_revision(&self, revision: u64) -> Result<(), GenericError> {
		if self.revision() == revision {
			Ok(())
		} else {
			Err(GenericError { reason: "permission decision was superseded".into() })
		}
	}

	fn register(&self, permissions: &Arc<TemporaryPermissions>) {
		let mut state = self.state.lock();
		state.temporary.retain(|entry| entry.strong_count() != 0);
		if !state.temporary.iter().any(|entry| entry.as_ptr() == Arc::as_ptr(permissions)) {
			state.temporary.push(Arc::downgrade(permissions));
		}
	}

	/// Called under `mutation`, before a host administrative write.
	pub(crate) fn invalidate(&self) {
		let mut state = self.state.lock();
		state.revision = state.revision.checked_add(1).expect("permission revision exhausted");
		for prompt in state.prompts.drain(..).filter_map(|entry| entry.upgrade()) {
			prompt.abort();
		}
		state.temporary.retain(|entry| {
			if let Some(permissions) = entry.upgrade() {
				permissions.clear();
				true
			} else {
				false
			}
		});
	}

	pub(crate) async fn prompt<T>(
		&self,
		revision: u64,
		future: impl Future<Output = Result<T, GenericError>>,
	) -> Result<T, GenericError> {
		let (handle, registration) = futures::future::AbortHandle::new_pair();
		let handle = Arc::new(handle);
		{
			let mut state = self.state.lock();
			if state.revision != revision {
				return Err(GenericError { reason: "permission decision was superseded".into() });
			}
			state.prompts.retain(|entry| entry.strong_count() != 0);
			state.prompts.push(Arc::downgrade(&handle));
		}
		futures::future::Abortable::new(future, registration)
			.await
			.map_err(|_| GenericError { reason: "permission decision was superseded".into() })?
	}
}

/// Permission prompts and one-use grants shared by a product execution's connections.
#[derive(Default)]
pub struct TemporaryPermissions {
	authorization: futures::lock::Mutex<()>,
	grants: parking_lot::Mutex<HashSet<Vec<u8>>>,
	signature_prompt: futures::lock::Mutex<()>,
}

impl TemporaryPermissions {
	pub fn clear(&self) {
		self.grants.lock().clear();
	}

	/// Wait until no signature prompt is open, and keep others from opening
	/// until the returned guard is dropped.
	pub async fn signature_prompt(&self) -> futures::lock::MutexGuard<'_, ()> {
		self.signature_prompt.lock().await
	}

	/// Whether this execution holds a one-use grant for the key.
	pub fn authorize(&self, key: &CoreStorageKey, consume: bool) -> bool {
		self.authorize_all(core::slice::from_ref(key), consume)
	}

	fn authorize_all(&self, keys: &[CoreStorageKey], consume: bool) -> bool {
		let keys: Vec<_> = keys.iter().map(Encode::encode).collect();
		let mut grants = self.grants.lock();
		if !keys.iter().all(|key| grants.contains(key)) {
			return false;
		}
		if consume {
			for key in keys {
				grants.remove(&key);
			}
		}
		true
	}

	fn revoke(&self, key: &CoreStorageKey) {
		self.grants.lock().remove(&key.encode());
	}

	pub fn grant(&self, key: CoreStorageKey) {
		self.grants.lock().insert(key.encode());
	}
}

/// Coordinates saved and one-use permissions with the platform's prompts.
pub struct PermissionsService<'a, S: CoreStorage + ?Sized, P: Permissions + ?Sized> {
	storage: &'a S,
	prompt: &'a P,
	product: &'a ProductContext,
	/// Live OS permission state, when the host serves that capability. Absent
	/// leaves the stored decision governing on its own.
	status: Option<&'a dyn PermissionStatusHost>,
	/// Whether the product receives the canonical trusted permission defaults.
	trusted_product: bool,
	/// One-use grants remain local to the execution that requested them.
	temporary_permissions: Arc<TemporaryPermissions>,
	authority: Arc<PermissionAuthority>,
}

impl<'a, S: CoreStorage + ?Sized, P: Permissions + ?Sized> PermissionsService<'a, S, P> {
	/// Construct a service backed by the given storage + prompt callbacks.
	///
	/// Device grants resolve without an OS check by default. Use
	/// [`Self::with_status_host`] on paths that enforce a device capability, so
	/// a grant the OS has since withdrawn stops reading as usable.
	pub fn new(storage: &'a S, prompt: &'a P, product: &'a ProductContext) -> Self {
		Self {
			storage,
			prompt,
			product,
			status: None,
			trusted_product: product.product_id.has_trusted_remote_permissions(),
			temporary_permissions: Arc::default(),
			authority: Arc::default(),
		}
	}

	pub fn with_temporary_permissions(mut self, permissions: Arc<TemporaryPermissions>) -> Self {
		self.temporary_permissions = permissions;
		self
	}

	pub fn with_authority(mut self, authority: Arc<PermissionAuthority>) -> Self {
		authority
			.scope(self.product_id().as_str())
			.register(&self.temporary_permissions);
		self.authority = authority;
		self
	}

	pub(crate) fn scope(&self) -> Arc<PermissionScope> {
		self.authority.scope(self.product_id().as_str())
	}

	fn product_id(&self) -> &ProductId {
		&self.product.product_id
	}

	/// Revalidate device capabilities against the OS state `status` reports.
	pub fn with_status_host(mut self, status: Option<&'a dyn PermissionStatusHost>) -> Self {
		self.status = status;
		self
	}

	/// Whether the OS currently refuses this capability to the host
	/// application, which is the only OS answer that overrides a stored
	/// decision.
	///
	/// False when the host serves no OS state, and also when the query fails: a
	/// failed query is transient — a busy host, a dropped IPC — and reading it
	/// as a refusal would let a flaky channel revoke a working capability.
	async fn os_refuses(&self, permission: HostDevicePermissionRequest) -> bool {
		let Some(status) = self.status else {
			return false;
		};
		status.device_permission_status(permission).await == Ok(DevicePermissionStatus::Denied)
	}

	/// Authorization status for a device permission, without prompting.
	///
	/// Resolves the same two gates a request does, so a host settings screen and
	/// `request_device_permission` cannot disagree. Reporting the stored grant
	/// alone would send the user looking for a product toggle when the OS is
	/// what refused. The stored decision is left untouched either way.
	pub async fn peek_device(
		&self,
		permission: &HostDevicePermissionRequest,
	) -> Result<PermissionAuthorizationStatus, GenericError> {
		if self.os_refuses(*permission).await {
			return Ok(PermissionAuthorizationStatus::Denied);
		}
		let key = CoreStorageKey::device_permission_authorization(self.product_id(), permission);
		self.cached_authorization(&key, false).await
	}

	/// Returns the current authorization status for a remote permission without
	/// prompting.
	///
	/// A domain bundle is authorized only when every domain in it is; a single
	/// denial denies the bundle, because the product asked to reach all of them.
	pub async fn peek_remote(
		&self,
		request: &RemotePermissionRequest,
	) -> Result<PermissionAuthorizationStatus, GenericError> {
		let Some(domains) = requested_domains(request) else {
			let key = CoreStorageKey::remote_permission_authorization(self.product_id(), request);
			return self.cached_remote_authorization(&key, false).await;
		};
		match self.resolve_domains(domains).await?.0 {
			BundleResolution::Authorized => Ok(PermissionAuthorizationStatus::Authorized),
			BundleResolution::Denied => Ok(PermissionAuthorizationStatus::Denied),
			// Nothing per-domain covers the rest, so the remaining question is
			// whether this exact set has already been refused.
			BundleResolution::Undecided(undecided) => {
				authorization_status(self.storage, self.bundle_key(&undecided)).await
			},
		}
	}

	/// Resolve a bundle against the per-domain decisions alone.
	async fn resolve_domains(
		&self,
		domains: &[String],
	) -> Result<(BundleResolution, Vec<CoreStorageKey>), GenericError> {
		if domains.is_empty() ||
			domains.iter().any(|domain| !is_valid_remote_domain_pattern(domain))
		{
			return Ok((BundleResolution::Denied, Vec::new()));
		}
		if self.trusted_product &&
			domains.len() > 1 &&
			peek_stored(self.storage, self.bundle_key(domains)).await? ==
				Some(StoredAuthorizationStatus::Denied)
		{
			return Ok((BundleResolution::Denied, Vec::new()));
		}
		let mut undecided = Vec::new();
		let mut temporary_keys = Vec::new();
		for domain in domains {
			let (status, temporary_key) = self.effective_domain_status(domain).await?;
			match status {
				PermissionAuthorizationStatus::Denied => {
					return Ok((BundleResolution::Denied, Vec::new()));
				},
				PermissionAuthorizationStatus::NotDetermined => undecided.push(domain.clone()),
				PermissionAuthorizationStatus::Authorized => {},
			}
			if let Some(key) = temporary_key {
				temporary_keys.push(key);
			}
		}
		if undecided.is_empty() {
			Ok((BundleResolution::Authorized, temporary_keys))
		} else {
			Ok((BundleResolution::Undecided(undecided), temporary_keys))
		}
	}

	/// Key holding the answer to a bundle taken as a whole, which is where a
	/// multi-domain denial lives. For one domain this is that domain's own key.
	fn bundle_key(&self, domains: &[String]) -> CoreStorageKey {
		CoreStorageKey::remote_permission_authorization(
			self.product_id(),
			&remote_bundle_request(domains),
		)
	}

	/// Effective decision covering one concrete host or pattern.
	///
	/// Walks [`remote_domain_candidates`] most-specific-first and returns the
	/// first stored decision, so an explicit grant for `api.example.com`
	/// survives a denial of `*.example.com` and vice versa. With no decision on
	/// any candidate, blessed domains and trusted products are authorized.
	async fn effective_domain_status(
		&self,
		domain: &str,
	) -> Result<(PermissionAuthorizationStatus, Option<CoreStorageKey>), GenericError> {
		let blessed = BLESSED_REMOTE_DOMAINS.contains(&normalize_remote_domain(domain).as_str());
		let mut temporary_keys = Vec::new();
		let mut fallback = if blessed || self.trusted_product {
			PermissionAuthorizationStatus::Authorized
		} else {
			self.default_remote_status()
		};
		for candidate in remote_domain_candidates(domain) {
			let key = CoreStorageKey::remote_domain_authorization(self.product_id(), &candidate);
			if let Some(stored) = peek_stored(self.storage, key.clone()).await? &&
				stored != StoredAuthorizationStatus::NotDetermined
			{
				fallback = stored.into();
				break;
			}
			temporary_keys.push(key);
		}
		if blessed && fallback == PermissionAuthorizationStatus::Authorized {
			return Ok((PermissionAuthorizationStatus::Authorized, None));
		}
		for key in temporary_keys {
			if self.temporary_permissions.authorize(&key, false) {
				return Ok((PermissionAuthorizationStatus::Authorized, Some(key)));
			}
		}
		Ok((fallback, None))
	}

	async fn cached_authorization(
		&self,
		key: &CoreStorageKey,
		consume: bool,
	) -> Result<PermissionAuthorizationStatus, GenericError> {
		if let Some(stored) = peek_stored(self.storage, key.clone()).await? &&
			stored != StoredAuthorizationStatus::NotDetermined
		{
			return Ok(stored.into());
		}
		Ok(if self.temporary_permissions.authorize(key, consume) {
			PermissionAuthorizationStatus::Authorized
		} else {
			PermissionAuthorizationStatus::NotDetermined
		})
	}

	async fn cached_remote_authorization(
		&self,
		key: &CoreStorageKey,
		consume: bool,
	) -> Result<PermissionAuthorizationStatus, GenericError> {
		if matches!(
			key,
			CoreStorageKey::PermissionAuthorization {
				request: crate::platform::StoredPermissionRequest::RemoteV2(
					RemotePermissionRequest { permission: RemotePermission::JamPeers { .. } }
				),
				..
			}
		) {
			return self.cached_authorization(key, consume).await;
		}
		match self.cached_authorization(key, consume).await? {
			PermissionAuthorizationStatus::NotDetermined => Ok(self.default_remote_status()),
			decided => Ok(decided),
		}
	}

	fn default_remote_status(&self) -> PermissionAuthorizationStatus {
		if self.trusted_product {
			PermissionAuthorizationStatus::Authorized
		} else {
			PermissionAuthorizationStatus::NotDetermined
		}
	}

	/// Returns the stored authorization status for a permission request
	/// without prompting.
	pub async fn authorization_status(
		&self,
		request: &PermissionAuthorizationRequest,
	) -> Result<PermissionAuthorizationStatus, GenericError> {
		let scope = self.scope();
		let _mutation = scope.mutation.lock().await;
		match request {
			PermissionAuthorizationRequest::Device(permission) => {
				self.peek_device(permission).await
			},
			PermissionAuthorizationRequest::Remote(request) => self.peek_remote(request).await,
			PermissionAuthorizationRequest::IdentityDisclosure |
			PermissionAuthorizationRequest::AccountAccess { .. } => {
				let status = authorization_status(
					self.storage,
					authorization_key(self.product_id(), request),
				)
				.await?;
				Ok(
					if self.trusted_product &&
						status == PermissionAuthorizationStatus::NotDetermined
					{
						PermissionAuthorizationStatus::Authorized
					} else {
						status
					},
				)
			},
			PermissionAuthorizationRequest::ChatAuthority => {
				authorization_status(
					self.storage,
					CoreStorageKey::chat_authority_authorization(self.product_id()),
				)
				.await
			},
			PermissionAuthorizationRequest::StatementStoreAllowance { derivation_index } => {
				authorization_status(
					self.storage,
					CoreStorageKey::statement_store_allowance_authorization(
						self.product_id(),
						derivation_index.clone(),
					),
				)
				.await
			},
			PermissionAuthorizationRequest::ProfileDisclosure => {
				authorization_status(
					self.storage,
					CoreStorageKey::profile_disclosure_authorization(self.product_id()),
				)
				.await
			},
			PermissionAuthorizationRequest::CrossProductSignature { target_product_id, kind } => {
				let approved = self.temporary_permissions.authorize(
					&cross_product_signature_key(self.product_id(), target_product_id, *kind),
					false,
				);
				Ok(if approved {
					PermissionAuthorizationStatus::Authorized
				} else {
					PermissionAuthorizationStatus::NotDetermined
				})
			},
		}
	}

	/// Returns the stored authorization statuses for permission requests
	/// without prompting. Results follow the same order as `requests`.
	pub async fn authorization_statuses(
		&self,
		requests: &[PermissionAuthorizationRequest],
	) -> Result<Vec<PermissionAuthorizationStatus>, GenericError> {
		let mut statuses = Vec::with_capacity(requests.len());
		for request in requests {
			statuses.push(self.authorization_status(request).await?);
		}
		Ok(statuses)
	}

	/// Update the stored authorization status for a permission request.
	///
	/// `NotDetermined` restores the default and retains a legacy-import tombstone.
	pub async fn set_authorization_status(
		&self,
		request: &PermissionAuthorizationRequest,
		status: PermissionAuthorizationStatus,
	) -> Result<(), GenericError> {
		let scope = self.scope();
		let _mutation = scope.mutation.lock().await;
		self.authority.invalidate(self.product_id().as_str(), request);
		self.write_authorization_status(request, status).await
	}

	/// Edit one canonical settings row without broadening a bundle denial.
	#[cfg(not(target_arch = "wasm32"))]
	pub(crate) async fn set_canonical_authorization_status(
		&self,
		request: &PermissionAuthorizationRequest,
		status: PermissionAuthorizationStatus,
	) -> Result<(), GenericError> {
		if matches!(request, PermissionAuthorizationRequest::CrossProductSignature { .. }) {
			return self.set_authorization_status(request, status).await;
		}
		let scope = self.scope();
		let _mutation = scope.mutation.lock().await;
		self.authority.invalidate(self.product_id().as_str(), request);
		set_authorization_status(
			self.storage,
			authorization_key(self.product_id(), request),
			status,
		)
		.await
	}

	#[cfg(not(target_arch = "wasm32"))]
	pub(crate) async fn commit_if_current(
		&self,
		request: &PermissionAuthorizationRequest,
		status: PermissionAuthorizationStatus,
		revision: u64,
	) -> Result<bool, GenericError> {
		let scope = self.scope();
		let _mutation = scope.mutation.lock().await;
		if scope.revision() != revision {
			return Ok(false);
		}
		if status != PermissionAuthorizationStatus::Authorized {
			self.authority.invalidate(self.product_id().as_str(), request);
		}
		if matches!(request, PermissionAuthorizationRequest::CrossProductSignature { .. }) {
			self.write_authorization_status(request, status).await?;
			return Ok(true);
		}
		set_authorization_status(
			self.storage,
			authorization_key(self.product_id(), request),
			status,
		)
		.await?;
		Ok(true)
	}

	#[cfg(not(target_arch = "wasm32"))]
	pub(crate) async fn import_authorization_status(
		&self,
		request: &PermissionAuthorizationRequest,
		status: PermissionAuthorizationStatus,
	) -> Result<(), GenericError> {
		if matches!(request, PermissionAuthorizationRequest::CrossProductSignature { .. }) {
			return Err(GenericError {
				reason: "signature approval is execution-local and cannot be imported".into(),
			});
		}
		let scope = self.scope();
		let _mutation = scope.mutation.lock().await;
		let key = authorization_key(self.product_id(), request);
		if stored_authorization_status(self.storage, key.clone()).await?.is_some() {
			return Ok(());
		}
		self.authority.invalidate(self.product_id().as_str(), request);
		set_authorization_status(self.storage, key, status).await?;
		Ok(())
	}

	/// Caller holds the shared mutation lock. Used by imports and fenced prompts.
	pub(crate) async fn write_authorization_status(
		&self,
		request: &PermissionAuthorizationRequest,
		status: PermissionAuthorizationStatus,
	) -> Result<(), GenericError> {
		let key = match request {
			PermissionAuthorizationRequest::Device(permission) => {
				CoreStorageKey::device_permission_authorization(self.product_id(), permission)
			},
			PermissionAuthorizationRequest::Remote(request) => {
				// This is the host's per-domain surface: it names the patterns it
				// means, so it writes each one. It also writes the set-shaped
				// slot, because a stored multi-domain denial lives there and
				// would otherwise survive an explicit reset of the same domains.
				if let Some(domains) = requested_domains(request) {
					for domain in domains {
						let key =
							CoreStorageKey::remote_domain_authorization(self.product_id(), domain);
						self.temporary_permissions.revoke(&key);
						set_authorization_status(self.storage, key, status).await?;
					}
					if domains.len() > 1 {
						set_authorization_status(self.storage, self.bundle_key(domains), status)
							.await?;
					}
					return Ok(());
				}
				CoreStorageKey::remote_permission_authorization(self.product_id(), request)
			},
			PermissionAuthorizationRequest::IdentityDisclosure => {
				CoreStorageKey::identity_disclosure_authorization(self.product_id())
			},
			PermissionAuthorizationRequest::AccountAccess { target_product_id } => {
				authorization_key(
					self.product_id(),
					&PermissionAuthorizationRequest::AccountAccess {
						target_product_id: target_product_id.clone(),
					},
				)
			},
			PermissionAuthorizationRequest::ChatAuthority => {
				CoreStorageKey::chat_authority_authorization(self.product_id())
			},
			PermissionAuthorizationRequest::StatementStoreAllowance { derivation_index } => {
				CoreStorageKey::statement_store_allowance_authorization(
					self.product_id(),
					derivation_index.clone(),
				)
			},
			PermissionAuthorizationRequest::ProfileDisclosure => {
				CoreStorageKey::profile_disclosure_authorization(self.product_id())
			},
			PermissionAuthorizationRequest::CrossProductSignature { target_product_id, kind } => {
				let approval =
					cross_product_signature_key(self.product_id(), target_product_id, *kind);
				if status == PermissionAuthorizationStatus::Authorized {
					self.temporary_permissions.grant(approval);
				} else {
					self.temporary_permissions.revoke(&approval);
				}
				return Ok(());
			},
		};
		self.temporary_permissions.revoke(&key);
		set_authorization_status(self.storage, key, status).await
	}

	/// Resolve the product's identity-disclosure grant, prompting once when no
	/// durable user decision exists.
	pub async fn check_or_prompt_identity_disclosure(
		&self,
		current: impl Fn() -> bool + Send + Sync,
	) -> Result<PermissionAuthorizationStatus, GenericError>
	where
		P: UserConfirmation,
	{
		let request = PermissionAuthorizationRequest::IdentityDisclosure;
		let scope = self.scope();
		let revision = scope.revision();
		if !current() {
			return Err(GenericError { reason: "identity disclosure session changed".into() });
		}
		let cached = self.authorization_status(&request).await?;
		if cached != PermissionAuthorizationStatus::NotDetermined {
			return Ok(cached);
		}
		let decision = match scope
			.prompt(
				revision,
				self.prompt.confirm_permission(UserConfirmationReview::IdentityDisclosure(
					IdentityDisclosureReview { product_id: self.product.product_id.clone() },
				)),
			)
			.await
		{
			Ok(decision) => decision,
			// A dismissed or unavailable confirmation carries no durable user
			// decision: fail this request closed but leave authorization in the
			// ask state so the next request can prompt again.
			Err(_) => return Ok(PermissionAuthorizationStatus::NotDetermined),
		};
		let _mutation = scope.mutation.lock().await;
		scope.require_revision(revision)?;
		if !current() {
			return Err(GenericError { reason: "identity disclosure session changed".into() });
		}
		let status = match decision {
			// Authorizes only this request; nothing is persisted, so the next
			// one prompts again.
			PermissionDecision::AllowOnce => return Ok(PermissionAuthorizationStatus::Authorized),
			PermissionDecision::AllowAlways => PermissionAuthorizationStatus::Authorized,
			PermissionDecision::Deny => PermissionAuthorizationStatus::Denied,
		};
		self.write_authorization_status(&request, status).await?;
		Ok(status)
	}

	/// Resolve the product's Chat authority grant, prompting once when no
	/// durable user decision exists.
	///
	/// `AllowOnce` stores nothing here: the caller binds the consent to the
	/// current authority session, never to the longer-lived product execution.
	pub async fn check_or_prompt_chat_authority(&self) -> Result<ChatAuthorityConsent, GenericError>
	where
		P: UserConfirmation,
	{
		let request = PermissionAuthorizationRequest::ChatAuthority;
		let scope = self.scope();
		let revision = scope.revision();
		match self.authorization_status(&request).await? {
			PermissionAuthorizationStatus::Authorized => {
				return Ok(ChatAuthorityConsent::Persisted);
			},
			PermissionAuthorizationStatus::Denied => return Ok(ChatAuthorityConsent::Refused),
			PermissionAuthorizationStatus::NotDetermined => {},
		}
		let decision = match scope
			.prompt(
				revision,
				self.prompt.confirm_permission(UserConfirmationReview::ChatAuthority(
					ChatAuthorityReview { product_id: self.product_id().to_string() },
				)),
			)
			.await
		{
			Ok(decision) => decision,
			Err(_) => return Ok(ChatAuthorityConsent::Refused),
		};
		let _mutation = scope.mutation.lock().await;
		scope.require_revision(revision)?;
		match decision {
			PermissionDecision::AllowOnce => Ok(ChatAuthorityConsent::Session),
			PermissionDecision::AllowAlways => {
				self.write_authorization_status(
					&request,
					PermissionAuthorizationStatus::Authorized,
				)
				.await?;
				Ok(ChatAuthorityConsent::Persisted)
			},
			PermissionDecision::Deny => {
				self.write_authorization_status(&request, PermissionAuthorizationStatus::Denied)
					.await?;
				Ok(ChatAuthorityConsent::Refused)
			},
		}
	}

	/// Resolve the product's grant to disclose a profile reference to the
	/// user's Chat contacts, prompting once when no durable user decision
	/// exists.
	pub async fn check_or_prompt_profile_disclosure(
		&self,
	) -> Result<PermissionAuthorizationStatus, GenericError>
	where
		P: UserConfirmation,
	{
		let request = PermissionAuthorizationRequest::ProfileDisclosure;
		let scope = self.scope();
		let revision = scope.revision();
		let cached = self.authorization_status(&request).await?;
		if cached != PermissionAuthorizationStatus::NotDetermined {
			return Ok(cached);
		}
		let decision = match scope
			.prompt(
				revision,
				self.prompt.confirm_permission(UserConfirmationReview::ProfileDisclosure(
					ProfileDisclosureReview { product_id: self.product_id().to_string() },
				)),
			)
			.await
		{
			Ok(decision) => decision,
			Err(_) => return Ok(PermissionAuthorizationStatus::NotDetermined),
		};
		let _mutation = scope.mutation.lock().await;
		scope.require_revision(revision)?;
		let status = match decision {
			PermissionDecision::AllowOnce => return Ok(PermissionAuthorizationStatus::Authorized),
			PermissionDecision::AllowAlways => PermissionAuthorizationStatus::Authorized,
			PermissionDecision::Deny => PermissionAuthorizationStatus::Denied,
		};
		self.write_authorization_status(&request, status).await?;
		Ok(status)
	}

	/// Resolves a device capability against both the OS state and the stored
	/// product decision, retaining the lifetime chosen through the platform's
	/// `device_permission` callback when the question is still open.
	///
	/// The two are combined, not substituted. A stored grant is a decision
	/// about this product; the OS grant behind it is the host application's and
	/// can move underneath us at any time.
	///
	/// Only an OS refusal overrides the stored decision. `NotDetermined` does
	/// not: the OS resolves its own gate at the point the capability is used,
	/// which is where its dialog belongs, and the core has no way to ask the OS
	/// without also putting the product's question to the user again. Prompting
	/// here would re-ask an answered question on every request and overwrite
	/// the product decision with the answer to a different one.
	pub async fn check_or_prompt_device(
		&self,
		permission: HostDevicePermissionRequest,
	) -> Result<PermissionAuthorizationStatus, GenericError> {
		self.device_authorization(permission, false).await
	}

	/// Authorize one device operation, consuming its one-use grant.
	pub async fn authorize_device(
		&self,
		permission: HostDevicePermissionRequest,
	) -> Result<PermissionAuthorizationStatus, GenericError> {
		self.device_authorization(permission, true).await
	}

	async fn device_authorization(
		&self,
		permission: HostDevicePermissionRequest,
		consume: bool,
	) -> Result<PermissionAuthorizationStatus, GenericError> {
		let _guard = self.temporary_permissions.authorization.lock().await;
		let scope = self.scope();
		let mutation = scope.mutation.lock().await;
		let revision = scope.revision();
		let key = CoreStorageKey::device_permission_authorization(self.product_id(), &permission);
		if self.os_refuses(permission).await {
			return Ok(PermissionAuthorizationStatus::Denied);
		}
		match self.cached_authorization(&key, consume).await? {
			PermissionAuthorizationStatus::NotDetermined => {},
			decided => return Ok(decided),
		}
		// Only a genuine user authorization is persisted. A prompt-callback
		// error is transient (dismissed UI, unavailable UI, IPC timeout), not
		// a denial, so leave the authorization ask/default.
		drop(mutation);
		let authorization = match scope
			.prompt(revision, self.prompt.device_permission(self.product, permission))
			.await
		{
			Ok(decision) => decision,
			Err(_) => return Ok(PermissionAuthorizationStatus::NotDetermined),
		};
		let _mutation = scope.mutation.lock().await;
		scope.require_revision(revision)?;
		self.record_decision(key, authorization, consume).await
	}

	/// Authorize one remote operation, consuming a temporary grant when present.
	pub async fn authorize_remote(
		&self,
		request: RemotePermissionRequest,
	) -> Result<PermissionAuthorizationStatus, GenericError> {
		self.remote_authorization(request, true).await
	}

	/// Request remote authorization without consuming a one-use grant.
	pub async fn check_or_prompt_remote(
		&self,
		request: RemotePermissionRequest,
	) -> Result<PermissionAuthorizationStatus, GenericError> {
		self.remote_authorization(request, false).await
	}

	async fn remote_authorization(
		&self,
		request: RemotePermissionRequest,
		consume: bool,
	) -> Result<PermissionAuthorizationStatus, GenericError> {
		let _guard = self.temporary_permissions.authorization.lock().await;
		let scope = self.scope();
		let mutation = scope.mutation.lock().await;
		let revision = scope.revision();
		let Some(domains) = requested_domains(&request).map(<[String]>::to_vec) else {
			let key = CoreStorageKey::remote_permission_authorization(self.product_id(), &request);
			match self.cached_remote_authorization(&key, consume).await? {
				PermissionAuthorizationStatus::NotDetermined => {},
				decided => return Ok(decided),
			}
			// See `check_or_prompt_device`: persist only a genuine user decision;
			// transient callback errors leave the authorization ask/default.
			drop(mutation);
			let authorization = match scope
				.prompt(revision, self.prompt.remote_permission(self.product, request))
				.await
			{
				Ok(decision) => decision,
				Err(_) => return Ok(PermissionAuthorizationStatus::NotDetermined),
			};
			let _mutation = scope.mutation.lock().await;
			scope.require_revision(revision)?;
			return self.record_decision(key, authorization, consume).await;
		};

		let (resolution, temporary_keys) = self.resolve_domains(&domains).await?;
		let authorize = || {
			if self.temporary_permissions.authorize_all(&temporary_keys, consume) {
				PermissionAuthorizationStatus::Authorized
			} else {
				PermissionAuthorizationStatus::NotDetermined
			}
		};
		let undecided = match resolution {
			BundleResolution::Authorized => return Ok(authorize()),
			BundleResolution::Denied => return Ok(PermissionAuthorizationStatus::Denied),
			BundleResolution::Undecided(undecided) => undecided,
		};
		// A refusal of this exact set is already an answer to this exact prompt.
		let bundle_key = self.bundle_key(&undecided);
		match peek_stored(self.storage, bundle_key.clone()).await? {
			Some(StoredAuthorizationStatus::Authorized) => return Ok(authorize()),
			Some(StoredAuthorizationStatus::Denied) => {
				return Ok(PermissionAuthorizationStatus::Denied);
			},
			Some(StoredAuthorizationStatus::NotDetermined) | None => {},
		}

		drop(mutation);
		let authorization = match scope
			.prompt(
				revision,
				self.prompt.remote_permission(self.product, remote_bundle_request(&undecided)),
			)
			.await
		{
			Ok(decision) => decision,
			Err(_) => return Ok(PermissionAuthorizationStatus::NotDetermined),
		};
		let _mutation = scope.mutation.lock().await;
		scope.require_revision(revision)?;
		match authorization {
			// Each granted domain is independently reachable afterwards, and
			// enforcement only ever looks one host up, so a grant fans out.
			PermissionDecision::AllowAlways | PermissionDecision::AllowOnce => {
				for domain in &undecided {
					self.record_decision(
						CoreStorageKey::remote_domain_authorization(self.product_id(), domain),
						authorization,
						consume,
					)
					.await?;
				}
				Ok(authorize())
			},
			// A denial answers only the question that was asked.
			PermissionDecision::Deny => {
				self.persist_decision(bundle_key, StoredAuthorizationStatus::Denied).await
			},
		}
	}

	async fn record_decision(
		&self,
		key: CoreStorageKey,
		decision: PermissionDecision,
		consume: bool,
	) -> Result<PermissionAuthorizationStatus, GenericError> {
		match decision {
			PermissionDecision::AllowOnce => {
				if !consume {
					self.temporary_permissions.grant(key);
				}
				Ok(PermissionAuthorizationStatus::Authorized)
			},
			PermissionDecision::AllowAlways => {
				self.persist_decision(key, StoredAuthorizationStatus::Authorized).await
			},
			PermissionDecision::Deny => {
				self.persist_decision(key, StoredAuthorizationStatus::Denied).await
			},
		}
	}

	/// Persist a fresh user decision and return its public status.
	async fn persist_decision(
		&self,
		key: CoreStorageKey,
		authorization: StoredAuthorizationStatus,
	) -> Result<PermissionAuthorizationStatus, GenericError> {
		self.storage.write_core_storage(key, authorization.encode()).await?;
		Ok(authorization.into())
	}
}

/// Canonical key for host administration, matching the product-call paths.
pub(crate) fn authorization_key(
	product: &ProductId,
	request: &PermissionAuthorizationRequest,
) -> CoreStorageKey {
	match request {
		PermissionAuthorizationRequest::Device(permission) => {
			CoreStorageKey::device_permission_authorization(product, permission)
		},
		PermissionAuthorizationRequest::Remote(remote) => {
			CoreStorageKey::remote_permission_authorization(product, remote)
		},
		PermissionAuthorizationRequest::IdentityDisclosure => {
			CoreStorageKey::identity_disclosure_authorization(product)
		},
		PermissionAuthorizationRequest::AccountAccess { target_product_id } => {
			CoreStorageKey::product_account_access_authorization(product, target_product_id)
		},
		PermissionAuthorizationRequest::ChatAuthority => {
			CoreStorageKey::chat_authority_authorization(product)
		},
		PermissionAuthorizationRequest::StatementStoreAllowance { derivation_index } => {
			CoreStorageKey::statement_store_allowance_authorization(
				product,
				derivation_index.clone(),
			)
		},
		PermissionAuthorizationRequest::ProfileDisclosure => {
			CoreStorageKey::profile_disclosure_authorization(product)
		},
		PermissionAuthorizationRequest::CrossProductSignature { target_product_id, kind } => {
			CoreStorageKey::product_signature_authorization(product, target_product_id, *kind)
		},
	}
}

// Persisted administration keys may already contain a bare label rather than
// a ProductId. Keep that label stable when comparing or resetting these keys.
fn permission_product_label(product: &str) -> &str {
	product.rsplit_once('.').map_or(product, |(above_tld, _)| {
		above_tld.rsplit_once('.').map_or(above_tld, |(_, label)| label)
	})
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn stored_authorization_status<S: CoreStorage + ?Sized>(
	storage: &S,
	key: CoreStorageKey,
) -> Result<Option<PermissionAuthorizationStatus>, GenericError> {
	let Some(raw) = storage.read_core_storage(key).await? else {
		return Ok(None);
	};
	let mut input = raw.as_slice();
	let status = StoredAuthorizationStatus::decode(&mut input)
		.map_err(|_| GenericError { reason: "invalid stored permission decision".into() })?;
	if !input.is_empty() {
		return Err(GenericError { reason: "trailing stored permission decision bytes".into() });
	}
	Ok(Some(status.into()))
}

/// Read the decision stored under an encoded permission scope.
pub async fn authorization_status<S: CoreStorage + ?Sized>(
	storage: &S,
	key: CoreStorageKey,
) -> Result<PermissionAuthorizationStatus, GenericError> {
	Ok(peek_stored(storage, key)
		.await?
		.map(Into::into)
		.unwrap_or(PermissionAuthorizationStatus::NotDetermined))
}

async fn peek_stored<S: CoreStorage + ?Sized>(
	storage: &S,
	key: CoreStorageKey,
) -> Result<Option<StoredAuthorizationStatus>, GenericError> {
	let Some(raw) = storage.read_core_storage(key).await? else {
		return Ok(None);
	};
	Ok(StoredAuthorizationStatus::decode(&mut &*raw).ok())
}

/// Save or clear the decision stored under an encoded permission scope.
pub async fn set_authorization_status<S: CoreStorage + ?Sized>(
	storage: &S,
	key: CoreStorageKey,
	status: PermissionAuthorizationStatus,
) -> Result<(), GenericError> {
	storage.write_core_storage(key, status_into_stored(status).encode()).await
}

fn status_into_stored(status: PermissionAuthorizationStatus) -> StoredAuthorizationStatus {
	match status {
		PermissionAuthorizationStatus::NotDetermined => StoredAuthorizationStatus::NotDetermined,
		PermissionAuthorizationStatus::Denied => StoredAuthorizationStatus::Denied,
		PermissionAuthorizationStatus::Authorized => StoredAuthorizationStatus::Authorized,
	}
}

#[cfg(test)]
mod tests;
