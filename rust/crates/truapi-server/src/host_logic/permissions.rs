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
//! Remote permissions have one product-scoped exception. A product whose label
//! is listed in [`truapi_platform::REMOTE_PERMISSION_TRUSTED_LABELS`] reads as
//! authorized for every remote permission while nothing is stored, and never
//! reaches the prompt callback. A stored decision still wins, so a denial
//! written through the admin surface revokes the grant. Device permissions,
//! identity disclosure, account access, Chat authority, and profile disclosure
//! are never covered.

use std::collections::HashSet;
use std::sync::Arc;

use parity_scale_codec::{Decode, Encode};

use truapi::latest::{
    GenericError, HostDevicePermissionRequest, RemotePermission, RemotePermissionRequest,
};
use truapi_platform::{
    BLESSED_REMOTE_DOMAINS, ChatAuthorityReview, CoreStorage, CoreStorageKey,
    DevicePermissionStatus, IdentityDisclosureReview, PermissionAuthorizationRequest,
    PermissionAuthorizationStatus, PermissionDecision, PermissionStatusHost, Permissions,
    ProductContext, ProfileDisclosureReview, UserConfirmation, UserConfirmationReview,
    has_trusted_remote_permissions, is_valid_remote_domain_pattern, normalize_remote_domain,
    remote_domain_candidates,
};

/// Persisted answer for a single permission request. Keep `Authorized` at
/// discriminant 0 and `Denied` at 1 to preserve the existing two-variant cache
/// encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
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
    RemotePermissionRequest {
        permission: RemotePermission::Remote {
            domains: domains.to_vec(),
        },
    }
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

/// Permission prompts and one-use grants shared by a product execution's connections.
#[derive(Default)]
pub(crate) struct TemporaryPermissions {
    authorization: futures::lock::Mutex<()>,
    grants: std::sync::Mutex<HashSet<Vec<u8>>>,
}

impl TemporaryPermissions {
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn clear(&self) {
        self.grants
            .lock()
            .expect("temporary permissions mutex poisoned")
            .clear();
    }

    fn authorize(&self, key: &CoreStorageKey, consume: bool) -> bool {
        self.authorize_all(core::slice::from_ref(key), consume)
    }

    fn authorize_all(&self, keys: &[CoreStorageKey], consume: bool) -> bool {
        let keys: Vec<_> = keys.iter().map(Encode::encode).collect();
        let mut grants = self
            .grants
            .lock()
            .expect("temporary permissions mutex poisoned");
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

    fn grant(&self, key: CoreStorageKey) {
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
    /// Whether `product` holds every remote permission without prompting.
    remote_auto_granted: bool,
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
            remote_auto_granted: has_trusted_remote_permissions(&product.product_id),
            temporary_permissions: Arc::default(),
        }
    }

    pub(crate) fn with_temporary_permissions(
        mut self,
        permissions: Arc<TemporaryPermissions>,
    ) -> Self {
        self.temporary_permissions = permissions;
        self
    }

