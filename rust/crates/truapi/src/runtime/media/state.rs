//! Runtime-owned capabilities, bounded listeners, and retained operation decisions.

use std::{collections::{BTreeMap, VecDeque}, pin::Pin, sync::{Arc, Mutex, Weak}, task::{Context, Poll, Waker}, time::Duration};
use futures::{channel::oneshot, future::{BoxFuture, Shared}, FutureExt, Stream};
use parity_scale_codec::Encode;
use rand_core::{OsRng, RngCore};
use truapi::{v01::*, CallError, CancellationToken};
use crate::platform::{MediaDescription, MediaIceCandidate};
use super::{group::GroupEngine, super::{authority::AuthoritySession, media_signaling::MediaSignaling}, Instant};
use crate::host_logic::media_protocol::MediaIdentity;

pub(super) type Error = CallError<HostMediaError>;
pub(super) type Result<T, E = Error> = std::result::Result<T, E>;
pub(super) const SESSION_BUDGET: usize = 256;
pub(super) const PARTICIPANT_BUDGET: usize = 2048;
pub(super) const INCOMING_BUDGET: usize = 4096;
pub(super) const OPERATION_BUDGET: usize = 8192;
pub(super) const INCOMING_LIMIT: usize = 32;
pub(super) const LISTENER_LIMIT: usize = 8;
pub(super) const EVENT_LIMIT: usize = 128;
pub(super) const OPERATION_TIMEOUT: Duration = Duration::from_secs(5);
pub(super) const CONSENT_TIMEOUT: Duration = Duration::from_secs(60);
pub(super) const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

pub(super) fn domain(error: HostMediaError) -> Error { CallError::Domain(error) }
pub(super) fn host_failure() -> Error { CallError::HostFailure { reason: "media host failure".into() } }
pub(super) fn exhausted(resource: MediaResource) -> Error { domain(HostMediaError::ResourceExhausted { resource }) }
pub(super) fn failure(error: &Error) -> MediaOperationFailure {
    match error {
        CallError::Denied => MediaOperationFailure::Denied,
        CallError::Domain(error) => MediaOperationFailure::Domain { error: error.clone() },
        _ => MediaOperationFailure::HostFailure,
    }
}
pub(super) fn from_failure(failure: MediaOperationFailure) -> Error {
    match failure {
        MediaOperationFailure::Denied => CallError::Denied,
        MediaOperationFailure::Domain { error } => domain(error),
        MediaOperationFailure::HostFailure => host_failure(),
    }
}
pub(super) fn off_tracks() -> MediaLocalTracks {
    MediaLocalTracks { microphone: false, camera: false, screen: false, camera_preference: None, audio_preference: None }
}
pub(super) fn all_off(tracks: &MediaLocalTracks) -> bool { !tracks.microphone && !tracks.camera && !tracks.screen }
pub(super) fn off_local() -> MediaLocalState {
    MediaLocalState { microphone: MediaTrackState::Off, camera: MediaTrackState::Off, screen: MediaTrackState::Off, camera_kind: None, audio_route: None }
}
pub(super) fn off_remote() -> MediaRemoteState {
    MediaRemoteState { microphone: MediaTrackState::Off, camera: MediaTrackState::Off, screen: MediaTrackState::Off }
}
pub(super) fn starting(tracks: &MediaLocalTracks) -> MediaLocalState {
    let state = |enabled| if enabled { MediaTrackState::Starting } else { MediaTrackState::Off };
    MediaLocalState { microphone: state(tracks.microphone), camera: state(tracks.camera), screen: state(tracks.screen), camera_kind: None, audio_route: None }
}
pub(super) fn same_authority(a: &AuthoritySession, b: &AuthoritySession) -> bool {
    a.public_key == b.public_key && a.validation_id == b.validation_id
}

