//! Trusted host-only capture, connectivity, and compositor boundary.
//!
//! These commands and events are not product TrUAPI messages. The core owns
//! authenticated signaling, permissions, operation admission, and public state;
//! the backend owns OS capture, configured ICE/relay credentials, playback,
//! trusted indicators and pickers, and unreadable host-rendered surfaces.

use async_trait::async_trait;
use futures::stream::BoxStream;
use parity_scale_codec::{Decode, Encode};
use truapi::v01::{
    GenericError, MediaLocalState, MediaLocalTracks, MediaOperationFailure, MediaOperationId, MediaParticipantId,
    MediaRemoteState, MediaSessionId, MediaSurface, MediaViewport,
};

use crate::platform::ProductContext;

/// Complete backend availability and simultaneous live-resource limits.
///
/// `supported` is true only when every mandatory facility is implemented,
/// including microphone/camera capture, screen picking, RTC, audio playback and
/// routing, trusted indicators, and unreadable compositing. Runtime device
/// absence or user denial does not mean a facility is unimplemented. There are
/// deliberately no per-facility public support flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct MediaBackendCapabilities {
    /// Whether the backend implements the complete Media contract.
    pub supported: bool,
    /// Maximum simultaneous local sessions; at least one when supported.
    pub max_sessions: u16,
    /// Remote endpoints per session, excluding the local endpoint; at least five.
    pub max_remote_participants: u16,
    /// Pictures per session; at least twice the total endpoint capacity.
    pub max_surfaces_per_session: u16,
}

/// User decision presented by trusted host UI for one cancellable operation.
/// The core alone reads and persists grants; these prompts never delegate
/// browser capture permission or media objects into the product realm.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum MediaConsentRequest {
    /// Calling under the immutable active authority.
    Calling {
        /// Configured signaling network.
        network: [u8; 32],
        /// Authority-derived product Index(0) account.
        account: [u8; 32],
    },
    /// Host-owned microphone capture, not raw product capture.
    Microphone,
    /// Host-owned camera capture, not raw product capture.
    Camera,
}

/// Trusted operations scoped by the method's product and runtime parameters.
///
/// IDs are core-admitted handles, not globally addressable resources. The
/// backend must fence delayed callbacks and release capture obtained after an
/// operation was cancelled, superseded, or its session/runtime was closed.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum MediaBackendCommand {
    /// Prepare a local session and authorized capture with a trusted pending
    /// indicator. Returns `Done`; does not attach/send tracks, create peers,
    /// or publish committed state before `CommitOperation`.
    OpenSession {
        /// Core-admitted session owned by this product and runtime.
        session_id: MediaSessionId,
        /// Admitted operation whose cancellation fences preparation and capture.
        operation_id: MediaOperationId,
        /// Complete authorized local intent to prepare, not yet publish or send.
        tracks: MediaLocalTracks,
    },
    /// Prepare replacement capture without attaching/sending the new tracks
    /// or publishing committed state. Returns `Done`. A newer revision
    /// supersedes uncommitted work; failed or cancelled acquisition preserves
    /// the last committed intent until `CommitOperation`.
    SetTracks {
        /// Owned session whose last committed capture remains active until commit.
        session_id: MediaSessionId,
        /// Admitted operation owning this replacement preparation.
        operation_id: MediaOperationId,
        /// Session-monotonic intent revision; newer intent fences older pending work.
        intent_revision: u64,
        /// Complete replacement intent, including explicit off states.
        tracks: MediaLocalTracks,
    },
    /// Cancel pending picker/device work, drop prepared capture, and stop
    /// late-acquired resources. The core dispatches this only if cancellation
    /// won before its commit decision; it must not undo committed state.
    CancelOperation {
        /// Pending operation owned by this product runtime, never another runtime's key.
        operation_id: MediaOperationId,
    },
    /// Stop capture/sending, clear surfaces, and release session resources.
    /// Cleanup is idempotent and must not ask permission or allocate quota.
    CloseSession {
        /// Owned session to terminate, including its capture, peers, and pictures.
        session_id: MediaSessionId,
    },
    /// Create one participant's connection; local descriptions/candidates are
    /// returned through the trusted event stream, never through product frames.
    CreatePeer {
        /// Owned session whose authorized tracks may be sent to this participant.
        session_id: MediaSessionId,
        /// Core-admitted participant within this session, not a backend connection handle.
        participant_id: MediaParticipantId,
        /// Whether this endpoint starts negotiation by creating the local offer.
        offerer: bool,
    },
    /// Apply a description delivered by core-authenticated signaling.
    ApplyDescription {
        /// Owned session containing the authenticated remote endpoint.
        session_id: MediaSessionId,
        /// Participant whose connection receives the description.
        participant_id: MediaParticipantId,
        /// Private SDP and role; never forward into product-visible events or diagnostics.
        description: MediaDescription,
    },
    /// Apply a candidate delivered by core-authenticated signaling.
    AddIceCandidate {
        /// Owned session containing the authenticated remote endpoint.
        session_id: MediaSessionId,
        /// Participant whose connection receives the connectivity candidate.
        participant_id: MediaParticipantId,
        /// Private connectivity data; never expose it to the product or its logs.
        candidate: MediaIceCandidate,
    },
    /// Idempotently close and release one participant's connection.
    RemovePeer {
        /// Owned session that remains live after this participant is removed.
        session_id: MediaSessionId,
        /// Participant connection to release without affecting other endpoints.
        participant_id: MediaParticipantId,
    },
    /// Atomically replace this session's pictures for the current viewport.
    /// An empty list clears its surfaces; no product-readable frame is returned.
    SetSurfaces {
        /// Owned session whose entire picture set is replaced together.
        session_id: MediaSessionId,
        /// Current host attachment revision; layouts for an older viewport are stale.
        viewport_revision: u64,
        /// Session layout revision; identical retries are idempotent, conflicting reuse is not.
        layout_revision: u64,
        /// Complete validated picture set, composed unreadably beneath trusted host UI.
        surfaces: Vec<MediaSurface>,
    },
    /// Idempotently release every resource owned by this product runtime and
    /// fence all late work so it cannot recreate capture, peers, or surfaces.
    CloseRuntime,
    /// Apply prepared tracks to all peer senders atomically, stop replaced/off
    /// tracks, and return `LocalState`. The core records its commit decision
    /// before dispatch; later cancellation waits for the committed/failed
    /// outcome and must never report the operation as cancelled.
    CommitOperation {
        /// Prepared operation whose commit decision the core already recorded.
        operation_id: MediaOperationId,
    },
    /// Present an operation-scoped trusted consent prompt only when the core
    /// has no stored decision. CancelOperation/CloseRuntime must dismiss it;
    /// a late answer must never prepare capture or write a permission grant.
    RequestConsent {
        /// Operation whose cancellation fences this prompt.
        operation_id: MediaOperationId,
        /// Immutable question; no product-authored prompt text.
        request: MediaConsentRequest,
    },
}

