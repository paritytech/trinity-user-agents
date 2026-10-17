//! Trusted native Media bridge: the optional capture/RTC/compositor backend a
//! native host installs per product execution.
//!
//! Native payloads mirror the canonical [`crate::platform::MediaPlatform`]
//! types field for field, so the host-facing callback surface stays stable and
//! capability bytes are validated once at publication.

use std::sync::{Arc, Mutex};

use futures::stream::{BoxStream, StreamExt};
use truapi::v01;

use crate::platform::{ProductContext, async_trait};

/// Finite, redacted failures on the trusted native Media bridge.
#[derive(Debug, Clone, Copy, thiserror::Error, uniffi::Error)]
pub enum NativeMediaError {
    /// The event sink or backend subscription is already closed.
    #[error("media backend closed")]
    Closed,
    /// The bounded event queue overflowed; the stream terminates.
    #[error("media event overflow")]
    EventOverflow,
    /// Any other backend failure, redacted before it reaches the core.
    #[error("media backend failure")]
    BackendFailure,
}

impl From<uniffi::UnexpectedUniFFICallbackError> for NativeMediaError {
    fn from(_: uniffi::UnexpectedUniFFICallbackError) -> Self {
        Self::BackendFailure
    }
}

impl From<NativeMediaError> for v01::GenericError {
    fn from(error: NativeMediaError) -> Self {
        Self { reason: error.to_string() }
    }
}

/// Native callback result for [`crate::platform::MediaBackendCapabilities`].
/// Fields map by value into the canonical platform record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct NativeMediaBackendCapabilities {
    /// Whether the backend implements the complete Media contract.
    pub supported: bool,
    /// Maximum simultaneous local sessions; at least one when supported.
    pub max_sessions: u16,
    /// Remote endpoints per session, excluding the local endpoint; at least five.
    pub max_remote_participants: u16,
    /// Pictures per session; at least twice the total endpoint capacity.
    pub max_surfaces_per_session: u16,
}

impl From<NativeMediaBackendCapabilities> for crate::platform::MediaBackendCapabilities {
    fn from(capabilities: NativeMediaBackendCapabilities) -> Self {
        let NativeMediaBackendCapabilities {
            supported,
            max_sessions,
            max_remote_participants,
            max_surfaces_per_session,
        } = capabilities;
        Self { supported, max_sessions, max_remote_participants, max_surfaces_per_session }
    }
}

/// Native mirror of [`v01::MediaCameraKind`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum NativeMediaCameraKind {
    /// Front-facing camera.
    Front,
    /// Rear-facing camera.
    Rear,
    /// Any other camera class.
    Other,
}

impl From<v01::MediaCameraKind> for NativeMediaCameraKind {
    fn from(value: v01::MediaCameraKind) -> Self {
        match value {
            v01::MediaCameraKind::Front => Self::Front,
            v01::MediaCameraKind::Rear => Self::Rear,
            v01::MediaCameraKind::Other => Self::Other,
        }
    }
}

impl From<NativeMediaCameraKind> for v01::MediaCameraKind {
    fn from(value: NativeMediaCameraKind) -> Self {
        match value {
            NativeMediaCameraKind::Front => Self::Front,
            NativeMediaCameraKind::Rear => Self::Rear,
            NativeMediaCameraKind::Other => Self::Other,
        }
    }
}

/// Native mirror of [`v01::MediaAudioRoute`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum NativeMediaAudioRoute {
    /// Handset earpiece.
    Earpiece,
    /// Loudspeaker.
    Speaker,
    /// Headset output.
    Headset,
    /// Any other route class.
    Other,
}

impl From<v01::MediaAudioRoute> for NativeMediaAudioRoute {
    fn from(value: v01::MediaAudioRoute) -> Self {
        match value {
            v01::MediaAudioRoute::Earpiece => Self::Earpiece,
            v01::MediaAudioRoute::Speaker => Self::Speaker,
            v01::MediaAudioRoute::Headset => Self::Headset,
            v01::MediaAudioRoute::Other => Self::Other,
        }
    }
}

impl From<NativeMediaAudioRoute> for v01::MediaAudioRoute {
    fn from(value: NativeMediaAudioRoute) -> Self {
        match value {
            NativeMediaAudioRoute::Earpiece => Self::Earpiece,
            NativeMediaAudioRoute::Speaker => Self::Speaker,
            NativeMediaAudioRoute::Headset => Self::Headset,
            NativeMediaAudioRoute::Other => Self::Other,
        }
    }
}

/// Native mirror of [`v01::MediaLocalTracks`].
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct NativeMediaLocalTracks {
    /// Request microphone capture.
    pub microphone: bool,
    /// Request camera capture.
    pub camera: bool,
    /// Request screen capture without selecting a source on the product side.
    pub screen: bool,
    /// Advisory camera class preference.
    pub camera_preference: Option<NativeMediaCameraKind>,
    /// Advisory audio route preference.
    pub audio_preference: Option<NativeMediaAudioRoute>,
}

