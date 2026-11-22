//! Permission authorization state machine (ask -> authorized | denied), backed
//! by the platform [`CoreStorage`] trait with typed [`CoreStorageKey`] slots.
//!
//! Device permissions (camera, mic, NFC, ...) are separate from remote
//! permissions (domain access, chain submit, ...), with separate
//! `check_or_prompt` entrypoints routing to the matching platform callback.
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
//! the OS grant. Calling always requires exact product/account/network consent.

use std::{collections::HashMap, sync::Arc};

use parity_scale_codec::{Decode, Encode};
use rand_core::{OsRng, RngCore};

use crate::platform::{
	BLESSED_REMOTE_DOMAINS, CallingReview, CoreStorage, CoreStorageKey, CrossProductSignatureKind,
	DevicePermissionStatus, PermissionAuthorizationRequest, PermissionAuthorizationStatus,
	PermissionDecision, PermissionStatusHost, Permissions, ProductContext, ProductId,
	UserConfirmation, UserConfirmationReview, is_valid_remote_domain_pattern,
	normalize_remote_domain, remote_domain_candidates,
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

impl From<bool> for StoredAuthorizationStatus {
	fn from(granted: bool) -> Self {
		if granted { Self::Authorized } else { Self::Denied }
	}
}

const AUTHORIZATION_CAS_ATTEMPTS: usize = 8;

/// The exact product/request generation that was presented for consent.
/// Missing slots are stamped before this can be constructed, so deletion
/// cannot make a late answer match an absent slot again.
pub(crate) struct PermissionAuthorizationSnapshot {
	pub(crate) status: PermissionAuthorizationStatus,
	key: CoreStorageKey,
	request: PermissionAuthorizationRequest,
	expected: Vec<u8>,
}

fn decode_authorization(raw: &[u8]) -> Option<PermissionAuthorizationStatus> {
	let status = match raw {
		[status @ (0 | 1 | 2)] => *status,
		[2, rest @ ..] if rest.len() == 33 => rest[32],
		_ => return None,
	};
	match status {
		0 => Some(PermissionAuthorizationStatus::Authorized),
		1 => Some(PermissionAuthorizationStatus::Denied),
		2 => Some(PermissionAuthorizationStatus::NotDetermined),
		_ => None,
	}
}

fn stamped_authorization(status: PermissionAuthorizationStatus) -> Result<Vec<u8>, GenericError> {
	let mut record = vec![0; 34];
	record[0] = 2;
	OsRng.try_fill_bytes(&mut record[1..33]).map_err(|_| GenericError {
		reason: "Permission authorization entropy unavailable".into(),
	})?;
	record[33] = match status {
		PermissionAuthorizationStatus::Authorized => 0,
		PermissionAuthorizationStatus::Denied => 1,
		PermissionAuthorizationStatus::NotDetermined => 2,
	};
	Ok(record)
}

fn authorization_contention() -> GenericError {
	GenericError { reason: "Permission authorization changed concurrently".into() }
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
	grants: std::sync::Mutex<HashMap<Vec<u8>, Option<Vec<u8>>>>,
	/// Held while a signature prompt is open. Separate from `authorization`,
	/// which every submission check takes, so a signature prompt never holds
	/// up the product's submissions.
	signature_prompt: futures::lock::Mutex<()>,
}

impl TemporaryPermissions {
	fn authorize_generation(
		&self,
		key: &CoreStorageKey,
		generation: Option<&[u8]>,
		consume: bool,
	) -> bool {
		let key = key.encode();
		let mut grants = self.grants.lock().expect("temporary permissions mutex poisoned");
		if !grants.get(&key).is_some_and(|grant| grant.as_deref() == generation) {
			grants.remove(&key);
			return false;
		}
		if consume {
			grants.remove(&key);
		}
		true
	}

	fn grant_generation(&self, key: CoreStorageKey, generation: Vec<u8>) {
		self.grants
			.lock()
			.expect("temporary permissions mutex poisoned")
			.insert(key.encode(), Some(generation));
	}

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
		if !keys.iter().all(|key| grants.get(key) == Some(&None)) {
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
			.insert(key.encode(), None);
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
		if matches!(request.permission, RemotePermission::Calling) {
			return Ok(PermissionAuthorizationStatus::Denied);
		}
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
		let raw = self.storage.read_core_storage(key.clone()).await?;
		if let Some(status) = raw.as_deref().and_then(decode_authorization) &&
			status != PermissionAuthorizationStatus::NotDetermined
		{
			return Ok(status);
		}
		Ok(if self.temporary_permissions.authorize_generation(key, raw.as_deref(), consume) {
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
			PermissionAuthorizationRequest::Calling { network, account } => {
				authorization_status(
					self.storage,
					CoreStorageKey::calling_authorization(self.product_id(), *network, *account),
				)
				.await
			},
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

	pub(crate) async fn stored_authorization_status(
		&self,
		request: PermissionAuthorizationRequest,
	) -> Result<PermissionAuthorizationStatus, GenericError> {
		match request {
			PermissionAuthorizationRequest::Device(permission) => {
				authorization_status(
					self.storage,
					CoreStorageKey::device_permission_authorization(self.product_id(), &permission),
				)
				.await
			},
			request => self.authorization_status(&request).await,
		}
	}

	fn snapshot_key(
		&self,
		request: &PermissionAuthorizationRequest,
	) -> Result<CoreStorageKey, GenericError> {
		match request {
			PermissionAuthorizationRequest::Device(permission) => {
				Ok(CoreStorageKey::device_permission_authorization(self.product_id(), permission))
			},
			PermissionAuthorizationRequest::Calling { network, account } => {
				Ok(CoreStorageKey::calling_authorization(self.product_id(), *network, *account))
			},
			_ => Err(GenericError {
				reason: "Authorization snapshots require Calling or Device scope".into(),
			}),
		}
	}

	/// Capture a consent generation before showing UI. Initial Ask stamping is
	/// not a policy decision and must not notify or revoke receive-only calls.
	pub(crate) async fn authorization_snapshot(
		&self,
		request: &PermissionAuthorizationRequest,
	) -> Result<PermissionAuthorizationSnapshot, GenericError> {
		let key = self.snapshot_key(request)?;
		for _ in 0..AUTHORIZATION_CAS_ATTEMPTS {
			let expected = match self.storage.read_core_storage(key.clone()).await? {
				Some(raw) => raw,
				None => {
					let initial =
						stamped_authorization(PermissionAuthorizationStatus::NotDetermined)?;
					if !self
						.storage
						.compare_exchange_core_storage(key.clone(), None, initial.clone(), false)
						.await?
					{
						continue;
					}
					initial
				},
			};
			let mut status = decode_authorization(&expected)
				.unwrap_or(PermissionAuthorizationStatus::NotDetermined);
			if let PermissionAuthorizationRequest::Device(permission) = request &&
				self.os_refuses(*permission).await
			{
				status = PermissionAuthorizationStatus::Denied;
			}
			return Ok(PermissionAuthorizationSnapshot {
				status,
				key,
				request: request.clone(),
				expected,
			});
		}
		Err(authorization_contention())
	}

	/// Commit one genuine answer against exactly the generation shown to the
	/// user. A stale answer is discarded, never rebased onto newer policy.
	/// Callers finish synchronous cancellation/authority/revision checks just
	/// before dispatch. Dispatch accepts this policy decision independently of
	/// later Media work: cancelling that work does not undo accepted consent.
	/// The host CAS job owns successful-policy notification, even if this
	/// requester disappears before the persisted result can be returned.
	pub(crate) async fn set_authorization_status_if_unchanged(
		&self,
		request: &PermissionAuthorizationRequest,
		snapshot: &PermissionAuthorizationSnapshot,
		status: PermissionAuthorizationStatus,
	) -> Result<bool, GenericError> {
		let key = self.snapshot_key(request)?;
		if snapshot.key != key || snapshot.request != *request {
			return Ok(false);
		}
		self.storage
			.compare_exchange_core_storage(
				key,
				Some(snapshot.expected.clone()),
				stamped_authorization(status)?,
				true,
			)
			.await
	}

	async fn set_stamped_authorization(
		&self,
		key: CoreStorageKey,
		status: PermissionAuthorizationStatus,
	) -> Result<(), GenericError> {
		for _ in 0..AUTHORIZATION_CAS_ATTEMPTS {
			let expected = self.storage.read_core_storage(key.clone()).await?;
			if self
				.storage
				.compare_exchange_core_storage(
					key.clone(),
					expected,
					stamped_authorization(status)?,
					true,
				)
				.await?
			{
				return Ok(());
			}
		}
		Err(authorization_contention())
	}

	/// Resolve calling consent for the authenticated network and the product's
	/// authority-derived `Index(0)` account, prompting only for an undecided scope.
	///
	/// The caller must resolve this immutable scope from the active authority
	/// session. Trusted remote labels and legacy unscoped grants never apply.
	/// Transient prompt failures leave the scope `NotDetermined`.
	/// The commit callback must fence and persist the answer after rechecking
	/// the authority and permission revision. It runs only after an actual
	/// user answer, never while the consent UI is open.
	pub(crate) async fn check_or_prompt_calling<F, Fut>(
		&self,
		network: [u8; 32],
		account: [u8; 32],
		commit: F,
	) -> Result<PermissionAuthorizationStatus, GenericError>
	where
		P: UserConfirmation,
		F: FnOnce(PermissionAuthorizationSnapshot, PermissionAuthorizationStatus) -> Fut,
		Fut: core::future::Future<Output = Result<PermissionAuthorizationStatus, GenericError>>,
	{
		let request = PermissionAuthorizationRequest::Calling { network, account };
		let snapshot = self.authorization_snapshot(&request).await?;
		if snapshot.status != PermissionAuthorizationStatus::NotDetermined {
			return Ok(snapshot.status);
		}
		let confirmed = match self
			.prompt
			.confirm_user_action(UserConfirmationReview::Calling(CallingReview {
				product_id: self.product_id().clone(),
				network,
				account,
			}))
			.await
		{
			Ok(confirmed) => confirmed,
			Err(_) => return Ok(PermissionAuthorizationStatus::NotDetermined),
		};
		let status = if confirmed {
			PermissionAuthorizationStatus::Authorized
		} else {
			PermissionAuthorizationStatus::Denied
		};
		commit(snapshot, status).await
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
			PermissionAuthorizationRequest::Calling { network, account } => {
				CoreStorageKey::calling_authorization(self.product_id(), *network, *account)
			},
			PermissionAuthorizationRequest::Device(permission) => {
				CoreStorageKey::device_permission_authorization(self.product_id(), permission)
			},
			PermissionAuthorizationRequest::Remote(request) => {
				if matches!(request.permission, RemotePermission::Calling) {
					return Err(GenericError {
						reason: "Calling authorization requires network and account scope".into(),
					});
				}
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
		if matches!(
			request,
			PermissionAuthorizationRequest::Calling { .. } |
				PermissionAuthorizationRequest::Device(_)
		) {
			self.set_stamped_authorization(key, status).await
		} else {
			set_authorization_status(self.storage, key, status).await
		}
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
	#[cfg(test)]
	pub async fn check_or_prompt_device(
		&self,
		permission: HostDevicePermissionRequest,
	) -> Result<PermissionAuthorizationStatus, GenericError> {
		self.device_authorization(permission, false, |snapshot, decision| async move {
			self.record_permission_decision_if_unchanged(&snapshot, decision, false).await
		})
		.await
	}

	/// Authorize one device operation, consuming a temporary grant when present.
	pub async fn authorize_device(
		&self,
		permission: HostDevicePermissionRequest,
	) -> Result<PermissionAuthorizationStatus, GenericError> {
		self.device_authorization(permission, true, |snapshot, decision| async move {
			self.record_permission_decision_if_unchanged(&snapshot, decision, true).await
		})
		.await
	}

	/// Let the runtime fence a device decision against cancellation and authority changes.
	pub(crate) async fn check_or_prompt_device_fenced<F, Fut>(
		&self,
		permission: HostDevicePermissionRequest,
		commit: F,
	) -> Result<PermissionAuthorizationStatus, GenericError>
	where
		F: FnOnce(PermissionAuthorizationSnapshot, PermissionDecision) -> Fut,
		Fut: core::future::Future<Output = Result<PermissionAuthorizationStatus, GenericError>>,
	{
		self.device_authorization(permission, false, commit).await
	}

	async fn device_authorization<F, Fut>(
		&self,
		permission: HostDevicePermissionRequest,
		consume: bool,
		commit: F,
	) -> Result<PermissionAuthorizationStatus, GenericError>
	where
		F: FnOnce(PermissionAuthorizationSnapshot, PermissionDecision) -> Fut,
		Fut: core::future::Future<Output = Result<PermissionAuthorizationStatus, GenericError>>,
	{
		let _guard = self.temporary_permissions.authorization.lock().await;
		if self.os_refuses(permission).await {
			return Ok(PermissionAuthorizationStatus::Denied);
		}
		let key = CoreStorageKey::device_permission_authorization(self.product_id(), &permission);
		match self.cached_authorization(&key, consume).await? {
			PermissionAuthorizationStatus::NotDetermined => {},
			decided => return Ok(decided),
		}
		let request = PermissionAuthorizationRequest::Device(permission);
		let snapshot = self.authorization_snapshot(&request).await?;
		if snapshot.status != PermissionAuthorizationStatus::NotDetermined {
			return Ok(snapshot.status);
		}
		let decision = match self.prompt.device_permission(self.product, permission).await {
			Ok(decision) => decision,
			Err(_) => return Ok(PermissionAuthorizationStatus::NotDetermined),
		};
		if self.os_refuses(permission).await {
			return Ok(PermissionAuthorizationStatus::Denied);
		}
		commit(snapshot, decision).await
	}

	/// Persist a durable answer or reserve a one-use grant against the displayed generation.
	pub(crate) async fn record_permission_decision_if_unchanged(
		&self,
		snapshot: &PermissionAuthorizationSnapshot,
		decision: PermissionDecision,
		consume: bool,
	) -> Result<PermissionAuthorizationStatus, GenericError> {
		let key = self.snapshot_key(&snapshot.request)?;
		if key != snapshot.key {
			return Ok(PermissionAuthorizationStatus::NotDetermined);
		}
		let status = match decision {
			PermissionDecision::AllowOnce => PermissionAuthorizationStatus::NotDetermined,
			PermissionDecision::AllowAlways => PermissionAuthorizationStatus::Authorized,
			PermissionDecision::Deny => PermissionAuthorizationStatus::Denied,
		};
		let replacement = stamped_authorization(status)?;
		let generation =
			(decision == PermissionDecision::AllowOnce && !consume).then(|| replacement.clone());
		if !self
			.storage
			.compare_exchange_core_storage(
				key.clone(),
				Some(snapshot.expected.clone()),
				replacement,
				decision != PermissionDecision::AllowOnce,
			)
			.await?
		{
			return Ok(PermissionAuthorizationStatus::NotDetermined);
		}
		if decision == PermissionDecision::AllowOnce {
			if let Some(generation) = generation {
				self.temporary_permissions.grant_generation(key, generation);
			}
			return Ok(PermissionAuthorizationStatus::Authorized);
		}
		Ok(status)
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
		if matches!(request.permission, RemotePermission::Calling) {
			return Ok(PermissionAuthorizationStatus::Denied);
		}
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
	Ok(decode_authorization(&raw).and_then(status_into_stored))
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
