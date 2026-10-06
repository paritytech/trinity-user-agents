#![cfg_attr(not(feature = "host-api"), no_std)]
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
//! Async API traits (feature `host-api`) use the `async_trait` macro so their
//! concise `async fn` methods still guarantee `Send` futures. Implementations
//! must annotate their impl blocks with `#[truapi::async_trait]`.
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

extern crate alloc;

use alloc::string::String;
#[cfg(feature = "host-api")]
use alloc::{boxed::Box, format, vec::Vec};
use core::convert::Infallible;
#[cfg(feature = "host-api")]
use core::fmt;
#[cfg(feature = "host-api")]
use core::future::Future;
#[cfg(feature = "host-api")]
use core::mem;
#[cfg(feature = "host-api")]
use core::pin::Pin;
#[cfg(feature = "host-api")]
use core::task::{Context, Poll, Waker};
#[cfg(feature = "host-api")]
use core::time::Duration;
#[cfg(feature = "host-api")]
use std::sync::{Arc, Mutex};

#[cfg(feature = "host-api")]
use futures::Stream;
use parity_scale_codec::{Decode, Encode};

#[cfg(feature = "host-api")]
pub use async_trait::async_trait;

#[cfg(feature = "host-api")]
pub mod api;
pub mod v01;
pub mod v02;
pub mod v03;
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
        AllocatableResource, AllocationOutcome, Arrangement, AvatarRect, Background, BlendingMode,
        BorderStyle, BoxProps, ButtonProps, ButtonVariant, ChainIdentifier, ChatAction,
        ChatActionLayout, ChatActions, ChatBotRegistrationStatus, ChatCustomMessage, ChatFile,
        ChatMedia, ChatMessageContent, ChatReaction, ChatRichText, ChatRoom, ChatRoomParticipation,
        ChatRoomRegistrationStatus, ColorToken, ColumnProps, ContactHandle, ContactPickOutcome,
        ContentAlignment, ContextualAlias, DerivationIndex, Dimensions, Effect, EffectProps,
        GenericError, HorizontalAlignment, HostAccountCreateProofRequest,
        HostAccountGetAliasRequest, HostAccountListRingVrfKeysRequest,
        HostAccountRegisterRingVrfKeyRequest, HostAccountRingVrfSignRequest,
        HostAccountSignVrfError, HostAccountSignVrfRequest, HostJamPeerTransportCloseError,
        HostJamPeerTransportCloseRequest, HostJamPeerTransportDialError,
        HostJamPeerTransportDialRequest, HostJamPeerTransportDialResponse,
        HostJamPeerTransportEventsError, HostJamPeerTransportEventsResponse,
        HostJamPeerTransportOpenError, HostJamPeerTransportOpenRequest,
        HostJamPeerTransportOpenResponse, HostJamPeerTransportRecvError,
        HostJamPeerTransportRecvRequest, HostJamPeerTransportRecvResponse,
        HostJamPeerTransportResetError, HostJamPeerTransportResetRequest,
        HostJamPeerTransportSendError, HostJamPeerTransportSendRequest,
        HostNotificationAcknowledgeReceiverEventRequest, HostNotificationDisableReceiverRequest,
        HostNotificationReceiptResult, HostNotificationReceiverEventsRequest,
        HostNotificationReceiverStatus, HostNotificationReceivingError,
        HostNotificationRecordReceiptRequest, HostNotificationReplaceReceiverRequest, HostPlatform,
        HostSignPayloadData, HostWorkerOperationError, ImageFit, ImageProps, ImageSource,
        JAM_PEER_TRANSPORT_MAX_BUFFERED_BYTES_PER_CONNECTION, JAM_PEER_TRANSPORT_MAX_CONNECTIONS,
        JAM_PEER_TRANSPORT_MAX_MESSAGE_BYTES, JAM_PEER_TRANSPORT_MAX_STREAMS_PER_CONNECTION,
        JamPeerTransportEvent, MediaAccount, MediaAudioRoute, MediaCallOutcome, MediaCameraKind,
        MediaCapabilities, MediaEvent, MediaFit, MediaIncomingDecision, MediaIncomingId,
        MediaIncomingOffer, MediaIncomingResolution, MediaIncomingResponse, MediaLocalState,
        MediaLocalTracks, MediaNetwork, MediaOperationFailure, MediaOperationId,
        MediaOperationKind, MediaOperationResult, MediaOperationSnapshot, MediaOperationState,
        MediaParticipantId, MediaParticipantSnapshot, MediaParticipantState, MediaPeer,
        MediaPictureKind, MediaPictureSource, MediaPlacement, MediaRect, MediaRemoteState,
        MediaResource, MediaRuntimeLimits, MediaSessionId, MediaSessionSnapshot, MediaSessionState,
        MediaSurface, MediaTrackState, MediaViewport, Modifier, OperationStartedResult, PocketCard,
        ProductAccountId, ProductProofContext, RawPayload, ReceivingEvent, ReceivingEventKind,
        ReceivingReceiptKind, ReceivingWatch, RegisteredRingVrfKey, RemotePermission,
        RemoteStatementStoreCreateProofError, RemoteStatementStoreCreateProofRequest,
        RemoteStatementStoreCreateProofResponse, RemoteStatementStoreSubscribeItem,
        RemoteStatementStoreSubscribeRequest, RenderContext, RendererNode, RingLocation,
        RingLocationJunction, RingVrfKeyDisclosure, RowProps, RuntimeApi, RuntimeSpec, RuntimeType,
        Shape, SignedStatement, Size, Statement, StatementProof, StorageQueryItem,
        StorageQueryType, StorageResultItem, TextFieldProps, TextProps, ThemeName, ThemeVariant,
        TxPayloadExtension, TypographyStyle, VerticalAlignment, VrfSignature,
    };
    pub use crate::v02::{
        HostNativeChatAcknowledgment, HostNativeChatAttachment, HostNativeChatAttachmentKind,
        HostNativeChatAttachmentMetadata, HostNativeChatAttachmentState, HostNativeChatDevice,
        HostNativeChatInvitation, HostNativeChatMessages, HostNativeChatPayment,
        HostNativeChatPaymentDirection, HostNativeChatPaymentFailure, HostNativeChatPaymentState,
        HostNativeChatPeer, HostNativeChatPeerDevice, HostNativeChatRichMessage,
        HostNativeChatRichMessageKind, OwnAvatarSlot, ProfileAudience, ProfileContact,
    };
    pub use crate::v03::{
        ContactAvatarSlot, HostNativeChatBinding, HostNativeChatMigrationInvitation,
        HostNativeChatOpenPage, HostNativeChatOpened, HostNativeChatPrepared, HostNativeChatRoute,
        HostNativeChatStatePage,
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
    /// Multi-contact picker request.
    pub type HostContactsPickManyRequest =
        LatestOf<versioned::contacts::HostContactsPickManyRequest>;
    /// Multi-contact picker result.
    pub type HostContactsPickManyResponse =
        LatestOf<versioned::contacts::HostContactsPickManyResponse>;
    /// Multi-contact picker failure.
    pub type HostContactsPickManyError = LatestOf<versioned::contacts::HostContactsPickManyError>;
    /// Host-owned contact name placement.
    pub type HostContactsPlaceLabelsRequest =
        LatestOf<versioned::contacts::HostContactsPlaceLabelsRequest>;
    /// Contact label placement acknowledgment.
    pub type HostContactsPlaceLabelsResponse =
        LatestOf<versioned::contacts::HostContactsPlaceLabelsResponse>;
    /// Contact label placement failure.
    pub type HostContactsPlaceLabelsError =
        LatestOf<versioned::contacts::HostContactsPlaceLabelsError>;
    pub use crate::v01::{ContactLabelSlot, ContactPickManyOutcome};
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
    /// Storage key change pushed to a subscriber.
    pub type HostLocalStorageChangeItem =
        LatestOf<versioned::local_storage::HostLocalStorageChangeItem>;
    /// Local storage operation error.
    pub type HostLocalStorageReadError =
        LatestOf<versioned::local_storage::HostLocalStorageReadError>;
    /// Sanitized shared Media domain error.
    pub type HostMediaError = LatestOf<versioned::media::HostMediaError>;
    /// Media discovery request.
    pub type HostMediaGetCapabilitiesRequest =
        LatestOf<versioned::media::HostMediaGetCapabilitiesRequest>;
    /// Complete Media service capabilities.
    pub type HostMediaGetCapabilitiesResponse =
        LatestOf<versioned::media::HostMediaGetCapabilitiesResponse>;
    /// Media discovery failure.
    pub type HostMediaGetCapabilitiesError =
        LatestOf<versioned::media::HostMediaGetCapabilitiesError>;
    /// Passive runtime-wide Media subscription request.
    pub type HostMediaSessionSubscribeRequest =
        LatestOf<versioned::media::HostMediaSessionSubscribeRequest>;
    /// Authoritative runtime-wide Media event.
    pub type HostMediaSessionSubscribeItem =
        LatestOf<versioned::media::HostMediaSessionSubscribeItem>;
    /// Media subscription interruption.
    pub type HostMediaSessionSubscribeError =
        LatestOf<versioned::media::HostMediaSessionSubscribeError>;
    /// Correlated Media session creation request.
    pub type HostMediaCreateSessionRequest =
        LatestOf<versioned::media::HostMediaCreateSessionRequest>;
    /// Media session creation result.
    pub type HostMediaCreateSessionResponse =
        LatestOf<versioned::media::HostMediaCreateSessionResponse>;
    /// Media session creation failure.
    pub type HostMediaCreateSessionError = LatestOf<versioned::media::HostMediaCreateSessionError>;
    /// Correlated remote endpoint invitation request.
    pub type HostMediaAddParticipantRequest =
        LatestOf<versioned::media::HostMediaAddParticipantRequest>;
    /// Admitted Media participant.
    pub type HostMediaAddParticipantResponse =
        LatestOf<versioned::media::HostMediaAddParticipantResponse>;
    /// Media participant admission failure.
    pub type HostMediaAddParticipantError =
        LatestOf<versioned::media::HostMediaAddParticipantError>;
    /// Incoming Media offer decision.
    pub type HostMediaRespondIncomingRequest =
        LatestOf<versioned::media::HostMediaRespondIncomingRequest>;
    /// Incoming Media decision result.
    pub type HostMediaRespondIncomingResponse =
        LatestOf<versioned::media::HostMediaRespondIncomingResponse>;
    /// Incoming Media decision failure.
    pub type HostMediaRespondIncomingError =
        LatestOf<versioned::media::HostMediaRespondIncomingError>;
    /// Idempotent Media participant removal request.
    pub type HostMediaRemoveParticipantRequest =
        LatestOf<versioned::media::HostMediaRemoveParticipantRequest>;
    /// Media participant removal result.
    pub type HostMediaRemoveParticipantResponse =
        LatestOf<versioned::media::HostMediaRemoveParticipantResponse>;
    /// Media participant removal failure.
    pub type HostMediaRemoveParticipantError =
        LatestOf<versioned::media::HostMediaRemoveParticipantError>;
    /// Correlated complete capture-intent replacement request.
    pub type HostMediaSetLocalTracksRequest =
        LatestOf<versioned::media::HostMediaSetLocalTracksRequest>;
    /// Authoritative session after capture-intent commitment.
    pub type HostMediaSetLocalTracksResponse =
        LatestOf<versioned::media::HostMediaSetLocalTracksResponse>;
    /// Capture-intent replacement failure.
    pub type HostMediaSetLocalTracksError =
        LatestOf<versioned::media::HostMediaSetLocalTracksError>;
    /// Atomic host-owned Media layout replacement request.
    pub type HostMediaSetSurfacesRequest = LatestOf<versioned::media::HostMediaSetSurfacesRequest>;
    /// Accepted Media layout revision.
    pub type HostMediaSetSurfacesResponse =
        LatestOf<versioned::media::HostMediaSetSurfacesResponse>;
    /// Media layout replacement failure.
    pub type HostMediaSetSurfacesError = LatestOf<versioned::media::HostMediaSetSurfacesError>;
    /// Idempotent Media session teardown request.
    pub type HostMediaEndSessionRequest = LatestOf<versioned::media::HostMediaEndSessionRequest>;
    /// Media session teardown result.
    pub type HostMediaEndSessionResponse = LatestOf<versioned::media::HostMediaEndSessionResponse>;
    /// Media session teardown failure.
    pub type HostMediaEndSessionError = LatestOf<versioned::media::HostMediaEndSessionError>;
    /// Exact Media operation outcome query.
    pub type HostMediaGetOperationRequest =
        LatestOf<versioned::media::HostMediaGetOperationRequest>;
    /// Authoritative Media operation outcome.
    pub type HostMediaGetOperationResponse =
        LatestOf<versioned::media::HostMediaGetOperationResponse>;
    /// Media operation query failure.
    pub type HostMediaGetOperationError = LatestOf<versioned::media::HostMediaGetOperationError>;
    /// Authoritative cancellation or pre-admission tombstone request.
    pub type HostMediaCancelOperationRequest =
        LatestOf<versioned::media::HostMediaCancelOperationRequest>;
    /// Media operation state after cancellation linearizes against commitment.
    pub type HostMediaCancelOperationResponse =
        LatestOf<versioned::media::HostMediaCancelOperationResponse>;
    /// Media operation cancellation failure.
    pub type HostMediaCancelOperationError =
        LatestOf<versioned::media::HostMediaCancelOperationError>;
    /// Locale the host currently presents its interface in.
    pub type HostLocaleSubscribeItem = LatestOf<versioned::locale::HostLocaleSubscribeItem>;
    /// Batched host-local calendar conversion request.
    pub type HostLocaleLocalizeTimestampsRequest =
        LatestOf<versioned::locale::HostLocaleLocalizeTimestampsRequest>;
    /// Batched host-local calendar conversion result.
    pub type HostLocaleLocalizeTimestampsResponse =
        LatestOf<versioned::locale::HostLocaleLocalizeTimestampsResponse>;
    pub use crate::v02::HostLocaleLocalizedTimestamp;
    /// Navigation request error.
    pub type HostNavigateToError = LatestOf<versioned::system::HostNavigateToError>;
    /// The calling product's Pocket cards.
    pub type HostPocketListSubscribeItem = LatestOf<versioned::pocket::HostPocketListSubscribeItem>;
    /// Pocket card removal request.
    pub type HostPocketRemoveCardRequest = LatestOf<versioned::pocket::HostPocketRemoveCardRequest>;
    /// Pocket card removal failure.
    pub type HostPocketRemoveCardError = LatestOf<versioned::pocket::HostPocketRemoveCardError>;
    /// Profile presentation request.
    pub type HostProfilePresentRequest = LatestOf<versioned::profile::HostProfilePresentRequest>;
    /// Profile presentation failure.
    pub type HostProfilePresentError = LatestOf<versioned::profile::HostProfilePresentError>;
    /// Contact avatar placement failure.
    pub type HostProfilePlaceContactAvatarsError =
        LatestOf<versioned::profile::HostProfilePlaceContactAvatarsError>;
    /// Profile disclosure request with explicit audiences.
    pub type HostProfileDiscloseRequest = LatestOf<versioned::profile::HostProfileDiscloseRequest>;
    /// Contact profile presentation selector.
    pub type HostProfilePresentContactRequest =
        LatestOf<versioned::profile::HostProfilePresentContactRequest>;
    /// Contact and own avatar geometry.
    pub type HostProfilePlaceContactAvatarsRequest =
        LatestOf<versioned::profile::HostProfilePlaceContactAvatarsRequest>;
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
    /// Native Chat cryptographic request using non-exportable Host keys.
    pub type HostProductDeviceChatRequest =
        LatestOf<versioned::account::HostProductDeviceChatRequest>;
    /// Authenticated native plaintext, prepared ciphertext, and custody metadata.
    pub type HostProductDeviceChatResponse =
        LatestOf<versioned::account::HostProductDeviceChatResponse>;
    /// Host-owned native Chat operation error.
    pub type HostProductDeviceChatError = LatestOf<versioned::account::HostProductDeviceChatError>;
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
#[cfg(feature = "host-api")]
#[derive(Clone, Default)]
pub struct CancellationToken {
    inner: Arc<CancellationInner>,
}

#[derive(Default)]
#[cfg(feature = "host-api")]
struct CancellationInner {
    state: Mutex<CancellationState>,
}

#[derive(Default)]
#[cfg(feature = "host-api")]
struct CancellationState {
    reason: Option<CancellationReason>,
    next_id: u64,
    wakers: Vec<(u64, Waker)>,
}

/// Cause attached to a cancelled call.
#[cfg(feature = "host-api")]
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
#[cfg(feature = "host-api")]
fn format_timeout(timeout: &Duration) -> String {
    if timeout.subsec_millis() == 0 {
        format!("{}s", timeout.as_secs())
    } else {
        format!("{}ms", timeout.as_millis())
    }
}

/// Future resolved when a [`CancellationToken`] is cancelled.
#[cfg(feature = "host-api")]
pub struct CancellationFuture {
    inner: Arc<CancellationInner>,
    id: Option<u64>,
}

#[cfg(feature = "host-api")]
impl fmt::Debug for CancellationToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CancellationToken")
            .field("reason", &self.reason())
            .finish_non_exhaustive()
    }
}

