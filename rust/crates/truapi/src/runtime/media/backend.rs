//! Ordered state transitions and bounded host-private media IO.

use std::{collections::VecDeque, sync::Arc, time::Duration};

use futures::{FutureExt, pin_mut, select_biased};
use truapi::{CallContext, CallError, CancellationToken, v01::*};
use crate::platform::{
    MediaBackendCommand, MediaBackendEvent, MediaBackendPeerState, MediaBackendResponse,
    MediaRevocationSource, MediaRevokedPermission, PermissionAuthorizationRequest,
    PermissionAuthorizationStatus,
};
use zeroize::{Zeroize, Zeroizing};

use super::{
    Instant, JobGuard, MediaService,
    group::{GroupAction, GroupDecline},
    operations::{abort_operation_locked, operation_reply},
    state::*,
};
use crate::{host_logic::media_protocol::VerifiedAdvertisement,
    runtime::media_signaling::{MediaSignaling, MediaSignalingEvent}};

const MAX_PEER_MESSAGES: usize = 66;
const MAX_PEER_BYTES: usize = 256 * 1024;
const MAX_MESSAGES: usize = 256;
const MAX_BYTES: usize = 1024 * 1024;
const MAX_DESCRIPTION: usize = 64 * 1024;
const MAX_CANDIDATE: usize = 4096;
const MAX_MID: usize = 256;
const MAX_ACTIONS: usize = 32_768;
/// Total attempts for one handshake message before its link is failed.
const SIGNAL_ATTEMPTS: u8 = 4;
const SIGNAL_RETRY_DELAY: Duration = Duration::from_secs(1);

pub(super) enum Effect {
    Command(MediaBackendCommand),
    StartPeer { session_id: MediaSessionId, participant_id: MediaParticipantId, offerer: bool, epoch: u64 },
    DrainPeer { session_id: MediaSessionId, participant_id: MediaParticipantId, epoch: u64 },
    DrainSignals,
}

pub(super) struct PendingSignal {
    signaling: Arc<MediaSignaling>,
    epoch: u64,
    deadline: Instant,
    recipient: VerifiedAdvertisement,
    payload: Zeroizing<Vec<u8>>,
    attempts: u8,
    not_before: Instant,
}

fn terminal_error(outcome: MediaCallOutcome) -> Error {
    match outcome {
        MediaCallOutcome::PermissionRevoked => CallError::Denied,
        MediaCallOutcome::IdentityLost => domain(HostMediaError::NotConnected),
        MediaCallOutcome::HostFailed | MediaCallOutcome::RuntimeClosed => host_failure(),
        _ => domain(HostMediaError::SessionEnded),
    }
}

fn finish_committing(state: &mut State, id: MediaOperationId) {
    let Some(result) = state.operations.get(&id).and_then(|operation| operation.committing.clone()) else { return; };
    let reply = operation_reply(state, &result);
    if let Some(operation) = state.operations.get_mut(&id) { operation.finish(reply); }
}

pub(super) fn end_session_locked(state: &mut State, id: MediaSessionId, outcome: MediaCallOutcome) -> Vec<Effect> {
    let Some(session) = state.sessions.get_mut(&id).filter(|session| session.live()) else { return Vec::new(); };
    // Fence first: every callback and in-flight command observes this tombstone.
    session.end(outcome);
    state.changed(id);
    let mut effects = vec![Effect::Command(MediaBackendCommand::CloseSession { session_id: id })];
    let operations: Vec<_> = state.operations.iter().filter_map(|(key, operation)|
        (operation.pending() && operation.session == Some(id)).then_some(*key)).collect();
    for operation in operations {
        effects.extend(abort_operation_locked(state, operation, terminal_error(outcome)));
        finish_committing(state, operation);
    }
    let actions = state.group.as_mut().map(|group| group.end_session(id)).unwrap_or_default();
    effects.extend(apply_actions_locked(state, actions));
    effects
}

pub(super) fn end_peer_locked(state: &mut State, sid: MediaSessionId, pid: MediaParticipantId, outcome: MediaCallOutcome) -> Vec<Effect> {
    let Some(participant) = state.sessions.get_mut(&sid)
        .and_then(|session| session.participants.get_mut(&pid)).filter(|participant| participant.live()) else { return Vec::new(); };
    participant.end(outcome);
    state.changed(sid);
    let mut effects = vec![Effect::Command(MediaBackendCommand::RemovePeer { session_id: sid, participant_id: pid })];
    let operations: Vec<_> = state.operations.iter().filter_map(|(key, operation)|
        (operation.pending() && operation.session == Some(sid) && operation.participant == Some(pid)).then_some(*key)).collect();
    for operation in operations {
        effects.extend(abort_operation_locked(state, operation, terminal_error(outcome)));
        finish_committing(state, operation);
    }
    let actions = state.group.as_mut().map(|group| group.remove(sid, pid)).unwrap_or_default();
    effects.extend(apply_actions_locked(state, actions));
    effects
}

