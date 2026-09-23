//! `ProductRuntimeHost` adapts one product connection into the
//! typed `truapi::api::*` host traits the generated dispatcher routes to.
//!
//! Most methods are straight delegations to the platform; the rest carry
//! host-agnostic logic owned by the core (the chainHead-v1 runtime behind
//! the Chain surface, `dotns` URL parsing for `navigate_to`, and the
//! permission cache layer). Methods with no platform backing return
//! `CallError::unavailable()`.

/// Connection-scoped, host-fed action streams.
pub(crate) mod actions;
mod allowances;
/// Core-owned auth/session UI state machine.
pub(crate) mod auth_state;
mod authority;
/// In-core Bulletin preimage submission over the shared Subxt client.
pub(crate) mod bulletin_rpc;
mod capabilities;
mod chat;
mod chat_device;
mod chat_identity;
mod coinage_chain;
mod coinage_store;
mod dotns_lookup;
mod identity;
pub(crate) mod login_failure;
mod native_chat;
mod pairing_host;
pub(crate) mod product_manifest;
mod product_subtree;
mod renderer;
mod ring_vrf_registry;
/// Role-neutral runtime services shared by product-facing runtimes.
pub(crate) mod services;
mod signing_host;
/// SSO pairing (login) flow over the statement store bootstrap topic.
pub(crate) mod sso_pairing;
/// SSO remote request/response messaging over the statement store.
pub(crate) mod sso_remote;
pub(crate) mod sso_service;
/// Statement Store and Bulletin allowance allocation.
pub mod statement_allowance;
/// `StatementStore` surface: proofs plus submit and subscribe flows.
pub(crate) mod statement_store;
mod statement_store_rpc;

use core::future::Future;
use core::time::Duration;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;

pub(crate) use actions::ActionChannel;
use authority::{AuthorityCancelError, AuthoritySession, ProductDeviceChatAuthorityError};
pub(crate) use authority::{AuthorityError, BulletinAllowanceKey, ProductAuthority};
pub(crate) use chat::chat_platform_for;
use futures::{FutureExt, StreamExt, pin_mut};
#[cfg(test)]
use pairing_host::PairingHost;
pub(crate) use pairing_host::PairingHost as PairingHostRole;
pub(crate) use renderer::renderer_access_for;
pub(crate) use services::RuntimeServices;
pub use signing_host::{
    AnnouncedPairing, DevicePairingObserver, MAX_PAIRING_METADATA_CHARS, PairedSsoPeer,
    PairingProposal, PairingProposalMetadata, ResponderExit,
};
pub(crate) use signing_host::{
    LocalActivation, SigningHost as SigningHostRole, SigningHostSsoService, disconnect_paired_host,
    establish_pairing, notify_pairing_allowance_allocation, notify_pairing_failed,
    respond_to_pairing, resume_pairing,
};
pub use signing_host::{LocalIdentity, LocalIdentityContext};
// `TrackedStatementRenewalTarget` is only read back by the native renewal
// reporting, so re-exporting it on wasm leaves an unused import.
#[cfg(any(test, not(target_arch = "wasm32")))]
pub use signing_host::StatementRenewalTarget;
#[cfg(not(target_arch = "wasm32"))]
pub use signing_host::TrackedStatementRenewalTarget;
use tracing::{instrument, warn};
use truapi::api::{Chat, Pocket, Renderer};
use truapi::versioned::account::{
    HostAccountGetError, HostAccountSignVrfError, HostProductDeviceChatError,
};
use truapi::versioned::chat::{
    HostChatActionSubscribeError, HostChatActionSubscribeItem, HostChatActionSubscribeRequest,
    HostChatCreateRoomError, HostChatCreateRoomRequest, HostChatCreateRoomResponse,
    HostChatListSubscribeError, HostChatListSubscribeItem, HostChatListSubscribeRequest,
    HostChatPostMessageError, HostChatPostMessageRequest, HostChatPostMessageResponse,
    HostChatRegisterBotError, HostChatRegisterBotRequest, HostChatRegisterBotResponse,
};
use truapi::versioned::pocket::{
    HostPocketListSubscribeError, HostPocketListSubscribeItem, HostPocketListSubscribeRequest,
    HostPocketRemoveCardError, HostPocketRemoveCardRequest, HostPocketRemoveCardResponse,
};
use truapi::versioned::preimage::RemotePreimageSubmitError;
use truapi::versioned::renderer::{
    HostRendererActionSubscribeError, HostRendererActionSubscribeItem,
    HostRendererActionSubscribeRequest,
};
use truapi::{CallContext, CallError, CancellationReason, Subscription, v01};
use truapi_platform::{
    AccountAccessReview, ChatFieldError, PermissionAuthorizationRequest,
    PermissionAuthorizationStatus, PermissionDecision, Platform, ProductContext, ProductStorageKey,
    SessionUiInfo, UserConfirmationReview, normalize_chat_identifier, normalize_product_identifier,
    validate_chat_icon, validate_chat_message_content, validate_chat_name,
};
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

use crate::chain_runtime::RuntimeFailure;
use crate::host_logic::bulletin::preimage_key;
use crate::host_logic::permissions::{PermissionsService, TemporaryPermissions};
use crate::host_logic::product_account::{
    derivation_index_bytes, derive_product_public_key, public_key_from_address,
};
use crate::host_logic::product_manifest::Granted;
use crate::host_logic::session::SessionInfo;
#[cfg(test)]
use crate::host_logic::session::SessionState;
use crate::host_logic::sso::messages::RingVrfError;
use crate::host_logic::sso::pairing::x25519_public_key;
#[cfg(test)]
use crate::subscription::Spawner;

/// Error reason surfaced to products when a permission is not granted.
pub(super) const PERMISSION_DENIED_REASON: &str = "Permission denied";
/// Host-spec B.6.2 recommends timing out unanswered SSO application requests
/// after 180 seconds:
/// <https://github.com/paritytech/host-spec/blob/adb3989208ae1c2107dbf0159611353e6989422c/spec/B-inter-host.md?plain=1#L303-L307>
pub(crate) const DEFAULT_REMOTE_AUTHORITY_RESPONSE_TIMEOUT: Duration = Duration::from_secs(180);
/// After a cancel or deadline, how long a remote authority call is given to
/// observe the cancellation and unwind (unsubscribing its statement streams)
/// before it is dropped. A call parked in the statement-store setup, which does
/// not watch the cancel token, would otherwise outlive its deadline, so this
/// caps the wait. A normal unwind completes in well under this.
const AUTHORITY_CANCEL_UNWIND_GRACE: Duration = Duration::from_secs(2);
/// Resource allocation may include a People -> Bulletin cross-chain
/// propagation before the signing host can truthfully report `Allocated`.
/// Keep this above the signing host's 240-second propagation ceiling while
/// still bounding an unanswered request.
const RESOURCE_ALLOCATION_REMOTE_AUTHORITY_RESPONSE_TIMEOUT: Duration = Duration::from_secs(300);
/// End-to-end timeout for an in-core Bulletin preimage submit, starting after
/// user confirmation and covering allowance allocation, chain submission, and
/// one optional allowance refresh and retry. A first live allocation may need
/// to fund and register the identity before the submit can start.
const PREIMAGE_SUBMIT_TIMEOUT: Duration = Duration::from_secs(360);
/// Per-request cap for obtaining or refreshing the Bulletin allowance. The
/// end-to-end submit deadline may reduce it further.
const PREIMAGE_REMOTE_AUTHORITY_RESPONSE_TIMEOUT: Duration =
    RESOURCE_ALLOCATION_REMOTE_AUTHORITY_RESPONSE_TIMEOUT;