#[derive(Clone, Encode)]
pub(super) enum Mutation {
    Create(HostMediaCreateSessionRequest),
    Add(HostMediaAddParticipantRequest),
    Accept { incoming_id: MediaIncomingId, operation_id: MediaOperationId, session_id: Option<MediaSessionId>, tracks: Option<MediaLocalTracks> },
    Tracks(HostMediaSetLocalTracksRequest),
}
impl Mutation {
    pub fn id(&self) -> MediaOperationId {
        match self {
            Self::Create(request) => request.operation_id,
            Self::Add(request) => request.operation_id,
            Self::Accept { operation_id, .. } => *operation_id,
            Self::Tracks(request) => request.operation_id,
        }
    }
    pub fn kind(&self) -> MediaOperationKind {
        match self {
            Self::Create(_) => MediaOperationKind::CreateSession,
            Self::Add(_) => MediaOperationKind::AddParticipant,
            Self::Accept { .. } => MediaOperationKind::AcceptIncoming,
            Self::Tracks(_) => MediaOperationKind::SetLocalTracks,
        }
    }
    pub fn fingerprint(&self) -> [u8; 32] {
        let mut hash = blake2b_simd::Params::new().hash_length(32).to_state();
        hash.update(b"truapi-media-operation-v1");
        struct HashOutput<'a>(&'a mut blake2b_simd::State);
        impl parity_scale_codec::Output for HashOutput<'_> {
            fn write(&mut self, bytes: &[u8]) { self.0.update(bytes); }
        }
        self.encode_to(&mut HashOutput(&mut hash));
        let mut result = [0; 32];
        result.copy_from_slice(hash.finalize().as_bytes());
        result
    }
}

#[derive(Clone)]
pub(super) enum MutationReply {
    Session(MediaSessionSnapshot),
    Participant(MediaParticipantSnapshot),
    Incoming(MediaIncomingResponse),
}
pub(super) type Completion = Shared<BoxFuture<'static, Result<MutationReply>>>;

pub(super) struct Operation {
    pub snapshot: MediaOperationSnapshot,
    pub fingerprint: Option<[u8; 32]>,
    pub completion: Option<Completion>,
    pub sender: Option<oneshot::Sender<Result<MutationReply>>>,
    pub cancel: CancellationToken,
    pub started: Instant,
    pub deadline: Instant,
    pub epoch: u64,
    pub session: Option<MediaSessionId>,
    pub participant: Option<MediaParticipantId>,
    pub incoming: Option<MediaIncomingId>,
    pub new_session: bool,
    pub intent_revision: u64,
    pub requested: Option<MediaLocalTracks>,
    pub owns_participant: bool,
    /// A logical decision cannot be changed into failure/cancellation by a lost
    /// backend reply. Its exact resource result survives subsequent teardown.
    pub committing: Option<MediaOperationResult>,
}
impl Operation {
    pub fn new(request: &Mutation, epoch: u64) -> Self {
        let (sender, receiver) = oneshot::channel();
        let completion = async move { receiver.await.unwrap_or_else(|_| Err(host_failure())) }.boxed().shared();
        let started = Instant::now();
        Self {
            snapshot: MediaOperationSnapshot { operation_id: request.id(), kind: Some(request.kind()), state: MediaOperationState::Pending },
            fingerprint: Some(request.fingerprint()), completion: Some(completion), sender: Some(sender), cancel: CancellationToken::default(),
            started, deadline: started + OPERATION_TIMEOUT, epoch,
            session: None, participant: None, incoming: None, new_session: false, intent_revision: 0, committing: None,
            requested: None, owns_participant: false,
        }
    }
    pub fn tombstone(id: MediaOperationId, epoch: u64) -> Self {
        let now = Instant::now();
        let cancel = CancellationToken::default();
        cancel.cancel();
        Self {
            snapshot: MediaOperationSnapshot { operation_id: id, kind: None, state: MediaOperationState::Cancelled },
            fingerprint: None, completion: None, sender: None, cancel, started: now, deadline: now, epoch,
            session: None, participant: None, incoming: None, new_session: false, intent_revision: 0, committing: None,
            requested: None, owns_participant: false,
        }
    }
    pub fn pending(&self) -> bool { matches!(self.snapshot.state, MediaOperationState::Pending) }
    pub fn abort(&mut self, error: Error) -> bool {
        if !self.pending() || self.committing.is_some() { return false; }
        self.cancel.cancel();
        self.snapshot.state = if error == domain(HostMediaError::OperationCancelled) {
            MediaOperationState::Cancelled
        } else { MediaOperationState::Failed { failure: failure(&error) } };
        if let Some(sender) = self.sender.take() { let _ = sender.send(Err(error)); }
        true
    }
    pub fn finish(&mut self, reply: Result<MutationReply>) {
        if !self.pending() { return; }
        if let Some(result) = self.committing.take() {
            self.snapshot.state = MediaOperationState::Committed { result };
        } else if let Err(error) = &reply {
            self.snapshot.state = MediaOperationState::Failed { failure: failure(error) };
        } else {
            // Successful mutations always establish their logical decision first.
            self.snapshot.state = MediaOperationState::Failed { failure: MediaOperationFailure::HostFailure };
            if let Some(sender) = self.sender.take() { let _ = sender.send(Err(host_failure())); }
            return;
        }
        if let Some(sender) = self.sender.take() { let _ = sender.send(reply); }
    }
}

