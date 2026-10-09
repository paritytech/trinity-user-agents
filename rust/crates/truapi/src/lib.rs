#![allow(
    clippy::double_must_use,
    reason = "async-trait generates must_use futures for async trait methods"
)]
// The pairing-flow future nests the chain, SSO and identity futures deeply
// enough that proving the tree's auto traits exceeds the default limit.
#![recursion_limit = "256"]
#![doc = include_str!("../README.md")]
//! TrUAPI trait and type definitions for the host product SDK, and the runtime
//! hosts embed (feature `runtime`, on by default).
//!
//! Concrete wire types live in per-version modules. Versioned envelopes are in
//! [`versioned`].
//! Async API traits use the `async_trait` macro so their concise `async fn` methods
//! still guarantee `Send` futures. Implementations must annotate their impl
//! blocks with `#[truapi::async_trait]`.
//!
//! The runtime: hosts instantiate a role runtime around a `platform::Platform`
//! implementation, then create product-scoped `ProductRuntime` endpoints that
//! expose the stable byte-frame API used from WASM, native mobile, or desktop
//! shells. Host-facing bridges:
//! - `ws_bridge`: localhost WebSocket bridge for
//!   native WebView hosts (Android/iOS).
//! - `bootstrap`: the JavaScript those hosts inject to reach that bridge.
//! - `native`: UniFFI surface exposing the native host runtime + callbacks.
//! - `wasm` (wasm32 only): wasm-bindgen surface exposing `WasmProductRuntime`.
//! - `native_debug` (non-wasm32 only): a loopback WebSocket `DebugSink` that
//!   streams tapped frames to the `@parity/truapi-debugger` app.

// Runtime code, the generated dispatcher and the macros all name this crate
// `truapi`, the way code outside it does.
extern crate self as truapi;

use core::convert::Infallible;
use core::fmt;
use core::future::Future;
use core::mem;
use core::pin::Pin;
use core::task::{Context, Poll, Waker};
use core::time::Duration;
use std::sync::Arc;
use std::sync::Mutex;

use futures::Stream;
use parity_scale_codec::{Decode, Encode};

pub use async_trait::async_trait;

pub mod api;
pub mod v01;
pub mod v02;
pub mod versioned;

/// A 32-byte value, passed as plain bytes on FFI surfaces. Version-neutral:
/// the FFI conversion below applies to `[u8; 32]` fields in every protocol
/// version.
pub type Bytes32 = [u8; 32];

#[cfg(all(feature = "runtime", not(target_arch = "wasm32")))]
uniffi::setup_scaffolding!();

#[cfg(all(feature = "runtime", not(target_arch = "wasm32")))]
uniffi::custom_type!(Bytes32, Vec<u8>, {
    remote,
    lower: |bytes| bytes.to_vec(),
    try_lift: |bytes| Ok(bytes.as_slice().try_into()?),
});

/// Latest-version protocol payload types, unwrapped from their versioned
/// envelopes. Runtime code should use these instead of per-version modules.
pub mod latest {
    use crate::versioned::{self, Versioned};