impl From<v01::MediaLocalTracks> for NativeMediaLocalTracks {
    fn from(value: v01::MediaLocalTracks) -> Self {
        Self {
            microphone: value.microphone,
            camera: value.camera,
            screen: value.screen,
            camera_preference: value.camera_preference.map(Into::into),
            audio_preference: value.audio_preference.map(Into::into),
        }
    }
}

/// Native mirror of [`v01::MediaTrackState`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum NativeMediaTrackState {
    /// No sending or stale picture remains after an off intent commits.
    Off,
    /// Requested but not yet live.
    Starting,
    /// Host-observed live track.
    Live,
    /// Temporarily interrupted, distinct from permission revocation.
    Interrupted,
}

impl From<NativeMediaTrackState> for v01::MediaTrackState {
    fn from(value: NativeMediaTrackState) -> Self {
        match value {
            NativeMediaTrackState::Off => Self::Off,
            NativeMediaTrackState::Starting => Self::Starting,
            NativeMediaTrackState::Live => Self::Live,
            NativeMediaTrackState::Interrupted => Self::Interrupted,
        }
    }
}

/// Native mirror of [`v01::MediaLocalState`].
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct NativeMediaLocalState {
    /// Actual microphone state.
    pub microphone: NativeMediaTrackState,
    /// Actual camera state.
    pub camera: NativeMediaTrackState,
    /// Actual screen-sharing state.
    pub screen: NativeMediaTrackState,
    /// Actual camera class, if selected.
    pub camera_kind: Option<NativeMediaCameraKind>,
    /// Actual audio route class, if selected.
    pub audio_route: Option<NativeMediaAudioRoute>,
}

impl From<NativeMediaLocalState> for v01::MediaLocalState {
    fn from(value: NativeMediaLocalState) -> Self {
        Self {
            microphone: value.microphone.into(),
            camera: value.camera.into(),
            screen: value.screen.into(),
            camera_kind: value.camera_kind.map(Into::into),
            audio_route: value.audio_route.map(Into::into),
        }
    }
}

/// Native mirror of [`v01::MediaRemoteState`].
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct NativeMediaRemoteState {
    /// Remote microphone state.
    pub microphone: NativeMediaTrackState,
    /// Remote camera state.
    pub camera: NativeMediaTrackState,
    /// Remote screen-sharing state.
    pub screen: NativeMediaTrackState,
}

impl From<NativeMediaRemoteState> for v01::MediaRemoteState {
    fn from(value: NativeMediaRemoteState) -> Self {
        Self {
            microphone: value.microphone.into(),
            camera: value.camera.into(),
            screen: value.screen.into(),
        }
    }
}

/// Native mirror of [`v01::MediaViewport`].
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct NativeMediaViewport {
    /// Runtime-monotonic transform/attachment revision.
    pub revision: u64,
    /// Logical viewport width.
    pub width: u32,
    /// Logical viewport height.
    pub height: u32,
    /// Effective uniform physical-pixels/logical-unit ratio numerator.
    pub device_scale_numerator: u32,
    /// Positive scale denominator; products must not pre-scale rectangles.
    pub device_scale_denominator: u32,
}

impl From<NativeMediaViewport> for v01::MediaViewport {
    fn from(value: NativeMediaViewport) -> Self {
        Self {
            revision: value.revision,
            width: value.width,
            height: value.height,
            device_scale_numerator: value.device_scale_numerator,
            device_scale_denominator: value.device_scale_denominator,
        }
    }
}

/// Native mirror of [`v01::MediaPictureKind`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum NativeMediaPictureKind {
    /// Camera picture.
    Camera,
    /// Screen-sharing picture.
    Screen,
}

impl From<v01::MediaPictureKind> for NativeMediaPictureKind {
    fn from(value: v01::MediaPictureKind) -> Self {
        match value {
            v01::MediaPictureKind::Camera => Self::Camera,
            v01::MediaPictureKind::Screen => Self::Screen,
        }
    }
}

/// Native mirror of [`v01::MediaPictureSource`].
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum NativeMediaPictureSource {
    /// Self-preview uses the same isolated compositor as remote pictures.
    Local { picture: NativeMediaPictureKind },
    /// Picture belonging to an owned participant in this session.
    Remote { participant_id: Vec<u8>, picture: NativeMediaPictureKind },
}

impl From<v01::MediaPictureSource> for NativeMediaPictureSource {
    fn from(value: v01::MediaPictureSource) -> Self {
        match value {
            v01::MediaPictureSource::Local { picture } => Self::Local { picture: picture.into() },
            v01::MediaPictureSource::Remote { participant_id, picture } => Self::Remote {
                participant_id: participant_id.to_vec(),
                picture: picture.into(),
            },
        }
    }
}

/// Native mirror of [`v01::MediaRect`].
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct NativeMediaRect {
    /// Left edge relative to viewport origin.
    pub x: i32,
    /// Top edge relative to viewport origin.
    pub y: i32,
    /// Logical width.
    pub width: u32,
    /// Logical height.
    pub height: u32,
}