pub(super) fn end_all_locked(state: &mut State, outcome: MediaCallOutcome) -> Vec<Effect> {
    // Pending data must not delay terminal control, or survive a new authority.
    state.pending_signals.clear();
    state.pending_signal_bytes = 0;
    let mut effects = Vec::new();
    let sessions: Vec<_> = state.sessions.iter().filter_map(|(id, session)| session.live().then_some(*id)).collect();
    for id in sessions { effects.extend(end_session_locked(state, id, outcome)); }
    let operations: Vec<_> = state.operations.iter().filter_map(|(id, operation)| operation.pending().then_some(*id)).collect();
    for id in operations {
        effects.extend(abort_operation_locked(state, id, terminal_error(outcome)));
        finish_committing(state, id);
    }
    let incoming: Vec<_> = state.incoming.iter().filter_map(|(id, incoming)|
        (!matches!(incoming.phase, IncomingPhase::Resolved(_))).then_some(*id)).collect();
    for id in incoming {
        let actions = state.group.as_mut().map(|group| group.decline(id, GroupDecline::Cancelled)).unwrap_or_default();
        effects.extend(apply_actions_locked(state, actions));
        state.resolve_incoming(id, MediaIncomingResolution::Cancelled);
    }
    effects
}

pub(super) fn revoke_permission_locked(state: &mut State, permission: MediaRevokedPermission) -> Vec<Effect> {
    if permission == MediaRevokedPermission::Calling {
        return end_all_locked(state, MediaCallOutcome::PermissionRevoked);
    }
    let requires_capture = |tracks: &MediaLocalTracks| match permission {
        MediaRevokedPermission::Microphone => tracks.microphone,
        MediaRevokedPermission::Camera => tracks.camera,
        MediaRevokedPermission::Calling => true,
    };
    // Rejecting new capture is not withdrawal of the independent calling grant.
    // Only committed capture intent or still-owned actual capture ends a session.
    let sessions: Vec<_> = state.sessions.iter().filter_map(|(id, session)| {
        let actual = match permission {
            MediaRevokedPermission::Microphone => session.snapshot.actual.microphone,
            MediaRevokedPermission::Camera => session.snapshot.actual.camera,
            MediaRevokedPermission::Calling => MediaTrackState::Off,
        };
        (session.live() && (requires_capture(&session.snapshot.requested) || actual != MediaTrackState::Off)).then_some(*id)
    }).collect();
    let mut effects = Vec::new();
    for id in sessions {
        effects.extend(end_session_locked(state, id, MediaCallOutcome::PermissionRevoked));
    }
    // Fence prepared/prompting capture as well, without tearing down its
    // previously committed receive-only session. Late results are canceled.
    let operations: Vec<_> = state.operations.iter().filter_map(|(id, operation)|
        (operation.pending() && operation.requested.as_ref().is_some_and(&requires_capture)).then_some(*id)).collect();
    for id in operations {
        effects.extend(abort_operation_locked(state, id, CallError::Denied));
    }
    effects
}

fn peer_is_current(state: &State, sid: MediaSessionId, pid: MediaParticipantId, epoch: u64) -> bool {
    !state.closed && !state.fatal && state.epoch == epoch && state.sessions.get(&sid).is_some_and(|session|
        session.epoch == epoch && session.visible && session.live()
            && session.participants.get(&pid).is_some_and(|participant| participant.visible && participant.live() && participant.backend_started))
}

fn input_bytes(input: &PeerInput) -> usize {
    match input {
        PeerInput::Description(description) => description.sdp.len(),
        PeerInput::Candidate(candidate) => candidate.candidate.len() + candidate.mid.as_ref().map_or(0, String::len),
    }
}

fn discard_input(mut input: PeerInput) {
    match &mut input {
        PeerInput::Description(description) => description.sdp.zeroize(),
        PeerInput::Candidate(candidate) => {
            candidate.candidate.zeroize();
            if let Some(mid) = &mut candidate.mid { mid.zeroize(); }
        }
    }
}