    pub use crate::v01::{
        AllocatableResource, AllocationOutcome, Arrangement, Background, BlendingMode, BorderStyle,
        BoxProps, ButtonProps, ButtonVariant, ChainIdentifier, ChatAction, ChatActionLayout,
        ChatActions, ChatBotRegistrationStatus, ChatCustomMessage, ChatFile, ChatMedia,
        ChatMessageContent, ChatReaction, ChatRichText, ChatRoom, ChatRoomParticipation,
        ChatRoomRegistrationStatus, ColorToken, ColumnProps, ContactHandle, ContactPickOutcome,
        ContentAlignment, ContextualAlias, DerivationIndex, Dimensions, Effect, EffectProps,
        GenericError, HorizontalAlignment, HostAccountCreateProofRequest,
        HostAccountGetAliasRequest, HostAccountListRingVrfKeysRequest,
        HostAccountRegisterRingVrfKeyRequest, HostAccountRingVrfSignRequest,
        HostAccountSignVrfError, HostAccountSignVrfRequest, HostPlatform, HostSignPayloadData,
        HostWorkerOperationError, ImageFit, ImageProps, ImageSource, Modifier,
        OperationStartedResult, PocketCard, ProductAccountId, ProductProofContext, RawPayload,
        RegisteredRingVrfKey, RemotePermission, RemoteStatementStoreCreateProofError,
        RemoteStatementStoreCreateProofRequest, RemoteStatementStoreCreateProofResponse,
        RemoteStatementStoreSubscribeItem, RemoteStatementStoreSubscribeRequest, RenderContext,
        RendererNode, RingLocation, RingLocationJunction, RingVrfKeyDisclosure, RowProps,
        RuntimeApi, RuntimeSpec, RuntimeType, Shape, SignedStatement, Size, Statement,
        StatementProof, StorageQueryItem, StorageQueryType, StorageResultItem, TextFieldProps,
        TextProps, ThemeName, ThemeVariant, TxPayloadExtension, TypographyStyle, VerticalAlignment,
        VrfSignature,
    };

    /// Latest payload type of a versioned envelope.
    pub type LatestOf<T> = <T as Versioned>::Latest;