impl From<v01::MediaRect> for NativeMediaRect {
    fn from(value: v01::MediaRect) -> Self {
        Self { x: value.x, y: value.y, width: value.width, height: value.height }
    }
}

/// Native mirror of [`v01::MediaPlacement`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum NativeMediaPlacement {
    /// Below all product content; the product must be transparent there.
    BelowProduct,
    /// Above all product content, not interleaved with arbitrary DOM z-indices.
    AboveProduct,
}

impl From<v01::MediaPlacement> for NativeMediaPlacement {
    fn from(value: v01::MediaPlacement) -> Self {
        match value {
            v01::MediaPlacement::BelowProduct => Self::BelowProduct,
            v01::MediaPlacement::AboveProduct => Self::AboveProduct,
        }
    }
}

/// Native mirror of [`v01::MediaFit`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum NativeMediaFit {
    /// Preserve the full picture with letterboxing.
    Contain,
    /// Crop to fill the rectangle.
    Cover,
}

impl From<v01::MediaFit> for NativeMediaFit {
    fn from(value: v01::MediaFit) -> Self {
        match value {
            v01::MediaFit::Contain => Self::Contain,
            v01::MediaFit::Cover => Self::Cover,
        }
    }
}

/// Native mirror of [`v01::MediaSurface`].
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct NativeMediaSurface {
    /// Product-chosen layout key, unique within the session's submitted set.
    pub surface_id: u32,
    /// Local or owned remote picture.
    pub source: NativeMediaPictureSource,
    /// Target logical viewport rectangle.
    pub rect: NativeMediaRect,
    /// Additional clipping rectangle, intersected with viewport and host region.
    pub clip: NativeMediaRect,
    /// Clamped to half the smaller target dimension.
    pub corner_radius: u32,
    /// Placement plane relative to all product content.
    pub placement: NativeMediaPlacement,
    /// Lower is behind higher; ties sort by session bytes then surface ID.
    pub depth: i32,
    /// Aspect-ratio treatment within the clip.
    pub fit: NativeMediaFit,
    /// Presentation only; does not change what peers receive.
    pub mirrored: bool,
    /// False draws no picture, without changing capture or audio playback.
    pub visible: bool,
}

impl From<v01::MediaSurface> for NativeMediaSurface {
    fn from(value: v01::MediaSurface) -> Self {
        Self {
            surface_id: value.surface_id,
            source: value.source.into(),
            rect: value.rect.into(),
            clip: value.clip.into(),
            corner_radius: value.corner_radius,
            placement: value.placement.into(),
            depth: value.depth,
            fit: value.fit.into(),
            mirrored: value.mirrored,
            visible: value.visible,
        }
    }
}

/// Native mirror of [`v01::MediaResource`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum NativeMediaResource {
    /// Runtime session issuance budget.
    Sessions,
    /// Runtime participant issuance budget.
    Participants,
    /// Incoming issuance or pending-offer budget.
    Incoming,
    /// Retained operation budget.
    Operations,
    /// Active subscription budget.
    Subscriptions,
}

impl From<NativeMediaResource> for v01::MediaResource {
    fn from(value: NativeMediaResource) -> Self {
        match value {
            NativeMediaResource::Sessions => Self::Sessions,
            NativeMediaResource::Participants => Self::Participants,
            NativeMediaResource::Incoming => Self::Incoming,
            NativeMediaResource::Operations => Self::Operations,
            NativeMediaResource::Subscriptions => Self::Subscriptions,
        }
    }
}

/// Native mirror of [`v01::HostMediaError`].
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum NativeMediaDomainError {
    /// Calling connectivity is unavailable.
    NotConnected,
    /// Peer identity is malformed or unauthenticated.
    InvalidPeer,
    /// Peer belongs to another network namespace.
    NetworkMismatch,
    /// Peer belongs to another canonical product.
    ProductMismatch,
    /// Random and unowned handles are indistinguishable.
    InvalidHandle,
    /// Owned session is terminal.
    SessionEnded,
    /// Incoming offer expired or was cancelled.
    IncomingExpired,
    /// Incoming offer was already claimed.
    IncomingConsumed,
    /// No runtime listener has an initial snapshot enqueued.
    SubscriptionRequired,
    /// Live endpoint capacity would be exceeded before any signaling.
    CapacityExceeded { limit: u16 },
    /// Mutation is invalid or superseded by a newer admitted intent.
    InvalidState,
    /// Invalid surface set, including conflicting reuse of a layout revision.
    InvalidSurface,
    /// Submitted viewport transform is no longer current.
    StaleViewport { current_revision: u64 },
    /// Submitted layout revision predates the committed or queued layout.
    StaleLayout { current_revision: u64 },
    /// No authorized rendering attachment is available.
    SurfaceUnavailable,
    /// A required capture device is currently unavailable.
    DeviceUnavailable,
    /// User cancelled the trusted screen picker.
    CaptureCancelled,
    /// Host operation or consent deadline elapsed; late work cannot commit.
    TimedOut,
    /// Listener queue overflowed; resubscribe for an atomic fresh snapshot.
    EventOverflow,
    /// No operation exists for the queried key in this runtime.
    InvalidOperation,
    /// Same key was previously bound to a different request.
    OperationConflict,
    /// Cancellation tombstone prevents the mutation from acting.
    OperationCancelled,
    /// A finite runtime budget would be exceeded before effects begin.
    ResourceExhausted { resource: NativeMediaResource },
}