/// Bounded result of a trusted backend command, without diagnostic strings.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum MediaBackendResponse {
    /// Command completed without publishing new committed capture state.
    Done,
    /// Accepted capture intent's currently observed device state.
    LocalState {
        /// Observed committed capture state, without device identifiers or media handles.
        state: MediaLocalState,
    },
    /// Expected refusal or domain failure, such as picker cancellation or a
    /// missing device. Unexpected callback/IPC errors remain `GenericError`
    /// and are sanitized to `HostFailure` by the core.
    Rejected {
        /// Safe failure suitable for retained operation outcomes.
        failure: MediaOperationFailure,
    },
    /// A genuine user decision. Dismissal/cancellation is a rejected response,
    /// never a remembered denial; only the core persists this answer.
    Consent {
        /// Whether the user approved the immutable consent question.
        granted: bool,
    },
}

/// Host-only session description; Debug never includes SDP.
#[derive(derive_more::Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct MediaDescription {
    /// Negotiation role of this description.
    pub kind: MediaDescriptionKind,
    /// Sensitive transport description for trusted host signaling only.
    #[debug("<redacted>")]
    pub sdp: String,
}

/// Negotiation role of a host-only description.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum MediaDescriptionKind {
    /// Description initiating negotiation for the participant's connection.
    Offer,
    /// Description responding to that connection's authenticated remote offer.
    Answer,
}

/// Host-only ICE candidate; Debug never includes candidate or media-ID strings.
#[derive(derive_more::Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct MediaIceCandidate {
    /// Sensitive connectivity data for trusted host signaling only.
    #[debug("<redacted>")]
    pub candidate: String,
    /// Optional media section identifier.
    #[debug("<redacted>")]
    pub mid: Option<String>,
    /// Optional media section index.
    pub mline_index: Option<u16>,
}

/// Backend-observed connection state for one remote endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum MediaBackendPeerState {
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