    /// Ring VRF proof creation result.
    pub type HostAccountCreateProofResponse =
        LatestOf<versioned::account::HostAccountCreateProofResponse>;
    /// Chat action delivered from the native host to a product worker.
    pub type HostChatActionSubscribeItem = LatestOf<versioned::chat::HostChatActionSubscribeItem>;
    /// Native chat room creation request.
    pub type HostChatCreateRoomRequest = LatestOf<versioned::chat::HostChatCreateRoomRequest>;
    /// Native chat room creation result.
    pub type HostChatCreateRoomResponse = LatestOf<versioned::chat::HostChatCreateRoomResponse>;
    /// Native chat room creation failure.
    pub type HostChatCreateRoomError = LatestOf<versioned::chat::HostChatCreateRoomError>;
    /// Native chat bot registration request.
    pub type HostChatRegisterBotRequest = LatestOf<versioned::chat::HostChatRegisterBotRequest>;
    /// Native chat bot registration result.
    pub type HostChatRegisterBotResponse = LatestOf<versioned::chat::HostChatRegisterBotResponse>;
    /// Native chat bot registration failure.
    pub type HostChatRegisterBotError = LatestOf<versioned::chat::HostChatRegisterBotError>;
    /// Current native room list for a product.
    pub type HostChatListSubscribeItem = LatestOf<versioned::chat::HostChatListSubscribeItem>;
    /// Native chat message posting request.
    pub type HostChatPostMessageRequest = LatestOf<versioned::chat::HostChatPostMessageRequest>;
    /// Native chat message posting result.
    pub type HostChatPostMessageResponse = LatestOf<versioned::chat::HostChatPostMessageResponse>;
    /// Native chat message posting failure.
    pub type HostChatPostMessageError = LatestOf<versioned::chat::HostChatPostMessageError>;
    /// Action triggered inside a product-rendered body, delivered to the worker.
    pub type HostRendererActionSubscribeItem =
        LatestOf<versioned::renderer::HostRendererActionSubscribeItem>;
    /// Host-to-product render request for one body.
    pub type ProductRendererRenderRequest =
        LatestOf<versioned::renderer::ProductRendererRenderRequest>;
    /// Product-to-host renderer tree.
    pub type ProductRendererRenderItem = LatestOf<versioned::renderer::ProductRendererRenderItem>;
    /// Contact picker request.
    pub type HostContactsPickRequest = LatestOf<versioned::contacts::HostContactsPickRequest>;
    /// Contact picker outcome.
    pub type HostContactsPickResponse = LatestOf<versioned::contacts::HostContactsPickResponse>;
    /// Contact picker failure.
    pub type HostContactsPickError = LatestOf<versioned::contacts::HostContactsPickError>;
    /// Contextual alias derivation result.
    pub type HostAccountGetAliasResponse =
        LatestOf<versioned::account::HostAccountGetAliasResponse>;
    /// Ring-VRF key registration result.
    pub type HostAccountRegisterRingVrfKeyResponse =
        LatestOf<versioned::account::HostAccountRegisterRingVrfKeyResponse>;
    /// Ring-VRF registry listing result.
    pub type HostAccountListRingVrfKeysResponse =
        LatestOf<versioned::account::HostAccountListRingVrfKeysResponse>;
    /// Direct ring-VRF key signing result.
    pub type HostAccountRingVrfSignResponse =
        LatestOf<versioned::account::HostAccountRingVrfSignResponse>;
    /// Legacy account listing result.
    pub type HostGetLegacyAccountsResponse =
        LatestOf<versioned::account::HostGetLegacyAccountsResponse>;
    /// Transaction creation result.
    pub type HostCreateTransactionResponse =
        LatestOf<versioned::signing::HostCreateTransactionResponse>;
    /// Device-capability permission request.
    pub type HostDevicePermissionRequest =
        LatestOf<versioned::permissions::HostDevicePermissionRequest>;
    /// Device-capability permission outcome.
    pub type HostDevicePermissionResponse =
        LatestOf<versioned::permissions::HostDevicePermissionResponse>;
    /// Feature-support query.
    pub type HostFeatureSupportedRequest = LatestOf<versioned::system::HostFeatureSupportedRequest>;
    /// Feature-support query result.
    pub type HostFeatureSupportedResponse =
        LatestOf<versioned::system::HostFeatureSupportedResponse>;
    /// Product context bound to the current host runtime.
    pub type HostGetProductContextResponse =
        LatestOf<versioned::system::HostGetProductContextResponse>;
    /// Request to drop the calling product's game reminder.
    pub type HostCancelNextGameRequest = LatestOf<versioned::game::HostCancelNextGameRequest>;
    /// Request to remind the user when the calling product's next game starts.
    pub type HostRemindNextGameRequest = LatestOf<versioned::game::HostRemindNextGameRequest>;
    /// Why a game reminder was not taken.
    pub type HostRemindNextGameError = LatestOf<versioned::game::HostRemindNextGameError>;
    /// Storage key change pushed to a subscriber.
    pub type HostLocalStorageChangeItem =
        LatestOf<versioned::local_storage::HostLocalStorageChangeItem>;
    /// Local storage operation error.
    pub type HostLocalStorageReadError =
        LatestOf<versioned::local_storage::HostLocalStorageReadError>;
    /// Locale the host currently presents its interface in.
    pub type HostLocaleSubscribeItem = LatestOf<versioned::locale::HostLocaleSubscribeItem>;
    /// Navigation request error.
    pub type HostNavigateToError = LatestOf<versioned::system::HostNavigateToError>;
    /// The calling product's Pocket cards.
    pub type HostPocketListSubscribeItem = LatestOf<versioned::pocket::HostPocketListSubscribeItem>;
    /// Pocket card removal request.
    pub type HostPocketRemoveCardRequest = LatestOf<versioned::pocket::HostPocketRemoveCardRequest>;
    /// Pocket card removal failure.
    pub type HostPocketRemoveCardError = LatestOf<versioned::pocket::HostPocketRemoveCardError>;
    /// Push notification scheduling request.
    pub type HostPushNotificationRequest =
        LatestOf<versioned::notifications::HostPushNotificationRequest>;
    /// Push notification scheduling result.
    pub type HostPushNotificationResponse =
        LatestOf<versioned::notifications::HostPushNotificationResponse>;
    /// Login request error.
    pub type HostRequestLoginError = LatestOf<versioned::account::HostRequestLoginError>;
    /// Login request result.
    pub type HostRequestLoginResponse = LatestOf<versioned::account::HostRequestLoginResponse>;
    /// Batched resource pre-allocation request.
    pub type HostRequestResourceAllocationRequest =
        LatestOf<versioned::resource_allocation::HostRequestResourceAllocationRequest>;
    /// Per-resource allocation outcomes.
    pub type HostRequestResourceAllocationResponse =
        LatestOf<versioned::resource_allocation::HostRequestResourceAllocationResponse>;
    /// Extrinsic payload signing request for a product account.
    pub type HostSignPayloadRequest = LatestOf<versioned::signing::HostSignPayloadRequest>;
    /// Signing operation result.
    pub type HostSignPayloadResponse = LatestOf<versioned::signing::HostSignPayloadResponse>;
    /// Extrinsic payload signing request for a legacy account.
    pub type HostSignPayloadWithLegacyAccountRequest =
        LatestOf<versioned::signing::HostSignPayloadWithLegacyAccountRequest>;
    /// Raw-bytes signing request for a product account.
    pub type HostSignRawRequest = LatestOf<versioned::signing::HostSignRawRequest>;
    /// Raw-bytes signing request for a legacy account.
    pub type HostSignRawWithLegacyAccountRequest =
        LatestOf<versioned::signing::HostSignRawWithLegacyAccountRequest>;
    /// Current host theme pushed to subscribers.
    pub type HostThemeSubscribeItem = LatestOf<versioned::theme::HostThemeSubscribeItem>;
    /// Result of beginning a worker pending operation.
    pub type HostWorkerBeginOperationResponse =
        LatestOf<versioned::worker::HostWorkerBeginOperationResponse>;
    /// Transaction creation payload for a legacy account.
    pub type LegacyAccountTxPayload =
        LatestOf<versioned::signing::HostCreateTransactionWithLegacyAccountRequest>;
    /// Preimage submission error.
    pub type PreimageSubmitError = LatestOf<versioned::preimage::RemotePreimageSubmitError>;
    /// Transaction creation payload for a product account.
    pub type ProductAccountTxPayload = LatestOf<versioned::signing::HostCreateTransactionRequest>;
    /// Chain-head runtime-API call request.
    pub type RemoteChainHeadCallRequest = LatestOf<versioned::chain::RemoteChainHeadCallRequest>;
    /// Chain-head subscription item.
    pub type RemoteChainHeadFollowItem = LatestOf<versioned::chain::RemoteChainHeadFollowItem>;
    /// Chain-identifier resolution error.
    pub type RemoteChainInfoError = LatestOf<versioned::chain::RemoteChainInfoError>;
    /// Chain-identifier resolution request.
    pub type RemoteChainInfoRequest = LatestOf<versioned::chain::RemoteChainInfoRequest>;
    /// Chain-identifier resolution result.
    pub type RemoteChainInfoResponse = LatestOf<versioned::chain::RemoteChainInfoResponse>;
    /// Chain-head subscription request.
    pub type RemoteChainHeadFollowRequest =
        LatestOf<versioned::chain::RemoteChainHeadFollowRequest>;
    /// Chain-head storage query request.
    pub type RemoteChainHeadStorageRequest =
        LatestOf<versioned::chain::RemoteChainHeadStorageRequest>;
    /// Chain-head storage query result.
    pub type RemoteChainHeadStorageResponse =
        LatestOf<versioned::chain::RemoteChainHeadStorageResponse>;
    /// Remote-operation permission request.
    pub type RemotePermissionRequest = LatestOf<versioned::permissions::RemotePermissionRequest>;
    /// Remote-operation permission outcome.
    pub type RemotePermissionResponse = LatestOf<versioned::permissions::RemotePermissionResponse>;
}