const LEGACY_PRODUCT_ACCOUNT_MISMATCH_REASON: &str =
    "Account can't be derived from product account id";
const LEGACY_ACCOUNT_UNAVAILABLE_REASON: &str = "Account is not available in the active session";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LegacySigner {
    Product,
    Identity([u8; 32]),
}

#[derive(Debug, PartialEq, Eq)]
enum LegacySignerError {
    Unavailable,
    ProductDerivation(String),
}

impl LegacySignerError {
    fn into_reason(self, unavailable_reason: &'static str) -> String {
        match self {
            Self::Unavailable => unavailable_reason.to_string(),
            Self::ProductDerivation(reason) => reason,
        }
    }

    fn into_host_error(self, unavailable_reason: &'static str) -> v01::HostSignPayloadError {
        v01::HostSignPayloadError::Unknown {
            reason: self.into_reason(unavailable_reason),
        }
    }
}

fn remote_authority_context(cx: &CallContext) -> CallContext {
    remote_authority_context_with_default(cx, DEFAULT_REMOTE_AUTHORITY_RESPONSE_TIMEOUT)
}

fn remote_authority_context_with_default(
    cx: &CallContext,
    default_timeout: Duration,
) -> CallContext {
    let mut cx = cx.clone();
    if cx.timeout().is_none() {
        cx.set_timeout(default_timeout);
    }
    cx
}

fn remote_authority_context_until(
    cx: &CallContext,
    default_timeout: Duration,
    deadline: Instant,
) -> CallContext {
    let mut cx = cx.clone();
    let timeout = cx
        .timeout()
        .unwrap_or(default_timeout)
        .min(deadline.saturating_duration_since(Instant::now()));
    cx.set_timeout(timeout);
    cx
}

async fn remote_authority_call<T, E, F>(cx: &CallContext, call: F) -> Result<T, E>
where
    F: Future<Output = Result<T, E>>,
    E: From<AuthorityError>,
{
    let call = call.fuse();
    let cancelled = cx.cancel().cancelled().fuse();
    pin_mut!(call, cancelled);

    // First, run the call against cancellation and the optional deadline. A
    // completed call returns directly; a cancel or timeout yields its reason and
    // falls through to a bounded unwind.
    let reason = if let Some(timeout_duration) = cx.timeout() {
        let timeout = futures_timer::Delay::new(timeout_duration).fuse();
        pin_mut!(timeout);
        futures::select! {
            result = call => return result,
            reason = cancelled => reason,
            () = timeout => {
                let reason = CancellationReason::TimedOut {
                    timeout: timeout_duration,
                };
                cx.cancel().cancel_with_reason(reason.clone());
                reason
            }
        }
    } else {
        futures::select! {
            result = call => return result,
            reason = cancelled => reason,
        }
    };

    // Give the call a bounded chance to observe the cancellation and tear down
    // its statement-store subscriptions, then drop it. The grace caps a call
    // parked in the setup region that never observes the token.
    let error = authority_cancellation_error(cx, reason);
    let unwind = futures_timer::Delay::new(AUTHORITY_CANCEL_UNWIND_GRACE).fuse();
    pin_mut!(unwind);
    futures::select! {
        _ = call => {},
        () = unwind => {},
    }
    Err(error.into())
}

fn authority_cancellation_error(cx: &CallContext, reason: CancellationReason) -> AuthorityError {
    AuthorityError::Cancelled(AuthorityCancelError::new(cx.request_id(), reason))
}

/// Product-scoped adapter that exposes a long-lived host runtime through the
/// `truapi::api::*` trait set the generated dispatcher routes to.
pub struct ProductRuntimeHost {
    services: Arc<RuntimeServices>,
    platform: Arc<dyn Platform>,
    chat_platform: Option<Arc<dyn truapi_platform::ChatPlatform>>,
    /// Live OS permission state for this connection, when the host serves it.
    permission_status: Option<Arc<dyn truapi_platform::PermissionStatusHost>>,
    /// Permission requests and consuming operations can arrive on different connections.
    temporary_permissions: Arc<TemporaryPermissions>,
    authority: Arc<dyn ProductAuthority>,
    product: ProductContext,
    /// Stable per-product-runtime id used to scope long-lived chain follow
    /// operation ids within one shared host runtime.
    core_instance: u64,
    chat: Arc<ActionChannel<HostChatActionSubscribeItem>>,
    renderer: Arc<ActionChannel<HostRendererActionSubscribeItem>>,
    pocket_platform: Option<Arc<dyn truapi_platform::PocketPlatform>>,
    /// Host-assigned ids of this connection's open pending operations, each
    /// holding one worker reference until it ends or the connection is torn
    /// down.
    ///
    /// Scoped to the connection rather than the product, which holds because
    /// only a Worker execution reaches `begin_operation`/`end_operation` and a
    /// product has one of those at a time.
    ///
    /// The set is unbounded here. Whether a product may hold a thousand open
    /// operations is the host's call, made in `begin_operation`, since the
    /// host is what the operations keep running.
    open_operations: Mutex<HashSet<u32>>,
}

/// A connection that goes away without ending its operations still owes the
/// ledger their references, so the host is told to stop rather than keeping a
/// worker alive for a product that is gone.
impl Drop for ProductRuntimeHost {
    fn drop(&mut self) {
        self.release_open_operations();
    }
}

impl ProductRuntimeHost {
    /// Build a product-scoped dispatcher target from a long-lived host runtime
    /// and the adapters scoped to this product connection.
    pub(crate) fn from_services(
        services: Arc<RuntimeServices>,
        adapters: crate::host_core::ConnectionAdapters,
        authority: Arc<dyn ProductAuthority>,
        product: ProductContext,
    ) -> Self {
        let core_instance = services.next_core_instance();
        Self {
            services,
            platform: adapters.platform,
            chat_platform: adapters.chat_platform,
            permission_status: adapters.permission_status,
            temporary_permissions: adapters.permission_grants,
            authority,
            product,
            core_instance,
            chat: adapters.chat,
            renderer: adapters.renderer,
            pocket_platform: adapters.pocket_platform,
            open_operations: Mutex::new(HashSet::new()),
        }
    }