/// Trusted events scoped to the product/runtime that opened the stream.
///
/// The core converts observations into public lifecycle events. Descriptions
/// and candidates go exclusively to core-authenticated signaling; they must
/// never become product events, error diagnostics, or product debug logs.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum MediaBackendEvent {
    /// Current host rendering attachment; `None` also clears its surfaces.
    ViewportChanged {
        /// Current host attachment and transform revision, or no authorized rendering surface.
        viewport: Option<MediaViewport>,
    },
    /// Observed capture state, fenced by the accepted track intent revision.
    LocalStateChanged {
        /// Owned session whose committed capture was observed.
        session_id: MediaSessionId,
        /// Accepted intent that produced the observation; stale revisions must not revive tracks.
        intent_revision: u64,
        /// Actual track state, without device identifiers or readable capture handles.
        state: MediaLocalState,
    },
    /// Observed connectivity, not inferred media quality.
    PeerStateChanged {
        /// Owned session containing the observed participant.
        session_id: MediaSessionId,
        /// Core-admitted participant whose connection changed state.
        participant_id: MediaParticipantId,
        /// Observed connectivity lifecycle, independent of remote track availability.
        state: MediaBackendPeerState,
    },
    /// Observed remote track availability.
    RemoteStateChanged {
        /// Owned session containing the observed participant.
        session_id: MediaSessionId,
        /// Participant whose incoming tracks were observed.
        participant_id: MediaParticipantId,
        /// Remote track availability only, never decoded frames or transport diagnostics.
        state: MediaRemoteState,
    },
    /// Locally generated description for authenticated signaling.
    Description {
        /// Owned session whose peer connection produced the description.
        session_id: MediaSessionId,
        /// Authenticated signaling recipient identified by its core-admitted participant.
        participant_id: MediaParticipantId,
        /// Private local SDP and role, sent only through core-authenticated signaling.
        description: MediaDescription,
    },
    /// Locally generated candidate for authenticated signaling.
    IceCandidate {
        /// Owned session whose peer connection produced the candidate.
        session_id: MediaSessionId,
        /// Authenticated signaling recipient identified by its core-admitted participant.
        participant_id: MediaParticipantId,
        /// Private local connectivity data, never a product event or diagnostic payload.
        candidate: MediaIceCandidate,
    },
    /// Trusted screen-stop control invalidated prior pending screen intent.
    ScreenStopped {
        /// Owned session whose screen track stopped; other tracks and the session remain live.
        session_id: MediaSessionId,
    },
    /// Trusted host hangup control ended the session.
    HostEnded {
        /// Owned session selected by trusted host controls, not by a product-supplied selector.
        session_id: MediaSessionId,
    },
    /// End affected sessions without silently downgrading capture. Only an
    /// explicit product-setting withdrawal changes the persisted core grant.
    PermissionRevoked {
        /// Authorization withdrawn within this event stream's product/runtime scope.
        permission: MediaRevokedPermission,
        /// Whether product consent or the independent OS gate was withdrawn.
        source: MediaRevocationSource,
    },
}

/// Revocation observations; screen stopping is not a permission revocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum MediaRevokedPermission {
    /// Scoped calling authority; withdrawal ends affected sessions, including receive-only ones.
    Calling,
    /// Microphone capture authorization; withdrawal is not a request to mute silently.
    Microphone,
    /// Camera capture authorization; withdrawal is not a request to disable video silently.
    Camera,
}

/// OS gating must not overwrite a separately remembered product decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum MediaRevocationSource {
    /// Explicit withdrawal in trusted product-permission settings.
    Product,
    /// Withdrawal of the host application's OS permission.
    OperatingSystem,
}

/// Optional complete Media backend, reachable only by the trusted runtime.
///
/// The runtime supplies the authenticated product and its own runtime identity;
/// commands cannot choose a different owner. Implementations must isolate all
/// resources and event streams by both values. The core owns all public
/// admission, resource budgets, permission, operation, and signaling authority.
///
/// A host without every mandatory facility omits this entire capability or
/// reports `supported: false`; the core answers public calls `Unsupported`.
/// See [`crate::platform::OptionalPlatform`]. Generic backend failures must be sanitized
/// and must never contain SDP, candidates, relay credentials, or device IDs.
#[async_trait]
pub trait MediaPlatform: Send + Sync {
    /// Probe complete backend availability without prompting or capturing.
    async fn media_backend_capabilities(
        &self,
        product: &ProductContext,
    ) -> Result<MediaBackendCapabilities, GenericError>;

    /// Observe only this runtime's backend state. Emit its current viewport
    /// first. Dropping an event listener does not implicitly close sessions;
    /// runtime destruction explicitly issues `CloseRuntime`.
    fn media_backend_events(
        &self,
        product: &ProductContext,
        runtime_id: u64,
    ) -> BoxStream<'static, Result<MediaBackendEvent, GenericError>>;

    /// Execute one admitted command. Cancellation and teardown must remain
    /// actionable while a picker or other capture operation is pending.
    async fn media_backend_command(
        &self,
        product: &ProductContext,
        runtime_id: u64,
        command: MediaBackendCommand,
    ) -> Result<MediaBackendResponse, GenericError>;
}
