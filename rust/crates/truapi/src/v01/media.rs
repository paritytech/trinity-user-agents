//! Host-owned Media V1 payloads. No transport, capture-device, or readable media
//! handles cross this boundary. Enum order is part of the SCALE wire contract.

use alloc::{string::String, vec::Vec};
use parity_scale_codec::{Decode, Encode};

/// Host-minted, unguessable session capability, bound to one product runtime.
pub type MediaSessionId = [u8; 32];
/// Host-minted remote endpoint capability within one owned session.
pub type MediaParticipantId = [u8; 32];
/// Host-minted, single-use authenticated incoming-offer capability.
pub type MediaIncomingId = [u8; 32];
/// Client-random operation correlation key, shared across runtime listeners.
/// Reusing a key with a different request is an operation conflict.
pub type MediaOperationId = [u8; 32];

/// Host-configured network namespace; never inferred from a display label or URL.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct MediaNetwork {
    /// Genesis hash identifying the authenticated signaling namespace.
    pub genesis_hash: [u8; 32],
}

/// Explicit algorithm and public key for a product-scoped remote account.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Enum)
)]
pub enum MediaAccount {
    /// Product-derived sr25519 account; not a legacy wallet or transport address.
    Sr25519([u8; 32]),
}

/// Authenticated destination. V1 permits only the caller's network and product.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct MediaPeer {
    /// Network namespace returned by discovery.
    pub network: MediaNetwork,
    /// Full canonical authenticated product identifier, including subdomains.
    pub product_id: String,
    /// Product account resolved and authenticated by the host.
    pub account: MediaAccount,
}

/// Finite runtime-lifetime retention and delivery budgets, reserved before effects.
/// Known cleanup never allocates quota; terminal and operation tombstones are not
/// silently evicted. Lower host budgets must still support the six-endpoint floor.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct MediaRuntimeLimits {
    /// Total session IDs that may be issued in this runtime (core default 256).
    pub max_issued_sessions: u32,
    /// Total participant IDs that may be issued (core default 2048).
    pub max_issued_participants: u32,
    /// Total incoming IDs that may be issued (core default 4096).
    pub max_issued_incoming: u32,
    /// Retained operation keys, including cancellation tombstones (default 8192).
    pub max_operations: u32,
    /// Simultaneously pending incoming offers (core default 32).
    pub max_pending_incoming: u16,
    /// Simultaneous runtime listeners (core default 8).
    pub max_subscriptions: u16,
    /// Queued events per listener (core default 128); overflow terminates it.
    pub event_queue_capacity: u16,
}

/// Complete service discovery, with no permission prompt or capture side effect.
/// Missing any mandatory capture, connectivity, picker, indicator, or unreadable
/// compositor facility makes the entire service unsupported, not partially ready.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct MediaCapabilities {
    /// Sorted unique supported contract versions, including 1 for this contract.
    pub contract_versions: Vec<u16>,
    /// Immutable host-configured network namespace for this runtime.
    pub network: MediaNetwork,
    /// Live remote endpoint limit, at least five; excludes the local endpoint.
    pub max_remote_participants: u16,
    /// Live session limit, at least one.
    pub max_sessions: u16,
    /// At least twice the endpoint limit including the local endpoint.
    pub max_surfaces_per_session: u16,
    /// Incoming offer lifetime, 60,000 milliseconds in V1.
    pub incoming_lifetime_ms: u32,
    /// Non-consent operation deadline, 5,000 milliseconds in V1.
    pub operation_timeout_ms: u32,
    /// Host consent/picker deadline, 60,000 milliseconds in V1.
    pub consent_timeout_ms: u32,
    /// Connection/reconnection deadline, 30,000 milliseconds in V1.
    pub reconnect_timeout_ms: u32,
    /// Actual finite budgets enforced by this runtime.
    pub limits: MediaRuntimeLimits,
}

/// Sanitized camera class, never a device identifier or model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Enum)
)]
pub enum MediaCameraKind {
    /// Front-facing camera.
    Front,
    /// Rear-facing camera.
    Rear,
    /// Any other camera class.
    Other,
}

/// Sanitized audio route class, never a device identifier or inventory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Enum)
)]
pub enum MediaAudioRoute {
    /// Handset earpiece.
    Earpiece,
    /// Loudspeaker.
    Speaker,
    /// Headset output.
    Headset,
    /// Any other route class.
    Other,
}