    /// Role-neutral services shared with the owning host runtime.
    pub(crate) fn services(&self) -> &Arc<RuntimeServices> {
        &self.services
    }

    /// Permission service for this product.
    ///
    /// Every device-reaching path is built here so a request and a status read
    /// resolve the same two gates. Remote, identity-disclosure and
    /// account-access decisions have no OS gate and are unaffected by the
    /// status adapter.
    fn permissions_service(&self) -> PermissionsService<'_, dyn Platform, dyn Platform> {
        PermissionsService::new(
            self.platform.as_ref(),
            self.platform.as_ref(),
            &self.product,
        )
        .with_status_host(self.permission_status.as_deref())
        .with_temporary_permissions(self.temporary_permissions.clone())
    }

    /// Trusted executable kind attached to this product connection.
    pub(crate) fn execution_kind(&self) -> truapi_platform::ProductExecutionKind {
        self.product.execution_kind
    }

    /// Test constructor building a standalone pairing-host runtime.
    #[cfg(test)]
    pub fn new<P>(
        platform: Arc<P>,
        config: (truapi_platform::PairingHostConfig, ProductContext),
        spawner: Spawner,
    ) -> Self
    where
        P: Platform + 'static,
    {
        let (host_config, product) = config;
        let platform: Arc<dyn Platform> = platform;
        Self::new_pairing_for_tests(platform, host_config, product, spawner).0
    }

    /// Compatibility constructor used only by tests that do not exercise
    /// product-scoped behavior.
    #[cfg(test)]
    fn new_compat(platform: Arc<dyn Platform>, spawner: Spawner) -> Self {
        Self::new_compat_with_pairing(platform, spawner).0
    }

    #[cfg(test)]
    fn compat_host_config() -> truapi_platform::PairingHostConfig {
        truapi_platform::PairingHostConfig::new(
            truapi_platform::HostInfo {
                name: "Polkadot Web".to_string(),
                icon: Some("https://example.invalid/dotli.png".to_string()),
                version: None,
                platform: truapi::latest::HostPlatform::Web,
            },
            truapi_platform::PlatformInfo::default(),
            [0; 32],
            [0xbb; 32],
            [0xcc; 32],
            "polkadotapp".to_string(),
        )
        .expect("compat runtime config is valid")
    }

    /// Compat host used by preimage tests.
    #[cfg(test)]
    fn new_compat_with_bulletin(platform: Arc<dyn Platform>, spawner: Spawner) -> Self {
        Self::new_pairing_for_tests(
            platform,
            Self::compat_host_config(),
            ProductContext::new("unknown.dot".to_string())
                .expect("compat product context is valid"),
            spawner,
        )
        .0
    }

    #[cfg(test)]
    fn new_compat_with_pairing(
        platform: Arc<dyn Platform>,
        spawner: Spawner,
    ) -> (Self, Arc<PairingHost>) {
        let host_config = Self::compat_host_config();
        Self::new_pairing_for_tests(
            platform,
            host_config,
            ProductContext::new("unknown.dot".to_string())
                .expect("compat product context is valid"),
            spawner,
        )
    }

    #[cfg(test)]
    fn new_pairing_for_tests(
        platform: Arc<dyn Platform>,
        host_config: truapi_platform::PairingHostConfig,
        product: ProductContext,
        spawner: Spawner,
    ) -> (Self, Arc<PairingHost>) {
        let services = RuntimeServices::new(
            platform.clone(),
            host_config.host.host_info.clone(),
            host_config.people_chain_genesis_hash,
            host_config.bulletin_chain_genesis_hash,
            host_config.asset_hub_chain_genesis_hash,
            spawner.clone(),
        );
        let pairing_host = PairingHost::new(services.clone(), host_config);
        let core_instance = services.next_core_instance();
        let chat = Arc::new(ActionChannel::chat());
        let renderer = Arc::new(ActionChannel::renderer());
        let host = Self {
            services,
            platform,
            chat_platform: None,
            permission_status: None,
            temporary_permissions: Arc::default(),
            authority: pairing_host.clone(),
            product,
            core_instance,
            chat,
            renderer,
            pocket_platform: None,
            open_operations: Mutex::new(HashSet::new()),
        };
        (host, pairing_host)
    }

    /// Test-only access to the shared session-state holder.
    #[cfg(test)]
    pub(crate) fn test_session_state(&self) -> Arc<SessionState> {
        self.authority.session_state()
    }

    /// Seed the paired Account Holder's hard product subtree for unit tests.
    #[cfg(test)]
    pub(crate) fn test_cache_product_subtree(
        &self,
        session: &SessionInfo,
        product_id: &str,
        public_key: [u8; 32],
    ) {
        self.authority
            .cache_product_subtree_for_test(session, product_id, public_key);
    }

    /// Disconnect this runtime from its paired signing host.
    #[cfg(test)]
    #[instrument(skip_all, fields(runtime.method = "account.disconnect"))]
    pub(crate) async fn disconnect(&self) {
        self.authority.disconnect().await;
    }

    fn is_product_account_valid_for_caller(&self, dot_ns_identifier: &str) -> bool {
        let Ok(dot_ns_identifier) = normalize_product_identifier(dot_ns_identifier) else {
            return false;
        };
        let product_id = self.product_id();
        // Localhost products are development-only wildcards once a host admits
        // them. Production hosts must reject localhost products before creating
        // the product runtime.
        product_id == "localhost"
            || product_id.starts_with("localhost:")
            || dot_ns_identifier == product_id
    }

    /// Resolve the grant under the caller's deadline and cancellation, answering
    /// the uniform refusal if either fires.
    ///
    /// Called before `remote_authority_call`, not inside it. Inside, two timers
    /// armed on the same budget race, and whichever fires first decides the
    /// error the caller sees: this one answers the uniform refusal, that one
    /// answers `Unknown` with a reason. The refusal shape would then depend on
    /// scheduling. Bounded here instead, the gate is decided before the
    /// authority call is made at all.
    ///
    /// Left to `remote_authority_call`, a deadline that expires during the
    /// lookup surfaces as `Unknown { reason }`, while an already-cached target
    /// that grants nothing answers immediately, so the error tag alone tells a
    /// caller which targets this device has resolved before. That is the
    /// enumeration the denial read was moved after the manifest to avoid,
    /// arriving by another route. Expiry here is indistinguishable from
    /// "granted nothing", like every other refusal on this path.
    pub(crate) async fn bounded_cross_product_scope_target(
        &self,
        target: &str,
        scope: Granted,
        cx: &CallContext,
    ) -> Option<String> {
        let lookup = self.cross_product_scope_target(target, scope).fuse();
        let cancelled = cx.cancel().cancelled().fuse();
        pin_mut!(lookup, cancelled);
        let Some(budget) = cx.timeout() else {
            return futures::select! {
                resolved = lookup => resolved,
                _ = cancelled => None,
            };
        };
        let deadline = futures_timer::Delay::new(budget).fuse();
        pin_mut!(deadline);
        futures::select! {
            resolved = lookup => resolved,
            _ = cancelled => None,
            () = deadline => None,
        }
    }

    /// The normalized id to act on when the calling product may reach `target`
    /// under `scope`, or `None` when it may not.
    ///
    /// The caller's own id is not a cross-product access and consults no grant.
    /// Any other product must name this caller in its manifest's
    /// `trustedProducts` with `scope` or `all`.
    ///
    /// Returning the id rather than a bare yes keeps one canonical spelling for
    /// the callers that go on to address the target — the grant and whatever it
    /// admits are then decided against the same string.
    pub(crate) async fn cross_product_scope_target(
        &self,
        target: &str,
        scope: Granted,
    ) -> Option<String> {
        let normalized = normalize_product_identifier(target).ok()?;
        if normalized == self.product_id() {
            return Some(normalized);
        }
        product_manifest::grants_scope(
            &self.services,
            &*self.platform,
            &self.product_id(),
            &normalized,
            scope,
        )
        .await
        .then_some(normalized)
    }

    fn normalize_product_account_id(
        product_account_id: v01::ProductAccountId,
    ) -> Result<v01::ProductAccountId, ()> {
        Ok(v01::ProductAccountId {
            dot_ns_identifier: normalize_product_identifier(&product_account_id.dot_ns_identifier)
                .map_err(|_| ())?,
            derivation_index: product_account_id.derivation_index,
        })
    }

    fn product_id(&self) -> String {
        self.product.product_id.as_str().to_string()
    }

    async fn product_account_public_key(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        product_account_id: &v01::ProductAccountId,
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
        .map_err(|err| AuthorityError::Unknown {
            reason: err.to_string(),
        })
    }

    async fn legacy_slot_zero_public_key(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
    ) -> Result<[u8; 32], String> {
        self.product_account_public_key(
            cx,
            session,
            &v01::ProductAccountId {
                dot_ns_identifier: self.product_id(),
                derivation_index: v01::DerivationIndex::Index(0),
            },
        )
        .await
        .map_err(|err| err.to_string())
    }

    /// The storage key `owner` holds `key` under.
    ///
    /// The owner is explicit because a read may be addressed at another product:
    /// deriving it from `self` would hand a granted foreign read the caller's own
    /// values instead of the ones it asked for.
    ///
    /// `owner` must already be normalized — either this product's validated id or
    /// an id returned by [`Self::cross_product_scope_target`]. `ProductStorageKey`
    /// re-applies the same normalization, so the key cannot fail to build.
    fn product_storage_key(&self, owner: &str, key: String) -> String {
        ProductStorageKey::new(owner, key)
            .expect("storage key owner was already normalized")
            .encode()
    }

    fn follow_id(&self, id: &str) -> String {
        format!("c{}:{id}", self.core_instance)
    }
}