impl From<NativeMediaDomainError> for v01::HostMediaError {
    fn from(value: NativeMediaDomainError) -> Self {
        match value {
            NativeMediaDomainError::NotConnected => Self::NotConnected,
            NativeMediaDomainError::InvalidPeer => Self::InvalidPeer,
            NativeMediaDomainError::NetworkMismatch => Self::NetworkMismatch,
            NativeMediaDomainError::ProductMismatch => Self::ProductMismatch,
            NativeMediaDomainError::InvalidHandle => Self::InvalidHandle,
            NativeMediaDomainError::SessionEnded => Self::SessionEnded,
            NativeMediaDomainError::IncomingExpired => Self::IncomingExpired,
            NativeMediaDomainError::IncomingConsumed => Self::IncomingConsumed,
            NativeMediaDomainError::SubscriptionRequired => Self::SubscriptionRequired,
            NativeMediaDomainError::CapacityExceeded { limit } => Self::CapacityExceeded { limit },
            NativeMediaDomainError::InvalidState => Self::InvalidState,
            NativeMediaDomainError::InvalidSurface => Self::InvalidSurface,
            NativeMediaDomainError::StaleViewport { current_revision } => Self::StaleViewport { current_revision },
            NativeMediaDomainError::StaleLayout { current_revision } => Self::StaleLayout { current_revision },
            NativeMediaDomainError::SurfaceUnavailable => Self::SurfaceUnavailable,
            NativeMediaDomainError::DeviceUnavailable => Self::DeviceUnavailable,
            NativeMediaDomainError::CaptureCancelled => Self::CaptureCancelled,
            NativeMediaDomainError::TimedOut => Self::TimedOut,
            NativeMediaDomainError::EventOverflow => Self::EventOverflow,
            NativeMediaDomainError::InvalidOperation => Self::InvalidOperation,
            NativeMediaDomainError::OperationConflict => Self::OperationConflict,
            NativeMediaDomainError::OperationCancelled => Self::OperationCancelled,
            NativeMediaDomainError::ResourceExhausted { resource } => Self::ResourceExhausted { resource: resource.into() },
        }
    }
}

/// Native mirror of [`v01::MediaOperationFailure`].
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum NativeMediaOperationFailure {
    /// User or authority denied the operation.
    Denied,
    /// Bounded Media domain failure.
    Domain { error: NativeMediaDomainError },
    /// Host failure without a diagnostic string.
    HostFailure,
}

impl From<NativeMediaOperationFailure> for v01::MediaOperationFailure {
    fn from(value: NativeMediaOperationFailure) -> Self {
        match value {
            NativeMediaOperationFailure::Denied => Self::Denied,
            NativeMediaOperationFailure::Domain { error } => Self::Domain { error: error.into() },
            NativeMediaOperationFailure::HostFailure => Self::HostFailure,
        }
    }
}

/// Native mirror of [`crate::platform::MediaConsentRequest`].
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum NativeMediaConsentRequest {
    /// Calling under the immutable active authority.
    Calling { network: Vec<u8>, account: Vec<u8> },
    /// Host-owned microphone capture, not raw product capture.
    Microphone,
    /// Host-owned camera capture, not raw product capture.
    Camera,
}

impl From<crate::platform::MediaConsentRequest> for NativeMediaConsentRequest {
    fn from(value: crate::platform::MediaConsentRequest) -> Self {
        match value {
            crate::platform::MediaConsentRequest::Calling { network, account } => Self::Calling {
                network: network.to_vec(),
                account: account.to_vec(),
            },
            crate::platform::MediaConsentRequest::Microphone => Self::Microphone,
            crate::platform::MediaConsentRequest::Camera => Self::Camera,
        }
    }
}

/// Native mirror of [`crate::platform::MediaDescriptionKind`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum NativeMediaDescriptionKind {
    /// Description initiating negotiation for the participant's connection.
    Offer,
    /// Description responding to that connection's authenticated remote offer.
    Answer,
}

impl From<crate::platform::MediaDescriptionKind> for NativeMediaDescriptionKind {
    fn from(value: crate::platform::MediaDescriptionKind) -> Self {
        match value {
            crate::platform::MediaDescriptionKind::Offer => Self::Offer,
            crate::platform::MediaDescriptionKind::Answer => Self::Answer,
        }
    }
}

impl From<NativeMediaDescriptionKind> for crate::platform::MediaDescriptionKind {
    fn from(value: NativeMediaDescriptionKind) -> Self {
        match value {
            NativeMediaDescriptionKind::Offer => Self::Offer,
            NativeMediaDescriptionKind::Answer => Self::Answer,
        }
    }
}