pub use truapi_macros::{service, wire, wire_trait};

/// Wire codec version this crate defines. Frames address a method with a
/// `(trait, method)` byte pair. The handshake accepts only this version, and
/// codegen stamps it into the generated clients, so every peer derives it
/// from here.
pub const WIRE_CODEC_VERSION: u8 = 3;

/// Framework-level outcomes shared by API methods.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum CallError<D> {
    /// Method-specific failure.
    Domain(D),
    /// The caller is not allowed to perform this operation.
    Denied,
    /// The host does not support this operation.
    Unsupported,
    /// The incoming request payload could not be decoded or validated.
    MalformedFrame {
        /// Why decoding or validation failed.
        reason: String,
    },
    /// Host-side failure with a diagnostic reason.
    HostFailure {
        /// Diagnostic reason for the failure.
        reason: String,
    },
    /// The caller withdrew the request before it produced a result.
    ///
    /// Appended last so the preceding variants keep their SCALE indices.
    Cancelled,
}

impl<D> CallError<D> {
    /// Convenience for default handlers whose implementation is not wired.
    pub fn unavailable() -> Self {
        Self::HostFailure {
            reason: "unavailable".into(),
        }
    }
}

/// Error type for methods with no domain-specific failures.
pub type FrameworkOnlyError = CallError<Infallible>;