pub(super) enum PeerInput { Description(MediaDescription), Candidate(MediaIceCandidate) }
pub(super) struct Participant {
    pub snapshot: MediaParticipantSnapshot,
    pub visible: bool,
    pub backend_started: bool,
    pub backend_ready: bool,
    pub pending_input: VecDeque<PeerInput>,
    pub draining: bool,
    pub pending_input_bytes: usize,
    pub remote_description_ready: bool,
    pub deadline: Option<Instant>,
}
impl Participant {
    pub fn new(id: MediaParticipantId, peer: MediaPeer) -> Self {
        Self { snapshot: MediaParticipantSnapshot { participant_id: id, peer, state: MediaParticipantState::Inviting, media: off_remote(), outcome: None },
            visible: false, backend_started: false, backend_ready: false, pending_input: VecDeque::new(), deadline: None,
            draining: false, pending_input_bytes: 0, remote_description_ready: false }
    }
    pub fn live(&self) -> bool { self.snapshot.state != MediaParticipantState::Left }
    pub fn end(&mut self, outcome: MediaCallOutcome) {
        self.snapshot.state = MediaParticipantState::Left;
        self.snapshot.media = off_remote();
        self.snapshot.outcome = Some(outcome);
        self.pending_input.clear();
        self.pending_input_bytes = 0;
        self.draining = false;
        self.remote_description_ready = false;
        self.deadline = None;
        self.backend_ready = false;
    }
}

pub(super) struct Session {
    /// Participants are materialized only at the public snapshot boundary.
    pub snapshot: MediaSessionSnapshot,
    pub participants: BTreeMap<MediaParticipantId, Participant>,
    pub visible: bool,
    pub epoch: u64,
    pub terminal_published: bool,
    pub intent_revision: u64,
    pub admitted_intent: u64,
    pub track_operation: Option<MediaOperationId>,
    /// Logical publication can precede the backend's session installation.
    pub backend_opening: Option<Completion>,
    pub layout: Option<HostMediaSetSurfacesRequest>,
    pub layout_completion: Option<Shared<BoxFuture<'static, Result<HostMediaSetSurfacesResponse>>>>,
}
impl Session {
    pub fn new(id: MediaSessionId, epoch: u64) -> Self {
        Self { snapshot: MediaSessionSnapshot { session_id: id, revision: 0, state: MediaSessionState::Ready,
            requested: off_tracks(), actual: off_local(), participants: Vec::new(), outcome: None },
            participants: BTreeMap::new(), visible: false, epoch, terminal_published: false,
            intent_revision: 0, admitted_intent: 0, track_operation: None, backend_opening: None, layout: None,
            layout_completion: None }
    }
    pub fn live(&self) -> bool { self.snapshot.state != MediaSessionState::Ended }
    pub fn current(&self) -> MediaSessionSnapshot {
        let mut snapshot = self.snapshot.clone();
        snapshot.participants = self.participants.values().filter(|p| p.visible).map(|p| p.snapshot.clone()).collect();
        snapshot
    }
    pub fn aggregate(&self) -> MediaSessionState {
        if !self.live() { return MediaSessionState::Ended; }
        let mut result = MediaSessionState::Ready;
        let rank = |state| match state {
            MediaSessionState::Ended => 0, MediaSessionState::Reconnecting => 1, MediaSessionState::Connecting => 2,
            MediaSessionState::Negotiating => 3, MediaSessionState::Connected => 4, MediaSessionState::Ready => 5,
        };
        for participant in self.participants.values().filter(|p| p.visible) {
            let state = match participant.snapshot.state {
                MediaParticipantState::Inviting => MediaSessionState::Negotiating,
                MediaParticipantState::Connecting => MediaSessionState::Connecting,
                MediaParticipantState::Connected => MediaSessionState::Connected,
                MediaParticipantState::Reconnecting => MediaSessionState::Reconnecting,
                MediaParticipantState::Left => continue,
            };
            if rank(state) < rank(result) { result = state; }
        }
        result
    }
    pub fn end(&mut self, outcome: MediaCallOutcome) {
        if !self.live() { return; }
        self.snapshot.state = MediaSessionState::Ended;
        self.snapshot.outcome = Some(outcome);
        self.snapshot.requested = off_tracks();
        self.snapshot.actual = off_local();
        self.track_operation = None;
        for participant in self.participants.values_mut().filter(|p| p.live()) { participant.end(outcome); }
    }
}