/// Complete replacement capture intent. All false means receive-only.
/// Preferences are advisory; screen capture always uses the trusted host picker.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct MediaLocalTracks {
    /// Request microphone capture.
    pub microphone: bool,
    /// Request camera capture.
    pub camera: bool,
    /// Request screen capture without selecting a source on the product side.
    pub screen: bool,
    /// Advisory camera class preference.
    pub camera_preference: Option<MediaCameraKind>,
    /// Advisory audio route preference.
    pub audio_preference: Option<MediaAudioRoute>,
}

/// Authoritative aggregate session state, independent of track interruption.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Enum)
)]
pub enum MediaSessionState {
    /// No nonterminal participants; the session remains explicitly owned.
    Ready,
    /// Invitations are outstanding.
    Negotiating,
    /// An accepted endpoint is connecting; takes precedence over negotiating.
    Connecting,
    /// All accepted nonterminal endpoints are connected and no invite remains.
    Connected,
    /// An established endpoint is recovering; takes precedence over connecting.
    Reconnecting,
    /// Terminal, with all capture, signaling, playback, and surfaces released.
    Ended,
}

/// Authoritative state of one remote endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Enum)
)]
pub enum MediaParticipantState {
    /// An outbound invitation is awaiting an answer.
    Inviting,
    /// Accepted and establishing media connectivity.
    Connecting,
    /// Media connectivity established.
    Connected,
    /// Recovering connectivity within the host deadline.
    Reconnecting,
    /// Terminal for this participant capability.
    Left,
}

/// Actual track state, without exposing underlying capture or stream objects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Enum)
)]
pub enum MediaTrackState {
    /// No sending or stale picture remains after an off intent commits.
    Off,
    /// Requested but not yet live.
    Starting,
    /// Host-observed live track.
    Live,
    /// Temporarily interrupted, distinct from permission revocation.
    Interrupted,
}

/// Host-observed local media and sanitized selected device classes.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct MediaLocalState {
    /// Actual microphone state.
    pub microphone: MediaTrackState,
    /// Actual camera state.
    pub camera: MediaTrackState,
    /// Actual screen-sharing state.
    pub screen: MediaTrackState,
    /// Actual camera class, if selected.
    pub camera_kind: Option<MediaCameraKind>,
    /// Actual audio route class, if selected.
    pub audio_route: Option<MediaAudioRoute>,
}

/// Remote track states, without quality metrics or device metadata.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct MediaRemoteState {
    /// Remote microphone state.
    pub microphone: MediaTrackState,
    /// Remote camera state.
    pub camera: MediaTrackState,
    /// Remote screen-sharing state.
    pub screen: MediaTrackState,
}

/// Bounded terminal business outcome; never a raw backend error string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Enum)
)]
pub enum MediaCallOutcome {
    /// Ended by the local product or trusted host controls.
    LocalEnded,
    /// Ended by the remote endpoint.
    RemoteEnded,
    /// Invitation refused.
    Refused,
    /// Remote endpoint busy.
    Busy,
    /// Invitation reached its answer deadline.
    Unanswered,
    /// Calling, microphone, or camera permission revoked.
    PermissionRevoked,
    /// Product/account/network authority lost.
    IdentityLost,
    /// Owning product runtime destroyed.
    RuntimeClosed,
    /// Connection or reconnection deadline reached.
    ConnectivityLost,
    /// Sanitized terminal host failure.
    HostFailed,
}

/// Current state of one remote endpoint; the local endpoint has no participant ID.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct MediaParticipantSnapshot {
    /// Runtime-scoped participant capability.
    pub participant_id: MediaParticipantId,
    /// Authenticated remote product account.
    pub peer: MediaPeer,
    /// Authoritative endpoint state.
    pub state: MediaParticipantState,
    /// Host-observed remote media state.
    pub media: MediaRemoteState,
    /// Terminal outcome, if any.
    pub outcome: Option<MediaCallOutcome>,
}

/// Authoritative session state. Remote departure does not implicitly end it.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct MediaSessionSnapshot {
    /// Runtime-scoped session capability.
    pub session_id: MediaSessionId,
    /// Strictly increasing per-session revision.
    pub revision: u64,
    /// Aggregate endpoint state.
    pub state: MediaSessionState,
    /// Last committed complete local intent.
    pub requested: MediaLocalTracks,
    /// Host-observed actual capture and routing state.
    pub actual: MediaLocalState,
    /// Remote endpoints, including their terminal outcomes.
    pub participants: Vec<MediaParticipantSnapshot>,
    /// Terminal session outcome, if any.
    pub outcome: Option<MediaCallOutcome>,
}