impl ProductRuntimeHost {
    /// Read a stored permission authorization status without prompting.
    ///
    /// A device capability also resolves the host application's OS gate, so an
    /// OS refusal reads as `Denied` whatever is stored. Remote,
    /// identity-disclosure and account-access decisions have no OS gate.
    #[instrument(skip_all, fields(runtime.method = "permissions.authorization_status"))]
    pub(crate) async fn permission_authorization_status(
        &self,
        request: PermissionAuthorizationRequest,
    ) -> Result<PermissionAuthorizationStatus, v01::GenericError> {
        let service = self.permissions_service();
        service.authorization_status(&request).await
    }

    /// Read stored permission authorization statuses without prompting.
    ///
    /// A device capability also resolves the host application's OS gate, so an
    /// OS refusal reads as `Denied` whatever is stored. Remote,
    /// identity-disclosure and account-access decisions have no OS gate.
    #[instrument(skip_all, fields(runtime.method = "permissions.authorization_statuses"))]
    pub(crate) async fn permission_authorization_statuses(
        &self,
        requests: Vec<PermissionAuthorizationRequest>,
    ) -> Result<Vec<PermissionAuthorizationStatus>, v01::GenericError> {
        let service = self.permissions_service();
        service.authorization_statuses(&requests).await
    }

    /// Update a stored permission authorization status. `NotDetermined`
    /// clears the stored value so the next product request prompts again.
    #[instrument(skip_all, fields(runtime.method = "permissions.set_authorization_status"))]
    pub(crate) async fn set_permission_authorization_status(
        &self,
        request: PermissionAuthorizationRequest,
        status: PermissionAuthorizationStatus,
    ) -> Result<(), v01::GenericError> {
        let service = self.permissions_service();
        service.set_authorization_status(&request, status).await
    }

    #[instrument(skip_all, fields(runtime.method = "permissions.remote_authorization"))]
    async fn remote_permission_authorization(
        &self,
        permission: v01::RemotePermission,
    ) -> Result<PermissionAuthorizationStatus, String> {
        let service = self.permissions_service();
        service
            .authorize_remote(v01::RemotePermissionRequest { permission })
            .await
            .map_err(|err| format!("permission storage failed: {err:?}"))
    }

    /// Gate a remote call on `permission`, prompting the user when it is
    /// undetermined. Anything short of `Authorized` fails with `denied_error`.
    pub(super) async fn require_remote_permission<E>(
        &self,
        permission: v01::RemotePermission,
        denied_error: E,
    ) -> Result<(), CallError<E>> {
        match self.remote_permission_authorization(permission).await {
            Ok(PermissionAuthorizationStatus::Authorized) => Ok(()),
            Ok(
                PermissionAuthorizationStatus::Denied
                | PermissionAuthorizationStatus::NotDetermined,
            ) => Err(CallError::Domain(denied_error)),
            Err(reason) => Err(CallError::HostFailure { reason }),
        }
    }

    async fn require_chain_submit<E>(&self, denied_error: E) -> Result<(), CallError<E>> {
        self.require_remote_permission(v01::RemotePermission::ChainSubmit, denied_error)
            .await
    }

    #[instrument(skip_all, fields(runtime.method = "permissions.identity_disclosure_authorization"))]
    async fn identity_disclosure_authorization(
        &self,
    ) -> Result<PermissionAuthorizationStatus, String> {
        self.permissions_service()
            .check_or_prompt_identity_disclosure()
            .await
            .map_err(|err| format!("permission storage failed: {err:?}"))
    }

    #[instrument(skip_all, fields(runtime.method = "permissions.chat_authority_authorization"))]
    async fn chat_authority_authorization(&self) -> Result<PermissionAuthorizationStatus, String> {
        self.permissions_service()
            .check_or_prompt_chat_authority()
            .await
            .map_err(|err| format!("permission storage failed: {err:?}"))
    }

    async fn classify_legacy_address_signer(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        signer: &str,
    ) -> Result<LegacySigner, LegacySignerError> {
        let requested_key = parse_legacy_signer_hex(signer)
            .or_else(|| public_key_from_address(signer))
            .ok_or(LegacySignerError::Unavailable)?;
        self.classify_legacy_signer(cx, session, requested_key)
            .await
    }