/// Native mirror of [`crate::platform::MediaDescription`].
#[derive(derive_more::Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct NativeMediaDescription {
    /// Negotiation role of this description.
    pub kind: NativeMediaDescriptionKind,
    #[debug("<redacted>")]
    pub sdp: String,
}

impl From<crate::platform::MediaDescription> for NativeMediaDescription {
    fn from(value: crate::platform::MediaDescription) -> Self {
        Self { kind: value.kind.into(), sdp: value.sdp }
    }
}

impl From<NativeMediaDescription> for crate::platform::MediaDescription {
    fn from(value: NativeMediaDescription) -> Self {
        Self { kind: value.kind.into(), sdp: value.sdp }
    }
}

/// Native mirror of [`crate::platform::MediaIceCandidate`].
#[derive(derive_more::Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct NativeMediaIceCandidate {
    #[debug("<redacted>")]
    pub candidate: String,
    #[debug("<redacted>")]
    pub mid: Option<String>,
    /// Optional media section index.
    pub mline_index: Option<u16>,
}

impl From<crate::platform::MediaIceCandidate> for NativeMediaIceCandidate {
    fn from(value: crate::platform::MediaIceCandidate) -> Self {
        Self { candidate: value.candidate, mid: value.mid, mline_index: value.mline_index }
    }
}

impl From<NativeMediaIceCandidate> for crate::platform::MediaIceCandidate {
    fn from(value: NativeMediaIceCandidate) -> Self {
        Self { candidate: value.candidate, mid: value.mid, mline_index: value.mline_index }
    }
}

/// Core-admitted native command; all capability bytes are exactly 32 bytes.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum NativeMediaBackendCommand {
    /// Prepare a local session and authorized capture with a trusted pending
    /// indicator. Returns `Done`; does not attach/send tracks, create peers,
    /// or publish committed state before `CommitOperation`.
    OpenSession { session_id: Vec<u8>, operation_id: Vec<u8>, tracks: NativeMediaLocalTracks },
    /// Prepare replacement capture without attaching/sending the new tracks
    /// or publishing committed state. Returns `Done`. A newer revision
    /// supersedes uncommitted work; failed or cancelled acquisition preserves
    /// the last committed intent until `CommitOperation`.
    SetTracks { session_id: Vec<u8>, operation_id: Vec<u8>, intent_revision: u64, tracks: NativeMediaLocalTracks },
    /// Cancel pending picker/device work, drop prepared capture, and stop
    /// late-acquired resources. The core dispatches this only if cancellation
    /// won before its commit decision; it must not undo committed state.
    CancelOperation { operation_id: Vec<u8> },
    /// Stop capture/sending, clear surfaces, and release session resources.
    /// Cleanup is idempotent and must not ask permission or allocate quota.
    CloseSession { session_id: Vec<u8> },
    /// Create one participant's connection; local descriptions/candidates are
    /// returned through the trusted event stream, never through product frames.
    CreatePeer { session_id: Vec<u8>, participant_id: Vec<u8>, offerer: bool },
    /// Apply a description delivered by core-authenticated signaling.
    ApplyDescription { session_id: Vec<u8>, participant_id: Vec<u8>, description: NativeMediaDescription },
    /// Apply a candidate delivered by core-authenticated signaling.
    AddIceCandidate { session_id: Vec<u8>, participant_id: Vec<u8>, candidate: NativeMediaIceCandidate },
    /// Idempotently close and release one participant's connection.
    RemovePeer { session_id: Vec<u8>, participant_id: Vec<u8> },
    /// Atomically replace this session's pictures for the current viewport.
    /// An empty list clears its surfaces; no product-readable frame is returned.
    SetSurfaces { session_id: Vec<u8>, viewport_revision: u64, layout_revision: u64, surfaces: Vec<NativeMediaSurface> },
    /// Idempotently release every resource owned by this product runtime and
    /// fence all late work so it cannot recreate capture, peers, or surfaces.
    CloseRuntime,
    /// Apply prepared tracks to all peer senders atomically, stop replaced/off
    /// tracks, and return `LocalState`. The core records its commit decision
    /// before dispatch; later cancellation waits for the committed/failed
    /// outcome and must never report the operation as cancelled.
    CommitOperation { operation_id: Vec<u8> },
    /// Present an operation-scoped trusted consent prompt only when the core
    /// has no stored decision. CancelOperation/CloseRuntime must dismiss it;
    /// a late answer must never prepare capture or write a permission grant.
    RequestConsent { operation_id: Vec<u8>, request: NativeMediaConsentRequest },
}

