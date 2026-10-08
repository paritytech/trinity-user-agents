//! `ProductRuntimeHost` adapts one product connection into the
//! typed `truapi::api::*` host traits the generated dispatcher routes to.
//!
//! Most methods are straight delegations to the platform; the rest carry
//! host-agnostic logic owned by the core (the chainHead-v1 runtime behind
//! the Chain surface, `dotns` URL parsing for `navigate_to`, and the
//! permission cache layer). Methods with no platform backing return
//! `CallError::unavailable()`.

/// Connection-scoped, host-fed action streams.
pub mod actions;
mod allowances;
/// Core-owned auth/session UI state machine.
pub mod auth_state;
mod authority;
/// In-core Bulletin preimage submission over the shared Subxt client.
pub mod bulletin_rpc;
mod capabilities;
mod chat;
pub mod contacts;
mod dotns_lookup;
mod host_accounts;
mod host_grants;
mod host_session;
mod identity;
pub mod login_failure;
pub mod product_manifest;
mod product_subtree;
mod renderer;
mod ring_vrf_registry;
/// Role-neutral runtime services shared by product-facing runtimes.
pub mod services;
mod signing_host;
mod sso_account_holder_client;
mod sso_account_holder_service;
/// SSO remote request/response messaging over the statement store.
pub mod sso_remote;
mod sso_request_service;
pub mod sso_service;
/// Statement Store and Bulletin allowance allocation.
pub mod statement_allowance;
/// `StatementStore` surface: proofs plus submit and subscribe flows.
pub mod statement_store;
mod statement_store_rpc;
mod vrf;

use core::future::Future;
use core::time::Duration;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;

pub use actions::ActionChannel;
use authority::AuthorityCancelError;
pub use authority::{
    AccountCaller, AccountHolder, AuthorityError, AuthoritySession, BulletinAllowanceKey,
};
/// Wallet-issued permission for one product during one activation.
#[derive(Clone)]
pub struct WalletAuthorization {
    issuer: std::sync::Weak<crate::host_logic::session::SessionState>,
    validation_id: Vec<u8>,
    product_id: String,
}

pub use chat::chat_platform_for;
pub use contacts::ContactResolutionError;