fn enqueue_input(state: &mut State, sid: MediaSessionId, pid: MediaParticipantId, input: PeerInput) -> Vec<Effect> {
    if !peer_is_current(state, sid, pid, state.epoch) { discard_input(input); return Vec::new(); }
    let valid = match &input {
        PeerInput::Description(description) => description.sdp.len() <= MAX_DESCRIPTION,
        PeerInput::Candidate(candidate) => candidate.candidate.len() <= MAX_CANDIDATE && candidate.mid.as_ref().is_none_or(|mid| mid.len() <= MAX_MID),
    };
    let bytes = input_bytes(&input);
    let (messages, total_bytes) = state.sessions.values().flat_map(|session| session.participants.values())
        .fold((0usize, 0usize), |(messages, bytes), participant|
            (messages + participant.pending_input.len(), bytes + participant.pending_input_bytes));
    let participant = state.sessions.get_mut(&sid).unwrap().participants.get_mut(&pid).unwrap();
    if !valid || participant.pending_input.len() >= MAX_PEER_MESSAGES || messages >= MAX_MESSAGES
        || bytes > MAX_PEER_BYTES.saturating_sub(participant.pending_input_bytes)
        || bytes > MAX_BYTES.saturating_sub(total_bytes)
    {
        discard_input(input);
        return end_peer_locked(state, sid, pid, MediaCallOutcome::HostFailed);
    }
    participant.pending_input_bytes += bytes;
    participant.pending_input.push_back(input);
    if participant.backend_ready && !participant.draining {
        participant.draining = true;
        vec![Effect::DrainPeer { session_id: sid, participant_id: pid, epoch: state.epoch }]
    } else { Vec::new() }
}

pub(super) fn apply_actions_locked(state: &mut State, actions: Vec<GroupAction>) -> Vec<Effect> {
    let mut actions = VecDeque::from(actions);
    let mut effects = Vec::new();
    let mut applied = 0usize;
    while let Some(action) = actions.pop_front() {
        applied += 1;
        if applied > MAX_ACTIONS {
            state.fatal = true;
            effects.extend(end_all_locked(state, MediaCallOutcome::HostFailed));
            break;
        }
        match action {
            GroupAction::Send { recipient, payload } => {
                let Some(signaling) = state.signaling.clone() else { continue; };
                if state.pending_signals.len() >= MAX_MESSAGES || payload.len() > MAX_BYTES.saturating_sub(state.pending_signal_bytes) {
                    let failures = state.group.as_mut().map(|group| group.send_failed(&recipient, &payload)).unwrap_or_default();
                    actions.extend(failures);
                    continue;
                }
                state.pending_signal_bytes += payload.len();
                state.pending_signals.push_back(PendingSignal { signaling, recipient, payload,
                    epoch: state.epoch, deadline: Instant::now() + OPERATION_TIMEOUT, attempts: 1, not_before: Instant::now() });
                if state.signal_workers < 8 {
                    state.signal_workers += 1;
                    effects.push(Effect::DrainSignals);
                }
            }
            GroupAction::Incoming(incoming) => {
                if state.incoming.contains_key(&incoming.incoming_id) { continue; }
                let now = state.signaling.as_ref().and_then(|signaling| signaling.now().ok());
                let live = state.incoming.values().filter(|incoming| !matches!(incoming.phase, IncomingPhase::Resolved(_))).count();
                let valid_session = incoming.existing_session.is_none_or(|id| state.live_session(&id).is_ok());
                if state.closed || state.fatal || state.incoming.len() >= INCOMING_BUDGET || live >= INCOMING_LIMIT
                    || !valid_session || now.is_none_or(|now| incoming.expires_at <= now)
                {
                    if let Some(group) = &mut state.group { actions.extend(group.decline(incoming.incoming_id, GroupDecline::Busy)); }
                    continue;
                }
                let remaining = Duration::from_secs(incoming.expires_at.saturating_sub(now.unwrap()).min(60));
                let offer = MediaIncomingOffer { incoming_id: incoming.incoming_id, peer: incoming.peer,
                    requested_remote_tracks: incoming.requested, remaining_ms: remaining.as_millis() as u32,
                    existing_session: incoming.existing_session };
                state.incoming.insert(offer.incoming_id, Incoming { offer: offer.clone(), deadline: Instant::now() + remaining,
                    epoch: state.epoch, phase: IncomingPhase::Open });
                state.emit(|sequence| MediaEvent::IncomingOffered { sequence, offer });
            }
            GroupAction::Resolved { incoming_id, resolution } => {
                // Resolve/cleanup never reserves a new incoming or operation key.
                let operation = state.incoming.get(&incoming_id).and_then(|incoming| match incoming.phase {
                    IncomingPhase::Claimed(id) => Some(id), _ => None,
                });
                if let Some(id) = operation {
                    // The group has already made the terminal decision. Detach
                    // the claim before abort cleanup so it cannot replace that
                    // reason with a local Cancelled resolution.
                    state.incoming.get_mut(&incoming_id).unwrap().phase = IncomingPhase::Awaiting;
                    effects.extend(abort_operation_locked(state, id, domain(HostMediaError::IncomingExpired)));
                }
                state.resolve_incoming(incoming_id, resolution);
            }
            GroupAction::StartPeer { session_id, participant_id, offerer } => {
                let epoch = state.epoch;
                if state.closed || state.fatal { continue; }
                let Some(session) = state.sessions.get_mut(&session_id).filter(|session| session.visible && session.live() && session.epoch == epoch) else { continue; };
                let Some(participant) = session.participants.get_mut(&participant_id).filter(|participant| participant.visible && participant.live() && !participant.backend_started) else { continue; };
                participant.backend_started = true;
                participant.snapshot.state = MediaParticipantState::Connecting;
                participant.deadline = Some(Instant::now() + CONNECT_TIMEOUT);
                state.changed(session_id);
                effects.push(Effect::StartPeer { session_id, participant_id, offerer, epoch });
            }
            GroupAction::EndPeer { session_id, participant_id, outcome } => {
                effects.extend(end_peer_locked(state, session_id, participant_id, outcome));
            }
            GroupAction::EndUnusedAcceptedSession { session_id } => {
                if state.sessions.get(&session_id).is_some_and(|session| session.live() && session.participants.values().all(|participant| !participant.live())) {
                    effects.extend(end_session_locked(state, session_id, MediaCallOutcome::RemoteEnded));
                }
            }
            GroupAction::Description { session_id, participant_id, description } => {
                effects.extend(enqueue_input(state, session_id, participant_id, PeerInput::Description(description)));
            }
            GroupAction::IceCandidate { session_id, participant_id, candidate } => {
                effects.extend(enqueue_input(state, session_id, participant_id, PeerInput::Candidate(candidate)));
            }
        }
    }
    effects
}