    async fn classify_legacy_signer(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        requested_key: [u8; 32],
    ) -> Result<LegacySigner, LegacySignerError> {
        if session.identity_account_id == Some(requested_key) {
            return Ok(LegacySigner::Identity(requested_key));
        }
        let product_public_key = self
            .legacy_slot_zero_public_key(cx, session)
            .await
            .map_err(LegacySignerError::ProductDerivation)?;
        if requested_key == product_public_key {
            return Ok(LegacySigner::Product);
        }
        Err(LegacySignerError::Unavailable)
    }
}

async fn account_access_authorization(
    platform: &dyn Platform,
    requesting_product_id: &str,
    target_product_id: &str,
) -> Result<PermissionAuthorizationStatus, AccountAccessAuthorizationError> {
    if requesting_product_id == target_product_id {
        return Ok(PermissionAuthorizationStatus::Authorized);
    }

    // Both sides bare-labelled, matching the grant this decision overrides and
    // the key `user_denied_account_access` reads back. A decision filed against
    // the full target would not be found when the grant is resolved for a
    // subname of it.
    let target = crate::host_logic::product_manifest::bare_product_label(target_product_id);
    // Stored per product, not per executable, because that is the granularity a
    // manifest grant uses: `dim2.dot`, `app.dim2.dot` and `worker.dim2.dot` are
    // one grantee. A decision filed under the full id could be missed by the
    // same product arriving under a subname it already owns, which would let a
    // refused product keep a `context` grant by respelling itself. The prompt
    // still names the id the user saw; only the slot it is filed under is the
    // product's.
    let caller = crate::host_logic::product_manifest::bare_product_label(requesting_product_id);
    let cached = crate::host_logic::permissions::account_access_status(platform, caller, target)
        .await
        .map_err(AccountAccessAuthorizationError::PermissionStorage)?;
    if cached != PermissionAuthorizationStatus::NotDetermined {
        return Ok(cached);
    }

    let decision = platform
        .confirm_permission(UserConfirmationReview::AccountAccess(AccountAccessReview {
            requesting_product_id: requesting_product_id.to_string(),
            target_product_id: target_product_id.to_string(),
        }))
        .await
        .map_err(AccountAccessAuthorizationError::Confirmation)?;
    let status = match decision {
        PermissionDecision::AllowOnce => return Ok(PermissionAuthorizationStatus::Authorized),
        PermissionDecision::AllowAlways => PermissionAuthorizationStatus::Authorized,
        PermissionDecision::Deny => PermissionAuthorizationStatus::Denied,
    };
    crate::host_logic::permissions::set_account_access_status(platform, caller, target, status)
        .await
        .map_err(AccountAccessAuthorizationError::PermissionStorage)?;
    Ok(status)
}

#[derive(Debug, thiserror::Error)]
enum AccountAccessAuthorizationError {
    #[error("permission storage failed: {0:?}")]
    PermissionStorage(v01::GenericError),
    #[error("account access confirmation failed: {0:?}")]
    Confirmation(v01::GenericError),
}

fn parse_legacy_signer_hex(signer: &str) -> Option<[u8; 32]> {
    let raw = signer
        .strip_prefix("0x")
        .or_else(|| signer.strip_prefix("0X"))
        .unwrap_or(signer);
    if raw.len() != 64 {
        return None;
    }
    hex::decode(raw).ok()?.try_into().ok()
}

fn runtime_failure_to_call_error<E>(failure: RuntimeFailure) -> CallError<E> {
    CallError::HostFailure {
        reason: failure.reason(),
    }
}

/// Host-UI projection of an active session for `AuthState::Connected`.
fn connected_session_ui_info(session: &SessionInfo) -> SessionUiInfo {
    SessionUiInfo {
        public_key: session.public_key,
        identity_account_id: session.identity_account_id,
        chat_public_key: session.identity_chat_private_key.map(x25519_public_key),
        device_enc_public_key: session.device_enc_public_key,
        peer_statement_account_id: session.sso.as_ref().map(|sso| sso.identity_account_id),
        device_statement_account_id: session.sso.as_ref().map(|sso| sso.ss_public_key),
        lite_username: session.lite_username.clone(),
        full_username: session.full_username.clone(),
    }
}

const MAX_VRF_TRANSCRIPT_ITEMS: usize = 32;
const MAX_VRF_TRANSCRIPT_BYTES: usize = 8 * 1024;

fn validate_vrf_transcript(request: &v01::HostAccountSignVrfRequest) -> Result<(), String> {
    if request.items.len() > MAX_VRF_TRANSCRIPT_ITEMS {
        return Err(format!(
            "VRF transcript has {} items, at most {MAX_VRF_TRANSCRIPT_ITEMS} are allowed",
            request.items.len()
        ));
    }
    let total_bytes =
        request
            .items
            .iter()
            .try_fold(request.transcript_label.len(), |total, item| {
                total
                    .checked_add(item.label.len())
                    .and_then(|total| total.checked_add(item.value.len()))
            });
    if total_bytes.is_none_or(|total| total > MAX_VRF_TRANSCRIPT_BYTES) {
        return Err(format!(
            "VRF transcript exceeds the {MAX_VRF_TRANSCRIPT_BYTES}-byte limit"
        ));
    }
    Ok(())
}

fn vrf_call_error(err: AuthorityError) -> CallError<HostAccountSignVrfError> {
    CallError::Domain(HostAccountSignVrfError::V1(err.into()))
}
fn account_get_authority_error(err: AuthorityError) -> CallError<HostAccountGetError> {
    let error = match err {
        AuthorityError::Disconnected => v01::HostAccountGetError::NotConnected,
        AuthorityError::Rejected => v01::HostAccountGetError::Rejected,
        AuthorityError::Cancelled(err) => v01::HostAccountGetError::Unknown {
            reason: err.to_string(),
        },
        AuthorityError::Unavailable { reason }
        | AuthorityError::NotSupported { reason }
        | AuthorityError::Unknown { reason } => v01::HostAccountGetError::Unknown { reason },
    };
    CallError::Domain(HostAccountGetError::V1(error))
}

fn product_device_chat_authority_error(
    error: ProductDeviceChatAuthorityError,
) -> CallError<HostProductDeviceChatError> {
    CallError::Domain(HostProductDeviceChatError::V1(error.into()))
}

fn ring_vrf_alias_error(err: RingVrfError) -> v01::HostAccountGetAliasError {
    match err {
        RingVrfError::RingNotFound => v01::HostAccountGetAliasError::RingNotFound,
        RingVrfError::NotMember => v01::HostAccountGetAliasError::NotMember,
        RingVrfError::KeyNotRegistered => v01::HostAccountGetAliasError::KeyNotRegistered,
        RingVrfError::KeyNotInRing => v01::HostAccountGetAliasError::KeyNotInRing,
        RingVrfError::NotAllowlisted => v01::HostAccountGetAliasError::Rejected,
        RingVrfError::Rejected => v01::HostAccountGetAliasError::Rejected,
        RingVrfError::Unknown { reason } => v01::HostAccountGetAliasError::Unknown { reason },
    }
}