/// Authenticated, expiring offer; passive delivery does not create a session.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct MediaIncomingOffer {
    /// Single-use capability bound to sender, nonce, destination, and authority.
    pub incoming_id: MediaIncomingId,
    /// Authenticated remote product account.
    pub peer: MediaPeer,
    /// Declared caller intent: only Off or Starting, not proof of live capture.
    pub requested_remote_tracks: MediaRemoteState,
    /// Remaining host-monotonic lifetime measured at emission.
    pub remaining_ms: u32,
    /// Existing local session only for an authenticated offer from its group.
    pub existing_session: Option<MediaSessionId>,
}

/// Incoming decisions atomically claim an offer once acceptance begins.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Enum)
)]
pub enum MediaIncomingDecision {
    /// Cleanup requiring no permission, subscription, or new operation quota.
    Refuse,
    /// Atomically create a session and accept after required consent.
    AcceptNew {
        /// Runtime-wide mutation correlation key.
        operation_id: MediaOperationId,
        /// Complete initial capture intent.
        tracks: MediaLocalTracks,
    },
    /// Join only the offer's matching existing session; never merge calls.
    AcceptExisting {
        /// Runtime-wide mutation correlation key.
        operation_id: MediaOperationId,
        /// Must equal the offer's existing_session.
        session_id: MediaSessionId,
    },
}

/// Result of a successful incoming decision.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Enum)
)]
pub enum MediaIncomingResponse {
    /// Offer refused without capture or signaling a new call.
    Refused,
    /// Offer accepted, returning both authoritative owned resources.
    Accepted {
        /// Session joined or created.
        session: MediaSessionSnapshot,
        /// Offering endpoint's participant.
        participant: MediaParticipantSnapshot,
    },
}

/// Bounded reason an incoming offer no longer rings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Enum)
)]
pub enum MediaIncomingResolution {
    /// Accepted locally.
    Accepted,
    /// Refused locally.
    Refused,
    /// Authoritative lifetime expired.
    Expired,
    /// Cancelled before completion.
    Cancelled,
    /// Claimed by another authorized host endpoint.
    AnsweredElsewhere,
}

/// One authorized rendering attachment shared by every session in a runtime.
/// Revision never restarts after detach/replace. Product coordinates are logical
/// viewport units (CSS pixels on web), not document or physical screen coordinates.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct MediaViewport {
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

/// Host-owned picture kind, without a readable frame or stream handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Enum)
)]
pub enum MediaPictureKind {
    /// Camera picture.
    Camera,
    /// Screen-sharing picture.
    Screen,
}

/// Selects an unreadable host-composited local preview or remote picture.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Enum)
)]
pub enum MediaPictureSource {
    /// Self-preview uses the same isolated compositor as remote pictures.
    Local {
        /// Local camera or screen picture.
        picture: MediaPictureKind,
    },
    /// Picture belonging to an owned participant in this session.
    Remote {
        /// Session-scoped remote endpoint capability.
        participant_id: MediaParticipantId,
        /// Remote camera or screen picture.
        picture: MediaPictureKind,
    },
}

/// Axis-aligned logical viewport rectangle. Negative positions permit clipping;
/// arithmetic overflow is rejected, and zero size draws nothing.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct MediaRect {
    /// Left edge relative to viewport origin.
    pub x: i32,
    /// Top edge relative to viewport origin.
    pub y: i32,
    /// Logical width.
    pub width: u32,
    /// Logical height.
    pub height: u32,
}

/// Placement relative to the complete product plane, always below trusted UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Enum)
)]
pub enum MediaPlacement {
    /// Below all product content; the product must be transparent there.
    BelowProduct,
    /// Above all product content, not interleaved with arbitrary DOM z-indices.
    AboveProduct,
}

/// Picture scaling inside its clipped, rounded rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Enum)
)]
pub enum MediaFit {
    /// Preserve the full picture with letterboxing.
    Contain,
    /// Crop to fill the rectangle.
    Cover,
}

/// One unreadable sibling compositor layer. Layers do not receive product input
/// and are excluded from product renderer/screenshot APIs. Off or interrupted
/// sources never retain stale captured frames. Hiding pictures does not stop audio.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct MediaSurface {
    /// Product-chosen layout key, unique within the session's submitted set.
    pub surface_id: u32,
    /// Local or owned remote picture.
    pub source: MediaPictureSource,
    /// Target logical viewport rectangle.
    pub rect: MediaRect,
    /// Additional clipping rectangle, intersected with viewport and host region.
    pub clip: MediaRect,
    /// Clamped to half the smaller target dimension.
    pub corner_radius: u32,
    /// Placement plane relative to all product content.
    pub placement: MediaPlacement,
    /// Lower is behind higher; ties sort by session bytes then surface ID.
    pub depth: i32,
    /// Aspect-ratio treatment within the clip.
    pub fit: MediaFit,
    /// Presentation only; does not change what peers receive.
    pub mirrored: bool,
    /// False draws no picture, without changing capture or audio playback.
    pub visible: bool,
}