/// Cooperative cancellation token exposed to handlers.
///
/// A request token fires when the peer sends a `Cancel` frame for the call,
/// when a runtime explicitly cancels it, or when an attached timeout elapses.
/// Subscription runtimes can cancel this token when the peer sends `_stop` or
/// disconnects.
#[derive(Clone, Default)]
pub struct CancellationToken {
    inner: Arc<CancellationInner>,
}

#[derive(Default)]
struct CancellationInner {
    state: Mutex<CancellationState>,
}

#[derive(Default)]
struct CancellationState {
    reason: Option<CancellationReason>,
    next_id: u64,
    wakers: Vec<(u64, Waker)>,
}

/// Cause attached to a cancelled call.
#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display)]
pub enum CancellationReason {
    /// The caller or runtime explicitly cancelled the call.
    #[display("cancelled")]
    Cancelled,
    /// The call exceeded the configured timeout.
    #[display("timed out after {}", format_timeout(timeout))]
    TimedOut {
        /// Timeout that elapsed.
        timeout: Duration,
    },
}

/// Render a timeout as whole seconds when possible, milliseconds otherwise.
fn format_timeout(timeout: &Duration) -> String {
    if timeout.subsec_millis() == 0 {
        format!("{}s", timeout.as_secs())
    } else {
        format!("{}ms", timeout.as_millis())
    }
}

/// Future resolved when a [`CancellationToken`] is cancelled.
pub struct CancellationFuture {
    inner: Arc<CancellationInner>,
    id: Option<u64>,
}

impl fmt::Debug for CancellationToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CancellationToken")
            .field("reason", &self.reason())
            .finish_non_exhaustive()
    }
}

impl CancellationToken {
    /// Mark the token as cancelled.
    pub fn cancel(&self) {
        self.cancel_with_reason(CancellationReason::Cancelled);
    }

    /// Mark the token as cancelled with an explicit `reason`.
    pub fn cancel_with_reason(&self, reason: CancellationReason) {
        let wakers = {
            let mut state = self.inner.state.lock().expect("cancel state poisoned");
            if state.reason.is_some() {
                return;
            }
            state.reason = Some(reason);
            mem::take(&mut state.wakers)
        };
        for (_, waker) in wakers {
            waker.wake();
        }
    }

    /// Returns the cancellation reason, if cancellation has been requested.
    pub fn reason(&self) -> Option<CancellationReason> {
        self.inner
            .state
            .lock()
            .expect("cancel state poisoned")
            .reason
            .clone()
    }

    /// Returns whether cancellation has been requested.
    pub fn is_cancelled(&self) -> bool {
        self.reason().is_some()
    }

    /// Future resolved with the cancellation reason when cancellation is requested.
    pub fn cancelled(&self) -> CancellationFuture {
        CancellationFuture {
            inner: self.inner.clone(),
            id: None,
        }
    }
}

impl Future for CancellationFuture {
    type Output = CancellationReason;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        let mut state = this.inner.state.lock().expect("cancel state poisoned");
        if let Some(reason) = state.reason.clone() {
            this.id = None;
            return Poll::Ready(reason);
        }