fn ring_vrf_proof_error(err: RingVrfError) -> v01::HostAccountCreateProofError {
    match err {
        RingVrfError::RingNotFound => v01::HostAccountCreateProofError::RingNotFound,
        RingVrfError::NotMember => v01::HostAccountCreateProofError::NotMember,
        RingVrfError::KeyNotRegistered => v01::HostAccountCreateProofError::KeyNotRegistered,
        RingVrfError::KeyNotInRing => v01::HostAccountCreateProofError::KeyNotInRing,
        RingVrfError::NotAllowlisted => v01::HostAccountCreateProofError::NotAllowlisted,
        RingVrfError::Rejected => v01::HostAccountCreateProofError::Rejected,
        RingVrfError::Unknown { reason } => v01::HostAccountCreateProofError::Unknown { reason },
    }
}

fn ring_vrf_register_error(err: RingVrfError) -> v01::HostAccountRegisterRingVrfKeyError {
    match err {
        RingVrfError::RingNotFound => v01::HostAccountRegisterRingVrfKeyError::RingNotFound,
        RingVrfError::Rejected => v01::HostAccountRegisterRingVrfKeyError::Rejected,
        RingVrfError::NotMember
        | RingVrfError::KeyNotRegistered
        | RingVrfError::KeyNotInRing
        | RingVrfError::NotAllowlisted => v01::HostAccountRegisterRingVrfKeyError::Unknown {
            reason: format!("{err:?}"),
        },
        RingVrfError::Unknown { reason } => {
            v01::HostAccountRegisterRingVrfKeyError::Unknown { reason }
        }
    }
}

fn ring_vrf_list_error(err: RingVrfError) -> v01::HostAccountListRingVrfKeysError {
    match err {
        RingVrfError::Rejected => v01::HostAccountListRingVrfKeysError::Rejected,
        RingVrfError::RingNotFound
        | RingVrfError::NotMember
        | RingVrfError::KeyNotRegistered
        | RingVrfError::KeyNotInRing
        | RingVrfError::NotAllowlisted => v01::HostAccountListRingVrfKeysError::Unknown {
            reason: format!("{err:?}"),
        },
        RingVrfError::Unknown { reason } => {
            v01::HostAccountListRingVrfKeysError::Unknown { reason }
        }
    }
}

fn ring_vrf_sign_error(err: RingVrfError) -> v01::HostAccountRingVrfSignError {
    match err {
        RingVrfError::KeyNotRegistered => v01::HostAccountRingVrfSignError::KeyNotRegistered,
        RingVrfError::NotAllowlisted => v01::HostAccountRingVrfSignError::NotAllowlisted,
        RingVrfError::Rejected => v01::HostAccountRingVrfSignError::Rejected,
        RingVrfError::RingNotFound | RingVrfError::NotMember | RingVrfError::KeyNotInRing => {
            v01::HostAccountRingVrfSignError::Unknown {
                reason: format!("{err:?}"),
            }
        }
        RingVrfError::Unknown { reason } => v01::HostAccountRingVrfSignError::Unknown { reason },
    }
}

fn signing_call_error<E>(
    wrap: fn(v01::HostSignPayloadError) -> E,
    err: AuthorityError,
) -> CallError<E> {
    CallError::Domain(wrap(match err {
        AuthorityError::Rejected | AuthorityError::Disconnected => {
            v01::HostSignPayloadError::Rejected
        }
        AuthorityError::Cancelled(err) => v01::HostSignPayloadError::Unknown {
            reason: err.to_string(),
        },
        AuthorityError::Unavailable { reason }
        | AuthorityError::NotSupported { reason }
        | AuthorityError::Unknown { reason } => v01::HostSignPayloadError::Unknown { reason },
    }))
}

fn transaction_call_error<E>(
    wrap: fn(v01::HostCreateTransactionError) -> E,
    err: AuthorityError,
) -> CallError<E> {
    CallError::Domain(wrap(match err {
        AuthorityError::Rejected | AuthorityError::Disconnected => {
            v01::HostCreateTransactionError::Rejected
        }
        AuthorityError::Cancelled(err) => v01::HostCreateTransactionError::Unknown {
            reason: err.to_string(),
        },
        AuthorityError::NotSupported { reason } => {
            v01::HostCreateTransactionError::NotSupported { reason }
        }
        AuthorityError::Unavailable { reason } | AuthorityError::Unknown { reason } => {
            v01::HostCreateTransactionError::Unknown { reason }
        }
    }))
}

const PAYMENTS_NOT_IMPLEMENTED: &str = "Payments are not supported in dot.li";

impl ProductRuntimeHost {
    /// Chat access policy for this connection; see [`chat_platform_for`].
    pub(crate) fn native_chat_platform(
        &self,
    ) -> Result<Arc<dyn truapi_platform::ChatPlatform>, crate::host_core::ProductRuntimeError> {
        chat_platform_for(
            self.product.execution_kind,
            self.authority.session_state().current().is_some(),
            self.chat_platform.as_ref(),
        )
    }

    fn chat_platform<E>(&self) -> Result<Arc<dyn truapi_platform::ChatPlatform>, CallError<E>> {
        self.native_chat_platform().map_err(|error| match error {
            crate::host_core::ProductRuntimeError::Denied => CallError::Denied,
            _ => CallError::Unsupported,
        })
    }

    pub(crate) fn detach_chat(&self) {
        self.chat.detach();
    }

    /// Buffer one host-authored Chat action for this connection's product,
    /// behind the same access policy as every other Chat entry point.
    pub(crate) fn publish_chat_action(
        &self,
        action: truapi::versioned::chat::HostChatActionSubscribeItem,
    ) -> Result<(), crate::host_core::ProductRuntimeError> {
        self.native_chat_platform()?;
        self.chat.publish(action)
    }

    /// Renderer access policy for this connection; see [`renderer_access_for`].
    pub(crate) fn renderer_access(&self) -> Result<(), crate::host_core::ProductRuntimeError> {
        renderer_access_for(self.product.execution_kind)
    }

    /// Take one core-held reference on this connection's product worker, for
    /// a body the product is drawing. Pair every call with one
    /// [`Self::release_worker_reference`].
    pub(crate) fn acquire_worker_reference(&self) {
        self.services
            .worker_ledger
            .acquire(&self.product.product_id);
    }

    /// Release one core-held reference on this connection's product worker.
    pub(crate) fn release_worker_reference(&self) {
        self.services
            .worker_ledger
            .release(&self.product.product_id);
    }