impl MediaService {
    pub(super) fn dispatch_effects(&self, effects: Vec<Effect>) {
        for effect in effects {
            match effect {
                Effect::Command(command) => {
                    // Retain the real backend independently of the service: Drop
                    // cleanup must remain executable after Weak stops upgrading.
                    let backend = self.backend.clone();
                    let product = self.product.clone();
                    let runtime_id = self.runtime_id;
                    let final_cleanup = matches!(command, MediaBackendCommand::CloseRuntime);
                    let mut guard = JobGuard::new(self.weak.clone());
                    if final_cleanup { guard.complete(); }
                    self.spawn(async move {
                        let call = backend.media_backend_command(&product, runtime_id, command).fuse();
                        let timeout = futures_timer::Delay::new(OPERATION_TIMEOUT).fuse();
                        pin_mut!(call, timeout);
                        let succeeded = select_biased! {
                            _ = timeout => false,
                            result = call => matches!(result, Ok(MediaBackendResponse::Done)),
                        };
                        if succeeded { guard.complete(); }
                    }.boxed());
                }
                Effect::StartPeer { session_id, participant_id, offerer, epoch } => {
                    let owner = self.weak.clone();
                    let mut guard = JobGuard::new(owner.clone());
                    self.spawn(async move {
                        if let Some(service) = owner.upgrade() { service.start_peer(session_id, participant_id, offerer, epoch).await; }
                        guard.complete();
                    }.boxed());
                }
                Effect::DrainPeer { session_id, participant_id, epoch } => {
                    let owner = self.weak.clone();
                    let mut guard = JobGuard::new(owner.clone());
                    self.spawn(async move {
                        if let Some(service) = owner.upgrade() { service.drain_peer(session_id, participant_id, epoch).await; }
                        guard.complete();
                    }.boxed());
                }
                Effect::DrainSignals => {
                    let owner = self.weak.clone();
                    let mut guard = JobGuard::new(owner.clone());
                    self.spawn(async move {
                        if let Some(service) = owner.upgrade() { service.drain_signals().await; }
                        guard.complete();
                    }.boxed());
                }
            }
        }
    }

    async fn start_peer(&self, sid: MediaSessionId, pid: MediaParticipantId, offerer: bool, epoch: u64) {
        let pending_commit = {
            let state = self.lock();
            if !peer_is_current(&state, sid, pid, epoch) || !self.current_authority(&state) { return; }
            state.operations.values().find(|operation| operation.pending() && operation.new_session
                && operation.session == Some(sid) && operation.committing.is_some())
                .and_then(|operation| operation.completion.clone().map(|completion| (completion, operation.deadline)))
        };
        if let Some((completion, deadline)) = pending_commit {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if self.bounded(&CancellationToken::default(), remaining, false, completion).await.is_err() {
                let effects = end_peer_locked(&mut self.lock(), sid, pid, MediaCallOutcome::HostFailed);
                self.dispatch_effects(effects);
                return;
            }
        }
        {
            let state = self.lock();
            if !peer_is_current(&state, sid, pid, epoch) || !self.current_authority(&state) { return; }
        }
        let result = self.command(MediaBackendCommand::CreatePeer { session_id: sid, participant_id: pid, offerer },
            CancellationToken::default(), OPERATION_TIMEOUT).await;
        let effects = {
            let mut state = self.lock();
            if !peer_is_current(&state, sid, pid, epoch) {
                vec![Effect::Command(MediaBackendCommand::RemovePeer { session_id: sid, participant_id: pid })]
            } else if !matches!(result, Ok(MediaBackendResponse::Done)) {
                end_peer_locked(&mut state, sid, pid, MediaCallOutcome::HostFailed)
            } else {
                let participant = state.sessions.get_mut(&sid).unwrap().participants.get_mut(&pid).unwrap();
                participant.backend_ready = true;
                if !participant.pending_input.is_empty() && !participant.draining {
                    participant.draining = true;
                    vec![Effect::DrainPeer { session_id: sid, participant_id: pid, epoch }]
                } else { Vec::new() }
            }
        };
        self.dispatch_effects(effects);
    }