#[cfg(feature = "host-api")]
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

#[cfg(feature = "host-api")]
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

#[cfg(feature = "host-api")]
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
#[cfg(feature = "host-api")]
#[derive(Clone, Default)]
pub struct CallContext {
    request_id: String,
    cancel: CancellationToken,
    timeout: Option<Duration>,
}

#[cfg(feature = "host-api")]
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
#[cfg(feature = "host-api")]
pub struct Subscription<Item, Interrupt> {
    inner: Pin<Box<dyn Stream<Item = Result<Item, Interrupt>> + Send>>,
}

#[cfg(feature = "host-api")]
impl<Item, Interrupt> Stream for Subscription<Item, Interrupt> {
    type Item = Result<Item, Interrupt>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.inner.as_mut().poll_next(cx)
    }
}

#[cfg(feature = "host-api")]
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
    pub mod jam_peer_transport;
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
    #[cfg(any(test, not(target_arch = "wasm32")))]
    pub use runtime::StatementRenewalTarget;
    pub use runtime::contacts::contact_handle;
    pub use runtime::login_failure::reports_exhausted_period;
    pub use runtime::product_manifest::{encode_cached_root_manifest, manifest_cache_key};
    pub use runtime::statement_allowance;
    pub use runtime::{
        AnnouncedPairing, DevicePairingObserver, MAX_PAIRING_METADATA_CHARS, PairedSsoPeer,
        PairingProposal, PairingProposalMetadata, ResponderExit,
    };
    pub use runtime::{
        LocalIdentity, LocalIdentityContext, NativeChatContact, NativeChatContactsSnapshot,
        WalletAllowanceSnapshot,
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

#[cfg(all(test, feature = "host-api"))]
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