#[derive(Clone, Copy)]
pub(super) enum IncomingPhase { Open, Claimed(MediaOperationId), Awaiting, Resolved(MediaIncomingResolution) }
pub(super) struct Incoming {
    pub offer: MediaIncomingOffer,
    pub deadline: Instant,
    pub epoch: u64,
    pub phase: IncomingPhase,
}
impl Incoming {
    pub fn current(&self, now: Instant) -> Option<MediaIncomingOffer> {
        if !matches!(self.phase, IncomingPhase::Open) || self.deadline <= now { return None; }
        let mut offer = self.offer.clone();
        offer.remaining_ms = self.deadline.saturating_duration_since(now).as_millis().min(60_000) as u32;
        Some(offer)
    }
}

pub(super) struct Binding { pub session: AuthoritySession, pub identity: MediaIdentity }
pub(super) struct State {
    pub capabilities: Option<MediaCapabilities>,
    pub backend_started: bool,
    pub fatal: bool,
    pub closed: bool,
    pub epoch: u64,
    pub permission_writes: usize,
    pub binding: Option<Binding>,
    pub signaling: Option<Arc<MediaSignaling>>,
    /// The background connector owns every signaling start for this runtime.
    pub signaling_started: bool,
    /// Present while the connector waits between attempts; firing it retries now.
    pub signaling_wake: Option<oneshot::Sender<()>>,
    /// Operations waiting, within their own deadline, for signaling to bind.
    pub ready_waiters: Vec<oneshot::Sender<()>>,
    pub group: Option<GroupEngine>,
    pub pending_signals: VecDeque<super::backend::PendingSignal>,
    pub pending_signal_bytes: usize,
    pub signal_workers: usize,
    pub sessions: BTreeMap<MediaSessionId, Session>,
    pub issued_participants: usize,
    pub incoming: BTreeMap<MediaIncomingId, Incoming>,
    pub operations: BTreeMap<MediaOperationId, Operation>,
    pub listeners: Vec<Weak<Listener>>,
    pub sequence: u64,
    pub viewport: Option<MediaViewport>,
    pub viewport_revision: u64,
}
impl State {
    pub fn new() -> Self {
        Self { capabilities: None, backend_started: false, fatal: false, closed: false, epoch: 0,
            permission_writes: 0, pending_signals: VecDeque::new(), pending_signal_bytes: 0, signal_workers: 0,
            binding: None, signaling: None, signaling_started: false, signaling_wake: None, ready_waiters: Vec::new(),
            group: None, sessions: BTreeMap::new(), issued_participants: 0,
            incoming: BTreeMap::new(), operations: BTreeMap::new(), listeners: Vec::new(), sequence: 0, viewport: None, viewport_revision: 0 }
    }
    pub fn ensure_open(&self) -> Result<()> {
        if self.closed || self.fatal { Err(host_failure()) } else { Ok(()) }
    }
    pub fn session(&self, id: &MediaSessionId) -> Result<&Session> {
        self.sessions.get(id).filter(|session| session.visible).ok_or_else(|| domain(HostMediaError::InvalidHandle))
    }
    pub fn live_session(&self, id: &MediaSessionId) -> Result<&Session> {
        let session = self.session(id)?;
        if session.epoch != self.epoch || !session.live() { Err(domain(HostMediaError::SessionEnded)) } else { Ok(session) }
    }
    pub fn prune_listeners(&mut self) {
        self.listeners.retain(|weak| weak.upgrade().is_some_and(|listener| listener.eligible()));
    }
    pub fn require_listener(&mut self) -> Result<()> {
        self.prune_listeners();
        if self.listeners.is_empty() { Err(domain(HostMediaError::SubscriptionRequired)) } else { Ok(()) }
    }
    pub fn subscribe(&mut self) -> Result<ListenerStream> {
        self.ensure_open()?;
        self.prune_listeners();
        if self.listeners.len() >= LISTENER_LIMIT { return Err(exhausted(MediaResource::Subscriptions)); }
        let now = Instant::now();
        let snapshot = MediaEvent::Snapshot { sequence: self.sequence,
            sessions: self.sessions.values().filter(|session| session.visible && session.epoch == self.epoch).map(Session::current).collect(),
            incoming: self.incoming.values().filter(|incoming| incoming.epoch == self.epoch).filter_map(|incoming| incoming.current(now)).collect(),
            viewport: self.viewport.clone() };
        let listener = Arc::new(Listener { state: Mutex::new(ListenerState { queue: VecDeque::from([snapshot]), error: None, closed: false, waker: None }) });
        self.listeners.push(Arc::downgrade(&listener));
        Ok(ListenerStream(listener))
    }
    pub fn emit(&mut self, make: impl FnOnce(u64) -> MediaEvent) {
        let Some(sequence) = self.sequence.checked_add(1) else {
            self.fatal = true;
            self.close_listeners(host_failure());
            return;
        };
        self.sequence = sequence;
        let event = make(sequence);
        self.listeners.retain(|weak| match weak.upgrade() {
            Some(listener) => listener.push(event.clone()), None => false,
        });
    }
    pub fn changed(&mut self, id: MediaSessionId) {
        let Some(session) = self.sessions.get_mut(&id) else { return; };
        if !session.visible || session.terminal_published { return; }
        let Some(revision) = session.snapshot.revision.checked_add(1) else {
            self.fatal = true;
            self.close_listeners(host_failure());
            return;
        };
        session.snapshot.revision = revision;
        session.snapshot.state = session.aggregate();
        session.terminal_published = !session.live();
        let snapshot = session.current();
        self.emit(|sequence| MediaEvent::SessionChanged { sequence, session: snapshot });
    }
    pub fn close_listeners(&mut self, error: Error) {
        for listener in self.listeners.drain(..).filter_map(|weak| weak.upgrade()) { listener.close(error.clone()); }
    }
    pub fn finish_listeners(&mut self) {
        for listener in self.listeners.drain(..).filter_map(|weak| weak.upgrade()) { listener.finish(); }
    }
    pub fn resolve_incoming(&mut self, id: MediaIncomingId, resolution: MediaIncomingResolution) {
        let Some(incoming) = self.incoming.get_mut(&id) else { return; };
        if matches!(incoming.phase, IncomingPhase::Resolved(_)) { return; }
        let claimed = match incoming.phase { IncomingPhase::Claimed(operation) => Some(operation), _ => None };
        incoming.phase = IncomingPhase::Resolved(resolution);
        if let Some(operation) = claimed.and_then(|id| self.operations.get_mut(&id)) {
            operation.abort(domain(HostMediaError::IncomingExpired));
        }
        self.emit(|sequence| MediaEvent::IncomingResolved { sequence, incoming_id: id, result: resolution });
    }
    pub fn new_session_id(&self) -> Result<MediaSessionId> {
        if self.sessions.len() >= SESSION_BUDGET { return Err(exhausted(MediaResource::Sessions)); }
        mint(|id| self.sessions.contains_key(id))
    }
    pub fn new_participant_id(&self) -> Result<MediaParticipantId> {
        if self.issued_participants >= PARTICIPANT_BUDGET { return Err(exhausted(MediaResource::Participants)); }
        mint(|id| self.sessions.values().any(|session| session.participants.contains_key(id)))
    }
}
fn mint(used: impl Fn(&[u8; 32]) -> bool) -> Result<[u8; 32]> {
    for _ in 0..8 {
        let mut id = [0; 32];
        OsRng.try_fill_bytes(&mut id).map_err(|_| host_failure())?;
        if !used(&id) { return Ok(id); }
    }
    Err(host_failure())
}