/// Runtime-wide events. Each listener starts with an atomic current snapshot;
/// subsequent sequence values strictly increase. Resubscription does not replay
/// old ringing events. Overflow terminates the stream with EventOverflow.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Enum)
)]
pub enum MediaEvent {
    /// Atomic initial state, including before any session exists.
    Snapshot {
        /// Runtime event sequence at this snapshot.
        sequence: u64,
        /// Current owned sessions.
        sessions: Vec<MediaSessionSnapshot>,
        /// Current unexpired incoming offers.
        incoming: Vec<MediaIncomingOffer>,
        /// Current authorized rendering attachment, if present.
        viewport: Option<MediaViewport>,
    },
    /// Authoritative session revision, including terminal teardown exactly once.
    SessionChanged {
        /// Runtime event sequence.
        sequence: u64,
        /// New authoritative session state.
        session: MediaSessionSnapshot,
    },
    /// Passive authenticated incoming offer; no consent or capture yet.
    IncomingOffered {
        /// Runtime event sequence.
        sequence: u64,
        /// New incoming offer.
        offer: MediaIncomingOffer,
    },
    /// An offer can no longer be accepted.
    IncomingResolved {
        /// Runtime event sequence.
        sequence: u64,
        /// Resolved incoming capability.
        incoming_id: MediaIncomingId,
        /// Bounded resolution reason.
        result: MediaIncomingResolution,
    },
    /// Transform changes hide stale pictures; detach/replace clears all layouts.
    ViewportChanged {
        /// Runtime event sequence.
        sequence: u64,
        /// Current attachment, or None after detachment.
        viewport: Option<MediaViewport>,
    },
}

/// Mutation category retained with a correlation key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Enum)
)]
pub enum MediaOperationKind {
    /// Session creation.
    CreateSession,
    /// Outbound invitation admission.
    AddParticipant,
    /// Incoming acceptance, new or existing session.
    AcceptIncoming,
    /// Complete local capture-intent replacement.
    SetLocalTracks,
}

/// Exact committed resources, without guessing from concurrent session snapshots.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Enum)
)]
pub enum MediaOperationResult {
    /// Created a session.
    Session {
        /// Created session capability.
        session_id: MediaSessionId,
    },
    /// Admitted an outbound participant.
    Participant {
        /// Owning session capability.
        session_id: MediaSessionId,
        /// Admitted or already nonterminal participant capability.
        participant_id: MediaParticipantId,
    },
    /// Accepted a specific incoming offer.
    Incoming {
        /// Claimed incoming capability.
        incoming_id: MediaIncomingId,
        /// Joined or created session capability.
        session_id: MediaSessionId,
        /// Accepted participant capability.
        participant_id: MediaParticipantId,
    },
    /// Committed local intent for a session.
    Tracks {
        /// Mutated session capability.
        session_id: MediaSessionId,
    },
}

/// Retained sanitized failure; diagnostics never enter the operation history.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Enum)
)]
pub enum MediaOperationFailure {
    /// User or authority denied the operation.
    Denied,
    /// Bounded Media domain failure.
    Domain {
        /// Sanitized domain error.
        error: HostMediaError,
    },
    /// Host failure without a diagnostic string.
    HostFailure,
}

/// Cancellation linearizes against commit; a committed result is never undone
/// or misreported as cancelled. Terminal records last for the owning runtime.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Enum)
)]
pub enum MediaOperationState {
    /// Admitted and not yet settled.
    Pending,
    /// Effects committed before any cancellation won.
    Committed {
        /// Exact committed resources.
        result: MediaOperationResult,
    },
    /// Settled without committing the mutation.
    Failed {
        /// Sanitized retained reason.
        failure: MediaOperationFailure,
    },
    /// Cancellation won; delayed original requests and callbacks cannot act.
    Cancelled,
}

/// Authoritative runtime-wide correlation record, shared across all listeners.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct MediaOperationSnapshot {
    /// Client-random key identifying the original request.
    pub operation_id: MediaOperationId,
    /// None only for cancellation-before-admission tombstones.
    pub kind: Option<MediaOperationKind>,
    /// Authoritative current or terminal outcome.
    pub state: MediaOperationState,
}