/// The host's contact picker plus the key its handles are minted under:
/// everything one `contacts.pick` call needs from the connection.
type ContactsPicker = (
    Arc<dyn crate::platform::ContactsPlatform>,
    crate::runtime::contacts::ContactHandles,
);
use futures::{FutureExt, StreamExt, pin_mut};
pub use host_accounts::HostAccounts;
#[cfg(feature = "test-host")]
mod test_resource_controls;
pub use host_grants::HostGrantStore;
pub use host_session::HostSession;
pub use renderer::renderer_access_for;
pub use services::RuntimeServices;
pub use signing_host::{
    AnnouncedPairing, DevicePairingObserver, MAX_PAIRING_METADATA_CHARS, PairedSsoPeer,
    PairingProposal, PairingProposalMetadata, ResponderExit,
};
pub use signing_host::{
    LocalActivation, SigningHost as SigningHostRole, disconnect_paired_host, establish_pairing,
    notify_pairing_allowance_allocation, notify_pairing_failed, respond_to_pairing, resume_pairing,
};
pub use sso_account_holder_client::SsoAccountHolderClient;
pub use sso_account_holder_service::SsoAccountHolderService;
pub use sso_request_service::SsoRequestService;
#[cfg(all(target_arch = "wasm32", feature = "test-host"))]
pub use vrf::ring_vrf_member;
// `TrackedStatementRenewalTarget` is only read back by the native renewal
// reporting, so re-exporting it on wasm leaves an unused import.
use crate::platform::{
    AccountAccessReview, ChatFieldError, IdentityDisclosureReview, PermissionAuthorizationRequest,
    PermissionAuthorizationStatus, PermissionDecision, Platform, ProductContext, ProductStorageKey,
    SessionUiInfo, UserConfirmationReview, normalize_chat_identifier, normalize_product_identifier,
    validate_chat_icon, validate_chat_message_content, validate_chat_name,
};
pub use signing_host::StatementRenewalTarget;
#[cfg(not(target_arch = "wasm32"))]
pub use signing_host::TrackedStatementRenewalTarget;
use tracing::{instrument, warn};
use truapi::api::{Chat, Contacts, Pocket, Renderer};
use truapi::versioned::account::HostAccountSignVrfError;
use truapi::versioned::chat::{
    HostChatActionSubscribeError, HostChatActionSubscribeItem, HostChatActionSubscribeRequest,
    HostChatCreateRoomError, HostChatCreateRoomRequest, HostChatCreateRoomResponse,
    HostChatListSubscribeError, HostChatListSubscribeItem, HostChatListSubscribeRequest,
    HostChatPostMessageError, HostChatPostMessageRequest, HostChatPostMessageResponse,
    HostChatRegisterBotError, HostChatRegisterBotRequest, HostChatRegisterBotResponse,
};
use truapi::versioned::contacts::{
    HostContactsPickError, HostContactsPickRequest, HostContactsPickResponse,
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
use truapi::{CallContext, CallError, CancellationReason, Subscription, latest, v01};
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

use crate::chain_runtime::RuntimeFailure;
use crate::host_internal::bulletin::preimage_key;
use crate::host_internal::permissions::{PermissionsService, TemporaryPermissions};
use crate::host_internal::product_manifest::Granted;
use crate::host_internal::sso_messages::RingVrfError;
use crate::host_logic::product_account::public_key_from_address;
use crate::host_logic::session::{SessionInfo, SessionState};
use crate::host_logic::sso::pairing::x25519_public_key;
#[cfg(test)]
use crate::subscription::Spawner;

/// Error reason surfaced to products when a permission is not granted.
pub const PERMISSION_DENIED_REASON: &str = "Permission denied";
/// Host-spec B.6.2 recommends timing out unanswered SSO application requests
/// after 180 seconds:
/// <https://github.com/paritytech/host-spec/blob/adb3989208ae1c2107dbf0159611353e6989422c/spec/B-inter-host.md?plain=1#L303-L307>
pub const DEFAULT_REMOTE_AUTHORITY_RESPONSE_TIMEOUT: Duration = Duration::from_secs(180);
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
    // A call already withdrawn never sends its request, not even within the
    // unwind grace below.
    if let Some(reason) = cx.cancel().reason() {
        return Err(authority_cancellation_error(cx, reason).into());
    }
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

/// Await `wait` unless the call is cancelled first.
///
/// For waits a person controls, such as a local confirmation prompt: a
/// withdrawn call stops waiting, so an answer given after the withdrawal
/// authorizes nothing. The error is the one `remote_authority_call` answers,
/// so each caller maps it into its method's own domain error.
async fn until_cancelled<T>(
    cx: &CallContext,
    wait: impl Future<Output = T>,
) -> Result<T, AuthorityError> {
    let wait = wait.fuse();
    let cancelled = cx.cancel().cancelled().fuse();
    pin_mut!(wait, cancelled);
    futures::select_biased! {
        reason = cancelled => Err(authority_cancellation_error(cx, reason)),
        output = wait => Ok(output),
    }
}

fn authority_cancellation_error(cx: &CallContext, reason: CancellationReason) -> AuthorityError {
    AuthorityError::Cancelled(AuthorityCancelError::new(cx.request_id(), reason))
}

/// Product-scoped adapter that exposes a long-lived host runtime through the
/// `truapi::api::*` trait set the generated dispatcher routes to.
pub struct ProductRuntimeHost<H: AccountHolder> {
    connection: Arc<ProductConnection>,
    accounts: Arc<HostAccounts<H>>,
    host_session: Arc<dyn HostSession>,
}

/// Per-connection adapters, permissions, channels and open operation demand.
pub struct ProductConnection {
    services: Arc<RuntimeServices>,
    platform: Arc<dyn Platform>,
    chat_platform: Option<Arc<dyn crate::platform::ChatPlatform>>,
    /// Live OS permission state for this connection, when the host serves it.
    permission_status: Option<Arc<dyn crate::platform::PermissionStatusHost>>,
    /// Permission requests and consuming operations can arrive on different connections.
    temporary_permissions: Arc<TemporaryPermissions>,
    session_state: Arc<SessionState>,
    product: ProductContext,
    core_instance: u64,
    chat: Arc<ActionChannel<HostChatActionSubscribeItem>>,
    renderer: Arc<ActionChannel<HostRendererActionSubscribeItem>>,
    pocket_platform: Option<Arc<dyn crate::platform::PocketPlatform>>,
    game_platform: Option<Arc<dyn crate::platform::GamePlatform>>,
    open_operations: Mutex<HashSet<u32>>,
}

impl Drop for ProductConnection {
    fn drop(&mut self) {
        self.release_open_operations();
    }
}

impl ProductConnection {
    /// Role-neutral services shared with the owning host runtime.
    pub fn services(&self) -> &Arc<RuntimeServices> {
        &self.services
    }

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
    pub fn execution_kind(&self) -> crate::platform::ProductExecutionKind {
        self.product.execution_kind
    }

    /// Canonical account owner allowed by this product's manifest grants.
    async fn authorized_product_account(
        &self,
        dot_ns_identifier: &str,
        cx: &CallContext,
    ) -> Option<String> {
        let product_id = self.product_id();
        // Admission already restricts localhost products to development hosts.
        if crate::platform::is_localhost_product_identifier(&product_id) {
            return normalize_product_identifier(dot_ns_identifier).ok();
        }
        let cx = remote_authority_context(cx);
        self.bounded_cross_product_scope_target(dot_ns_identifier, Granted::Context, &cx)
            .await
    }

    /// Resolve access before authority execution; timeout and cancellation answer
    /// the same refusal as a missing grant, without revealing cached targets.
    async fn bounded_cross_product_scope_target(
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

    /// Canonical target whose manifest grants this caller the requested scope.
    async fn cross_product_scope_target(&self, target: &str, scope: Granted) -> Option<String> {
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

    fn product_id(&self) -> String {
        self.product.product_id.as_str().to_string()
    }

    // Foreign reads must keep the authorized owner, not the calling product.
    fn product_storage_key(&self, owner: &str, key: String) -> String {
        ProductStorageKey::new(owner, key)
            .expect("storage key owner was already normalized")
            .encode()
    }

    fn follow_id(&self, id: &str) -> String {
        format!("c{}:{id}", self.core_instance)
    }

    /// Read stored authorization, including the current OS gate for device access.
    #[instrument(skip_all, fields(runtime.method = "permissions.authorization_status"))]
    pub async fn permission_authorization_status(
        &self,
        request: PermissionAuthorizationRequest,
    ) -> Result<PermissionAuthorizationStatus, latest::GenericError> {
        let service = self.permissions_service();
        service.authorization_status(&request).await
    }

    /// Read stored authorizations, including current OS gates for device access.
    #[instrument(skip_all, fields(runtime.method = "permissions.authorization_statuses"))]
    pub async fn permission_authorization_statuses(
        &self,
        requests: Vec<PermissionAuthorizationRequest>,
    ) -> Result<Vec<PermissionAuthorizationStatus>, latest::GenericError> {
        let service = self.permissions_service();
        service.authorization_statuses(&requests).await
    }

    /// Store authorization; `NotDetermined` clears the saved decision.
    #[instrument(skip_all, fields(runtime.method = "permissions.set_authorization_status"))]
    pub async fn set_permission_authorization_status(
        &self,
        request: PermissionAuthorizationRequest,
        status: PermissionAuthorizationStatus,
    ) -> Result<(), latest::GenericError> {
        let service = self.permissions_service();
        service.set_authorization_status(&request, status).await
    }

    #[instrument(skip_all, fields(runtime.method = "permissions.remote_authorization"))]
    async fn remote_permission_authorization(
        &self,
        permission: latest::RemotePermission,
    ) -> Result<PermissionAuthorizationStatus, String> {
        let service = self.permissions_service();
        service
            .authorize_remote(latest::RemotePermissionRequest { permission })
            .await
            .map_err(|err| format!("permission storage failed: {err:?}"))
    }

    /// Require remote authorization, prompting only when no decision is stored.
    async fn require_remote_permission<E>(
        &self,
        permission: latest::RemotePermission,
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

    async fn confirm_product_action(
        &self,
        review: UserConfirmationReview,
    ) -> Result<bool, latest::GenericError> {
        if crate::platform::has_trusted_remote_permissions(&self.product_id()) {
            return Ok(true);
        }
        self.platform.confirm_user_action(review).await
    }

    async fn require_chain_submit<E>(&self, denied_error: E) -> Result<(), CallError<E>> {
        self.require_remote_permission(latest::RemotePermission::ChainSubmit, denied_error)
            .await
    }

    #[instrument(skip_all, fields(runtime.method = "permissions.identity_disclosure_authorization"))]
    async fn identity_disclosure_authorization(
        &self,
    ) -> Result<PermissionAuthorizationStatus, String> {
        let product_id = self.product_id();
        let request = PermissionAuthorizationRequest::IdentityDisclosure;
        let service = self.permissions_service();
        let cached = service
            .authorization_status(&request)
            .await
            .map_err(|err| format!("permission storage failed: {err:?}"))?;
        if cached != PermissionAuthorizationStatus::NotDetermined {
            return Ok(cached);
        }

        // A dismissed confirmation must not persist a refusal.
        let decision = match self
            .platform
            .confirm_permission(UserConfirmationReview::IdentityDisclosure(
                IdentityDisclosureReview {
                    product_id: product_id.clone(),
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
        service
            .set_authorization_status(&request, status)
            .await
            .map_err(|err| format!("permission storage failed: {err:?}"))?;
        Ok(status)
    }

    /// Chat access policy for this connection; see [`chat_platform_for`].
    fn native_chat_platform(
        &self,
    ) -> Result<Arc<dyn crate::platform::ChatPlatform>, crate::host_core::ProductRuntimeError> {
        chat_platform_for(
            self.product.execution_kind,
            self.session_state.current().is_some(),
            self.chat_platform.as_ref(),
        )
    }

    fn chat_platform<E>(&self) -> Result<Arc<dyn crate::platform::ChatPlatform>, CallError<E>> {
        self.native_chat_platform().map_err(|error| match error {
            crate::host_core::ProductRuntimeError::Denied => CallError::Denied,
            _ => CallError::Unsupported,
        })
    }

    /// End this connection's Chat action stream.
    pub fn detach_chat(&self) {
        self.chat.detach();
    }

    /// Buffer one host-authored Chat action for this connection's product,
    /// behind the same access policy as every other Chat entry point.
    pub fn publish_chat_action(
        &self,
        action: truapi::versioned::chat::HostChatActionSubscribeItem,
    ) -> Result<(), crate::host_core::ProductRuntimeError> {
        self.native_chat_platform()?;
        self.chat.publish(action)
    }

    /// Renderer access policy for this connection; see [`renderer_access_for`].
    pub fn renderer_access(&self) -> Result<(), crate::host_core::ProductRuntimeError> {
        renderer_access_for(self.product.execution_kind)
    }

    /// Hold worker demand until the matching [`Self::release_worker_reference`].
    pub fn acquire_worker_reference(&self) {
        self.services
            .worker_ledger
            .acquire(&self.product.product_id);
    }

    /// Release one core-held reference on this connection's product worker.
    pub fn release_worker_reference(&self) {
        self.services
            .worker_ledger
            .release(&self.product.product_id);
    }

    /// Begin off-dispatch so cancellation cannot strand an unreported host operation.
    async fn begin_operation_with_host(
        &self,
        label: String,
    ) -> Result<latest::HostWorkerBeginOperationResponse, latest::HostWorkerOperationError> {
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
            Err(latest::HostWorkerOperationError::Unknown {
                reason: "the host did not answer".to_string(),
            })
        })
    }

    /// Hold worker demand until this operation ends or the connection closes.
    fn hold_worker_for_operation(&self, id: u32) {
        if self
            .open_operations
            .lock()
            .expect("open operations mutex poisoned")
            .insert(id)
        {
            self.acquire_worker_reference();
        }
    }

    /// Release all operation demand immediately and schedule host cleanup.
    /// Repeated calls leave already released operations alone.
    pub fn release_open_operations(&self) {
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
        let platform = self.platform.clone();
        let product = self.product.clone();
        (self.services.spawner)(Box::pin(async move {
            for id in open {
                let _ = platform.end_operation(&product, id).await;
            }
        }));
    }

    /// Release one open operation's demand; unknown ids leave demand unchanged.
    fn release_worker_for_operation(&self, id: u32) {
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
    pub fn detach_renderer(&self) {
        self.renderer.detach();
    }

    /// Buffer one renderer action for this connection's product.
    pub fn publish_renderer_action(
        &self,
        item: HostRendererActionSubscribeItem,
    ) -> Result<(), crate::host_core::ProductRuntimeError> {
        self.renderer_access()?;
        self.renderer.publish(item)
    }

    fn pocket_platform<E>(&self) -> Result<Arc<dyn crate::platform::PocketPlatform>, CallError<E>> {
        if self.product.execution_kind != crate::platform::ProductExecutionKind::Worker
            || self.session_state.current().is_none()
        {
            return Err(CallError::Denied);
        }
        self.pocket_platform.clone().ok_or(CallError::Unsupported)
    }
}

impl<H: AccountHolder> ProductRuntimeHost<H> {
    /// Build a product-scoped dispatcher target from a long-lived host runtime
    /// and the adapters scoped to this product connection.
    pub fn from_services(
        services: Arc<RuntimeServices>,
        adapters: crate::host_core::ConnectionAdapters,
        accounts: Arc<HostAccounts<H>>,
        host_session: Arc<dyn HostSession>,
        product: ProductContext,
    ) -> Self {
        let core_instance = services.next_core_instance();
        let connection = Arc::new(ProductConnection {
            services,
            platform: adapters.platform,
            chat_platform: adapters.chat_platform,
            permission_status: adapters.permission_status,
            temporary_permissions: adapters.permission_grants,
            session_state: host_session.session_state(),
            product,
            core_instance,
            chat: adapters.chat,
            renderer: adapters.renderer,
            pocket_platform: adapters.pocket_platform,
            game_platform: adapters.game_platform,
            open_operations: Mutex::new(HashSet::new()),
        });
        Self {
            connection,
            accounts,
            host_session,
        }
    }

    /// Shared connection state used by product control handles.
    pub fn connection(&self) -> &Arc<ProductConnection> {
        &self.connection
    }

    /// Lifecycle owner shared by this connection and its host administration.
    pub fn host_session(&self) -> &Arc<dyn HostSession> {
        &self.host_session
    }

    /// Test-only access to the shared session-state holder.
    #[cfg(test)]
    fn test_session_state(&self) -> Arc<SessionState> {
        self.host_session.session_state()
    }

    /// Seed the paired Account Holder's hard product subtree for unit tests.
    #[cfg(test)]
    fn test_cache_product_subtree(
        &self,
        session: &SessionInfo,
        product_id: &str,
        public_key: [u8; 32],
    ) {
        self.accounts
            .cache_product_subtree_for_test(session, product_id, public_key);
    }

    /// Disconnect this runtime from its paired signing host.
    #[cfg(test)]
    #[instrument(skip_all, fields(runtime.method = "account.disconnect"))]
    async fn disconnect(&self) {
        self.host_session.disconnect().await;
    }

    fn normalize_product_account_id(
        product_account_id: latest::ProductAccountId,
    ) -> Result<latest::ProductAccountId, ()> {
        Ok(latest::ProductAccountId {
            dot_ns_identifier: normalize_product_identifier(&product_account_id.dot_ns_identifier)
                .map_err(|_| ())?,
            derivation_index: product_account_id.derivation_index,
        })
    }

    async fn legacy_slot_zero_public_key(
        &self,
        cx: &CallContext,
        authority_session: &AuthoritySession,
    ) -> Result<[u8; 32], String> {
        self.accounts
            .product_account_public_key(
                cx,
                authority_session,
                &self.connection.product,
                &latest::ProductAccountId {
                    dot_ns_identifier: self.connection.product_id(),
                    derivation_index: latest::DerivationIndex::Index(0),
                },
                None,
            )
            .await
            .map_err(|err| err.to_string())
    }

    async fn classify_legacy_address_signer(
        &self,
        cx: &CallContext,
        authority_session: &AuthoritySession,
        signer: &str,
    ) -> Result<LegacySigner, LegacySignerError> {
        let requested_key = parse_legacy_signer_hex(signer)
            .or_else(|| public_key_from_address(signer))
            .ok_or(LegacySignerError::Unavailable)?;
        self.classify_legacy_signer(cx, authority_session, requested_key)
            .await
    }

    async fn classify_legacy_signer(
        &self,
        cx: &CallContext,
        authority_session: &AuthoritySession,
        requested_key: [u8; 32],
    ) -> Result<LegacySigner, LegacySignerError> {
        if authority_session.identity_account_id == Some(requested_key) {
            return Ok(LegacySigner::Identity(requested_key));
        }
        let product_public_key = self
            .legacy_slot_zero_public_key(cx, authority_session)
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
    if requesting_product_id == target_product_id
        || crate::platform::normalizes_to_trusted_remote_permissions(requesting_product_id)
    {
        return Ok(PermissionAuthorizationStatus::Authorized);
    }

    // Both sides bare-labelled, matching the grant this decision overrides and
    // the key `user_denied_account_access` reads back. A decision filed against
    // the full target would not be found when the grant is resolved for a
    // subname of it.
    let target = crate::host_internal::product_manifest::bare_product_label(target_product_id);
    // Stored per product, not per executable, because that is the granularity a
    // manifest grant uses: `dim2.dot`, `app.dim2.dot` and `worker.dim2.dot` are
    // one grantee. A decision filed under the full id could be missed by the
    // same product arriving under a subname it already owns, which would let a
    // refused product keep a `context` grant by respelling itself. The prompt
    // still names the id the user saw; only the slot it is filed under is the
    // product's.
    let caller = crate::host_internal::product_manifest::bare_product_label(requesting_product_id);
    let cached = crate::host_internal::permissions::account_access_status(platform, caller, target)
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
    crate::host_internal::permissions::set_account_access_status(platform, caller, target, status)
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

fn account_get_authority_error(err: AuthorityError) -> CallError<latest::HostAccountGetError> {
    let error = match err {
        AuthorityError::Disconnected => latest::HostAccountGetError::NotConnected,
        AuthorityError::Rejected => latest::HostAccountGetError::Rejected,
        AuthorityError::ConfirmationFailed(error) => {
            return CallError::HostFailure {
                reason: error.reason,
            };
        }
        AuthorityError::Cancelled(err) => latest::HostAccountGetError::Unknown {
            reason: err.to_string(),
        },
        AuthorityError::Unavailable { reason }
        | AuthorityError::NotSupported { reason }
        | AuthorityError::Unknown { reason } => latest::HostAccountGetError::Unknown { reason },
    };
    CallError::Domain(error)
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
        AuthorityError::ConfirmationFailed(error) => {
            return CallError::HostFailure {
                reason: format!("sign payload confirmation failed: {error:?}"),
            };
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
        AuthorityError::ConfirmationFailed(error) => {
            return CallError::HostFailure {
                reason: format!("create transaction confirmation failed: {error:?}"),
            };
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

impl<H: AccountHolder> ProductRuntimeHost<H> {
    /// Replace the contact handles a call declares with the accounts they
    /// name, before the call is shown to the user or signed.
    ///
    /// Substituting here rather than at the authority is what lets the
    /// confirmation show who is being paid: the host draws the review from the
    /// call it is about to sign, and by then the handle is an account it can
    /// put a name to. A call declaring no contacts reaches for no host, but is
    /// still refused if it carries a handle it forgot to declare.
    async fn substitute_declared_contacts(
        &self,
        call_data: Vec<u8>,
        declared: &[v01::ContactHandle],
    ) -> Result<Vec<u8>, ContactResolutionError> {
        let cache = &self.connection.services.contact_handles;
        let declared_bytes: Vec<[u8; 32]> = declared.iter().map(|handle| handle.bytes).collect();
        if cache.has_undeclared_handle(&call_data, &declared_bytes) {
            return Err(ContactResolutionError::UnknownContact);
        }
        if declared.is_empty() {
            return Ok(call_data);
        }
        let (platform, handles) = self.contacts_picker().map_err(|error| match error {
            CallError::Unsupported => ContactResolutionError::Unsupported,
            CallError::Domain(v01::HostContactsPickError::NotConnected) => {
                ContactResolutionError::NotConnected
            }
            CallError::Domain(v01::HostContactsPickError::Unknown { reason }) => {
                ContactResolutionError::Host(reason)
            }
            other => ContactResolutionError::Host(format!("{other:?}")),
        })?;
        let mut resolved: Vec<([u8; 32], Option<[u8; 32]>)> = declared_bytes
            .iter()
            .map(|handle| (*handle, cache.get(handle, &handles)))
            .collect();
        // The host is asked only about handles the cache cannot answer, all
        // of them in one lookup.
        let misses: Vec<[u8; 32]> = resolved
            .iter()
            .filter(|(_, account)| account.is_none())
            .map(|(handle, _)| *handle)
            .collect();
        if !misses.is_empty() {
            let generation = cache.generation();
            let lookup = crate::platform::HostContactLookup {
                handle_key: handles.handle_key(),
                handles: misses,
            };
            let matches = platform
                .contacts(&lookup)
                .await
                .map_err(|error| ContactResolutionError::Host(error.reason))?;
            // One answer per handle, or the answers cannot be paired up.
            if matches.accounts.len() != lookup.handles.len() {
                return Err(ContactResolutionError::Host(format!(
                    "contacts lookup answered {} of {} handles",
                    matches.accounts.len(),
                    lookup.handles.len()
                )));
            }
            let mut answers = matches.accounts.into_iter();
            for (handle, account) in resolved.iter_mut().filter(|(_, account)| account.is_none()) {
                let answer = answers.next().expect("one answer per miss; qed");
                // A host answer is checked, not trusted: an account that does
                // not hash to its handle is treated as no contact at all.
                *account = answer.filter(|account| handles.names(handle, account));
                if let Some(account) = account {
                    cache.insert(*handle, *account, generation);
                }
            }
        }
        crate::host_logic::contact_substitution::substitute(&call_data, &resolved)
            .map_err(|_| ContactResolutionError::UnknownContact)
    }

    /// The contact picker for this connection, plus the key its handles are
    /// minted under.
    ///
    /// Ordered so a host that serves no picker answers `Unsupported` without an
    /// overlay ever being raised. There is no permission step: the user
    /// selecting a contact is the consent.
    fn contacts_picker(&self) -> Result<ContactsPicker, CallError<v01::HostContactsPickError>> {
        // A capability the host does not serve is a framework answer; a
        // missing session is one the product handles.
        let platform = self
            .connection
            .services
            .contacts_platform()
            .ok_or(CallError::Unsupported)?;
        let authority_session = self
            .accounts
            .current_session()
            .ok_or(CallError::Domain(v01::HostContactsPickError::NotConnected))?;
        let handle_key = self
            .accounts
            .contacts_handle_key(&authority_session)
            .map_err(|error| match error {
                AuthorityError::Disconnected => {
                    CallError::Domain(v01::HostContactsPickError::NotConnected)
                }
                other => CallError::Domain(v01::HostContactsPickError::Unknown {
                    reason: other.to_string(),
                }),
            })?;
        Ok((
            platform,
            crate::runtime::contacts::ContactHandles::from_handle_key(handle_key),
        ))
    }
}

#[crate::platform::async_trait]
impl<H: AccountHolder> Contacts for ProductRuntimeHost<H> {
    #[instrument(skip_all, fields(runtime.method = "contacts.pick"))]
    async fn pick(
        &self,
        _cx: &CallContext,
        _request: HostContactsPickRequest,
    ) -> Result<HostContactsPickResponse, CallError<HostContactsPickError>> {
        let wrap = HostContactsPickError::V1;
        let (platform, handles) = self
            .contacts_picker()
            .map_err(|error| contacts_error(error, wrap))?;

        let unknown = |error: v01::GenericError| {
            CallError::Domain(wrap(v01::HostContactsPickError::Unknown {
                reason: error.reason,
            }))
        };

        // Read before the picker opens: a removal signalled while the user is
        // choosing must not be undone by caching their choice.
        let generation = self.connection.services.contact_handles.generation();
        let outcome = match platform
            .pick_contact(&self.connection.product)
            .await
            .map_err(unknown)?
        {
            crate::platform::HostContactPick::Picked { account } => {
                let handle = handles.mint(&account);
                self.connection
                    .services
                    .contact_handles
                    .insert(handle, account, generation);
                v01::ContactPickOutcome::Picked {
                    handle: v01::ContactHandle { bytes: handle },
                }
            }
            crate::platform::HostContactPick::Dismissed => v01::ContactPickOutcome::Dismissed,
            crate::platform::HostContactPick::NoContacts => v01::ContactPickOutcome::NoContacts,
            // A host that resolves contacts but cannot present them is a
            // framework-level gap, not an outcome the user produced.
            crate::platform::HostContactPick::Unsupported => return Err(CallError::Unsupported),
        };
        Ok(HostContactsPickResponse::V1(
            v01::HostContactsPickResponse { outcome },
        ))
    }
}

/// Re-wrap a latest-payload picker error into its versioned envelope.
fn contacts_error<E>(
    error: CallError<v01::HostContactsPickError>,
    wrap: fn(v01::HostContactsPickError) -> E,
) -> CallError<E> {
    match error {
        CallError::Domain(domain) => CallError::Domain(wrap(domain)),
        CallError::Denied => CallError::Denied,
        CallError::Unsupported => CallError::Unsupported,
        CallError::MalformedFrame { reason } => CallError::MalformedFrame { reason },
        CallError::HostFailure { reason } => CallError::HostFailure { reason },
        CallError::Cancelled => CallError::Cancelled,
    }
}

#[crate::platform::async_trait]
impl<H: AccountHolder> Chat for ProductRuntimeHost<H> {
    #[instrument(skip_all, fields(runtime.method = "chat.create_room"))]
    async fn create_room(
        &self,
        _cx: &CallContext,
        request: HostChatCreateRoomRequest,
    ) -> Result<HostChatCreateRoomResponse, CallError<HostChatCreateRoomError>> {
        let platform = self.connection.chat_platform()?;
        let HostChatCreateRoomRequest::V1(mut request) = request;
        request.room_id = normalize_chat_identifier("roomId", &request.room_id)
            .map_err(chat_create_room_field_error)?;
        request.name =
            validate_chat_name("name", &request.name).map_err(chat_create_room_field_error)?;
        request.icon =
            validate_chat_icon("icon", &request.icon).map_err(chat_create_room_field_error)?;
        platform
            .create_chat_room(&self.connection.product, request)
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
        let platform = self.connection.chat_platform()?;
        let HostChatRegisterBotRequest::V1(mut request) = request;
        request.bot_id = normalize_chat_identifier("botId", &request.bot_id)
            .map_err(chat_register_bot_field_error)?;
        request.name =
            validate_chat_name("name", &request.name).map_err(chat_register_bot_field_error)?;
        request.icon =
            validate_chat_icon("icon", &request.icon).map_err(chat_register_bot_field_error)?;
        platform
            .register_chat_bot(&self.connection.product, request)
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
        let platform = match self
            .connection
            .chat_platform::<HostChatListSubscribeError>()
        {
            Ok(platform) => platform,
            Err(error) => return Subscription::interrupted(error),
        };
        Subscription::new(
            platform
                .subscribe_chat_rooms(&self.connection.product)
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
        let platform = self.connection.chat_platform()?;
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
            .post_chat_message(&self.connection.product, request)
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
        if let Err(error) = self
            .connection
            .chat_platform::<HostChatActionSubscribeError>()
        {
            return Subscription::interrupted(error);
        }
        self.connection.chat.subscribe()
    }
}

#[crate::platform::async_trait]
impl<H: AccountHolder> Renderer for ProductRuntimeHost<H> {
    #[instrument(skip_all, fields(runtime.method = "renderer.action_subscribe"))]
    async fn action_subscribe(
        &self,
        _cx: &CallContext,
        _request: HostRendererActionSubscribeRequest,
    ) -> Subscription<HostRendererActionSubscribeItem, CallError<HostRendererActionSubscribeError>>
    {
        if self.connection.renderer_access().is_err() {
            return Subscription::interrupted(CallError::Denied);
        }
        self.connection.renderer.subscribe()
    }
}

#[truapi::async_trait]
impl<H: AccountHolder> Pocket for ProductRuntimeHost<H> {
    #[instrument(skip_all, fields(runtime.method = "pocket.list_subscribe"))]
    async fn list_subscribe(
        &self,
        _cx: &CallContext,
        _request: HostPocketListSubscribeRequest,
    ) -> Subscription<HostPocketListSubscribeItem, CallError<HostPocketListSubscribeError>> {
        let platform = match self
            .connection
            .pocket_platform::<HostPocketListSubscribeError>()
        {
            Ok(platform) => platform,
            Err(error) => return Subscription::interrupted(error),
        };
        Subscription::new(
            platform
                .subscribe_pocket_cards(&self.connection.product)
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
        let platform = self.connection.pocket_platform()?;
        let HostPocketRemoveCardRequest::V1(mut request) = request;
        // A card id is a product-chosen label the host renders in its own
        // chrome, so it is screened before the host ever sees it: trimmed,
        // NFC-normalized, and rejected if it carries characters that let two
        // distinct ids render identically.
        request.card_id =
            normalize_chat_identifier("cardId", &request.card_id).map_err(pocket_field_error)?;
        platform
            .remove_pocket_card(&self.connection.product, request)
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
    error: crate::platform::ChatFieldError,
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
    error: crate::platform::ChatFieldError,
) -> CallError<HostChatCreateRoomError> {
    CallError::Domain(HostChatCreateRoomError::V1(
        v01::HostChatCreateRoomError::Unknown {
            reason: error.to_string(),
        },
    ))
}

impl<H: AccountHolder> ProductRuntimeHost<H> {
    /// Cache a just-submitted preimage under its key for immediate lookups.
    fn prime_preimage_cache(&self, key: &[u8], value: Vec<u8>) {
        if let Ok(key_bytes) = <[u8; 32]>::try_from(key) {
            debug_assert_eq!(key_bytes, preimage_key(&value));
            self.connection.services.cache_preimage(key_bytes, value);
        }
    }
}

/// Build the product-facing `Unknown` wire error carrying `reason`.
fn preimage_submit_error(reason: String) -> CallError<RemotePreimageSubmitError> {
    CallError::Domain(RemotePreimageSubmitError::V1(
        v01::PreimageSubmitError::Unknown { reason },
    ))
}

#[cfg(test)]
impl ProductRuntimeHost<SsoAccountHolderClient> {
    /// Test constructor building a standalone pairing-host runtime.
    fn new<P>(
        platform: Arc<P>,
        config: (crate::platform::PairingHostConfig, ProductContext),
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
    fn new_compat(platform: Arc<dyn Platform>, spawner: Spawner) -> Self {
        Self::new_compat_with_pairing(platform, spawner).0
    }

    fn compat_host_config() -> crate::platform::PairingHostConfig {
        crate::platform::PairingHostConfig::new(
            crate::platform::HostInfo {
                name: "Polkadot Web".to_string(),
                icon: Some("https://example.invalid/dotli.png".to_string()),
                version: None,
                platform: truapi::latest::HostPlatform::Web,
            },
            crate::platform::PlatformInfo::default(),
            [0; 32],
            [0xbb; 32],
            [0xcc; 32],
            "polkadotapp".to_string(),
        )
        .expect("compat runtime config is valid")
    }

    /// Compat host used by preimage tests.
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

    fn new_compat_with_pairing(
        platform: Arc<dyn Platform>,
        spawner: Spawner,
    ) -> (
        Self,
        Arc<HostAccounts<SsoAccountHolderClient>>,
        Arc<SsoRequestService>,
    ) {
        let host_config = Self::compat_host_config();
        Self::new_pairing_for_tests(
            platform,
            host_config,
            ProductContext::new("unknown.dot".to_string())
                .expect("compat product context is valid"),
            spawner,
        )
    }

    fn new_pairing_for_tests(
        platform: Arc<dyn Platform>,
        host_config: crate::platform::PairingHostConfig,
        product: ProductContext,
        spawner: Spawner,
    ) -> (
        Self,
        Arc<HostAccounts<SsoAccountHolderClient>>,
        Arc<SsoRequestService>,
    ) {
        let services = RuntimeServices::new(
            platform.clone(),
            host_config.host.host_info.clone(),
            host_config.people_chain_genesis_hash,
            host_config.bulletin_chain_genesis_hash,
            host_config.asset_hub_chain_genesis_hash,
            spawner.clone(),
        );
        let (accounts, sso) = HostAccounts::pairing(services.clone(), host_config);
        let adapters = crate::host_core::ConnectionAdapters::from_services(&services);
        let host = Self::from_services(services, adapters, accounts.clone(), sso.clone(), product);
        (host, accounts, sso)
    }
}

#[cfg(test)]
mod tests;