        if let Some(id) = this.id
            && let Some((_, waker)) = state
                .wakers
                .iter_mut()
                .find(|(waiter_id, _)| *waiter_id == id)
        {
            if !waker.will_wake(cx.waker()) {
                *waker = cx.waker().clone();
            }
            return Poll::Pending;
        }

        state.next_id = state.next_id.wrapping_add(1);
        let id = state.next_id;
        state.wakers.push((id, cx.waker().clone()));
        this.id = Some(id);
        Poll::Pending
    }
}

impl Drop for CancellationFuture {
    fn drop(&mut self) {
        let Some(id) = self.id.take() else {
            return;
        };
        let mut state = self.inner.state.lock().expect("cancel state poisoned");
        if state.reason.is_some() {
            return;
        }
        state.wakers.retain(|(waiter_id, _)| *waiter_id != id);
    }
}

/// Ambient context passed to every trait method.
#[derive(Clone, Default)]
pub struct CallContext {
    request_id: String,
    cancel: CancellationToken,
    timeout: Option<Duration>,
}

impl CallContext {
    /// Construct a context bound to the given `request_id` with a fresh cancellation token.
    pub fn with_request_id(request_id: String) -> Self {
        Self {
            request_id,
            cancel: CancellationToken::default(),
            timeout: None,
        }
    }

    /// Construct a context from explicit `request_id` and `cancel` parts.
    pub fn with_parts(request_id: String, cancel: CancellationToken) -> Self {
        Self {
            request_id,
            cancel,
            timeout: None,
        }
    }

    /// Attach a timeout to this call.
    pub fn set_timeout(&mut self, timeout: Duration) {
        self.timeout = Some(timeout);
    }

    /// Return the request id this context is associated with.
    pub fn request_id(&self) -> &str {
        &self.request_id
    }

    /// Return the cancellation token that signals when the call should abort.
    pub fn cancel(&self) -> &CancellationToken {
        &self.cancel
    }

    /// Return the timeout attached to this call, if any.
    pub fn timeout(&self) -> Option<Duration> {
        self.timeout
    }
}

/// Handle to an active subscription. Implements [`Stream`] to yield values
/// pushed by the host. Drop to unsubscribe.
///
/// The stream yields `Ok(item)` for each value and at most one `Err`, which
/// ends it: the runtime encodes that value as the `_interrupt` payload and
/// polls no further. A stream that ends without an `Err` interrupts with
/// `Ok(())`, which the peer reads as a normal completion.
pub struct Subscription<Item, Interrupt> {
    inner: Pin<Box<dyn Stream<Item = Result<Item, Interrupt>> + Send>>,
}

impl<Item, Interrupt> Stream for Subscription<Item, Interrupt> {
    type Item = Result<Item, Interrupt>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.inner.as_mut().poll_next(cx)
    }
}

impl<Item, Interrupt> Subscription<Item, Interrupt> {
    /// Creates a subscription from a stream of items and at most one
    /// terminating interrupt.
    pub fn new<S>(stream: S) -> Self
    where
        S: Stream<Item = Result<Item, Interrupt>> + Send + 'static,
    {
        Self {
            inner: Box::pin(stream),
        }
    }

    /// Creates a subscription that yields no items and ends with `interrupt`.
    /// The default trait bodies of unimplemented methods interrupt with
    /// [`CallError::unavailable`], so a caller sees a failure rather than a
    /// stream that finished.
    pub fn interrupted(interrupt: Interrupt) -> Self
    where
        Item: Send + 'static,
        Interrupt: Send + 'static,
    {
        Self::new(futures::stream::once(core::future::ready(Err(interrupt))))
    }
}