impl From<crate::platform::MediaBackendCommand> for NativeMediaBackendCommand {
    fn from(value: crate::platform::MediaBackendCommand) -> Self {
        use crate::platform::MediaBackendCommand;
        match value {
            MediaBackendCommand::OpenSession { session_id, operation_id, tracks } => Self::OpenSession {
                session_id: session_id.to_vec(), operation_id: operation_id.to_vec(), tracks: tracks.into(),
            },
            MediaBackendCommand::SetTracks { session_id, operation_id, intent_revision, tracks } => Self::SetTracks {
                session_id: session_id.to_vec(), operation_id: operation_id.to_vec(), intent_revision, tracks: tracks.into(),
            },
            MediaBackendCommand::CancelOperation { operation_id } => Self::CancelOperation { operation_id: operation_id.to_vec() },
            MediaBackendCommand::CloseSession { session_id } => Self::CloseSession { session_id: session_id.to_vec() },
            MediaBackendCommand::CreatePeer { session_id, participant_id, offerer } => Self::CreatePeer {
                session_id: session_id.to_vec(), participant_id: participant_id.to_vec(), offerer,
            },
            MediaBackendCommand::ApplyDescription { session_id, participant_id, description } => Self::ApplyDescription {
                session_id: session_id.to_vec(), participant_id: participant_id.to_vec(), description: description.into(),
            },
            MediaBackendCommand::AddIceCandidate { session_id, participant_id, candidate } => Self::AddIceCandidate {
                session_id: session_id.to_vec(), participant_id: participant_id.to_vec(), candidate: candidate.into(),
            },
            MediaBackendCommand::RemovePeer { session_id, participant_id } => Self::RemovePeer {
                session_id: session_id.to_vec(), participant_id: participant_id.to_vec(),
            },
            MediaBackendCommand::SetSurfaces { session_id, viewport_revision, layout_revision, surfaces } => Self::SetSurfaces {
                session_id: session_id.to_vec(), viewport_revision, layout_revision,
                surfaces: surfaces.into_iter().map(Into::into).collect(),
            },
            MediaBackendCommand::CloseRuntime => Self::CloseRuntime,
            MediaBackendCommand::CommitOperation { operation_id } => Self::CommitOperation { operation_id: operation_id.to_vec() },
            MediaBackendCommand::RequestConsent { operation_id, request } => Self::RequestConsent {
                operation_id: operation_id.to_vec(), request: request.into(),
            },
        }
    }
}

/// Native mirror of [`crate::platform::MediaBackendPeerState`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum NativeMediaBackendPeerState {
    /// Initial connection establishment is still in progress.
    Connecting,
    /// The connection is established; this does not assert track availability or quality.
    Connected,
    /// An established connection is temporarily interrupted and may recover.
    Reconnecting,
    /// The connection has failed and cannot continue carrying this participant's media.
    Failed,
    /// The participant's connection has been closed and its resources released.
    Closed,
}

impl From<NativeMediaBackendPeerState> for crate::platform::MediaBackendPeerState {
    fn from(value: NativeMediaBackendPeerState) -> Self {
        match value {
            NativeMediaBackendPeerState::Connecting => Self::Connecting,
            NativeMediaBackendPeerState::Connected => Self::Connected,
            NativeMediaBackendPeerState::Reconnecting => Self::Reconnecting,
            NativeMediaBackendPeerState::Failed => Self::Failed,
            NativeMediaBackendPeerState::Closed => Self::Closed,
        }
    }
}

/// Native mirror of [`crate::platform::MediaRevokedPermission`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum NativeMediaRevokedPermission {
    /// Scoped calling authority; withdrawal ends affected sessions, including receive-only ones.
    Calling,
    /// Microphone capture authorization; withdrawal is not a request to mute silently.
    Microphone,
    /// Camera capture authorization; withdrawal is not a request to disable video silently.
    Camera,
}

impl From<NativeMediaRevokedPermission> for crate::platform::MediaRevokedPermission {
    fn from(value: NativeMediaRevokedPermission) -> Self {
        match value {
            NativeMediaRevokedPermission::Calling => Self::Calling,
            NativeMediaRevokedPermission::Microphone => Self::Microphone,
            NativeMediaRevokedPermission::Camera => Self::Camera,
        }
    }
}

/// Native mirror of [`crate::platform::MediaRevocationSource`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum NativeMediaRevocationSource {
    /// Explicit withdrawal in trusted product-permission settings.
    Product,
    /// Withdrawal of the host application's OS permission.
    OperatingSystem,
}

impl From<NativeMediaRevocationSource> for crate::platform::MediaRevocationSource {
    fn from(value: NativeMediaRevocationSource) -> Self {
        match value {
            NativeMediaRevocationSource::Product => Self::Product,
            NativeMediaRevocationSource::OperatingSystem => Self::OperatingSystem,
        }
    }
}