    async fn drain_peer(&self, sid: MediaSessionId, pid: MediaParticipantId, epoch: u64) {
        loop {
            let input = {
                let mut state = self.lock();
                if !peer_is_current(&state, sid, pid, epoch) || !self.current_authority(&state) { return; }
                let participant = state.sessions.get_mut(&sid).unwrap().participants.get_mut(&pid).unwrap();
                let input = if participant.remote_description_ready {
                    participant.pending_input.pop_front()
                } else {
                    // Authenticated transport can deliver ICE before SDP. RTC
                    // cannot consume it before a remote description is installed.
                    participant.pending_input.iter().position(|input| matches!(input, PeerInput::Description(_)))
                        .and_then(|position| participant.pending_input.remove(position))
                };
                let Some(input) = input else { participant.draining = false; return; };
                participant.pending_input_bytes -= input_bytes(&input);
                input
            };
            let description = matches!(input, PeerInput::Description(_));
            let command = match input {
                PeerInput::Description(description) => MediaBackendCommand::ApplyDescription { session_id: sid, participant_id: pid, description },
                PeerInput::Candidate(candidate) => MediaBackendCommand::AddIceCandidate { session_id: sid, participant_id: pid, candidate },
            };
            if !matches!(self.command(command, CancellationToken::default(), OPERATION_TIMEOUT).await, Ok(MediaBackendResponse::Done)) {
                let effects = end_peer_locked(&mut self.lock(), sid, pid, MediaCallOutcome::ConnectivityLost);
                self.dispatch_effects(effects);
                return;
            }
            if description {
                let mut state = self.lock();
                if !peer_is_current(&state, sid, pid, epoch) { return; }
                state.sessions.get_mut(&sid).unwrap().participants.get_mut(&pid).unwrap().remote_description_ready = true;
            }
        }
    }

    async fn drain_signals(&self) {
        loop {
            let pending = {
                let mut state = self.lock();
                let Some(pending) = state.pending_signals.pop_front() else {
                    state.signal_workers -= 1;
                    return;
                };
                state.pending_signal_bytes -= pending.payload.len();
                if pending.epoch != state.epoch || !self.current_authority(&state)
                    || !state.group.as_ref().is_some_and(|group| group.send_current(&pending.recipient, &pending.payload)) { continue; }
                pending
            };
            // A retried handshake waits for a retired transport to reconnect,
            // then is fenced again: the link may have ended meanwhile.
            let wait = pending.not_before.saturating_duration_since(Instant::now());
            if !wait.is_zero() {
                futures_timer::Delay::new(wait).await;
                let state = self.lock();
                if pending.epoch != state.epoch || !self.current_authority(&state)
                    || !state.group.as_ref().is_some_and(|group| group.send_current(&pending.recipient, &pending.payload)) { continue; }
            }
            let remaining = pending.deadline.saturating_duration_since(Instant::now());
            let mut cx = CallContext::default();
            cx.set_timeout(remaining);
            let sent = !remaining.is_zero() && pending.signaling.send(&pending.recipient, &pending.payload, &cx).await.is_ok();
            if !sent {
                let effects = {
                    let mut state = self.lock();
                    if state.epoch != pending.epoch { continue; }
                    let room = state.pending_signals.len() < MAX_MESSAGES
                        && pending.payload.len() <= MAX_BYTES.saturating_sub(state.pending_signal_bytes);
                    if pending.attempts < SIGNAL_ATTEMPTS && room
                        && state.group.as_ref().is_some_and(|group| group.retries_send(&pending.recipient, &pending.payload))
                    {
                        // Same plaintext, new packet: the receiver deduplicates the
                        // handshake, so a copy that did land is harmless.
                        state.pending_signal_bytes += pending.payload.len();
                        let not_before = Instant::now() + SIGNAL_RETRY_DELAY;
                        state.pending_signals.push_back(PendingSignal {
                            not_before, deadline: not_before + OPERATION_TIMEOUT,
                            attempts: pending.attempts + 1,
                            ..pending
                        });
                        Vec::new()
                    } else {
                        let actions = state.group.as_mut().map(|group| group.send_failed(&pending.recipient, &pending.payload)).unwrap_or_default();
                        apply_actions_locked(&mut state, actions)
                    }
                };
                self.dispatch_effects(effects);
            }
        }
    }