pub(super) struct Listener { state: Mutex<ListenerState> }
struct ListenerState { queue: VecDeque<MediaEvent>, error: Option<Error>, closed: bool, waker: Option<Waker> }
impl Listener {
    fn eligible(&self) -> bool { !self.state.lock().unwrap_or_else(|e| e.into_inner()).closed }
    fn push(&self, event: MediaEvent) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.closed { return false; }
        if state.queue.len() >= EVENT_LIMIT {
            state.queue.clear();
            state.error = Some(domain(HostMediaError::EventOverflow));
            state.closed = true;
        } else { state.queue.push_back(event); }
        if let Some(waker) = state.waker.take() { waker.wake(); }
        !state.closed
    }
    fn close(&self, error: Error) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.closed { return; }
        state.queue.clear(); state.closed = true; state.error = Some(error);
        if let Some(waker) = state.waker.take() { waker.wake(); }
    }
    fn finish(&self) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.closed = true;
        if let Some(waker) = state.waker.take() { waker.wake(); }
    }
}
pub(super) struct ListenerStream(Arc<Listener>);
impl Stream for ListenerStream {
    type Item = Result<MediaEvent>;
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut state = self.0.state.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(error) = state.error.take() { return Poll::Ready(Some(Err(error))); }
        if let Some(event) = state.queue.pop_front() { return Poll::Ready(Some(Ok(event))); }
        if state.closed { return Poll::Ready(None); }
        if !state.waker.as_ref().is_some_and(|waker| waker.will_wake(cx.waker())) { state.waker = Some(cx.waker().clone()); }
        Poll::Pending
    }
}
