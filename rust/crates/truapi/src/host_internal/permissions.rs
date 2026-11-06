//! Permission authorization state machine (ask -> authorized | denied), backed
//! by the platform [`CoreStorage`] trait with typed [`CoreStorageKey`] slots.
//!
//! Device permissions (camera, mic, NFC, ...) are separate from remote
//! permissions (domain access, chain submit, ...), so this module exposes two
//! `check_or_prompt` entrypoints that route to the matching platform callback.
//! The cache layer is shared but keys are typed so a device grant cannot
//! authorize a remote operation by accident. Keys are also scoped by product id
//! so one product's authorization never grants another product's request.
//! Identity disclosure is also represented as a product-scoped authorization,
//! but the prompt itself is handled by the account runtime because it uses the
//! richer user-confirmation surface rather than the device/remote callbacks.
//! One-use grants stay in memory and are consumed by operation authorization,
//! separately from upfront permission requests.
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
//! Products listed in [`crate::platform::REMOTE_PERMISSION_TRUSTED_LABELS`]
//! get remote permissions, device permissions, identity disclosure and account
//! access without a prompt or a stored record. Device permissions still need
//! the OS grant.

use std::{collections::HashSet, sync::Arc};

use parity_scale_codec::{Decode, Encode};

use crate::platform::{
	BLESSED_REMOTE_DOMAINS, CoreStorage, CoreStorageKey, CrossProductSignatureKind,
	DevicePermissionStatus, PermissionAuthorizationRequest, PermissionAuthorizationStatus,
	PermissionDecision, PermissionStatusHost, Permissions, ProductContext, ProductId,
	is_valid_remote_domain_pattern, normalize_remote_domain, remote_domain_candidates,
};
use truapi::latest::{
	GenericError, HostDevicePermissionRequest, RemotePermission, RemotePermissionRequest,
};

/// Persisted answer for a single permission request. Keep `Authorized` at
/// discriminant 0 and `Denied` at 1 to preserve the existing two-variant cache
/// encoding.
#[derive(Clone, Encode, Decode)]
enum StoredAuthorizationStatus {
	/// User authorized the permission.
	Authorized,
	/// User denied the permission.
	Denied,
}