    pub(super) fn backend_event(&self, event: MediaBackendEvent) {
        if let MediaBackendEvent::PermissionRevoked { permission, source } = event {
            self.backend_permission_revoked(permission, source);
            return;
        }
        let effects = {
            let mut state = self.lock();
            if state.closed || state.fatal { return; }
            if !matches!(event, MediaBackendEvent::ViewportChanged { .. }) && !self.current_authority(&state) { return; }
            match event {
                MediaBackendEvent::ViewportChanged { viewport } => {
                    if viewport == state.viewport { return; }
                    if let Some(viewport) = &viewport {
                        if viewport.revision <= state.viewport_revision || viewport.device_scale_numerator == 0 || viewport.device_scale_denominator == 0 { return; }
                        state.viewport_revision = viewport.revision;
                    }
                    state.viewport = viewport.clone();
                    // Retain requested layouts and completions: detach invalidates
                    // their viewport, never their monotonically consumed revision.
                    state.emit(|sequence| MediaEvent::ViewportChanged { sequence, viewport });
                    Vec::new()
                }
                MediaBackendEvent::LocalStateChanged { session_id, intent_revision, state: mut observed } => {
                    let epoch = state.epoch;
                    let Some(session) = state.sessions.get_mut(&session_id).filter(|session| session.visible && session.live()
                        && session.epoch == epoch && session.intent_revision == intent_revision) else { return; };
                    if !session.snapshot.requested.microphone { observed.microphone = MediaTrackState::Off; }
                    if !session.snapshot.requested.camera { observed.camera = MediaTrackState::Off; observed.camera_kind = None; }
                    if !session.snapshot.requested.screen { observed.screen = MediaTrackState::Off; }
                    if session.snapshot.actual == observed { return; }
                    session.snapshot.actual = observed;
                    state.changed(session_id);
                    Vec::new()
                }
                MediaBackendEvent::PeerStateChanged { session_id, participant_id, state: observed } => {
                    if !peer_is_current(&state, session_id, participant_id, state.epoch) { return; }
                    match observed {
                        MediaBackendPeerState::Failed | MediaBackendPeerState::Closed =>
                            end_peer_locked(&mut state, session_id, participant_id, MediaCallOutcome::ConnectivityLost),
                        _ => {
                            let participant = state.sessions.get_mut(&session_id).unwrap().participants.get_mut(&participant_id).unwrap();
                            let next = match observed {
                                MediaBackendPeerState::Connected => MediaParticipantState::Connected,
                                MediaBackendPeerState::Reconnecting => MediaParticipantState::Reconnecting,
                                _ => if matches!(participant.snapshot.state, MediaParticipantState::Connected | MediaParticipantState::Reconnecting) {
                                    MediaParticipantState::Reconnecting
                                } else { MediaParticipantState::Connecting },
                            };
                            if next == MediaParticipantState::Connected { participant.deadline = None; }
                            else if participant.deadline.is_none() { participant.deadline = Some(Instant::now() + CONNECT_TIMEOUT); }
                            if next == participant.snapshot.state { return; }
                            participant.snapshot.state = next;
                            state.changed(session_id);
                            Vec::new()
                        }
                    }
                }
                MediaBackendEvent::RemoteStateChanged { session_id, participant_id, state: observed } => {
                    if !peer_is_current(&state, session_id, participant_id, state.epoch) { return; }
                    let participant = state.sessions.get_mut(&session_id).unwrap().participants.get_mut(&participant_id).unwrap();
                    if participant.snapshot.media == observed { return; }
                    participant.snapshot.media = observed;
                    state.changed(session_id);
                    Vec::new()
                }
                MediaBackendEvent::Description { session_id, participant_id, description } => {
                    if !peer_is_current(&state, session_id, participant_id, state.epoch) { discard_input(PeerInput::Description(description)); return; }
                    if description.sdp.len() > MAX_DESCRIPTION {
                        discard_input(PeerInput::Description(description));
                        end_peer_locked(&mut state, session_id, participant_id, MediaCallOutcome::HostFailed)
                    } else {
                        let actions = state.group.as_mut().map(|group| group.local_description(session_id, participant_id, description)).unwrap_or_default();
                        apply_actions_locked(&mut state, actions)
                    }
                }
                MediaBackendEvent::IceCandidate { session_id, participant_id, candidate } => {
                    if !peer_is_current(&state, session_id, participant_id, state.epoch) { discard_input(PeerInput::Candidate(candidate)); return; }
                    if candidate.candidate.len() > MAX_CANDIDATE || candidate.mid.as_ref().is_some_and(|mid| mid.len() > MAX_MID) {
                        discard_input(PeerInput::Candidate(candidate));
                        end_peer_locked(&mut state, session_id, participant_id, MediaCallOutcome::HostFailed)
                    } else {
                        let actions = state.group.as_mut().map(|group| group.local_candidate(session_id, participant_id, candidate)).unwrap_or_default();
                        apply_actions_locked(&mut state, actions)
                    }
                }
                MediaBackendEvent::ScreenStopped { session_id } => {
                    let epoch = state.epoch;
                    let Some(session) = state.sessions.get_mut(&session_id).filter(|session| session.live() && session.epoch == epoch) else { return; };
                    let changed = session.snapshot.requested.screen || session.snapshot.actual.screen != MediaTrackState::Off;
                    session.snapshot.requested.screen = false;
                    session.snapshot.actual.screen = MediaTrackState::Off;
                    let mut effects = Vec::new();
                    let operations: Vec<_> = state.operations.iter().filter_map(|(id, operation)|
                        (operation.pending() && operation.committing.is_none() && operation.session == Some(session_id)
                            && operation.requested.as_ref().is_some_and(|tracks| tracks.screen)).then_some(*id)).collect();
                    for id in operations { effects.extend(abort_operation_locked(&mut state, id, domain(HostMediaError::OperationCancelled))); }
                    if changed { state.changed(session_id); }
                    effects
                }
                MediaBackendEvent::HostEnded { session_id } => {
                    if !state.sessions.get(&session_id).is_some_and(|session| session.live() && session.epoch == state.epoch) { return; }
                    end_session_locked(&mut state, session_id, MediaCallOutcome::LocalEnded)
                }
                MediaBackendEvent::PermissionRevoked { .. } => unreachable!(),
            }
        };
        self.dispatch_effects(effects);
    }