    /// Begin a pending operation with the host, on a task this dispatch's
    /// cancellation cannot reach.
    ///
    /// A cancelled dispatch drops whatever it is awaiting, and dropping the
    /// host's call mid-answer would leave the host holding an operation the
    /// core never counted and the product never learned the id of, which
    /// nothing could then end. The call runs to completion either way, and
    /// ends the operation itself when nobody is left to receive it.
    pub(crate) async fn begin_operation_with_host(
        &self,
        label: String,
    ) -> Result<v01::HostWorkerBeginOperationResponse, v01::HostWorkerOperationError> {
        let (tx, rx) = futures::channel::oneshot::channel();
        let platform = self.platform.clone();
        let product = self.product.clone();
        (self.services.spawner)(Box::pin(async move {
            let begun = platform.begin_operation(&product, label).await;
            if let Err(Ok(response)) = tx.send(begun) {
                let _ = platform.end_operation(&product, response.id).await;
            }
        }));
        rx.await.unwrap_or_else(|_| {
            Err(v01::HostWorkerOperationError::Unknown {
                reason: "the host did not answer".to_string(),
            })
        })
    }

    /// Record a pending operation and take the worker reference it holds, so
    /// an operation outliving the product's surface still reads as demand.
    pub(crate) fn hold_worker_for_operation(&self, id: u32) {
        if self
            .open_operations
            .lock()
            .expect("open operations mutex poisoned")
            .insert(id)
        {
            self.acquire_worker_reference();
        }
    }

    /// Drop every worker reference this connection's open operations hold.
    ///
    /// Teardown calls this rather than leaving it to `Drop`: a disposed
    /// connection can outlive its last `Arc` holder, and a reference kept past
    /// dispose would leave the host running a worker for a connection that is
    /// gone.
    ///
    /// Telling the host runs on the spawner, so a spawner whose runtime is
    /// already gone drops that work. The references are still released, and
    /// the host is shutting down with its own records anyway.
    pub(crate) fn release_open_operations(&self) {
        let open = core::mem::take(
            &mut *self
                .open_operations
                .lock()
                .expect("open operations mutex poisoned"),
        );
        if open.is_empty() {
            return;
        }
        for _ in &open {
            self.release_worker_reference();
        }
        // The host holds its own record of each operation, and nothing else
        // ever ends one for a connection that is gone: left alone they
        // accumulate against whatever limit the host puts on a product's open
        // operations. Ending them reaches the host, so it runs off this
        // thread.
        let platform = self.platform.clone();
        let product = self.product.clone();
        (self.services.spawner)(Box::pin(async move {
            for id in open {
                let _ = platform.end_operation(&product, id).await;
            }
        }));
    }

    /// Drop the worker reference a pending operation held. An id that is not
    /// open releases nothing, which is what keeps `end_operation` idempotent.
    pub(crate) fn release_worker_for_operation(&self, id: u32) {
        if self
            .open_operations
            .lock()
            .expect("open operations mutex poisoned")
            .remove(&id)
        {
            self.release_worker_reference();
        }
    }

    /// End the renderer action stream this connection's product is reading.
    pub(crate) fn detach_renderer(&self) {
        self.renderer.detach();
    }

    /// Buffer one renderer action for this connection's product.
    pub(crate) fn publish_renderer_action(
        &self,
        item: HostRendererActionSubscribeItem,
    ) -> Result<(), crate::host_core::ProductRuntimeError> {
        self.renderer_access()?;
        self.renderer.publish(item)
    }

    /// Pocket access policy for this connection: the collection is reachable
    /// only from a Worker execution with an active session, and only where the
    /// host installed an adapter. The kind and session checks come first, so a
    /// connection that may never reach Pocket is told `Denied` even on a host
    /// that serves nothing.
    fn pocket_platform<E>(&self) -> Result<Arc<dyn truapi_platform::PocketPlatform>, CallError<E>> {
        if self.product.execution_kind != truapi_platform::ProductExecutionKind::Worker
            || self.authority.session_state().current().is_none()
        {
            return Err(CallError::Denied);
        }
        self.pocket_platform.clone().ok_or(CallError::Unsupported)
    }
}

#[truapi_platform::async_trait]
impl Chat for ProductRuntimeHost {
    #[instrument(skip_all, fields(runtime.method = "chat.create_room"))]
    async fn create_room(
        &self,
        _cx: &CallContext,
        request: HostChatCreateRoomRequest,
    ) -> Result<HostChatCreateRoomResponse, CallError<HostChatCreateRoomError>> {
        let platform = self.chat_platform()?;
        let HostChatCreateRoomRequest::V1(mut request) = request;
        request.room_id = normalize_chat_identifier("roomId", &request.room_id)
            .map_err(chat_create_room_field_error)?;
        request.name =
            validate_chat_name("name", &request.name).map_err(chat_create_room_field_error)?;
        request.icon =
            validate_chat_icon("icon", &request.icon).map_err(chat_create_room_field_error)?;
        platform
            .create_chat_room(&self.product, request)
            .await
            .map(HostChatCreateRoomResponse::V1)
            .map_err(|error| CallError::Domain(HostChatCreateRoomError::V1(error)))
    }

    #[instrument(skip_all, fields(runtime.method = "chat.register_bot"))]
    async fn register_bot(
        &self,
        _cx: &CallContext,
        request: HostChatRegisterBotRequest,
    ) -> Result<HostChatRegisterBotResponse, CallError<HostChatRegisterBotError>> {
        let platform = self.chat_platform()?;
        let HostChatRegisterBotRequest::V1(mut request) = request;
        request.bot_id = normalize_chat_identifier("botId", &request.bot_id)
            .map_err(chat_register_bot_field_error)?;
        request.name =
            validate_chat_name("name", &request.name).map_err(chat_register_bot_field_error)?;
        request.icon =
            validate_chat_icon("icon", &request.icon).map_err(chat_register_bot_field_error)?;
        platform
            .register_chat_bot(&self.product, request)
            .await
            .map(HostChatRegisterBotResponse::V1)
            .map_err(|error| CallError::Domain(HostChatRegisterBotError::V1(error)))
    }

    #[instrument(skip_all, fields(runtime.method = "chat.list_subscribe"))]
    async fn list_subscribe(
        &self,
        _cx: &CallContext,
        _request: HostChatListSubscribeRequest,
    ) -> Subscription<HostChatListSubscribeItem, CallError<HostChatListSubscribeError>> {
        let platform = match self.chat_platform::<HostChatListSubscribeError>() {
            Ok(platform) => platform,
            Err(error) => return Subscription::interrupted(error),
        };
        Subscription::new(
            platform
                .subscribe_chat_rooms(&self.product)
                .map(|item| match item {
                    Ok(item) => Ok(HostChatListSubscribeItem::V1(item)),
                    Err(error) => {
                        warn!(
                            reason = %error.reason,
                            "chat room list platform stream failed"
                        );
                        Err(CallError::HostFailure {
                            reason: error.reason,
                        })
                    }
                }),
        )
    }