/// Applies `#[cfg(feature = "runtime")]` to every item it wraps.
macro_rules! runtime_items {
    ($($item:item)*) => { $( #[cfg(feature = "runtime")] $item )* };
}

runtime_items! {
    pub mod bootstrap;
    pub mod chain;
    mod chain_runtime;
    mod truapi_core;
    mod dispatcher;
    mod dotns_views;
    mod dynamic_vrf;
    pub mod frame;
    mod host_core;
    mod host_internal;
    pub mod host_logic;
    mod host_rpc_client;
    mod interrupt;
    pub mod logging;
    pub mod platform;
    mod protocol_error;
    mod runtime;
    mod session_usernames;
    pub mod subscription;
    pub mod transport;

    #[cfg(test)]
    mod test_support;
    mod unix_time;

    // Dispatch must keep serving deprecated APIs while clients migrate.
    #[allow(deprecated)]
    pub mod generated;

    #[cfg(not(target_arch = "wasm32"))]
    pub mod native;

    #[cfg(target_arch = "wasm32")]
    pub mod wasm;

    #[cfg(not(target_arch = "wasm32"))]
    pub mod native_debug;

    #[cfg(not(target_arch = "wasm32"))]
    pub mod store;

    pub use truapi_core::TrUApiCore;
    pub use host_core::{
        ChannelId, DebugEvent, DebugSink, FrameDirection, FrameSink, HostAdmin, PairingHostRuntime,
        ProductRuntime, ProductRuntimeControl, ProductRuntimeError, SigningHostRuntime,
    };
    pub use host_internal::bulletin::{preimage_cid, preimage_key};
    pub use host_logic::session::{
        ExternalPairedSession, SsoSessionInfo, decode_persisted_session, encode_external_paired_session,
    };
    pub use host_logic::worker::{WorkerLedger, WorkerTransition};
    #[cfg(not(target_arch = "wasm32"))]
    pub use native_debug::{DebugSinkError, WsDebugSink};
    pub use platform::{
        CoreStorageKeyDescription, CoreStorageKeyDescriptionError, HostIdentity, PairingHostConfig,
        PermissionAuthorizationRequest, PermissionAuthorizationStatus, Platform, ProductContext,
        SigningHostConfig, describe_core_storage_key,
    };
    pub use runtime::StatementRenewalTarget;
    pub use runtime::contacts::contact_handle;
    pub use runtime::login_failure::reports_exhausted_period;
    pub use runtime::product_manifest::{encode_cached_root_manifest, manifest_cache_key};
    pub use runtime::statement_allowance;
    pub use runtime::{
        AnnouncedPairing, DevicePairingObserver, MAX_PAIRING_METADATA_CHARS, PairedSsoPeer,
        PairingProposal, PairingProposalMetadata, ResponderExit,
    };

    #[cfg(not(target_arch = "wasm32"))]
    pub use native::{
        NativeRendererObserver, NativeRendererSubscription, WsBridgeEndpoint, WsBridgeStartError,
    };

    #[cfg(all(target_arch = "wasm32", feature = "wasm-signing-host"))]
    pub use wasm::WasmSigningHostRuntime;
    #[cfg(target_arch = "wasm32")]
    pub use wasm::{
        WasmPairingHostRuntime, WasmProductRuntime, WasmRendererSubscription,
        derive_product_account_public_key, describe_core_storage_key_for_wasm,
        has_trusted_remote_permissions_for_wasm, product_account_address, set_log_level,
        wire_schema_hash,
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn call_context_timeout_can_be_set_and_replaced() {
        let default = Duration::from_secs(180);
        let explicit = Duration::from_millis(25);

        let mut cx = CallContext::with_request_id("request-1".to_string());
        assert_eq!(cx.timeout(), None);
        cx.set_timeout(default);
        assert_eq!(cx.timeout(), Some(default));

        cx.set_timeout(explicit);
        assert_eq!(cx.timeout(), Some(explicit));
    }

    #[test]
    fn cancellation_token_clones_share_cancellation() {
        let token = CancellationToken::default();
        let cloned = token.clone();
        let wait = cloned.cancelled();

        token.cancel();

        let reason = futures::executor::block_on(wait);
        assert_eq!(reason, CancellationReason::Cancelled);
        assert!(cloned.is_cancelled());
    }
}