/// Native observations; malformed capability bytes are rejected at publication.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum NativeMediaBackendEvent {
    /// Current host rendering attachment; `None` also clears its surfaces.
    ViewportChanged { viewport: Option<NativeMediaViewport> },
    /// Observed capture state, fenced by the accepted track intent revision.
    LocalStateChanged { session_id: Vec<u8>, intent_revision: u64, state: NativeMediaLocalState },
    /// Observed connectivity, not inferred media quality.
    PeerStateChanged { session_id: Vec<u8>, participant_id: Vec<u8>, state: NativeMediaBackendPeerState },
    /// Observed remote track availability.
    RemoteStateChanged { session_id: Vec<u8>, participant_id: Vec<u8>, state: NativeMediaRemoteState },
    /// Locally generated description for authenticated signaling.
    Description { session_id: Vec<u8>, participant_id: Vec<u8>, description: NativeMediaDescription },
    /// Locally generated candidate for authenticated signaling.
    IceCandidate { session_id: Vec<u8>, participant_id: Vec<u8>, candidate: NativeMediaIceCandidate },
    /// Trusted screen-stop control invalidated prior pending screen intent.
    ScreenStopped { session_id: Vec<u8> },
    /// Trusted host hangup control ended the session.
    HostEnded { session_id: Vec<u8> },
    /// End affected sessions without silently downgrading capture. Only an
    /// explicit product-setting withdrawal changes the persisted core grant.
    PermissionRevoked { permission: NativeMediaRevokedPermission, source: NativeMediaRevocationSource },
}

impl TryFrom<NativeMediaBackendEvent> for crate::platform::MediaBackendEvent {
    type Error = NativeMediaError;

    fn try_from(value: NativeMediaBackendEvent) -> Result<Self, Self::Error> {
        fn handle(bytes: Vec<u8>) -> Result<[u8; 32], NativeMediaError> {
            bytes.try_into().map_err(|_| NativeMediaError::BackendFailure)
        }

        Ok(match value {
            NativeMediaBackendEvent::ViewportChanged { viewport } => Self::ViewportChanged {
                viewport: viewport.map(Into::into),
            },
            NativeMediaBackendEvent::LocalStateChanged { session_id, intent_revision, state } => Self::LocalStateChanged {
                session_id: handle(session_id)?, intent_revision, state: state.into(),
            },
            NativeMediaBackendEvent::PeerStateChanged { session_id, participant_id, state } => Self::PeerStateChanged {
                session_id: handle(session_id)?, participant_id: handle(participant_id)?, state: state.into(),
            },
            NativeMediaBackendEvent::RemoteStateChanged { session_id, participant_id, state } => Self::RemoteStateChanged {
                session_id: handle(session_id)?, participant_id: handle(participant_id)?, state: state.into(),
            },
            NativeMediaBackendEvent::Description { session_id, participant_id, description } => Self::Description {
                session_id: handle(session_id)?, participant_id: handle(participant_id)?, description: description.into(),
            },
            NativeMediaBackendEvent::IceCandidate { session_id, participant_id, candidate } => Self::IceCandidate {
                session_id: handle(session_id)?, participant_id: handle(participant_id)?, candidate: candidate.into(),
            },
            NativeMediaBackendEvent::ScreenStopped { session_id } => Self::ScreenStopped { session_id: handle(session_id)? },
            NativeMediaBackendEvent::HostEnded { session_id } => Self::HostEnded { session_id: handle(session_id)? },
            NativeMediaBackendEvent::PermissionRevoked { permission, source } => Self::PermissionRevoked {
                permission: permission.into(), source: source.into(),
            },
        })
    }
}

/// Native callback result for [`crate::platform::MediaBackendResponse`].
///
/// Conversion moves its fields into the canonical platform response.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum NativeMediaBackendResponse {
    /// Command completed without publishing new committed capture state.
    Done,
    /// Accepted capture intent's currently observed device state.
    LocalState { state: NativeMediaLocalState },
    /// Expected refusal or domain failure. Unexpected callback errors remain
    /// [`NativeMediaError`] and are sanitized to `HostFailure` by the core.
    Rejected {
        /// Safe failure suitable for retained operation outcomes.
        failure: NativeMediaOperationFailure,
    },
    /// A genuine user decision. Dismissal/cancellation is a rejected response,
    /// never a remembered denial; only the core persists this answer.
    Consent {
        /// Whether the user approved the immutable consent question.
        granted: bool,
    },
}

impl From<NativeMediaBackendResponse> for crate::platform::MediaBackendResponse {
    fn from(response: NativeMediaBackendResponse) -> Self {
        match response {
            NativeMediaBackendResponse::Done => Self::Done,
            NativeMediaBackendResponse::LocalState { state } => Self::LocalState { state: state.into() },
            NativeMediaBackendResponse::Rejected { failure } => Self::Rejected { failure: failure.into() },
            NativeMediaBackendResponse::Consent { granted } => Self::Consent { granted },
        }
    }
}

/// Complete, trusted capture/RTC/compositor adapter. Callbacks must not log
/// descriptions, candidates, device identifiers, or relay credentials.
#[uniffi::export(rust, foreign)]
#[async_trait::async_trait]
pub trait NativeMediaCallbacks: Send + Sync {
    /// Probe availability without prompting or capturing.
    async fn capabilities(
        &self,
        product: ProductContext,
    ) -> Result<NativeMediaBackendCapabilities, NativeMediaError>;