    fn backend_permission_revoked(&self, permission: MediaRevokedPermission, source: MediaRevocationSource) {
        let pending_bit = match permission {
            MediaRevokedPermission::Calling => 1,
            MediaRevokedPermission::Microphone => 2,
            MediaRevokedPermission::Camera => 4,
        };
        let (effects, request) = {
            let mut state = self.lock();
            if state.closed || state.fatal { return; }
            let request = match permission {
                MediaRevokedPermission::Calling => state.binding.as_ref().filter(|_| self.current_authority(&state))
                    .map(|binding| PermissionAuthorizationRequest::Calling { network: binding.identity.network, account: binding.identity.account }),
                MediaRevokedPermission::Microphone => Some(PermissionAuthorizationRequest::Device(HostDevicePermissionRequest::Microphone)),
                MediaRevokedPermission::Camera => Some(PermissionAuthorizationRequest::Device(HostDevicePermissionRequest::Camera)),
            };
            let effects = revoke_permission_locked(&mut state, permission);
            let request = request.filter(|_| source == MediaRevocationSource::Product && state.permission_writes & pending_bit == 0);
            if request.is_some() { state.permission_writes |= pending_bit; }
            (effects, request)
        };
        self.dispatch_effects(effects);
        let Some(request) = request else { return; };
        let owner = self.weak.clone();
        let mut guard = JobGuard::new(owner.clone());
        self.spawn(async move {
            let Some(service) = owner.upgrade() else { guard.complete(); return; };
            let cancel = CancellationToken::default();
            let result = service.bounded(&cancel, OPERATION_TIMEOUT, false, async {
                let _gate = service.services.media_permission_gate.lock().await;
                service.services.advance_media_permission_revision().map_err(|_| host_failure())?;
                // Fence all matching runtimes before awaiting storage. OS events
                // never enter this branch or rewrite remembered product consent.
                service.services.notify_media_permission_revoked(service.product_id(), &request);
                service.permissions_service().set_authorization_status(&request, PermissionAuthorizationStatus::Denied)
                    .await.map_err(|_| host_failure())
            }).await;
            if result.is_ok() {
                service.lock().permission_writes &= !pending_bit;
                guard.complete();
            }
        }.boxed());
    }

    pub(super) fn signaling_event(&self, epoch: u64, event: MediaSignalingEvent) {
        let effects = {
            let mut state = self.lock();
            if state.closed || state.fatal || state.epoch != epoch || !self.current_authority(&state) { return; }
            let Some(signaling) = state.signaling.clone() else { return; };
            let Ok(now) = signaling.now() else { return; };
            let actions = match event {
                MediaSignalingEvent::Message(message) => state.group.as_mut().map(|group| group.receive(*message, now)).unwrap_or_default(),
                // Signaling transport alone is not a media connectivity observation.
                MediaSignalingEvent::Reconnecting => Vec::new(),
                MediaSignalingEvent::Ready => {
                    if let (Some(group), Ok(advertisement)) = (&mut state.group, signaling.advertisement()) { group.update_advertisement(advertisement); }
                    state.group.as_mut().map(|group| group.tick(now)).unwrap_or_default()
                }
            };
            apply_actions_locked(&mut state, actions)
        };
        self.dispatch_effects(effects);
    }