impl From<StoredAuthorizationStatus> for PermissionAuthorizationStatus {
	fn from(status: StoredAuthorizationStatus) -> Self {
		match status {
			StoredAuthorizationStatus::Authorized => PermissionAuthorizationStatus::Authorized,
			StoredAuthorizationStatus::Denied => PermissionAuthorizationStatus::Denied,
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

/// Permission prompts and one-use grants shared by a product execution's connections.
#[derive(Default)]
pub struct TemporaryPermissions {
	authorization: futures::lock::Mutex<()>,
	grants: std::sync::Mutex<HashSet<Vec<u8>>>,
	/// Held while a signature prompt is open. Separate from `authorization`,
	/// which every submission check takes, so a signature prompt never holds
	/// up the product's submissions.
	signature_prompt: futures::lock::Mutex<()>,
}

impl TemporaryPermissions {
	#[cfg(not(target_arch = "wasm32"))]
	pub fn clear(&self) {
		self.grants.lock().expect("temporary permissions mutex poisoned").clear();
	}

	/// Wait until no signature prompt is open, and keep others from opening
	/// until the returned guard is dropped.
	pub async fn signature_prompt(&self) -> futures::lock::MutexGuard<'_, ()> {
		self.signature_prompt.lock().await
	}

	/// Whether a grant covers `key`, consuming it when `consume` is set.
	pub fn authorize(&self, key: &CoreStorageKey, consume: bool) -> bool {
		self.authorize_all(core::slice::from_ref(key), consume)
	}

	fn authorize_all(&self, keys: &[CoreStorageKey], consume: bool) -> bool {
		let keys: Vec<_> = keys.iter().map(Encode::encode).collect();
		let mut grants = self.grants.lock().expect("temporary permissions mutex poisoned");
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
		self.grants
			.lock()
			.expect("temporary permissions mutex poisoned")
			.remove(&key.encode());
	}

	/// Grant `key` until it is consumed or the product execution closes.
	pub fn grant(&self, key: CoreStorageKey) {
		self.grants
			.lock()
			.expect("temporary permissions mutex poisoned")
			.insert(key.encode());
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
	/// Whether permissions are granted without a prompt or a stored record.
	trusted_product: bool,
	/// One-use grants remain local to the execution that requested them.
	temporary_permissions: Arc<TemporaryPermissions>,
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
		}
	}

	pub fn with_temporary_permissions(mut self, permissions: Arc<TemporaryPermissions>) -> Self {
		self.temporary_permissions = permissions;
		self
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
	async fn peek_device(
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
			return self.cached_authorization(&key, false).await;
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
		if self.trusted_product {
			return Ok((BundleResolution::Authorized, Vec::new()));
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
	/// any candidate, blessed domains are authorized.
	async fn effective_domain_status(
		&self,
		domain: &str,
	) -> Result<(PermissionAuthorizationStatus, Option<CoreStorageKey>), GenericError> {
		let blessed = BLESSED_REMOTE_DOMAINS.contains(&normalize_remote_domain(domain).as_str());
		let mut temporary_keys = Vec::new();
		let mut fallback = if blessed {
			PermissionAuthorizationStatus::Authorized
		} else {
			PermissionAuthorizationStatus::NotDetermined
		};
		for candidate in remote_domain_candidates(domain) {
			let key = CoreStorageKey::remote_domain_authorization(self.product_id(), &candidate);
			if let Some(stored) = peek_stored(self.storage, key.clone()).await? {
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
		if self.trusted_product {
			return Ok(PermissionAuthorizationStatus::Authorized);
		}
		if let Some(stored) = peek_stored(self.storage, key.clone()).await? {
			return Ok(stored.into());
		}
		Ok(if self.temporary_permissions.authorize(key, consume) {
			PermissionAuthorizationStatus::Authorized
		} else {
			PermissionAuthorizationStatus::NotDetermined
		})
	}

	/// Returns the current authorization status for a permission request
	/// without prompting.
	pub async fn authorization_status(
		&self,
		request: &PermissionAuthorizationRequest,
	) -> Result<PermissionAuthorizationStatus, GenericError> {
		if self.trusted_product &&
			matches!(
				request,
				PermissionAuthorizationRequest::IdentityDisclosure |
					PermissionAuthorizationRequest::AccountAccess { .. }
			) {
			return Ok(PermissionAuthorizationStatus::Authorized);
		}
		match request {
			PermissionAuthorizationRequest::Device(permission) => {
				self.peek_device(permission).await
			},
			PermissionAuthorizationRequest::Remote(request) => self.peek_remote(request).await,
			PermissionAuthorizationRequest::IdentityDisclosure => {
				authorization_status(
					self.storage,
					CoreStorageKey::identity_disclosure_authorization(self.product_id()),
				)
				.await
			},
			PermissionAuthorizationRequest::AccountAccess { target_product_id } => {
				authorization_status(
					self.storage,
					CoreStorageKey::account_access_authorization(
						self.product_id(),
						target_product_id,
					),
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

	/// Returns the current authorization statuses for permission requests
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
	/// Setting `NotDetermined` clears the stored value, restoring the default.
	pub async fn set_authorization_status(
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
				CoreStorageKey::account_access_authorization(self.product_id(), target_product_id)
			},
			// Held in memory for the execution only, so nothing is stored. A
			// denial is not kept either, like a refusal at the prompt.
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

	/// Authorize one device operation, consuming a temporary grant when present.
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
		let authorization = match self.prompt.device_permission(self.product, permission).await {
			Ok(decision) => decision,
			Err(_) => return Ok(PermissionAuthorizationStatus::NotDetermined),
		};
		self.record_decision(key, authorization, consume).await
	}

	/// Requests remote authorization without consuming a one-use grant.
	///
	/// For a domain bundle the prompt covers only the domains with no stored
	/// decision. Re-asking about an already-granted domain would let one denial
	/// revoke it, and re-asking about a denied one contradicts the prompt-once
	/// rule. A grant is written per domain; a denial of more than one domain is
	/// written against the set, per the asymmetry in the module docs.
	pub async fn check_or_prompt_remote(
		&self,
		request: RemotePermissionRequest,
	) -> Result<PermissionAuthorizationStatus, GenericError> {
		self.remote_authorization(request, false).await
	}

	/// Authorize one remote operation, consuming a temporary grant when present.
	pub async fn authorize_remote(
		&self,
		request: RemotePermissionRequest,
	) -> Result<PermissionAuthorizationStatus, GenericError> {
		self.remote_authorization(request, true).await
	}

	async fn remote_authorization(
		&self,
		request: RemotePermissionRequest,
		consume: bool,
	) -> Result<PermissionAuthorizationStatus, GenericError> {
		if self.trusted_product {
			return self.peek_remote(&request).await;
		}
		let _guard = self.temporary_permissions.authorization.lock().await;
		let Some(domains) = requested_domains(&request).map(<[String]>::to_vec) else {
			let key = CoreStorageKey::remote_permission_authorization(self.product_id(), &request);
			match self.cached_authorization(&key, consume).await? {
				PermissionAuthorizationStatus::NotDetermined => {},
				decided => return Ok(decided),
			}
			// See `check_or_prompt_device`: persist only a genuine user decision;
			// transient callback errors leave the authorization ask/default.
			let authorization = match self.prompt.remote_permission(self.product, request).await {
				Ok(decision) => decision,
				Err(_) => return Ok(PermissionAuthorizationStatus::NotDetermined),
			};
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
		if let Some(cached) = peek_stored(self.storage, bundle_key.clone()).await? {
			return Ok(match cached {
				StoredAuthorizationStatus::Authorized => authorize(),
				StoredAuthorizationStatus::Denied => PermissionAuthorizationStatus::Denied,
			});
		}

		let authorization = match self
			.prompt
			.remote_permission(self.product, remote_bundle_request(&undecided))
			.await
		{
			Ok(decision) => decision,
			Err(_) => return Ok(PermissionAuthorizationStatus::NotDetermined),
		};
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
	match status_into_stored(status) {
		Some(stored) => storage.write_core_storage(key, stored.encode()).await,
		None => storage.clear_core_storage(key).await,
	}
}

fn status_into_stored(status: PermissionAuthorizationStatus) -> Option<StoredAuthorizationStatus> {
	match status {
		PermissionAuthorizationStatus::NotDetermined => None,
		PermissionAuthorizationStatus::Denied => Some(StoredAuthorizationStatus::Denied),
		PermissionAuthorizationStatus::Authorized => Some(StoredAuthorizationStatus::Authorized),
	}
}

#[cfg(test)]
mod tests;