    #[instrument(skip_all, fields(runtime.method = "chat.post_message"))]
    async fn post_message(
        &self,
        _cx: &CallContext,
        request: HostChatPostMessageRequest,
    ) -> Result<HostChatPostMessageResponse, CallError<HostChatPostMessageError>> {
        let platform = self.chat_platform()?;
        let HostChatPostMessageRequest::V1(mut request) = request;
        // The same normalization create_room applied, so a product's own
        // spelling of a room id still resolves to the stored room.
        request.room_id =
            normalize_chat_identifier("roomId", &request.room_id).map_err(chat_post_field_error)?;
        // Content is product-authored and host-rendered, so it gets the same
        // treatment the room fields get rather than reaching a host raw.
        request.payload =
            validate_chat_message_content(request.payload).map_err(chat_post_field_error)?;
        platform
            .post_chat_message(&self.product, request)
            .await
            .map(HostChatPostMessageResponse::V1)
            .map_err(|error| CallError::Domain(HostChatPostMessageError::V1(error)))
    }

    #[instrument(skip_all, fields(runtime.method = "chat.action_subscribe"))]
    async fn action_subscribe(
        &self,
        _cx: &CallContext,
        _request: HostChatActionSubscribeRequest,
    ) -> Subscription<HostChatActionSubscribeItem, CallError<HostChatActionSubscribeError>> {
        if let Err(error) = self.chat_platform::<HostChatActionSubscribeError>() {
            return Subscription::interrupted(error);
        }
        self.chat.subscribe()
    }
}

#[truapi_platform::async_trait]
impl Renderer for ProductRuntimeHost {
    #[instrument(skip_all, fields(runtime.method = "renderer.action_subscribe"))]
    async fn action_subscribe(
        &self,
        _cx: &CallContext,
        _request: HostRendererActionSubscribeRequest,
    ) -> Subscription<HostRendererActionSubscribeItem, CallError<HostRendererActionSubscribeError>>
    {
        if self.renderer_access().is_err() {
            return Subscription::interrupted(CallError::Denied);
        }
        self.renderer.subscribe()
    }
}

#[truapi::async_trait]
impl Pocket for ProductRuntimeHost {
    #[instrument(skip_all, fields(runtime.method = "pocket.list_subscribe"))]
    async fn list_subscribe(
        &self,
        _cx: &CallContext,
        _request: HostPocketListSubscribeRequest,
    ) -> Subscription<HostPocketListSubscribeItem, CallError<HostPocketListSubscribeError>> {
        let platform = match self.pocket_platform::<HostPocketListSubscribeError>() {
            Ok(platform) => platform,
            Err(error) => return Subscription::interrupted(error),
        };
        Subscription::new(
            platform
                .subscribe_pocket_cards(&self.product)
                .map(|item| match item {
                    Ok(item) => Ok(HostPocketListSubscribeItem::V1(item)),
                    Err(error) => {
                        warn!(
                            reason = %error.reason,
                            "pocket card list platform stream failed"
                        );
                        Err(CallError::HostFailure {
                            reason: error.reason,
                        })
                    }
                }),
        )
    }

    #[instrument(skip_all, fields(runtime.method = "pocket.remove_card"))]
    async fn remove_card(
        &self,
        _cx: &CallContext,
        request: HostPocketRemoveCardRequest,
    ) -> Result<HostPocketRemoveCardResponse, CallError<HostPocketRemoveCardError>> {
        let platform = self.pocket_platform()?;
        let HostPocketRemoveCardRequest::V1(mut request) = request;
        // A card id is a product-chosen label the host renders in its own
        // chrome, so it is screened before the host ever sees it: trimmed,
        // NFC-normalized, and rejected if it carries characters that let two
        // distinct ids render identically.
        request.card_id =
            normalize_chat_identifier("cardId", &request.card_id).map_err(pocket_field_error)?;
        platform
            .remove_pocket_card(&self.product, request)
            .await
            .map(|()| HostPocketRemoveCardResponse::V1)
            .map_err(|error| CallError::Domain(HostPocketRemoveCardError::V1(error)))
    }
}

/// Report a rejected card id as a removal domain error.
fn pocket_field_error(error: ChatFieldError) -> CallError<HostPocketRemoveCardError> {
    CallError::Domain(HostPocketRemoveCardError::V1(
        v01::HostPocketRemoveCardError::Unknown {
            reason: error.to_string(),
        },
    ))
}

/// Report a rejected chat bot field as a bot-registration domain error.
fn chat_register_bot_field_error(
    error: truapi_platform::ChatFieldError,
) -> CallError<HostChatRegisterBotError> {
    CallError::Domain(HostChatRegisterBotError::V1(
        v01::HostChatRegisterBotError::Unknown {
            reason: error.to_string(),
        },
    ))
}

/// Fields whose rejection a product resolves by sending less content, and the
/// only ones the fieldless `MessageTooLarge` can describe.
const CHAT_SIZED_CONTENT_FIELDS: [&str; 2] = ["text", "payload"];

/// Maps a rejected `post_message` field onto the wire error, reporting a size
/// rejection as the size variant the protocol declares for it.
fn chat_post_field_error(error: ChatFieldError) -> CallError<HostChatPostMessageError> {
    let payload = match error {
        // `MessageTooLarge` names no field, so it is reserved for the content
        // a product shrinks by sending less. Every other rejection reports
        // through `Unknown`, whose reason names the field and its limit.
        ChatFieldError::TooLong { field, .. } if CHAT_SIZED_CONTENT_FIELDS.contains(&field) => {
            v01::HostChatPostMessageError::MessageTooLarge
        }
        error => v01::HostChatPostMessageError::Unknown {
            reason: error.to_string(),
        },
    };
    CallError::Domain(HostChatPostMessageError::V1(payload))
}

/// Report a rejected chat room field as a room-creation domain error.
fn chat_create_room_field_error(
    error: truapi_platform::ChatFieldError,
) -> CallError<HostChatCreateRoomError> {
    CallError::Domain(HostChatCreateRoomError::V1(
        v01::HostChatCreateRoomError::Unknown {
            reason: error.to_string(),
        },
    ))
}

impl ProductRuntimeHost {
    /// Cache a just-submitted preimage under its key for immediate lookups.
    fn prime_preimage_cache(&self, key: &[u8], value: Vec<u8>) {
        if let Ok(key_bytes) = <[u8; 32]>::try_from(key) {
            debug_assert_eq!(key_bytes, preimage_key(&value));
            self.services.cache_preimage(key_bytes, value);
        }
    }
}

/// Build the product-facing `Unknown` wire error carrying `reason`.
fn preimage_submit_error(reason: String) -> CallError<RemotePreimageSubmitError> {
    CallError::Domain(RemotePreimageSubmitError::V1(
        v01::PreimageSubmitError::Unknown { reason },
    ))
}

fn bulletin_allowance_error_reason(err: AuthorityError) -> String {
    match err {
        AuthorityError::Rejected => {
            "Bulletin allowance allocation was rejected by the signing host".to_string()
        }
        AuthorityError::Disconnected => {
            "Signing host disconnected while allocating Bulletin allowance".to_string()
        }
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests;