    fn product_id(&self) -> &str {
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
            }
        }
    }

    /// Resolve a bundle against the per-domain decisions alone.
    async fn resolve_domains(
        &self,
        domains: &[String],
    ) -> Result<(BundleResolution, Vec<CoreStorageKey>), GenericError> {
        if domains.is_empty()
            || domains
                .iter()
                .any(|domain| !is_valid_remote_domain_pattern(domain))
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
                }
                PermissionAuthorizationStatus::NotDetermined => undecided.push(domain.clone()),
                PermissionAuthorizationStatus::Authorized => {}
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
        let mut fallback = if blessed {
            PermissionAuthorizationStatus::Authorized
        } else {
            self.default_remote_status()
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
        if let Some(stored) = peek_stored(self.storage, key.clone()).await? {
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
        match self.cached_authorization(key, consume).await? {
            PermissionAuthorizationStatus::NotDetermined => Ok(self.default_remote_status()),
            decided => Ok(decided),
        }
    }

    fn default_remote_status(&self) -> PermissionAuthorizationStatus {
        if self.remote_auto_granted {
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
        match request {
            PermissionAuthorizationRequest::Device(permission) => {
                self.peek_device(permission).await
            }
            PermissionAuthorizationRequest::Remote(request) => self.peek_remote(request).await,
            PermissionAuthorizationRequest::IdentityDisclosure => {
                authorization_status(
                    self.storage,
                    CoreStorageKey::identity_disclosure_authorization(self.product_id()),
                )
                .await
            }
            PermissionAuthorizationRequest::AccountAccess { target_product_id } => {
                authorization_status(
                    self.storage,
                    CoreStorageKey::account_access_authorization(
                        self.product_id(),
                        target_product_id,
                    ),
                )
                .await
            }
            PermissionAuthorizationRequest::ChatAuthority => {
                authorization_status(
                    self.storage,
                    CoreStorageKey::chat_authority_authorization(self.product_id()),
                )
                .await
            }
            PermissionAuthorizationRequest::StatementStoreAllowance { derivation_index } => {
                authorization_status(
                    self.storage,
                    CoreStorageKey::statement_store_allowance_authorization(
                        self.product_id(),
                        derivation_index.clone(),
                    ),
                )
                .await
            }
            PermissionAuthorizationRequest::ProfileDisclosure => {
                authorization_status(
                    self.storage,
                    CoreStorageKey::profile_disclosure_authorization(self.product_id()),
                )
                .await
            }
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
    /// Setting `NotDetermined` clears the stored value so the next product
    /// request prompts again.
    pub async fn set_authorization_status(
        &self,
        request: &PermissionAuthorizationRequest,
        status: PermissionAuthorizationStatus,
    ) -> Result<(), GenericError> {
        let key = match request {
            PermissionAuthorizationRequest::Device(permission) => {
                CoreStorageKey::device_permission_authorization(self.product_id(), permission)
            }
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
            }
            PermissionAuthorizationRequest::IdentityDisclosure => {
                CoreStorageKey::identity_disclosure_authorization(self.product_id())
            }
            PermissionAuthorizationRequest::AccountAccess { target_product_id } => {
                CoreStorageKey::account_access_authorization(self.product_id(), target_product_id)
            }
            PermissionAuthorizationRequest::ChatAuthority => {
                CoreStorageKey::chat_authority_authorization(self.product_id())
            }
            PermissionAuthorizationRequest::StatementStoreAllowance { derivation_index } => {
                CoreStorageKey::statement_store_allowance_authorization(
                    self.product_id(),
                    derivation_index.clone(),
                )
            }
            PermissionAuthorizationRequest::ProfileDisclosure => {
                CoreStorageKey::profile_disclosure_authorization(self.product_id())
            }
        };
        self.temporary_permissions.revoke(&key);
        set_authorization_status(self.storage, key, status).await
    }

    /// Resolve the product's identity-disclosure grant, prompting once when no
    /// durable user decision exists.
    pub async fn check_or_prompt_identity_disclosure(
        &self,
    ) -> Result<PermissionAuthorizationStatus, GenericError>
    where
        P: UserConfirmation,
    {
        let request = PermissionAuthorizationRequest::IdentityDisclosure;
        let cached = self.authorization_status(&request).await?;
        if cached != PermissionAuthorizationStatus::NotDetermined {
            return Ok(cached);
        }
        let decision = match self
            .prompt
            .confirm_permission(UserConfirmationReview::IdentityDisclosure(
                IdentityDisclosureReview {
                    product_id: self.product_id().to_string(),
                },
            ))
            .await
        {
            Ok(decision) => decision,
            // A dismissed or unavailable confirmation carries no durable user
            // decision: fail this request closed but leave authorization in the
            // ask state so the next request can prompt again.
            Err(_) => return Ok(PermissionAuthorizationStatus::NotDetermined),
        };
        let status = match decision {
            // Authorizes only this request; nothing is persisted, so the next
            // one prompts again.
            PermissionDecision::AllowOnce => return Ok(PermissionAuthorizationStatus::Authorized),
            PermissionDecision::AllowAlways => PermissionAuthorizationStatus::Authorized,
            PermissionDecision::Deny => PermissionAuthorizationStatus::Denied,
        };
        self.set_authorization_status(&request, status).await?;
        Ok(status)
    }

    /// Resolve the product's Chat authority grant, prompting once when no
    /// durable user decision exists.
    pub async fn check_or_prompt_chat_authority(
        &self,
    ) -> Result<PermissionAuthorizationStatus, GenericError>
    where
        P: UserConfirmation,
    {
        let request = PermissionAuthorizationRequest::ChatAuthority;
        let cached = self.authorization_status(&request).await?;
        if cached != PermissionAuthorizationStatus::NotDetermined {
            return Ok(cached);
        }
        let decision = match self
            .prompt
            .confirm_permission(UserConfirmationReview::ChatAuthority(ChatAuthorityReview {
                product_id: self.product_id().to_string(),
            }))
            .await
        {
            Ok(decision) => decision,
            Err(_) => return Ok(PermissionAuthorizationStatus::NotDetermined),
        };
        let status = match decision {
            PermissionDecision::AllowOnce => return Ok(PermissionAuthorizationStatus::Authorized),
            PermissionDecision::AllowAlways => PermissionAuthorizationStatus::Authorized,
            PermissionDecision::Deny => PermissionAuthorizationStatus::Denied,
        };
        self.set_authorization_status(&request, status).await?;
        Ok(status)
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
        let cached = self.authorization_status(&request).await?;
        if cached != PermissionAuthorizationStatus::NotDetermined {
            return Ok(cached);
        }
        let decision = match self
            .prompt
            .confirm_permission(UserConfirmationReview::ProfileDisclosure(
                ProfileDisclosureReview {
                    product_id: self.product_id().to_string(),
                },
            ))
            .await
        {
            Ok(decision) => decision,
            Err(_) => return Ok(PermissionAuthorizationStatus::NotDetermined),
        };
        let status = match decision {
            PermissionDecision::AllowOnce => return Ok(PermissionAuthorizationStatus::Authorized),
            PermissionDecision::AllowAlways => PermissionAuthorizationStatus::Authorized,
            PermissionDecision::Deny => PermissionAuthorizationStatus::Denied,
        };
        self.set_authorization_status(&request, status).await?;
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
            PermissionAuthorizationStatus::NotDetermined => {}
            decided => return Ok(decided),
        }
        // Only a genuine user authorization is persisted. A prompt-callback
        // error is transient (dismissed UI, unavailable UI, IPC timeout), not
        // a denial, so leave the authorization ask/default.
        let authorization = match self
            .prompt
            .device_permission(self.product, permission)
            .await
        {
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
        let _guard = self.temporary_permissions.authorization.lock().await;
        let Some(domains) = requested_domains(&request).map(<[String]>::to_vec) else {
            let key = CoreStorageKey::remote_permission_authorization(self.product_id(), &request);
            match self.cached_remote_authorization(&key, consume).await? {
                PermissionAuthorizationStatus::NotDetermined => {}
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
            if self
                .temporary_permissions
                .authorize_all(&temporary_keys, consume)
            {
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
            }
            // A denial answers only the question that was asked.
            PermissionDecision::Deny => {
                self.persist_decision(bundle_key, StoredAuthorizationStatus::Denied)
                    .await
            }
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
            }
            PermissionDecision::AllowAlways => {
                self.persist_decision(key, StoredAuthorizationStatus::Authorized)
                    .await
            }
            PermissionDecision::Deny => {
                self.persist_decision(key, StoredAuthorizationStatus::Denied)
                    .await
            }
        }
    }

    /// Persist a fresh user decision and return its public status.
    async fn persist_decision(
        &self,
        key: CoreStorageKey,
        authorization: StoredAuthorizationStatus,
    ) -> Result<PermissionAuthorizationStatus, GenericError> {
        self.storage
            .write_core_storage(key, authorization.encode())
            .await?;
        Ok(authorization.into())
    }
}

/// Stored decision on `caller_id` reaching `target_product_id`'s account.
pub async fn account_access_status<S: CoreStorage + ?Sized>(
    storage: &S,
    caller_id: &str,
    target_product_id: &str,
) -> Result<PermissionAuthorizationStatus, GenericError> {
    authorization_status(
        storage,
        CoreStorageKey::account_access_authorization(caller_id, target_product_id),
    )
    .await
}

/// Store a decision on `caller_id` reaching `target_product_id`'s account.
/// `NotDetermined` clears it.
pub async fn set_account_access_status<S: CoreStorage + ?Sized>(
    storage: &S,
    caller_id: &str,
    target_product_id: &str,
    status: PermissionAuthorizationStatus,
) -> Result<(), GenericError> {
    set_authorization_status(
        storage,
        CoreStorageKey::account_access_authorization(caller_id, target_product_id),
        status,
    )
    .await
}

async fn authorization_status<S: CoreStorage + ?Sized>(
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

async fn set_authorization_status<S: CoreStorage + ?Sized>(
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
mod tests {
    use super::*;
    use crate::test_support::StubPlatform;
    use futures::lock::Mutex;
    use std::collections::HashMap;
    use std::sync::LazyLock;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use truapi::latest::RemotePermission;
    use truapi::v01;
    use truapi::v01::GenericError;

    static PRODUCT: LazyLock<ProductContext> = LazyLock::new(|| {
        ProductContext::new("product.dot".to_string()).expect("product id is valid")
    });

    static OTHER_PRODUCT: LazyLock<ProductContext> = LazyLock::new(|| {
        ProductContext::new("other.dot".to_string()).expect("product id is valid")
    });

    static PEOPL_APP: LazyLock<ProductContext> = LazyLock::new(|| {
        ProductContext::new("app.peopl.dot".to_string()).expect("product id is valid")
    });

    static PEOPL: LazyLock<ProductContext> = LazyLock::new(|| {
        ProductContext::new("peopl.dot".to_string()).expect("product id is valid")
    });

    #[derive(Default)]
    struct MemStorage {
        inner: Mutex<HashMap<String, Vec<u8>>>,
    }

    #[truapi_platform::async_trait]
    impl CoreStorage for MemStorage {
        async fn read_core_storage(
            &self,
            key: CoreStorageKey,
        ) -> Result<Option<Vec<u8>>, v01::GenericError> {
            Ok(self.inner.lock().await.get(&test_key(key)).cloned())
        }
        async fn write_core_storage(
            &self,
            key: CoreStorageKey,
            value: Vec<u8>,
        ) -> Result<(), v01::GenericError> {
            self.inner.lock().await.insert(test_key(key), value);
            Ok(())
        }
        async fn clear_core_storage(&self, key: CoreStorageKey) -> Result<(), v01::GenericError> {
            self.inner.lock().await.remove(&test_key(key));
            Ok(())
        }
    }

    fn test_key(key: CoreStorageKey) -> String {
        hex::encode(key.encode())
    }

    #[test]
    fn concurrent_remote_operations_recheck_the_first_decision() {
        futures::executor::block_on(async {
            for request in [
                remote_domains(&["cdn.example.com"]),
                RemotePermissionRequest {
                    permission: RemotePermission::WebRtc,
                },
            ] {
                for (decision, first_status, second_status, calls) in [
                    (
                        PermissionDecision::AllowAlways,
                        PermissionAuthorizationStatus::Authorized,
                        PermissionAuthorizationStatus::Authorized,
                        1,
                    ),
                    (
                        PermissionDecision::Deny,
                        PermissionAuthorizationStatus::Denied,
                        PermissionAuthorizationStatus::Denied,
                        1,
                    ),
                    (
                        PermissionDecision::AllowOnce,
                        PermissionAuthorizationStatus::Authorized,
                        PermissionAuthorizationStatus::Denied,
                        2,
                    ),
                ] {
                    let storage = MemStorage::default();
                    let prompt =
                        ScriptedPrompt::decisions(vec![], vec![PermissionDecision::Deny, decision]);
                    let grants = Arc::default();
                    let sdk = PermissionsService::new(&storage, &prompt, &PRODUCT)
                        .with_temporary_permissions(Arc::clone(&grants));
                    let native = PermissionsService::new(&storage, &prompt, &PRODUCT)
                        .with_temporary_permissions(grants);
                    let pending_answer = prompt.remote_answers.lock().await;
                    let mut first = Box::pin(sdk.authorize_remote(request.clone()));
                    let mut second = Box::pin(native.authorize_remote(request.clone()));

                    assert!(futures::poll!(&mut first).is_pending());
                    assert!(futures::poll!(&mut second).is_pending());
                    drop(pending_answer);

                    assert_eq!(
                        (
                            first.await.unwrap(),
                            second.await.unwrap(),
                            prompt.remote_calls.load(Ordering::SeqCst),
                        ),
                        (first_status, second_status, calls),
                    );
                }
            }
        });
    }

    #[test]
    fn concurrent_device_operations_reuse_a_persisted_decision() {
        futures::executor::block_on(async {
            for (decision, expected) in [
                (
                    PermissionDecision::AllowAlways,
                    PermissionAuthorizationStatus::Authorized,
                ),
                (
                    PermissionDecision::Deny,
                    PermissionAuthorizationStatus::Denied,
                ),
            ] {
                let storage = MemStorage::default();
                let prompt =
                    ScriptedPrompt::decisions(vec![PermissionDecision::Deny, decision], vec![]);
                let service = PermissionsService::new(&storage, &prompt, &PRODUCT);
                let pending_answer = prompt.device_answers.lock().await;
                let mut first =
                    Box::pin(service.authorize_device(HostDevicePermissionRequest::Camera));
                let mut second =
                    Box::pin(service.authorize_device(HostDevicePermissionRequest::Camera));

                assert!(futures::poll!(&mut first).is_pending());
                assert!(futures::poll!(&mut second).is_pending());
                drop(pending_answer);

                assert_eq!(
                    (
                        first.await.unwrap(),
                        second.await.unwrap(),
                        prompt.device_calls.load(Ordering::SeqCst),
                    ),
                    (expected, expected, 1),
                );
            }
        });
    }

    #[test]
    fn cancelling_a_prompt_releases_waiting_operations_without_blocking_other_products() {
        futures::executor::block_on(async {
            let storage = MemStorage::default();
            let prompt = ScriptedPrompt::decisions(vec![], vec![PermissionDecision::AllowAlways]);
            let service = PermissionsService::new(&storage, &prompt, &PRODUCT);
            let request = remote_domains(&["cdn.example.com"]);
            let pending_answer = prompt.remote_answers.lock().await;
            let mut cancelled = Box::pin(service.authorize_remote(request.clone()));
            let mut waiting = Box::pin(service.authorize_remote(request.clone()));
            assert!(futures::poll!(&mut cancelled).is_pending());
            assert!(futures::poll!(&mut waiting).is_pending());

            let other_prompt =
                ScriptedPrompt::decisions(vec![], vec![PermissionDecision::AllowAlways]);
            let other = PermissionsService::new(&storage, &other_prompt, &OTHER_PRODUCT);
            let mut independent = Box::pin(other.authorize_remote(request));
            assert_eq!(
                futures::poll!(&mut independent),
                std::task::Poll::Ready(Ok(PermissionAuthorizationStatus::Authorized)),
            );

            drop(cancelled);
            drop(pending_answer);
            assert_eq!(
                (
                    waiting.await.unwrap(),
                    prompt.remote_calls.load(Ordering::SeqCst)
                ),
                (PermissionAuthorizationStatus::Authorized, 2),
            );
        });
    }

    #[test]
    fn an_upfront_one_use_grant_is_shared_with_one_operation_but_never_persisted() {
        futures::executor::block_on(async {
            let storage = MemStorage::default();
            let prompt = ScriptedPrompt::decisions(
                vec![],
                vec![PermissionDecision::Deny, PermissionDecision::AllowOnce],
            );
            let grants = Arc::default();
            let sdk = PermissionsService::new(&storage, &prompt, &PRODUCT)
                .with_temporary_permissions(Arc::clone(&grants));
            let operation = PermissionsService::new(&storage, &prompt, &PRODUCT)
                .with_temporary_permissions(Arc::clone(&grants));
            let other_product = PermissionsService::new(&storage, &prompt, &OTHER_PRODUCT)
                .with_temporary_permissions(grants);
            let other_execution = PermissionsService::new(&storage, &prompt, &PRODUCT);
            let request = remote_domains(&["api.example.com"]);
            let requested = sdk.check_or_prompt_remote(request.clone()).await.unwrap();
            assert_eq!(
                (
                    requested,
                    sdk.peek_remote(&request).await.unwrap(),
                    other_product.peek_remote(&request).await.unwrap(),
                    other_execution.peek_remote(&request).await.unwrap(),
                    storage.inner.lock().await.len(),
                ),
                (
                    PermissionAuthorizationStatus::Authorized,
                    PermissionAuthorizationStatus::Authorized,
                    PermissionAuthorizationStatus::NotDetermined,
                    PermissionAuthorizationStatus::NotDetermined,
                    0,
                ),
            );
            let first = operation.authorize_remote(request.clone()).await.unwrap();
            let second = operation.authorize_remote(request).await.unwrap();
            assert_eq!(
                (first, second, prompt.remote_calls.load(Ordering::SeqCst)),
                (
                    PermissionAuthorizationStatus::Authorized,
                    PermissionAuthorizationStatus::Denied,
                    2,
                ),
            );
        });
    }

    #[test]
    fn a_denied_bundle_preserves_one_use_grants_for_a_later_operation() {
        futures::executor::block_on(async {
            for prompt_denial in [false, true] {
                let storage = MemStorage::default();
                let prompt = ScriptedPrompt::decisions(
                    vec![],
                    vec![PermissionDecision::Deny, PermissionDecision::AllowOnce],
                );
                let service = PermissionsService::new(&storage, &prompt, &PRODUCT);
                let granted = remote_domains(&["api.example.com"]);
                service
                    .check_or_prompt_remote(granted.clone())
                    .await
                    .unwrap();
                if !prompt_denial {
                    service
                        .set_authorization_status(
                            &PermissionAuthorizationRequest::Remote(remote_domains(&[
                                "blocked.com",
                            ])),
                            PermissionAuthorizationStatus::Denied,
                        )
                        .await
                        .unwrap();
                }
                let denied = service
                    .authorize_remote(remote_domains(&["api.example.com", "blocked.com"]))
                    .await
                    .unwrap();
                let next = service
                    .authorize_remote(remote_domains(&["api.example.com", "API.EXAMPLE.COM."]))
                    .await
                    .unwrap();
                assert_eq!(
                    (denied, next, service.peek_remote(&granted).await.unwrap()),
                    (
                        PermissionAuthorizationStatus::Denied,
                        PermissionAuthorizationStatus::Authorized,
                        PermissionAuthorizationStatus::NotDetermined,
                    ),
                );
            }
        });
    }

    #[test]
    fn concurrent_operations_cannot_share_a_one_use_wildcard_grant() {
        futures::executor::block_on(async {
            let storage = MemStorage::default();
            let prompt = ScriptedPrompt::decisions(
                vec![],
                vec![PermissionDecision::Deny, PermissionDecision::AllowOnce],
            );
            let service = PermissionsService::new(&storage, &prompt, &PRODUCT);
            service
                .check_or_prompt_remote(remote_domains(&["*.example.com"]))
                .await
                .unwrap();
            let (first, second) = futures::join!(
                service.authorize_remote(remote_domains(&["deep.a.example.com"])),
                service.authorize_remote(remote_domains(&["deeper.deep.b.example.com"])),
            );
            assert_eq!(
                (
                    first.unwrap(),
                    second.unwrap(),
                    prompt.remote_calls.load(Ordering::SeqCst)
                ),
                (
                    PermissionAuthorizationStatus::Authorized,
                    PermissionAuthorizationStatus::Denied,
                    2,
                ),
            );
        });
    }

    #[test]
    fn a_one_use_answer_to_an_operation_is_consumed_by_that_operation() {
        futures::executor::block_on(async {
            let storage = MemStorage::default();
            let prompt = ScriptedPrompt::decisions(
                vec![PermissionDecision::AllowOnce],
                vec![PermissionDecision::AllowOnce],
            );
            let service = PermissionsService::new(&storage, &prompt, &PRODUCT);
            let remote = remote_domains(&["api.example.com"]);
            let device = HostDevicePermissionRequest::Camera;
            assert_eq!(
                (
                    service.authorize_remote(remote.clone()).await.unwrap(),
                    service.peek_remote(&remote).await.unwrap(),
                    service.authorize_device(device).await.unwrap(),
                    service.peek_device(&device).await.unwrap(),
                    storage.inner.lock().await.len(),
                ),
                (
                    PermissionAuthorizationStatus::Authorized,
                    PermissionAuthorizationStatus::NotDetermined,
                    PermissionAuthorizationStatus::Authorized,
                    PermissionAuthorizationStatus::NotDetermined,
                    0,
                ),
            );
        });
    }

    #[test]
    fn blessed_domains_need_no_prompt_but_explicit_denials_still_apply() {
        futures::executor::block_on(async {
            let storage = MemStorage::default();
            let prompt = ScriptedPrompt::new(vec![], vec![]);
            let service = PermissionsService::new(&storage, &prompt, &PRODUCT);
            for domain in BLESSED_REMOTE_DOMAINS {
                let request = remote_domains(&[domain]);
                let granted = service.authorize_remote(request.clone()).await.unwrap();
                service
                    .set_authorization_status(
                        &PermissionAuthorizationRequest::Remote(request.clone()),
                        PermissionAuthorizationStatus::Denied,
                    )
                    .await
                    .unwrap();
                assert_eq!(
                    (granted, service.authorize_remote(request).await.unwrap()),
                    (
                        PermissionAuthorizationStatus::Authorized,
                        PermissionAuthorizationStatus::Denied,
                    ),
                );
            }
            assert_eq!(prompt.remote_calls.load(Ordering::SeqCst), 0);
        });
    }

    #[test]
    fn blessed_domains_do_not_consume_a_one_use_grant_for_other_hosts() {
        futures::executor::block_on(async {
            let storage = MemStorage::default();
            let prompt = ScriptedPrompt::decisions(vec![], vec![PermissionDecision::AllowOnce]);
            let service = PermissionsService::new(&storage, &prompt, &PRODUCT);
            let wildcard = remote_domains(&["*"]);
            service
                .check_or_prompt_remote(wildcard.clone())
                .await
                .unwrap();
            let blessed = service
                .authorize_remote(remote_domains(&[BLESSED_REMOTE_DOMAINS[0]]))
                .await
                .unwrap();
            let still_available = service.peek_remote(&wildcard).await.unwrap();
            let other = service
                .authorize_remote(remote_domains(&["other.com"]))
                .await
                .unwrap();
            assert_eq!(
                (
                    blessed,
                    still_available,
                    other,
                    service.peek_remote(&wildcard).await.unwrap()
                ),
                (
                    PermissionAuthorizationStatus::Authorized,
                    PermissionAuthorizationStatus::Authorized,
                    PermissionAuthorizationStatus::Authorized,
                    PermissionAuthorizationStatus::NotDetermined,
                ),
            );
        });
    }

    #[test]
    fn a_one_use_exception_to_a_denial_is_consumed_even_for_a_blessed_domain() {
        futures::executor::block_on(async {
            let storage = MemStorage::default();
            let prompt = ScriptedPrompt::decisions(vec![], vec![PermissionDecision::AllowOnce]);
            let service = PermissionsService::new(&storage, &prompt, &PRODUCT);
            service
                .check_or_prompt_remote(remote_domains(&["*.googleapis.com"]))
                .await
                .unwrap();
            service
                .set_authorization_status(
                    &PermissionAuthorizationRequest::Remote(remote_domains(&["*"])),
                    PermissionAuthorizationStatus::Denied,
                )
                .await
                .unwrap();
            let request = remote_domains(&["fonts.googleapis.com"]);
            assert_eq!(
                (
                    service.authorize_remote(request.clone()).await.unwrap(),
                    service.authorize_remote(request).await.unwrap(),
                    prompt.remote_calls.load(Ordering::SeqCst),
                ),
                (
                    PermissionAuthorizationStatus::Authorized,
                    PermissionAuthorizationStatus::Denied,
                    1,
                ),
            );
        });
    }

    #[test]
    fn resetting_a_permission_removes_its_temporary_grant() {
        futures::executor::block_on(async {
            let storage = MemStorage::default();
            let prompt = ScriptedPrompt::decisions(vec![], vec![PermissionDecision::AllowOnce]);
            let service = PermissionsService::new(&storage, &prompt, &PRODUCT);
            let request = remote_domains(&["api.example.com"]);
            service
                .check_or_prompt_remote(request.clone())
                .await
                .unwrap();
            service
                .set_authorization_status(
                    &PermissionAuthorizationRequest::Remote(request.clone()),
                    PermissionAuthorizationStatus::NotDetermined,
                )
                .await
                .unwrap();
            assert_eq!(
                service.peek_remote(&request).await.unwrap(),
                PermissionAuthorizationStatus::NotDetermined
            );
        });
    }

    struct ScriptedPrompt {
        device_answers: Mutex<Vec<PermissionDecision>>,
        remote_answers: Mutex<Vec<PermissionDecision>>,
        device_calls: AtomicUsize,
        remote_calls: AtomicUsize,
        /// Domain bundles the remote callback was actually asked about, in call
        /// order, so a test can assert which subset reached the user.
        remote_domains_asked: Mutex<Vec<Vec<String>>>,
    }

    impl ScriptedPrompt {
        fn new(device_answers: Vec<bool>, remote_answers: Vec<bool>) -> Self {
            let decision = |granted| {
                if granted {
                    PermissionDecision::AllowAlways
                } else {
                    PermissionDecision::Deny
                }
            };
            Self::decisions(
                device_answers.into_iter().map(decision).collect(),
                remote_answers.into_iter().map(decision).collect(),
            )
        }

        fn decisions(
            device_answers: Vec<PermissionDecision>,
            remote_answers: Vec<PermissionDecision>,
        ) -> Self {
            Self {
                device_answers: Mutex::new(device_answers),
                remote_answers: Mutex::new(remote_answers),
                device_calls: AtomicUsize::new(0),
                remote_calls: AtomicUsize::new(0),
                remote_domains_asked: Mutex::new(Vec::new()),
            }
        }

        fn domains_asked(&self) -> Vec<Vec<String>> {
            futures::executor::block_on(self.remote_domains_asked.lock()).clone()
        }
    }

    #[truapi_platform::async_trait]
    impl Permissions for ScriptedPrompt {
        async fn device_permission(
            &self,
            _product: &ProductContext,
            _request: HostDevicePermissionRequest,
        ) -> Result<PermissionDecision, GenericError> {
            self.device_calls.fetch_add(1, Ordering::SeqCst);
            let decision = self
                .device_answers
                .lock()
                .await
                .pop()
                .expect("ScriptedPrompt ran out of device answers");
            Ok(decision)
        }

        async fn remote_permission(
            &self,
            _product: &ProductContext,
            request: RemotePermissionRequest,
        ) -> Result<PermissionDecision, GenericError> {
            self.remote_calls.fetch_add(1, Ordering::SeqCst);
            if let RemotePermission::Remote { domains } = &request.permission {
                self.remote_domains_asked.lock().await.push(domains.clone());
            }
            let decision = self
                .remote_answers
                .lock()
                .await
                .pop()
                .expect("ScriptedPrompt ran out of remote answers");
            Ok(decision)
        }
    }

    /// OS status source, scripted per capability so a test can deny one
    /// capability while another stays granted.
    struct ScriptedStatus {
        answers: Vec<(HostDevicePermissionRequest, DevicePermissionStatus)>,
        /// Answer for any capability `answers` does not name. `None` fails the
        /// query instead, standing in for a busy host or a dropped IPC.
        fallback: Option<DevicePermissionStatus>,
        asked: Mutex<Vec<HostDevicePermissionRequest>>,
    }

    impl ScriptedStatus {
        fn always(status: DevicePermissionStatus) -> Self {
            Self {
                answers: Vec::new(),
                fallback: Some(status),
                asked: Mutex::new(Vec::new()),
            }
        }

        fn per_capability(
            answers: Vec<(HostDevicePermissionRequest, DevicePermissionStatus)>,
            fallback: DevicePermissionStatus,
        ) -> Self {
            Self {
                answers,
                fallback: Some(fallback),
                asked: Mutex::new(Vec::new()),
            }
        }

        fn failing() -> Self {
            Self {
                answers: Vec::new(),
                fallback: None,
                asked: Mutex::new(Vec::new()),
            }
        }

        fn asked(&self) -> Vec<HostDevicePermissionRequest> {
            futures::executor::block_on(self.asked.lock()).clone()
        }
    }

    #[truapi_platform::async_trait]
    impl PermissionStatusHost for ScriptedStatus {
        async fn device_permission_status(
            &self,
            request: HostDevicePermissionRequest,
        ) -> Result<DevicePermissionStatus, GenericError> {
            self.asked.lock().await.push(request);
            if let Some((_, status)) = self
                .answers
                .iter()
                .find(|(capability, _)| *capability == request)
            {
                return Ok(*status);
            }
            self.fallback.ok_or_else(|| v01::GenericError {
                reason: "status channel unavailable".to_string(),
            })
        }
    }

    /// Persist a product-scoped grant for `capability` the way a first
    /// successful request does, with no OS status source involved.
    fn grant_stored(storage: &MemStorage, capability: HostDevicePermissionRequest) {
        let prompt = ScriptedPrompt::new(vec![true], vec![]);
        let service = PermissionsService::new(storage, &prompt, &PRODUCT);
        assert_eq!(
            futures::executor::block_on(service.check_or_prompt_device(capability)).unwrap(),
            PermissionAuthorizationStatus::Authorized,
        );
    }

    fn remote_domains(domains: &[&str]) -> RemotePermissionRequest {
        RemotePermissionRequest {
            permission: RemotePermission::Remote {
                domains: domains.iter().map(|domain| domain.to_string()).collect(),
            },
        }
    }

    #[test]
    fn check_or_prompt_device_caches_grant() {
        let storage = MemStorage::default();
        let prompt = ScriptedPrompt::new(vec![true], vec![]);
        let service = PermissionsService::new(&storage, &prompt, &PRODUCT);

        let first = futures::executor::block_on(
            service.check_or_prompt_device(HostDevicePermissionRequest::Camera),
        )
        .unwrap();
        let second = futures::executor::block_on(
            service.check_or_prompt_device(HostDevicePermissionRequest::Camera),
        )
        .unwrap();

        assert_eq!(first, PermissionAuthorizationStatus::Authorized);
        assert_eq!(second, PermissionAuthorizationStatus::Authorized);
        assert_eq!(prompt.device_calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn check_or_prompt_remote_caches_denial() {
        let storage = MemStorage::default();
        let prompt = ScriptedPrompt::new(vec![], vec![false]);
        let service = PermissionsService::new(&storage, &prompt, &PRODUCT);

        let request = RemotePermissionRequest {
            permission: RemotePermission::ChainSubmit,
        };
        let first =
            futures::executor::block_on(service.check_or_prompt_remote(request.clone())).unwrap();
        let second = futures::executor::block_on(service.check_or_prompt_remote(request)).unwrap();

        assert_eq!(first, PermissionAuthorizationStatus::Denied);
        assert_eq!(second, PermissionAuthorizationStatus::Denied);
        assert_eq!(prompt.remote_calls.load(Ordering::SeqCst), 1);
    }

    /// The defect this storage model exists to fix: a product grants a bundle,
    /// then enforcement asks about one host in it. Keying the bundle as a set
    /// made that lookup miss and re-prompt, so a granted domain read as
    /// undecided forever.
    #[test]
    fn a_bundle_grant_is_visible_to_a_single_domain_lookup() {
        let storage = MemStorage::default();
        let prompt = ScriptedPrompt::new(vec![], vec![true]);
        let service = PermissionsService::new(&storage, &prompt, &PRODUCT);

        let granted = futures::executor::block_on(
            service.check_or_prompt_remote(remote_domains(&["a.example.com", "b.example.com"])),
        )
        .unwrap();
        assert_eq!(granted, PermissionAuthorizationStatus::Authorized);

        for domain in ["a.example.com", "b.example.com"] {
            assert_eq!(
                futures::executor::block_on(service.peek_remote(&remote_domains(&[domain])))
                    .unwrap(),
                PermissionAuthorizationStatus::Authorized,
                "{domain} was granted as part of the bundle"
            );
            assert_eq!(
                futures::executor::block_on(
                    service.check_or_prompt_remote(remote_domains(&[domain]))
                )
                .unwrap(),
                PermissionAuthorizationStatus::Authorized,
            );
        }
        assert_eq!(
            prompt.remote_calls.load(Ordering::SeqCst),
            1,
            "a domain already covered by the bundle must not re-prompt"
        );
    }

    #[test]
    fn a_wildcard_grant_covers_descendants_but_not_its_root() {
        let storage = MemStorage::default();
        let prompt = ScriptedPrompt::new(vec![], vec![true]);
        let service = PermissionsService::new(&storage, &prompt, &PRODUCT);

        futures::executor::block_on(
            service.check_or_prompt_remote(remote_domains(&["*.example.com"])),
        )
        .unwrap();

        for covered in [
            "api.example.com",
            "deep.api.example.com",
            "deeper.deep.api.example.com",
        ] {
            assert_eq!(
                futures::executor::block_on(service.peek_remote(&remote_domains(&[covered])))
                    .unwrap(),
                PermissionAuthorizationStatus::Authorized,
                "{covered} is a descendant of example.com"
            );
        }
        for uncovered in ["example.com", "notexample.com", "example.com.other"] {
            assert_eq!(
                futures::executor::block_on(service.peek_remote(&remote_domains(&[uncovered])))
                    .unwrap(),
                PermissionAuthorizationStatus::NotDetermined,
                "{uncovered} is not a descendant of example.com"
            );
        }
    }

    #[test]
    fn the_most_specific_stored_decision_wins() {
        let storage = MemStorage::default();
        let prompt = ScriptedPrompt::new(vec![], vec![]);
        let service = PermissionsService::new(&storage, &prompt, &PRODUCT);

        // Deny the whole wildcard, then allow one host under it explicitly.
        futures::executor::block_on(service.set_authorization_status(
            &PermissionAuthorizationRequest::Remote(remote_domains(&["*.example.com"])),
            PermissionAuthorizationStatus::Denied,
        ))
        .unwrap();
        futures::executor::block_on(service.set_authorization_status(
            &PermissionAuthorizationRequest::Remote(remote_domains(&["api.example.com"])),
            PermissionAuthorizationStatus::Authorized,
        ))
        .unwrap();

        assert_eq!(
            futures::executor::block_on(service.peek_remote(&remote_domains(&["api.example.com"])))
                .unwrap(),
            PermissionAuthorizationStatus::Authorized,
            "an explicit host grant outranks a denied parent wildcard"
        );
        assert_eq!(
            futures::executor::block_on(
                service.peek_remote(&remote_domains(&["other.example.com"]))
            )
            .unwrap(),
            PermissionAuthorizationStatus::Denied,
            "a host with no decision of its own inherits the wildcard denial"
        );

        futures::executor::block_on(service.set_authorization_status(
            &PermissionAuthorizationRequest::Remote(remote_domains(&["*.example.com"])),
            PermissionAuthorizationStatus::Authorized,
        ))
        .unwrap();
        futures::executor::block_on(service.set_authorization_status(
            &PermissionAuthorizationRequest::Remote(remote_domains(&["*.api.example.com"])),
            PermissionAuthorizationStatus::Denied,
        ))
        .unwrap();
        for denied in ["deep.api.example.com", "deeper.deep.api.example.com"] {
            assert_eq!(
                futures::executor::block_on(service.peek_remote(&remote_domains(&[denied])))
                    .unwrap(),
                PermissionAuthorizationStatus::Denied,
                "the narrower wildcard denial overrides the ancestor's grant"
            );
        }
    }

    #[test]
    fn a_prompt_covers_only_undetermined_domains_and_never_revokes_a_grant() {
        let storage = MemStorage::default();
        // Answers pop from the end: grant first, then deny.
        let prompt = ScriptedPrompt::new(vec![], vec![false, true]);
        let service = PermissionsService::new(&storage, &prompt, &PRODUCT);

        futures::executor::block_on(service.check_or_prompt_remote(remote_domains(&["a.com"])))
            .unwrap();
        let second = futures::executor::block_on(
            service.check_or_prompt_remote(remote_domains(&["a.com", "b.com"])),
        )
        .unwrap();

        assert_eq!(
            prompt.domains_asked(),
            vec![vec!["a.com".to_string()], vec!["b.com".to_string()]],
            "the second prompt must ask only about the undecided domain"
        );
        assert_eq!(
            second,
            PermissionAuthorizationStatus::Denied,
            "the bundle needs every domain, so one denial denies it"
        );
        assert_eq!(
            futures::executor::block_on(service.peek_remote(&remote_domains(&["a.com"]))).unwrap(),
            PermissionAuthorizationStatus::Authorized,
            "denying b.com must not revoke the existing a.com grant"
        );
    }

    #[test]
    fn a_multi_domain_denial_leaves_the_narrower_question_askable() {
        let storage = MemStorage::default();
        // Answers pop from the end: deny the pair, then grant the single domain.
        let prompt = ScriptedPrompt::new(vec![], vec![true, false]);
        let service = PermissionsService::new(&storage, &prompt, &PRODUCT);

        let pair = remote_domains(&["api.coingecko.com", "analytics.vendor.com"]);
        assert_eq!(
            futures::executor::block_on(service.check_or_prompt_remote(pair.clone())).unwrap(),
            PermissionAuthorizationStatus::Denied
        );
        assert_eq!(
            futures::executor::block_on(service.check_or_prompt_remote(pair.clone())).unwrap(),
            PermissionAuthorizationStatus::Denied,
        );
        assert_eq!(
            prompt.remote_calls.load(Ordering::SeqCst),
            1,
            "the refused set is answered and must not be re-asked"
        );

        // Refusing the pair is not refusing either domain on its own: that is a
        // question the user was never put, so it stays open.
        for domain in ["api.coingecko.com", "analytics.vendor.com"] {
            assert_eq!(
                futures::executor::block_on(service.peek_remote(&remote_domains(&[domain])))
                    .unwrap(),
                PermissionAuthorizationStatus::NotDetermined,
                "{domain} was never refused by itself"
            );
        }
        assert_eq!(
            futures::executor::block_on(
                service.check_or_prompt_remote(remote_domains(&["api.coingecko.com"]))
            )
            .unwrap(),
            PermissionAuthorizationStatus::Authorized,
        );
        assert_eq!(
            prompt.domains_asked(),
            vec![
                vec![
                    "api.coingecko.com".to_string(),
                    "analytics.vendor.com".to_string(),
                ],
                vec!["api.coingecko.com".to_string()],
            ],
        );
    }

    #[test]
    fn unsupported_wildcards_are_rejected_without_prompting_or_storing() {
        let storage = MemStorage::default();
        let prompt = ScriptedPrompt::new(vec![], vec![]);
        let service = PermissionsService::new(&storage, &prompt, &PRODUCT);

        for domain in ["*.com", "*.dot", "*.127.0.0.1", "*.[::1]"] {
            assert_eq!(
                futures::executor::block_on(
                    service.check_or_prompt_remote(remote_domains(&[domain]))
                )
                .unwrap(),
                PermissionAuthorizationStatus::Denied,
            );
        }
        assert_eq!(
            (
                prompt.remote_calls.load(Ordering::SeqCst),
                futures::executor::block_on(storage.inner.lock()).len()
            ),
            (0, 0),
        );
    }

    #[test]
    fn a_grant_covers_every_spelling_of_the_granted_host() {
        let storage = MemStorage::default();
        let prompt = ScriptedPrompt::new(vec![], vec![true]);
        let service = PermissionsService::new(&storage, &prompt, &PRODUCT);

        futures::executor::block_on(
            service.check_or_prompt_remote(remote_domains(&["Bücher.example"])),
        )
        .unwrap();

        // Enforcement normalizes a live URL host the same way the grant was
        // keyed, so no spelling of the same site opens a second prompt.
        for spelling in [
            "bücher.example",
            "xn--bcher-kva.example",
            "XN--BCHER-KVA.example.",
        ] {
            assert_eq!(
                futures::executor::block_on(service.peek_remote(&remote_domains(&[spelling])))
                    .unwrap(),
                PermissionAuthorizationStatus::Authorized,
                "{spelling} is the granted host"
            );
        }
        assert_eq!(prompt.remote_calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_denied_domain_short_circuits_the_bundle_without_prompting() {
        let storage = MemStorage::default();
        let prompt = ScriptedPrompt::new(vec![], vec![false]);
        let service = PermissionsService::new(&storage, &prompt, &PRODUCT);

        futures::executor::block_on(service.check_or_prompt_remote(remote_domains(&["a.com"])))
            .unwrap();
        let second = futures::executor::block_on(
            service.check_or_prompt_remote(remote_domains(&["a.com", "b.com"])),
        )
        .unwrap();

        assert_eq!(second, PermissionAuthorizationStatus::Denied);
        assert_eq!(
            prompt.remote_calls.load(Ordering::SeqCst),
            1,
            "a stored denial is not re-asked"
        );
    }

    #[test]
    fn an_empty_domain_bundle_is_denied_without_prompting() {
        let storage = MemStorage::default();
        let prompt = ScriptedPrompt::new(vec![], vec![]);
        let service = PermissionsService::new(&storage, &prompt, &PRODUCT);

        assert_eq!(
            futures::executor::block_on(service.check_or_prompt_remote(remote_domains(&[])))
                .unwrap(),
            PermissionAuthorizationStatus::Denied
        );
        assert_eq!(
            futures::executor::block_on(service.peek_remote(&remote_domains(&[]))).unwrap(),
            PermissionAuthorizationStatus::Denied
        );
        assert_eq!(prompt.remote_calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn clearing_a_bundle_clears_each_domain_it_names() {
        let storage = MemStorage::default();
        let prompt = ScriptedPrompt::new(vec![], vec![true]);
        let service = PermissionsService::new(&storage, &prompt, &PRODUCT);

        futures::executor::block_on(
            service.check_or_prompt_remote(remote_domains(&["a.com", "b.com"])),
        )
        .unwrap();
        futures::executor::block_on(service.set_authorization_status(
            &PermissionAuthorizationRequest::Remote(remote_domains(&["a.com", "b.com"])),
            PermissionAuthorizationStatus::NotDetermined,
        ))
        .unwrap();

        for domain in ["a.com", "b.com"] {
            assert_eq!(
                futures::executor::block_on(service.peek_remote(&remote_domains(&[domain])))
                    .unwrap(),
                PermissionAuthorizationStatus::NotDetermined,
                "{domain} must be revocable through the bundle it was granted in"
            );
        }
    }

    #[test]
    fn resetting_a_bundle_clears_a_recorded_denial() {
        let storage = MemStorage::default();
        let prompt = ScriptedPrompt::new(vec![], vec![false]);
        let service = PermissionsService::new(&storage, &prompt, &PRODUCT);
        let pair = remote_domains(&["a.com", "b.com"]);

        futures::executor::block_on(service.check_or_prompt_remote(pair.clone())).unwrap();
        futures::executor::block_on(service.set_authorization_status(
            &PermissionAuthorizationRequest::Remote(pair.clone()),
            PermissionAuthorizationStatus::NotDetermined,
        ))
        .unwrap();

        assert_eq!(
            futures::executor::block_on(service.peek_remote(&pair)).unwrap(),
            PermissionAuthorizationStatus::NotDetermined,
            "the host's reset must also reach the slot a set denial lives in"
        );
    }

    #[test]
    fn remote_domain_grants_are_scoped_to_one_product() {
        let storage = MemStorage::default();
        let prompt = ScriptedPrompt::new(vec![], vec![true]);
        let service = PermissionsService::new(&storage, &prompt, &PRODUCT);
        futures::executor::block_on(service.check_or_prompt_remote(remote_domains(&["a.com"])))
            .unwrap();

        let other = PermissionsService::new(&storage, &prompt, &OTHER_PRODUCT);
        assert_eq!(
            futures::executor::block_on(other.peek_remote(&remote_domains(&["a.com"]))).unwrap(),
            PermissionAuthorizationStatus::NotDetermined
        );
    }

    /// A trusted product, so `ScriptedPrompt` is built with no scripted answers:
    /// reaching either callback panics rather than silently answering.
    fn trusted_service<'a>(
        storage: &'a MemStorage,
        prompt: &'a ScriptedPrompt,
    ) -> PermissionsService<'a, MemStorage, ScriptedPrompt> {
        PermissionsService::new(storage, prompt, &PEOPL)
    }

    fn remote(permission: RemotePermission) -> RemotePermissionRequest {
        RemotePermissionRequest { permission }
    }

    fn every_remote_permission() -> Vec<RemotePermission> {
        vec![
            RemotePermission::Remote {
                domains: vec!["example.com".to_string()],
            },
            RemotePermission::WebRtc,
            RemotePermission::ChainSubmit,
            RemotePermission::PreimageSubmit,
            RemotePermission::StatementSubmit,
        ]
    }

    #[test]
    fn a_trusted_product_holds_every_remote_permission_without_prompting() {
        let storage = MemStorage::default();
        let prompt = ScriptedPrompt::new(vec![], vec![]);
        let service = trusted_service(&storage, &prompt);

        for permission in every_remote_permission() {
            assert_eq!(
                futures::executor::block_on(
                    service.check_or_prompt_remote(remote(permission.clone()))
                )
                .unwrap(),
                PermissionAuthorizationStatus::Authorized,
                "{permission:?} must be granted to a trusted product without a prompt"
            );
        }
        assert_eq!(prompt.remote_calls.load(Ordering::SeqCst), 0);
        assert!(prompt.domains_asked().is_empty());
    }

    #[test]
    fn a_trusted_product_is_authorized_for_any_domain() {
        let storage = MemStorage::default();
        let prompt = ScriptedPrompt::new(vec![], vec![]);
        let service = trusted_service(&storage, &prompt);

        for domains in [
            vec!["a.com"],
            vec!["deep.api.example.com"],
            vec!["*"],
            vec!["a.com", "b.com", "*.c.com"],
        ] {
            assert_eq!(
                futures::executor::block_on(service.peek_remote(&remote_domains(&domains)))
                    .unwrap(),
                PermissionAuthorizationStatus::Authorized,
                "{domains:?} must be authorized for a trusted product"
            );
        }
    }

    #[test]
    fn a_trusted_product_reports_authorized_to_the_admin_surface() {
        let storage = MemStorage::default();
        let prompt = ScriptedPrompt::new(vec![], vec![]);
        let service = trusted_service(&storage, &prompt);

        for request in [
            remote(RemotePermission::ChainSubmit),
            remote_domains(&["a.com"]),
        ] {
            assert_eq!(
                futures::executor::block_on(
                    service.authorization_status(&PermissionAuthorizationRequest::Remote(request))
                )
                .unwrap(),
                PermissionAuthorizationStatus::Authorized
            );
        }
    }

    #[test]
    fn a_trusted_product_grant_is_not_persisted() {
        let storage = MemStorage::default();
        let prompt = ScriptedPrompt::new(vec![], vec![]);
        let service = trusted_service(&storage, &prompt);
        let request = remote(RemotePermission::ChainSubmit);

        futures::executor::block_on(service.check_or_prompt_remote(request.clone())).unwrap();

        assert_eq!(
            futures::executor::block_on(storage.read_core_storage(
                CoreStorageKey::remote_permission_authorization("peopl.dot", &request)
            ))
            .unwrap(),
            None,
            "an auto-granted permission must leave the slot free for a later user decision"
        );
    }

    #[test]
    fn a_stored_denial_outranks_a_trusted_product_grant() {
        let storage = MemStorage::default();
        let prompt = ScriptedPrompt::new(vec![], vec![]);
        let service = trusted_service(&storage, &prompt);

        for request in [
            remote(RemotePermission::ChainSubmit),
            remote_domains(&["a.com"]),
        ] {
            futures::executor::block_on(service.set_authorization_status(
                &PermissionAuthorizationRequest::Remote(request.clone()),
                PermissionAuthorizationStatus::Denied,
            ))
            .unwrap();

            assert_eq!(
                futures::executor::block_on(service.peek_remote(&request)).unwrap(),
                PermissionAuthorizationStatus::Denied
            );
            assert_eq!(
                futures::executor::block_on(service.check_or_prompt_remote(request)).unwrap(),
                PermissionAuthorizationStatus::Denied
            );
        }
        assert_eq!(prompt.remote_calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn a_wildcard_denial_revokes_every_domain_for_a_trusted_product() {
        let storage = MemStorage::default();
        let prompt = ScriptedPrompt::new(vec![], vec![]);
        let service = trusted_service(&storage, &prompt);

        futures::executor::block_on(service.set_authorization_status(
            &PermissionAuthorizationRequest::Remote(remote_domains(&["*"])),
            PermissionAuthorizationStatus::Denied,
        ))
        .unwrap();

        for domain in ["a.com", "deep.api.example.com"] {
            assert_eq!(
                futures::executor::block_on(service.peek_remote(&remote_domains(&[domain])))
                    .unwrap(),
                PermissionAuthorizationStatus::Denied,
                "the wildcard denial must be how a trusted product's domain access is revoked"
            );
        }
    }

    #[test]
    fn clearing_a_denial_restores_a_trusted_product_grant() {
        let storage = MemStorage::default();
        let prompt = ScriptedPrompt::new(vec![], vec![]);
        let service = trusted_service(&storage, &prompt);
        let request = PermissionAuthorizationRequest::Remote(remote(RemotePermission::ChainSubmit));

        for status in [
            PermissionAuthorizationStatus::Denied,
            PermissionAuthorizationStatus::NotDetermined,
        ] {
            futures::executor::block_on(service.set_authorization_status(&request, status))
                .unwrap();
        }

        assert_eq!(
            futures::executor::block_on(service.authorization_status(&request)).unwrap(),
            PermissionAuthorizationStatus::Authorized
        );
        assert_eq!(prompt.remote_calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn a_trusted_label_on_every_product_network_is_trusted() {
        let storage = MemStorage::default();
        let prompt = ScriptedPrompt::new(vec![], vec![]);

        for product_id in [
            "peopl.dot",
            "peopl.paseo",
            "peopl.testnet",
            "dim2.dot",
            "stash.dot",
        ] {
            let product = ProductContext::new(product_id.to_string()).expect("product id is valid");
            let service = PermissionsService::new(&storage, &prompt, &product);
            assert_eq!(
                futures::executor::block_on(
                    service.peek_remote(&remote(RemotePermission::ChainSubmit))
                )
                .unwrap(),
                PermissionAuthorizationStatus::Authorized,
                "{product_id} must be trusted on every accepted product network"
            );
        }
    }

    #[test]
    fn a_subdomain_of_a_trusted_label_is_not_trusted() {
        let storage = MemStorage::default();
        let prompt = ScriptedPrompt::new(vec![], vec![true]);
        let service = PermissionsService::new(&storage, &prompt, &PEOPL_APP);
        let request = remote(RemotePermission::ChainSubmit);

        assert_eq!(
            futures::executor::block_on(service.peek_remote(&request)).unwrap(),
            PermissionAuthorizationStatus::NotDetermined
        );
        assert_eq!(
            futures::executor::block_on(service.check_or_prompt_remote(request)).unwrap(),
            PermissionAuthorizationStatus::Authorized
        );
        assert_eq!(prompt.remote_calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_localhost_product_is_not_trusted() {
        let storage = MemStorage::default();
        let prompt = ScriptedPrompt::new(vec![], vec![true, true]);

        for product_id in ["localhost", "localhost:3000"] {
            let product = ProductContext::new(product_id.to_string()).expect("product id is valid");
            let service = PermissionsService::new(&storage, &prompt, &product);
            let request = remote(RemotePermission::ChainSubmit);
            assert_eq!(
                futures::executor::block_on(service.peek_remote(&request)).unwrap(),
                PermissionAuthorizationStatus::NotDetermined,
                "{product_id} carries no label to match and must prompt"
            );
            futures::executor::block_on(service.check_or_prompt_remote(request)).unwrap();
        }
        assert_eq!(prompt.remote_calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn an_untrusted_product_still_prompts_for_every_remote_permission() {
        let storage = MemStorage::default();
        let prompt = ScriptedPrompt::new(vec![], vec![true; 5]);
        let service = PermissionsService::new(&storage, &prompt, &PRODUCT);

        for permission in every_remote_permission() {
            futures::executor::block_on(service.check_or_prompt_remote(remote(permission)))
                .unwrap();
        }
        assert_eq!(prompt.remote_calls.load(Ordering::SeqCst), 5);
    }

    #[test]
    fn a_trusted_product_still_prompts_for_device_permissions() {
        let storage = MemStorage::default();
        let prompt = ScriptedPrompt::new(vec![false], vec![]);
        let service = trusted_service(&storage, &prompt);

        assert_eq!(
            futures::executor::block_on(service.peek_device(&HostDevicePermissionRequest::Camera))
                .unwrap(),
            PermissionAuthorizationStatus::NotDetermined
        );
        assert_eq!(
            futures::executor::block_on(
                service.check_or_prompt_device(HostDevicePermissionRequest::Camera)
            )
            .unwrap(),
            PermissionAuthorizationStatus::Denied,
            "a trusted product's device answer is the user's, not the whitelist's"
        );
        assert_eq!(prompt.device_calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_trusted_product_does_not_bypass_sensitive_permissions() {
        let storage = MemStorage::default();
        let prompt = ScriptedPrompt::new(vec![], vec![]);
        let service = trusted_service(&storage, &prompt);

        for request in [
            PermissionAuthorizationRequest::IdentityDisclosure,
            PermissionAuthorizationRequest::AccountAccess {
                target_product_id: "other.dot".to_string(),
            },
            PermissionAuthorizationRequest::ChatAuthority,
        ] {
            assert_eq!(
                futures::executor::block_on(service.authorization_status(&request)).unwrap(),
                PermissionAuthorizationStatus::NotDetermined,
                "{request:?} is outside the remote-permission whitelist"
            );
        }
    }

    #[test]
    fn an_empty_domain_bundle_is_denied_for_a_trusted_product() {
        let storage = MemStorage::default();
        let prompt = ScriptedPrompt::new(vec![], vec![]);
        let service = trusted_service(&storage, &prompt);

        assert_eq!(
            futures::executor::block_on(service.peek_remote(&remote_domains(&[]))).unwrap(),
            PermissionAuthorizationStatus::Denied
        );
        assert_eq!(
            futures::executor::block_on(service.check_or_prompt_remote(remote_domains(&[])))
                .unwrap(),
            PermissionAuthorizationStatus::Denied,
            "an empty bundle grants nothing, so failing closed outranks the whitelist"
        );
        assert_eq!(prompt.remote_calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn device_and_remote_caches_are_independent() {
        let storage = MemStorage::default();
        // Device denies, remote grants. If the caches collided we'd see the
        // same answer on the second call.
        let prompt = ScriptedPrompt::new(vec![false], vec![true]);
        let service = PermissionsService::new(&storage, &prompt, &PRODUCT);

        let device = futures::executor::block_on(
            service.check_or_prompt_device(HostDevicePermissionRequest::Camera),
        )
        .unwrap();
        let remote =
            futures::executor::block_on(service.check_or_prompt_remote(RemotePermissionRequest {
                permission: RemotePermission::ChainSubmit,
            }))
            .unwrap();

        assert_eq!(device, PermissionAuthorizationStatus::Denied);
        assert_eq!(remote, PermissionAuthorizationStatus::Authorized);
        assert_eq!(prompt.device_calls.load(Ordering::SeqCst), 1);
        assert_eq!(prompt.remote_calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn device_prompt_does_not_invoke_remote_callback() {
        let storage = MemStorage::default();
        let prompt = ScriptedPrompt::new(vec![true], vec![]);
        let service = PermissionsService::new(&storage, &prompt, &PRODUCT);

        let _ = futures::executor::block_on(
            service.check_or_prompt_device(HostDevicePermissionRequest::Camera),
        )
        .unwrap();
        assert_eq!(prompt.device_calls.load(Ordering::SeqCst), 1);
        assert_eq!(prompt.remote_calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn remote_prompt_does_not_invoke_device_callback() {
        let storage = MemStorage::default();
        let prompt = ScriptedPrompt::new(vec![], vec![true]);
        let service = PermissionsService::new(&storage, &prompt, &PRODUCT);

        let _ =
            futures::executor::block_on(service.check_or_prompt_remote(RemotePermissionRequest {
                permission: RemotePermission::WebRtc,
            }))
            .unwrap();
        assert_eq!(prompt.device_calls.load(Ordering::SeqCst), 0);
        assert_eq!(prompt.remote_calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn peek_returns_not_determined_until_authorized() {
        let storage = MemStorage::default();
        let prompt = ScriptedPrompt::new(vec![true], vec![]);
        let service = PermissionsService::new(&storage, &prompt, &PRODUCT);

        let before =
            futures::executor::block_on(service.peek_device(&HostDevicePermissionRequest::Camera))
                .unwrap();
        assert_eq!(before, PermissionAuthorizationStatus::NotDetermined);

        futures::executor::block_on(
            service.check_or_prompt_device(HostDevicePermissionRequest::Camera),
        )
        .unwrap();

        let after =
            futures::executor::block_on(service.peek_device(&HostDevicePermissionRequest::Camera))
                .unwrap();
        assert_eq!(after, PermissionAuthorizationStatus::Authorized);
    }

    #[test]
    fn set_authorization_status_writes_and_clears() {
        let storage = MemStorage::default();
        let prompt = ScriptedPrompt::new(vec![], vec![]);
        let service = PermissionsService::new(&storage, &prompt, &PRODUCT);
        let request = PermissionAuthorizationRequest::Device(HostDevicePermissionRequest::Camera);

        futures::executor::block_on(
            service.set_authorization_status(&request, PermissionAuthorizationStatus::Authorized),
        )
        .unwrap();
        assert_eq!(
            futures::executor::block_on(service.authorization_status(&request)).unwrap(),
            PermissionAuthorizationStatus::Authorized
        );

        futures::executor::block_on(
            service
                .set_authorization_status(&request, PermissionAuthorizationStatus::NotDetermined),
        )
        .unwrap();
        assert_eq!(
            futures::executor::block_on(service.authorization_status(&request)).unwrap(),
            PermissionAuthorizationStatus::NotDetermined
        );
    }

    #[test]
    fn identity_disclosure_authorization_round_trips() {
        let storage = MemStorage::default();
        let prompt = ScriptedPrompt::new(vec![], vec![]);
        let service = PermissionsService::new(&storage, &prompt, &PRODUCT);
        let request = PermissionAuthorizationRequest::IdentityDisclosure;

        assert_eq!(
            futures::executor::block_on(service.authorization_status(&request)).unwrap(),
            PermissionAuthorizationStatus::NotDetermined
        );

        futures::executor::block_on(
            service.set_authorization_status(&request, PermissionAuthorizationStatus::Authorized),
        )
        .unwrap();
        assert_eq!(
            futures::executor::block_on(service.authorization_status(&request)).unwrap(),
            PermissionAuthorizationStatus::Authorized
        );

        let other_product_service = PermissionsService::new(&storage, &prompt, &OTHER_PRODUCT);
        assert_eq!(
            futures::executor::block_on(other_product_service.authorization_status(&request))
                .unwrap(),
            PermissionAuthorizationStatus::NotDetermined
        );
    }

    #[test]
    fn chat_authority_uses_its_own_cached_permission() {
        let platform = StubPlatform {
            chat_authority_confirmed: true,
            ..Default::default()
        };
        let chat_product = ProductContext::new_with_execution(
            "chat.paseo".to_owned(),
            truapi_platform::ProductExecutionKind::Worker,
        )
        .expect("test product id is valid");
        let service = PermissionsService::new(&platform, &platform, &chat_product);

        assert_eq!(
            futures::executor::block_on(service.check_or_prompt_chat_authority()).unwrap(),
            PermissionAuthorizationStatus::Authorized
        );
        assert_eq!(
            futures::executor::block_on(service.check_or_prompt_chat_authority()).unwrap(),
            PermissionAuthorizationStatus::Authorized
        );
        assert_eq!(
            platform.chat_authority_reviews.lock().as_slice(),
            &[ChatAuthorityReview {
                product_id: "chat.paseo".to_string(),
            }]
        );
        assert_eq!(platform.identity_disclosure_calls.load(Ordering::SeqCst), 0);
        assert_eq!(
            futures::executor::block_on(
                service.authorization_status(&PermissionAuthorizationRequest::IdentityDisclosure)
            )
            .unwrap(),
            PermissionAuthorizationStatus::NotDetermined
        );
    }

    #[test]
    fn account_access_authorization_is_scoped_by_requester_and_target() {
        let storage = MemStorage::default();
        let prompt = ScriptedPrompt::new(vec![], vec![]);
        let service = PermissionsService::new(&storage, &prompt, &PRODUCT);
        let request = PermissionAuthorizationRequest::AccountAccess {
            target_product_id: "target.dot".to_string(),
        };

        futures::executor::block_on(
            service.set_authorization_status(&request, PermissionAuthorizationStatus::Authorized),
        )
        .unwrap();
        assert_eq!(
            futures::executor::block_on(service.authorization_status(&request)).unwrap(),
            PermissionAuthorizationStatus::Authorized
        );
        assert_eq!(
            futures::executor::block_on(service.authorization_status(
                &PermissionAuthorizationRequest::AccountAccess {
                    target_product_id: "other.dot".to_string(),
                }
            ))
            .unwrap(),
            PermissionAuthorizationStatus::NotDetermined
        );

        let other_product_service = PermissionsService::new(&storage, &prompt, &OTHER_PRODUCT);
        assert_eq!(
            futures::executor::block_on(other_product_service.authorization_status(&request))
                .unwrap(),
            PermissionAuthorizationStatus::NotDetermined
        );
    }

    /// Prompt callback that always errors, to exercise the transient-failure
    /// path (fail closed for the current call, but do not persist the error).
    struct FailingPrompt;

    #[truapi_platform::async_trait]
    impl Permissions for FailingPrompt {
        async fn device_permission(
            &self,
            _product: &ProductContext,
            _request: HostDevicePermissionRequest,
        ) -> Result<PermissionDecision, GenericError> {
            Err(GenericError {
                reason: "boom".into(),
            })
        }

        async fn remote_permission(
            &self,
            _product: &ProductContext,
            _request: RemotePermissionRequest,
        ) -> Result<PermissionDecision, GenericError> {
            Err(GenericError {
                reason: "boom".into(),
            })
        }
    }

    #[test]
    fn prompt_failure_stays_not_determined_without_persisting() {
        let storage = MemStorage::default();
        let prompt = FailingPrompt;
        let service = PermissionsService::new(&storage, &prompt, &PRODUCT);

        let device_decision = futures::executor::block_on(
            service.check_or_prompt_device(HostDevicePermissionRequest::Camera),
        )
        .unwrap();
        assert_eq!(
            device_decision,
            PermissionAuthorizationStatus::NotDetermined
        );

        let remote_request = RemotePermissionRequest {
            permission: RemotePermission::ChainSubmit,
        };
        let remote_decision =
            futures::executor::block_on(service.check_or_prompt_remote(remote_request.clone()))
                .unwrap();
        assert_eq!(
            remote_decision,
            PermissionAuthorizationStatus::NotDetermined
        );

        // A transient callback error is not cached, so peek still sees no
        // authorization and the next request re-prompts rather than
        // permanently locking out the capability.
        let cached_device =
            futures::executor::block_on(service.peek_device(&HostDevicePermissionRequest::Camera))
                .unwrap();
        assert_eq!(
            cached_device,
            PermissionAuthorizationStatus::NotDetermined,
            "a transient prompt error must not be persisted"
        );
        let cached_remote =
            futures::executor::block_on(service.peek_remote(&remote_request)).unwrap();
        assert_eq!(
            cached_remote,
            PermissionAuthorizationStatus::NotDetermined,
            "a transient prompt error must not be persisted"
        );
    }

    /// A corrupt SCALE-encoded cache entry must be treated as "no cache",
    /// not panic. The service falls back to prompting.
    #[test]
    fn corrupt_cache_entry_returns_none() {
        let storage = MemStorage::default();
        // Write garbage bytes under the canonical key.
        futures::executor::block_on(storage.write_core_storage(
            CoreStorageKey::device_permission_authorization(
                "product.dot",
                &HostDevicePermissionRequest::Camera,
            ),
            vec![0xff, 0xfe, 0xfd],
        ))
        .unwrap();

        let prompt = ScriptedPrompt::new(vec![true], vec![]);
        let service = PermissionsService::new(&storage, &prompt, &PRODUCT);

        let peeked =
            futures::executor::block_on(service.peek_device(&HostDevicePermissionRequest::Camera))
                .unwrap();
        assert_eq!(
            peeked,
            PermissionAuthorizationStatus::NotDetermined,
            "corrupt entry must decode as absent"
        );
    }

    /// Storage failures must propagate to the caller; the service must not
    /// swallow them by silently returning a default authorization.
    #[derive(Default)]
    struct FailingStorage;

    #[truapi_platform::async_trait]
    impl CoreStorage for FailingStorage {
        async fn read_core_storage(
            &self,
            _key: CoreStorageKey,
        ) -> Result<Option<Vec<u8>>, v01::GenericError> {
            Err(v01::GenericError {
                reason: "read failed".into(),
            })
        }
        async fn write_core_storage(
            &self,
            _key: CoreStorageKey,
            _value: Vec<u8>,
        ) -> Result<(), v01::GenericError> {
            Err(v01::GenericError {
                reason: "write failed".into(),
            })
        }
        async fn clear_core_storage(&self, _key: CoreStorageKey) -> Result<(), v01::GenericError> {
            Err(v01::GenericError {
                reason: "clear failed".into(),
            })
        }
    }

    #[test]
    fn storage_read_error_propagates() {
        let storage = FailingStorage;
        let prompt = ScriptedPrompt::new(vec![], vec![]);
        let service = PermissionsService::new(&storage, &prompt, &PRODUCT);

        let err = futures::executor::block_on(
            service.check_or_prompt_device(HostDevicePermissionRequest::Camera),
        )
        .expect_err("read failure must surface");
        assert!(matches!(err, v01::GenericError { .. }));
    }

    #[test]
    fn an_os_denial_overrides_a_stored_grant() {
        let storage = MemStorage::default();
        grant_stored(&storage, HostDevicePermissionRequest::Camera);

        // No device answers scripted: reaching the prompt at all would panic,
        // which is the assertion that a settings-level refusal is not something
        // the user can be asked about.
        let prompt = ScriptedPrompt::new(vec![], vec![]);
        let status = ScriptedStatus::always(DevicePermissionStatus::Denied);
        let service =
            PermissionsService::new(&storage, &prompt, &PRODUCT).with_status_host(Some(&status));

        assert_eq!(
            futures::executor::block_on(
                service.check_or_prompt_device(HostDevicePermissionRequest::Camera)
            )
            .unwrap(),
            PermissionAuthorizationStatus::Denied,
        );
    }

    #[test]
    fn an_os_denial_leaves_the_stored_grant_intact() {
        let storage = MemStorage::default();
        grant_stored(&storage, HostDevicePermissionRequest::Camera);

        let denied_prompt = ScriptedPrompt::new(vec![], vec![]);
        let denied = ScriptedStatus::always(DevicePermissionStatus::Denied);
        futures::executor::block_on(
            PermissionsService::new(&storage, &denied_prompt, &PRODUCT)
                .with_status_host(Some(&denied))
                .check_or_prompt_device(HostDevicePermissionRequest::Camera),
        )
        .unwrap();

        // The user restores the OS grant in settings. The product decision was
        // never theirs to lose, so this resolves without asking them again.
        let prompt = ScriptedPrompt::new(vec![], vec![]);
        let restored = ScriptedStatus::always(DevicePermissionStatus::Granted);
        let service =
            PermissionsService::new(&storage, &prompt, &PRODUCT).with_status_host(Some(&restored));

        assert_eq!(
            futures::executor::block_on(
                service.check_or_prompt_device(HostDevicePermissionRequest::Camera)
            )
            .unwrap(),
            PermissionAuthorizationStatus::Authorized,
        );
        assert_eq!(prompt.device_calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn an_os_reset_never_reprompts_a_stored_grant() {
        let storage = MemStorage::default();
        grant_stored(&storage, HostDevicePermissionRequest::Camera);

        // Android auto-resets runtime permissions for unused apps. The core
        // cannot ask the OS on its own — the prompt callback also puts the
        // product's question to the user — so it does not try. No answers are
        // scripted: reaching the prompt at all would panic.
        let prompt = ScriptedPrompt::new(vec![], vec![]);
        let status = ScriptedStatus::always(DevicePermissionStatus::NotDetermined);
        let service =
            PermissionsService::new(&storage, &prompt, &PRODUCT).with_status_host(Some(&status));

        // Repeated, because a condition a prompt cannot clear re-fires forever.
        for _ in 0..3 {
            assert_eq!(
                futures::executor::block_on(
                    service.check_or_prompt_device(HostDevicePermissionRequest::Camera)
                )
                .unwrap(),
                PermissionAuthorizationStatus::Authorized,
            );
        }
        assert_eq!(prompt.device_calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn an_os_reset_cannot_turn_a_stored_grant_into_a_permanent_denial() {
        let storage = MemStorage::default();
        grant_stored(&storage, HostDevicePermissionRequest::Camera);

        // On iOS and Android the prompt callback *is* the OS dialog, so a
        // "Don't Allow" answered there is an answer about the OS, not about the
        // product. Persisting it would replace the product's grant, and
        // restoring the capability in system settings could never recover it.
        let declining = ScriptedPrompt::new(vec![false], vec![]);
        let reset = ScriptedStatus::always(DevicePermissionStatus::NotDetermined);
        futures::executor::block_on(
            PermissionsService::new(&storage, &declining, &PRODUCT)
                .with_status_host(Some(&reset))
                .check_or_prompt_device(HostDevicePermissionRequest::Camera),
        )
        .unwrap();

        let prompt = ScriptedPrompt::new(vec![], vec![]);
        let restored = ScriptedStatus::always(DevicePermissionStatus::Granted);
        let after =
            PermissionsService::new(&storage, &prompt, &PRODUCT).with_status_host(Some(&restored));
        assert_eq!(
            futures::executor::block_on(
                after.check_or_prompt_device(HostDevicePermissionRequest::Camera)
            )
            .unwrap(),
            PermissionAuthorizationStatus::Authorized,
        );
    }

    #[test]
    fn a_prompt_failure_cannot_mask_a_stored_grant() {
        let storage = MemStorage::default();
        grant_stored(&storage, HostDevicePermissionRequest::Camera);

        // A failing prompt callback resolves to `NotDetermined`, which the
        // runtime reports as `granted: false`. A grant the product already
        // holds must never be reached through that path.
        let failing = FailingPrompt;
        let reset = ScriptedStatus::always(DevicePermissionStatus::NotDetermined);
        let service =
            PermissionsService::new(&storage, &failing, &PRODUCT).with_status_host(Some(&reset));
        assert_eq!(
            futures::executor::block_on(
                service.check_or_prompt_device(HostDevicePermissionRequest::Camera)
            )
            .unwrap(),
            PermissionAuthorizationStatus::Authorized,
        );
    }

    #[test]
    fn a_first_request_still_prompts_while_the_os_is_undetermined() {
        // The guard against re-prompting must not swallow the first ask, which
        // is the only one that establishes the product decision at all.
        let storage = MemStorage::default();
        let prompt = ScriptedPrompt::new(vec![true], vec![]);
        let status = ScriptedStatus::always(DevicePermissionStatus::NotDetermined);
        let service =
            PermissionsService::new(&storage, &prompt, &PRODUCT).with_status_host(Some(&status));

        assert_eq!(
            futures::executor::block_on(
                service.check_or_prompt_device(HostDevicePermissionRequest::Camera)
            )
            .unwrap(),
            PermissionAuthorizationStatus::Authorized,
        );
        assert_eq!(prompt.device_calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn an_os_reset_does_not_reprompt_a_stored_denial() {
        let storage = MemStorage::default();
        let seed = ScriptedPrompt::new(vec![false], vec![]);
        futures::executor::block_on(
            PermissionsService::new(&storage, &seed, &PRODUCT)
                .check_or_prompt_device(HostDevicePermissionRequest::Camera),
        )
        .unwrap();

        // The product-level "no" is still the user's answer. An OS that forgot
        // its own state is not a reason to put the question again.
        let prompt = ScriptedPrompt::new(vec![], vec![]);
        let status = ScriptedStatus::always(DevicePermissionStatus::NotDetermined);
        let service =
            PermissionsService::new(&storage, &prompt, &PRODUCT).with_status_host(Some(&status));

        assert_eq!(
            futures::executor::block_on(
                service.check_or_prompt_device(HostDevicePermissionRequest::Camera)
            )
            .unwrap(),
            PermissionAuthorizationStatus::Denied,
        );
        assert_eq!(prompt.device_calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn a_failed_status_query_falls_back_to_the_stored_grant() {
        let storage = MemStorage::default();
        grant_stored(&storage, HostDevicePermissionRequest::Camera);

        // A dropped IPC is not a refusal. Reading it as one would let a flaky
        // channel revoke a capability the OS still allows.
        let prompt = ScriptedPrompt::new(vec![], vec![]);
        let status = ScriptedStatus::failing();
        let service =
            PermissionsService::new(&storage, &prompt, &PRODUCT).with_status_host(Some(&status));

        assert_eq!(
            futures::executor::block_on(
                service.check_or_prompt_device(HostDevicePermissionRequest::Camera)
            )
            .unwrap(),
            PermissionAuthorizationStatus::Authorized,
        );
        assert_eq!(prompt.device_calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn an_os_denial_denies_before_any_prompt_and_persists_nothing() {
        let storage = MemStorage::default();
        let prompt = ScriptedPrompt::new(vec![], vec![]);
        let denied = ScriptedStatus::always(DevicePermissionStatus::Denied);
        let service =
            PermissionsService::new(&storage, &prompt, &PRODUCT).with_status_host(Some(&denied));

        assert_eq!(
            futures::executor::block_on(
                service.check_or_prompt_device(HostDevicePermissionRequest::Camera)
            )
            .unwrap(),
            PermissionAuthorizationStatus::Denied,
        );

        // Nothing was written, so the product question is still open: once the
        // OS allows it, the user gets asked rather than inheriting a denial
        // they never gave.
        let peek = PermissionsService::new(&storage, &prompt, &PRODUCT);
        assert_eq!(
            futures::executor::block_on(peek.peek_device(&HostDevicePermissionRequest::Camera))
                .unwrap(),
            PermissionAuthorizationStatus::NotDetermined,
        );
    }

    #[test]
    fn os_status_is_read_for_the_capability_being_checked() {
        let storage = MemStorage::default();
        grant_stored(&storage, HostDevicePermissionRequest::Camera);
        grant_stored(&storage, HostDevicePermissionRequest::Microphone);

        // Only the camera is refused by the OS. A mix-up in which capability
        // reaches the status host would move the denial to the microphone.
        let prompt = ScriptedPrompt::new(vec![], vec![]);
        let status = ScriptedStatus::per_capability(
            vec![(
                HostDevicePermissionRequest::Camera,
                DevicePermissionStatus::Denied,
            )],
            DevicePermissionStatus::Granted,
        );
        let service =
            PermissionsService::new(&storage, &prompt, &PRODUCT).with_status_host(Some(&status));

        assert_eq!(
            futures::executor::block_on(
                service.check_or_prompt_device(HostDevicePermissionRequest::Camera)
            )
            .unwrap(),
            PermissionAuthorizationStatus::Denied,
        );
        assert_eq!(
            futures::executor::block_on(
                service.check_or_prompt_device(HostDevicePermissionRequest::Microphone)
            )
            .unwrap(),
            PermissionAuthorizationStatus::Authorized,
        );
        assert_eq!(
            status.asked(),
            vec![
                HostDevicePermissionRequest::Camera,
                HostDevicePermissionRequest::Microphone,
            ],
        );
    }

    #[test]
    fn a_host_without_the_capability_resolves_from_stored_state_alone() {
        let storage = MemStorage::default();
        let prompt = ScriptedPrompt::new(vec![true], vec![]);
        let service = PermissionsService::new(&storage, &prompt, &PRODUCT).with_status_host(None);

        let first = futures::executor::block_on(
            service.check_or_prompt_device(HostDevicePermissionRequest::Camera),
        )
        .unwrap();
        let second = futures::executor::block_on(
            service.check_or_prompt_device(HostDevicePermissionRequest::Camera),
        )
        .unwrap();

        assert_eq!(first, PermissionAuthorizationStatus::Authorized);
        assert_eq!(second, PermissionAuthorizationStatus::Authorized);
        assert_eq!(prompt.device_calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn os_device_status_does_not_reach_remote_permissions() {
        let storage = MemStorage::default();
        let prompt = ScriptedPrompt::new(vec![], vec![true]);
        // Every device capability refused by the OS. A remote grant is a
        // TrUAPI-level decision with no OS gate behind it, so it must resolve
        // untouched.
        let status = ScriptedStatus::always(DevicePermissionStatus::Denied);
        let service =
            PermissionsService::new(&storage, &prompt, &PRODUCT).with_status_host(Some(&status));

        assert_eq!(
            futures::executor::block_on(
                service.check_or_prompt_remote(remote_domains(&["example.com"]))
            )
            .unwrap(),
            PermissionAuthorizationStatus::Authorized,
        );
        assert!(status.asked().is_empty());
    }

    #[test]
    fn a_peeked_grant_the_os_refuses_reads_as_denied() {
        // A settings screen and a request must not disagree. Reporting the
        // stored grant here sends the user hunting for a product toggle when
        // the OS is what refused.
        let storage = MemStorage::default();
        grant_stored(&storage, HostDevicePermissionRequest::Camera);
        let prompt = ScriptedPrompt::new(vec![], vec![]);
        let refusing = ScriptedStatus::always(DevicePermissionStatus::Denied);
        let service =
            PermissionsService::new(&storage, &prompt, &PRODUCT).with_status_host(Some(&refusing));

        assert_eq!(
            futures::executor::block_on(service.peek_device(&HostDevicePermissionRequest::Camera))
                .unwrap(),
            PermissionAuthorizationStatus::Denied,
        );
    }

    #[test]
    fn a_peek_under_an_os_refusal_leaves_the_stored_grant_intact() {
        let storage = MemStorage::default();
        grant_stored(&storage, HostDevicePermissionRequest::Camera);
        let prompt = ScriptedPrompt::new(vec![], vec![]);
        let refusing = ScriptedStatus::always(DevicePermissionStatus::Denied);
        futures::executor::block_on(
            PermissionsService::new(&storage, &prompt, &PRODUCT)
                .with_status_host(Some(&refusing))
                .peek_device(&HostDevicePermissionRequest::Camera),
        )
        .unwrap();

        // A read is not a decision, so the grant is still there once the user
        // restores the OS grant.
        let restored = ScriptedStatus::always(DevicePermissionStatus::Granted);
        assert_eq!(
            futures::executor::block_on(
                PermissionsService::new(&storage, &prompt, &PRODUCT)
                    .with_status_host(Some(&restored))
                    .peek_device(&HostDevicePermissionRequest::Camera),
            )
            .unwrap(),
            PermissionAuthorizationStatus::Authorized,
        );
    }

    #[test]
    fn a_peeked_grant_survives_an_os_reset_and_a_failed_query() {
        let storage = MemStorage::default();
        grant_stored(&storage, HostDevicePermissionRequest::Camera);
        let prompt = ScriptedPrompt::new(vec![], vec![]);

        for status in [
            ScriptedStatus::always(DevicePermissionStatus::NotDetermined),
            ScriptedStatus::failing(),
        ] {
            assert_eq!(
                futures::executor::block_on(
                    PermissionsService::new(&storage, &prompt, &PRODUCT)
                        .with_status_host(Some(&status))
                        .peek_device(&HostDevicePermissionRequest::Camera),
                )
                .unwrap(),
                PermissionAuthorizationStatus::Authorized,
                "only an outright refusal overrides the stored decision"
            );
        }
    }

    #[test]
    fn a_status_read_of_a_remote_permission_never_reaches_the_os() {
        let storage = MemStorage::default();
        let prompt = ScriptedPrompt::new(vec![], vec![true]);
        let refusing = ScriptedStatus::always(DevicePermissionStatus::Denied);
        let service =
            PermissionsService::new(&storage, &prompt, &PRODUCT).with_status_host(Some(&refusing));
        futures::executor::block_on(
            service.check_or_prompt_remote(remote_domains(&["example.com"])),
        )
        .unwrap();

        assert_eq!(
            futures::executor::block_on(service.authorization_status(
                &PermissionAuthorizationRequest::Remote(remote_domains(&["example.com"]))
            ))
            .unwrap(),
            PermissionAuthorizationStatus::Authorized,
        );
        assert!(refusing.asked().is_empty());
    }

    #[test]
    fn a_device_request_and_a_status_read_agree_once_the_os_refuses() {
        let storage = MemStorage::default();
        grant_stored(&storage, HostDevicePermissionRequest::Camera);
        let prompt = ScriptedPrompt::new(vec![], vec![]);
        let refusing = ScriptedStatus::always(DevicePermissionStatus::Denied);
        let service =
            PermissionsService::new(&storage, &prompt, &PRODUCT).with_status_host(Some(&refusing));

        let requested = futures::executor::block_on(
            service.check_or_prompt_device(HostDevicePermissionRequest::Camera),
        )
        .unwrap();
        let read = futures::executor::block_on(service.authorization_status(
            &PermissionAuthorizationRequest::Device(HostDevicePermissionRequest::Camera),
        ))
        .unwrap();
        assert_eq!(
            (requested, read),
            (
                PermissionAuthorizationStatus::Denied,
                PermissionAuthorizationStatus::Denied,
            )
        );
    }
}