    pub(super) fn maintenance(&self) {
        let effects = {
            let mut state = self.lock();
            if state.closed || state.fatal { return; }
            let now = Instant::now();
            let mut effects = Vec::new();
            let expired: Vec<_> = state.operations.iter().filter_map(|(id, operation)|
                (operation.pending() && operation.deadline <= now).then_some(*id)).collect();
            for id in expired {
                let committed = state.operations.get(&id).and_then(|operation| operation.committing.as_ref().map(|_| operation.session));
                if let Some(session) = committed {
                    if let Some(session) = session { effects.extend(end_session_locked(&mut state, session, MediaCallOutcome::HostFailed)); }
                    finish_committing(&mut state, id);
                } else { effects.extend(abort_operation_locked(&mut state, id, domain(HostMediaError::TimedOut))); }
            }
            let expired: Vec<_> = state.incoming.iter().filter_map(|(id, incoming)|
                (!matches!(incoming.phase, IncomingPhase::Resolved(_)) && incoming.deadline <= now).then_some(*id)).collect();
            for id in expired {
                let actions = state.group.as_mut().map(|group| group.decline(id, GroupDecline::Expired)).unwrap_or_default();
                effects.extend(apply_actions_locked(&mut state, actions));
                state.resolve_incoming(id, MediaIncomingResolution::Expired);
            }
            let expired: Vec<_> = state.sessions.iter().flat_map(|(sid, session)| session.participants.iter()
                .filter_map(move |(pid, participant)| (session.live() && participant.live() && participant.deadline.is_some_and(|deadline| deadline <= now)).then_some((*sid, *pid)))).collect();
            for (sid, pid) in expired { effects.extend(end_peer_locked(&mut state, sid, pid, MediaCallOutcome::ConnectivityLost)); }
            if let Some(signaling) = state.signaling.clone() {
                if let (Some(group), Ok(advertisement)) = (&mut state.group, signaling.advertisement()) { group.update_advertisement(advertisement); }
                if let Ok(now) = signaling.now() {
                    let actions = state.group.as_mut().map(|group| group.tick(now)).unwrap_or_default();
                    effects.extend(apply_actions_locked(&mut state, actions));
                }
            }
            effects
        };
        self.dispatch_effects(effects);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_denial_preserves_receive_and_fences_only_affected_capture() {
        let mut state = State::new();
        let receiving_id = [1; 32];
        let capturing_id = [2; 32];
        let operation_id = [3; 32];
        let mut receiving = Session::new(receiving_id, 0);
        receiving.visible = true;
        let mut participant = Participant::new([4; 32], MediaPeer {
            network: MediaNetwork { genesis_hash: [5; 32] },
            product_id: "vox.dot".into(),
            account: MediaAccount::Sr25519([6; 32]),
        });
        participant.visible = true;
        participant.snapshot.state = MediaParticipantState::Connected;
        participant.snapshot.media.camera = MediaTrackState::Live;
        receiving.participants.insert([4; 32], participant);
        receiving.snapshot.state = MediaSessionState::Connected;
        let before = receiving.current();
        state.sessions.insert(receiving_id, receiving);
        let mut capturing = Session::new(capturing_id, 0);
        capturing.visible = true;
        capturing.snapshot.requested.camera = true;
        capturing.snapshot.actual.camera = MediaTrackState::Live;
        state.sessions.insert(capturing_id, capturing);
        let mut tracks = off_tracks();
        tracks.camera = true;
        let mut operation = Operation::new(&Mutation::Tracks(HostMediaSetLocalTracksRequest {
            operation_id, session_id: receiving_id, tracks: tracks.clone(),
        }), 0);
        operation.session = Some(receiving_id);
        operation.requested = Some(tracks);
        state.operations.insert(operation_id, operation);

        revoke_permission_locked(&mut state, MediaRevokedPermission::Camera);

        assert_eq!(state.session(&receiving_id).unwrap().current(), before);
        let ended = state.session(&capturing_id).unwrap().current();
        assert_eq!(ended.state, MediaSessionState::Ended);
        assert_eq!(ended.outcome, Some(MediaCallOutcome::PermissionRevoked));
        assert_eq!(state.operations[&operation_id].snapshot.state,
            MediaOperationState::Failed { failure: MediaOperationFailure::Denied });

        revoke_permission_locked(&mut state, MediaRevokedPermission::Calling);
        assert_eq!(state.session(&receiving_id).unwrap().current().outcome,
            Some(MediaCallOutcome::PermissionRevoked));
    }
}