    /// Start a live stream, publishing the current viewport first. Return
    /// promptly; the sink may be called from any thread, including this call.
    fn subscribe(
        &self,
        product: ProductContext,
        runtime_id: u64,
        event_sink: Arc<NativeMediaEventSink>,
    ) -> Result<(), NativeMediaError>;

    /// Capture may await a picker, but cancellation and teardown must remain
    /// independently actionable while another command is pending.
    async fn command(
        &self,
        product: ProductContext,
        runtime_id: u64,
        command: NativeMediaBackendCommand,
    ) -> Result<NativeMediaBackendResponse, NativeMediaError>;

    /// Release the event subscription; idempotent, prompt, and nonblocking.
    /// Runtime resources are released separately by CloseRuntime.
    fn unsubscribe(&self, runtime_id: u64);
}

const NATIVE_MEDIA_EVENT_CAPACITY: usize = 128;

#[derive(Default)]
struct NativeMediaQueue {
    events: std::collections::VecDeque<crate::platform::MediaBackendEvent>,
    closed: bool,
    terminal: Option<NativeMediaError>,
}

/// Trusted live event publisher scoped to one product runtime. Closing the
/// subscription invalidates every retained copy of this sink.
#[derive(uniffi::Object, Default)]
pub struct NativeMediaEventSink {
    queue: Mutex<NativeMediaQueue>,
    waker: futures::task::AtomicWaker,
}

#[uniffi::export]
impl NativeMediaEventSink {
    /// Publish one private event. Overflow terminates the stream instead of
    /// silently dropping state; no further event can be accepted.
    pub fn publish(
        &self,
        event: NativeMediaBackendEvent,
    ) -> Result<(), NativeMediaError> {
        let mut queue = self.queue.lock().unwrap_or_else(|p| p.into_inner());
        if queue.closed {
            return Err(NativeMediaError::Closed);
        }
        let result = if queue.events.len() == NATIVE_MEDIA_EVENT_CAPACITY {
            queue.events.clear();
            queue.closed = true;
            queue.terminal = Some(NativeMediaError::EventOverflow);
            Err(NativeMediaError::EventOverflow)
        } else {
            queue.events.push_back(event.try_into()?);
            Ok(())
        };
        drop(queue);
        self.waker.wake();
        result
    }
}

struct NativeMediaStream {
    sink: Arc<NativeMediaEventSink>,
    callbacks: Option<Arc<dyn NativeMediaCallbacks>>,
    runtime_id: u64,
}

impl NativeMediaStream {
    fn close(&mut self) {
        {
            let mut queue = self.sink.queue.lock().unwrap_or_else(|p| p.into_inner());
            queue.closed = true;
            queue.events.clear();
        }
        if let Some(callbacks) = self.callbacks.take() {
            callbacks.unsubscribe(self.runtime_id);
        }
    }
}

impl futures::Stream for NativeMediaStream {
    type Item = Result<crate::platform::MediaBackendEvent, v01::GenericError>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.sink.waker.register(cx.waker());
        let mut queue = self.sink.queue.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(error) = queue.terminal.take() {
            drop(queue);
            self.close();
            return std::task::Poll::Ready(Some(Err(error.into())));
        }
        if let Some(event) = queue.events.pop_front() {
            return std::task::Poll::Ready(Some(Ok(event)));
        }
        if queue.closed {
            std::task::Poll::Ready(None)
        } else {
            std::task::Poll::Pending
        }
    }
}

impl Drop for NativeMediaStream {
    fn drop(&mut self) {
        self.close();
    }
}

pub(super) struct MediaCallbackPlatform {
    pub(super) callbacks: Arc<dyn NativeMediaCallbacks>,
}

#[async_trait]
impl crate::platform::MediaPlatform for MediaCallbackPlatform {
    async fn media_backend_capabilities(
        &self,
        product: &ProductContext,
    ) -> Result<crate::platform::MediaBackendCapabilities, v01::GenericError> {
        self.callbacks.capabilities(product.clone()).await.map(Into::into).map_err(Into::into)
    }

    fn media_backend_events(
        &self,
        product: &ProductContext,
        runtime_id: u64,
    ) -> BoxStream<'static, Result<crate::platform::MediaBackendEvent, v01::GenericError>> {
        let sink = Arc::new(NativeMediaEventSink::default());
        let stream = NativeMediaStream {
            sink: sink.clone(),
            callbacks: Some(self.callbacks.clone()),
            runtime_id,
        };
        if let Err(error) = self.callbacks.subscribe(product.clone(), runtime_id, sink.clone()) {
            let mut queue = sink.queue.lock().unwrap_or_else(|p| p.into_inner());
            queue.events.clear();
            queue.closed = true;
            queue.terminal = Some(error);
        }
        stream.boxed()
    }

    async fn media_backend_command(
        &self,
        product: &ProductContext,
        runtime_id: u64,
        command: crate::platform::MediaBackendCommand,
    ) -> Result<crate::platform::MediaBackendResponse, v01::GenericError> {
        self.callbacks.command(product.clone(), runtime_id, command.into()).await.map(Into::into).map_err(Into::into)
    }
}