/// Exhausted finite resource class, without exposing another runtime's state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Enum)
)]
pub enum MediaResource {
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

/// Bounded public domain errors. Permission denial and unsupported service use
/// framework CallError variants. Never attach backend/device/transport strings.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Enum)
)]
pub enum HostMediaError {
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
    CapacityExceeded {
        /// Applicable remote endpoint limit.
        limit: u16,
    },
    /// Mutation is invalid or superseded by a newer admitted intent.
    InvalidState,
    /// Invalid surface set, including conflicting reuse of a layout revision.
    InvalidSurface,
    /// Submitted viewport transform is no longer current.
    StaleViewport {
        /// Authoritative viewport revision.
        current_revision: u64,
    },
    /// Submitted layout revision predates the committed or queued layout.
    StaleLayout {
        /// Authoritative layout revision.
        current_revision: u64,
    },
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
    ResourceExhausted {
        /// Exhausted budget class.
        resource: MediaResource,
    },
}

/// Create an owned session without connecting to any peer.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct HostMediaCreateSessionRequest {
    /// Idempotent runtime-wide mutation key.
    pub operation_id: MediaOperationId,
    /// Complete initial capture intent, subject to calling and device consent.
    pub tracks: MediaLocalTracks,
}

/// Successful session creation.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct HostMediaCreateSessionResponse {
    /// Authoritative ready session.
    pub session: MediaSessionSnapshot,
}

/// Admit an outgoing invitation, not a wait for the remote person's answer.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct HostMediaAddParticipantRequest {
    /// Idempotent runtime-wide mutation key.
    pub operation_id: MediaOperationId,
    /// Owned live session capability.
    pub session_id: MediaSessionId,
    /// Authenticated peer in the same product and network.
    pub peer: MediaPeer,
}

/// Admitted participant, or the current nonterminal participant for a duplicate peer.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct HostMediaAddParticipantResponse {
    /// Authoritative participant, initially inviting for a new outbound offer.
    pub participant: MediaParticipantSnapshot,
}

/// Decide a single-use offer; a peer address cannot substitute for its capability.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct HostMediaRespondIncomingRequest {
    /// Owned, authenticated incoming capability.
    pub incoming_id: MediaIncomingId,
    /// Refusal or operation-correlated acceptance.
    pub decision: MediaIncomingDecision,
}

/// Idempotently remove a known participant, even after subscription or grant loss.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct HostMediaRemoveParticipantRequest {
    /// Owning session capability.
    pub session_id: MediaSessionId,
    /// Previously owned participant capability, live or terminal.
    pub participant_id: MediaParticipantId,
}

/// Replace capture intent. Admission orders changes before asynchronous consent;
/// newer intent cancels older uncommitted work, whose late capture is released.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct HostMediaSetLocalTracksRequest {
    /// Idempotent runtime-wide mutation key.
    pub operation_id: MediaOperationId,
    /// Owned live session capability.
    pub session_id: MediaSessionId,
    /// Complete replacement intent; all-off remains allowed without a listener.
    pub tracks: MediaLocalTracks,
}

/// Successful capture intent update; actual tracks may still be starting.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct HostMediaSetLocalTracksResponse {
    /// Authoritative session with the newly committed intent.
    pub session: MediaSessionSnapshot,
}

/// Atomically replace the complete session layout at one compositor frame.
/// Validate every entry before queueing. A viewport change discards the entire
/// stale queued set, and teardown overrides all pending layouts.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct HostMediaSetSurfacesRequest {
    /// Owned live session capability.
    pub session_id: MediaSessionId,
    /// Current authorized attachment/transform revision.
    pub viewport_revision: u64,
    /// Product-monotonic session revision; identical current retries succeed.
    pub layout_revision: u64,
    /// Complete set with unique surface keys; an empty set clears pictures.
    pub surfaces: Vec<MediaSurface>,
}

/// Acknowledges an atomic queued commit, not immunity from viewport changes.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct HostMediaSetSurfacesResponse {
    /// Accepted product layout revision.
    pub layout_revision: u64,
}

/// Idempotent owned-session teardown, never gated on permission or subscriptions.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct HostMediaEndSessionRequest {
    /// Previously owned live or terminal session capability.
    pub session_id: MediaSessionId,
}

/// Recover the exact outcome of a potentially lost mutation response.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct HostMediaGetOperationRequest {
    /// Original mutation key; unknown keys return InvalidOperation.
    pub operation_id: MediaOperationId,
}

/// Cancel before commit, or recover the committed outcome if commit won.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct HostMediaCancelOperationRequest {
    /// Original key; unknown keys reserve a cancellation tombstone before reply.
    pub operation_id: MediaOperationId,
}
