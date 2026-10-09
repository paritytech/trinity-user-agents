//! Correlated admission, permission decisions, and atomic public mutations.

use std::{collections::BTreeSet, sync::Arc, time::Duration};

use futures::{channel::oneshot, FutureExt};
use truapi::{v01::*, CallContext, CallError, CancellationToken};
use crate::platform::{
    MediaBackendCommand, MediaBackendResponse, MediaConsentRequest,
    PermissionAuthorizationRequest, PermissionAuthorizationStatus,
};

use super::{
    backend::{apply_actions_locked, end_peer_locked, end_session_locked, Effect},
    group::GroupDecline,
    state::{self, all_off, domain, exhausted, host_failure, Completion, Error, IncomingPhase,
        Mutation, MutationReply, Operation, Participant, Result, Session, State,
        CONSENT_TIMEOUT, OPERATION_BUDGET, OPERATION_TIMEOUT},
    Instant, JobGuard, MediaService,
};
use crate::{host_logic::media_protocol::MediaIdentity, runtime::authority::AuthoritySession};

struct Admission {
    /// An already-committing capture change must finish before another prepare.
    predecessor: Option<Completion>,
    /// A duplicate hidden participant joins its owner instead of leaking its ID.
    duplicate: bool,
    effects: Vec<Effect>,
}

pub(super) fn operation_reply(state: &State, result: &MediaOperationResult) -> Result<MutationReply> {
    match result {
        MediaOperationResult::Session { session_id } | MediaOperationResult::Tracks { session_id } => {
            Ok(MutationReply::Session(state.session(session_id)?.current()))
        }
        MediaOperationResult::Participant { session_id, participant_id } => {
            let participant = state.session(session_id)?.participants.get(participant_id)
                .filter(|participant| participant.visible).ok_or_else(host_failure)?;
            Ok(MutationReply::Participant(participant.snapshot.clone()))
        }
        MediaOperationResult::Incoming { session_id, participant_id, .. } => {
            let session = state.session(session_id)?;
            let participant = session.participants.get(participant_id)
                .filter(|participant| participant.visible).ok_or_else(host_failure)?;
            Ok(MutationReply::Incoming(MediaIncomingResponse::Accepted {
                session: session.current(), participant: participant.snapshot.clone(),
            }))
        }
    }
}

/// Abort the decision before releasing anything; recursive teardown then cannot
/// cancel a committing operation or repeatedly consume the same incoming offer.
pub(super) fn abort_operation_locked(state: &mut State, id: MediaOperationId, error: Error) -> Vec<Effect> {
    let Some(operation) = state.operations.get_mut(&id) else { return Vec::new(); };
    if !operation.abort(error) { return Vec::new(); }
    let (sid, pid, incoming, new_session, owns_participant) =
        (operation.session, operation.participant, operation.incoming, operation.new_session, operation.owns_participant);
    let mut effects = vec![Effect::Command(MediaBackendCommand::CancelOperation { operation_id: id })];
    if let Some(incoming) = incoming {
        let claimed = state.incoming.get(&incoming)
            .is_some_and(|value| matches!(value.phase, IncomingPhase::Claimed(owner) if owner == id));
        if claimed {
            let actions = state.group.as_mut().map(|group| group.decline(incoming, GroupDecline::Cancelled)).unwrap_or_default();
            effects.extend(apply_actions_locked(state, actions));
            state.resolve_incoming(incoming, MediaIncomingResolution::Cancelled);
        }
    }
    if let Some(sid) = sid {
        if new_session {
            effects.extend(end_session_locked(state, sid, MediaCallOutcome::LocalEnded));
        } else if let Some(pid) = pid {
            // A duplicate operation never owns the reservation it joins.
            let hidden = state.sessions.get(&sid).and_then(|session| session.participants.get(&pid))
                .is_some_and(|participant| !participant.visible && participant.live());
            if hidden && owns_participant {
                effects.extend(end_peer_locked(state, sid, pid, MediaCallOutcome::LocalEnded));
            }
        }
        if let Some(session) = state.sessions.get_mut(&sid)
            && session.track_operation == Some(id) { session.track_operation = None; }
    }
    effects
}

impl MediaService {
    pub(super) async fn mutate(self: &Arc<Self>, request: Mutation) -> Result<MutationReply> {
        let id = request.id();
        let authority = self.authority.current_session();
        let (completion, work, effects) = {
            let mut state = self.lock();
            if let Some(operation) = state.operations.get(&id) {
                if operation.fingerprint.is_none() { return Err(domain(HostMediaError::OperationCancelled)); }
                if operation.fingerprint != Some(request.fingerprint()) { return Err(domain(HostMediaError::OperationConflict)); }
                (operation.completion.clone().ok_or_else(host_failure)?, None, Vec::new())
            } else {
                state.ensure_open()?;
                if state.operations.len() >= OPERATION_BUDGET { return Err(exhausted(MediaResource::Operations)); }
                let operation = Operation::new(&request, state.epoch);
                let completion = operation.completion.clone().ok_or_else(host_failure)?;
                state.operations.insert(id, operation);
                // Existing handles have known capabilities. In particular track
                // intent is ordered here, before the spawned future is polled.
                let admission = if state.capabilities.is_some() {
                    self.admit_locked(&mut state, &request).map(Some)
                } else {
                    state.require_listener().map(|()| None)
                };
                match admission {
                    Ok(mut admission) => {
                        let effects = admission.as_mut().map(|admission| std::mem::take(&mut admission.effects)).unwrap_or_default();
                        (completion, Some(admission), effects)
                    }
                    Err(error) => {
                        let effects = abort_operation_locked(&mut state, id, error);
                        (completion, None, effects)
                    }
                }
            }
        };
        self.dispatch_effects(effects);
        if let Some(admission) = work {
            let service = self.clone();
            let mut guard = JobGuard::new(Arc::downgrade(self));
            self.spawn(async move {
                if let Err(error) = service.run_mutation(request, authority, admission).await {
                    let effects = {
                        let mut state = service.lock();
                        let decision = state.operations.get(&id).and_then(|operation| operation.committing.clone());
                        if let Some(result) = decision {
                            let sid = state.operations.get(&id).and_then(|operation| operation.session);
                            let effects = sid.map(|sid| end_session_locked(&mut state, sid, MediaCallOutcome::HostFailed)).unwrap_or_default();
                            let reply = operation_reply(&state, &result);
                            if let Some(operation) = state.operations.get_mut(&id) { operation.finish(reply); }
                            effects
                        } else {
                            abort_operation_locked(&mut state, id, error)
                        }
                    };
                    service.dispatch_effects(effects);
                }
                guard.complete();
            }.boxed());
        }
        completion.await?;
        let state = self.lock();
        let operation = state.operations.get(&id).ok_or_else(host_failure)?;
        match &operation.snapshot.state {
            MediaOperationState::Committed { result } => operation_reply(&state, result),
            _ => Err(host_failure()),
        }
    }

    fn admit_locked(&self, state: &mut State, request: &Mutation) -> Result<Admission> {
        state.ensure_open()?;
        let id = request.id();
        if !state.operations.get(&id).is_some_and(Operation::pending) {
            return Err(domain(HostMediaError::OperationCancelled));
        }
        if !matches!(request, Mutation::Tracks(request) if all_off(&request.tracks)) {
            if state.permission_writes != 0 { return Err(CallError::Denied); }
            state.require_listener()?;
        }
        let capabilities = state.capabilities.as_ref().ok_or_else(host_failure)?;
        let session_limit = capabilities.max_sessions;
        let participant_limit = capabilities.max_remote_participants;
        let network = capabilities.network.genesis_hash;
        let mut predecessor = None;
        let mut duplicate = false;
        let mut effects = Vec::new();
        let (sid, pid, incoming_id, tracks, new_session) = match request {
            Mutation::Create(request) => {
                check_session_capacity(state, session_limit)?;
                (state.new_session_id()?, None, None, Some(request.tracks.clone()), true)
            }
            Mutation::Tracks(request) => {
                let session = state.live_session(&request.session_id)?;
                let revision = session.admitted_intent.checked_add(1).ok_or_else(host_failure)?;
                let previous = session.track_operation;
                predecessor = state.operations.values().find(|operation| operation.session == Some(request.session_id)
                    && operation.pending() && operation.committing.is_some()).and_then(|operation| operation.completion.clone());
                let operation = state.operations.get_mut(&id).ok_or_else(host_failure)?;
                operation.intent_revision = revision;
                let session = state.sessions.get_mut(&request.session_id).ok_or_else(host_failure)?;
                session.admitted_intent = revision;
                session.track_operation = Some(id);
                if let Some(previous) = previous {
                    effects.extend(abort_operation_locked(state, previous, domain(HostMediaError::InvalidState)));
                }
                (request.session_id, None, None, Some(request.tracks.clone()), false)
            }
            Mutation::Add(request) => {
                if request.peer.network.genesis_hash != network { return Err(domain(HostMediaError::NetworkMismatch)); }
                if request.peer.product_id != self.product_id() { return Err(domain(HostMediaError::ProductMismatch)); }
                let MediaAccount::Sr25519(account) = request.peer.account;
                MediaIdentity { network, product_id: request.peer.product_id.clone(), account }
                    .validate().map_err(|_| domain(HostMediaError::InvalidPeer))?;
                let session = state.live_session(&request.session_id)?;
                if let Some(participant) = session.participants.values().find(|participant| participant.live() && participant.snapshot.peer == request.peer) {
                    duplicate = true;
                    let pid = participant.snapshot.participant_id;
                    if !participant.visible {
                        predecessor = state.operations.values().find(|operation| operation.session == Some(request.session_id)
                            && operation.participant == Some(pid) && operation.pending())
                            .and_then(|operation| operation.completion.clone());
                        if predecessor.is_none() { return Err(domain(HostMediaError::InvalidState)); }
                    }
                    (request.session_id, Some(pid), None, None, false)
                } else {
                    check_participant_capacity(session, participant_limit)?;
                    (request.session_id, Some(state.new_participant_id()?), None, None, false)
                }
            }
            Mutation::Accept { incoming_id, session_id, tracks, .. } => {
                let incoming = state.incoming.get(incoming_id).ok_or_else(|| domain(HostMediaError::InvalidHandle))?;
                check_incoming(incoming, state.epoch)?;
                if incoming.offer.existing_session != *session_id { return Err(domain(HostMediaError::InvalidHandle)); }
                match (session_id, tracks) {
                    (Some(sid), None) => {
                        let session = state.live_session(sid)?;
                        check_participant_capacity(session, participant_limit)?;
                        if session.participants.values().any(|participant| participant.live() && participant.snapshot.peer == incoming.offer.peer) {
                            return Err(domain(HostMediaError::InvalidState));
                        }
                        (*sid, Some(state.new_participant_id()?), Some(*incoming_id), None, false)
                    }
                    (None, Some(tracks)) => {
                        check_session_capacity(state, session_limit)?;
                        (state.new_session_id()?, Some(state.new_participant_id()?), Some(*incoming_id), Some(tracks.clone()), true)
                    }
                    _ => return Err(domain(HostMediaError::InvalidState)),
                }
            }
        };
        if new_session {
            let opening = state.operations.get(&id).and_then(|operation| operation.completion.clone()).ok_or_else(host_failure)?;
            let mut session = Session::new(sid, state.epoch);
            session.backend_opening = Some(opening);
            state.sessions.insert(sid, session);
        }
        if let Some(pid) = pid
            && !duplicate {
                let peer = match request {
                    Mutation::Add(request) => request.peer.clone(),
                    Mutation::Accept { incoming_id, .. } => state.incoming.get(incoming_id).ok_or_else(host_failure)?.offer.peer.clone(),
                    _ => return Err(host_failure()),
                };
                state.sessions.get_mut(&sid).ok_or_else(host_failure)?.participants.insert(pid, Participant::new(pid, peer));
                state.issued_participants += 1;
            }
        let operation = state.operations.get_mut(&id).ok_or_else(host_failure)?;
        operation.session = Some(sid);
        operation.participant = pid;
        operation.owns_participant = pid.is_some() && !duplicate;
        operation.incoming = incoming_id;
        operation.new_session = new_session;
        operation.requested = tracks;
        operation.epoch = state.epoch;
        if let Some(incoming) = incoming_id.and_then(|incoming| state.incoming.get_mut(&incoming)) {
            incoming.phase = IncomingPhase::Claimed(id);
        }
        Ok(Admission { predecessor, duplicate, effects })
    }

    fn check_operation_locked(&self, state: &State, id: MediaOperationId, cleanup: bool) -> Result<()> {
        state.ensure_open()?;
        if !cleanup && state.permission_writes != 0 { return Err(CallError::Denied); }
        let operation = state.operations.get(&id).ok_or_else(|| domain(HostMediaError::InvalidOperation))?;
        if !operation.pending() || operation.cancel.is_cancelled() { return Err(domain(HostMediaError::OperationCancelled)); }
        if operation.deadline <= Instant::now() { return Err(domain(HostMediaError::TimedOut)); }
        if operation.epoch != state.epoch || (!cleanup && !self.current_authority(state)) { return Err(domain(HostMediaError::NotConnected)); }
        if let Some(sid) = operation.session {
            let session = state.sessions.get(&sid).ok_or_else(host_failure)?;
            if !session.live() || session.epoch != state.epoch { return Err(domain(HostMediaError::SessionEnded)); }
            if operation.snapshot.kind == Some(MediaOperationKind::SetLocalTracks) && operation.committing.is_none()
                && session.admitted_intent != operation.intent_revision {
                return Err(domain(HostMediaError::InvalidState));
            }
        }
        if let Some(incoming) = operation.incoming {
            let incoming = state.incoming.get(&incoming).ok_or_else(host_failure)?;
            if incoming.deadline <= Instant::now() { return Err(domain(HostMediaError::IncomingExpired)); }
            if operation.committing.is_none() && !matches!(incoming.phase, IncomingPhase::Claimed(owner) if owner == id) {
                return Err(domain(HostMediaError::IncomingConsumed));
            }
        }
        Ok(())
    }

    fn operation_window(&self, id: MediaOperationId, cleanup: bool) -> Result<(CancellationToken, Duration)> {
        let state = self.lock();
        self.check_operation_locked(&state, id, cleanup)?;
        let operation = state.operations.get(&id).ok_or_else(host_failure)?;
        let deadline = operation.incoming.and_then(|incoming| state.incoming.get(&incoming))
            .map_or(operation.deadline, |incoming| operation.deadline.min(incoming.deadline));
        Ok((operation.cancel.clone(), deadline.saturating_duration_since(Instant::now())))
    }

    fn extend_for_consent(&self, id: MediaOperationId) -> Result<()> {
        let mut state = self.lock();
        self.check_operation_locked(&state, id, false)?;
        let operation = state.operations.get_mut(&id).ok_or_else(host_failure)?;
        operation.deadline = operation.started + CONSENT_TIMEOUT;
        Ok(())
    }

    async fn run_mutation(self: &Arc<Self>, request: Mutation, authority: Option<AuthoritySession>, admission: Option<Admission>) -> Result<()> {
        let id = request.id();
        let cleanup = matches!(&request, Mutation::Tracks(request) if all_off(&request.tracks));
        if !cleanup {
            let (cancel, remaining) = {
                let state = self.lock();
                let operation = state.operations.get(&id).ok_or_else(host_failure)?;
                if !operation.pending() { return Ok(()); }
                (operation.cancel.clone(), operation.deadline.saturating_duration_since(Instant::now()))
            };
            self.bounded(&cancel, remaining, false, self.ensure_ready()).await?;
            let state = self.lock();
            if !authority.as_ref().zip(state.binding.as_ref()).is_some_and(|(expected, binding)| state::same_authority(expected, &binding.session)) {
                return Err(domain(HostMediaError::NotConnected));
            }
        }
        let mut admission = match admission {
            Some(admission) => admission,
            None => {
                let mut state = self.lock();
                self.admit_locked(&mut state, &request)?
            }
        };
        self.dispatch_effects(std::mem::take(&mut admission.effects));
        self.operation_window(id, cleanup)?;
        if let Some(predecessor) = admission.predecessor {
            let (cancel, remaining) = self.operation_window(id, cleanup)?;
            let previous = self.bounded(&cancel, remaining, false, async { Ok(predecessor.await) }).await?;
            if admission.duplicate { previous?; }
            self.operation_window(id, cleanup)?;
        }
        let requirements = self.permission_requirements(id, cleanup)?;
        for (request, consent) in &requirements { self.consent(id, request, consent.clone()).await?; }
        let (sid, pid, tracks, new_session, revision, signaling) = {
            let state = self.lock();
            self.check_operation_locked(&state, id, cleanup)?;
            let operation = state.operations.get(&id).ok_or_else(host_failure)?;
            (operation.session.ok_or_else(host_failure)?, operation.participant,
                operation.requested.clone(), operation.new_session, operation.intent_revision, state.signaling.clone())
        };
        let endpoints = if let Mutation::Add(request) = &request {
            if admission.duplicate { None } else {
                let signaling = signaling.as_ref().ok_or_else(|| domain(HostMediaError::NotConnected))?;
                let MediaAccount::Sr25519(account) = request.peer.account;
                let peer = MediaIdentity { network: request.peer.network.genesis_hash, product_id: request.peer.product_id.clone(), account };
                let (cancel, remaining) = self.operation_window(id, cleanup)?;
                let context = CallContext::with_parts(Default::default(), cancel.clone());
                Some(self.bounded(&cancel, remaining, false, async {
                    signaling.lookup(&peer, &context).await.map_err(|_| domain(HostMediaError::NotConnected))
                }).await?)
            }
        } else { None };
        if let Some(tracks) = &tracks {
            if !all_off(tracks) { self.extend_for_consent(id)?; }
            self.check_permissions(id, cleanup, &requirements).await?;
            let (cancel, remaining) = self.operation_window(id, cleanup)?;
            let command = if new_session {
                MediaBackendCommand::OpenSession { session_id: sid, operation_id: id, tracks: tracks.clone() }
            } else {
                MediaBackendCommand::SetTracks { session_id: sid, operation_id: id, intent_revision: revision, tracks: tracks.clone() }
            };
            if !matches!(self.command(command, cancel, remaining).await?, MediaBackendResponse::Done) { return Err(host_failure()); }
            self.operation_window(id, cleanup)?;
        }
        // Permission mutations and the logical decision share one gate. No
        // prompt or capture wait holds it; final effective OS status is reread.
        let (cancel, remaining) = self.operation_window(id, cleanup)?;
        let (result, effects) = self.bounded(&cancel, remaining, false, async {
            let _guard = self.services.media_permission_gate.lock().await;
            let final_requirements = self.permission_requirements(id, cleanup)?;
            self.read_permissions(&final_requirements).await?;
            let mut state = self.lock();
            self.check_operation_locked(&state, id, cleanup)?;
            let now = if matches!(&request, Mutation::Accept { .. })
                || matches!(&request, Mutation::Add(_) if !admission.duplicate) {
                signaling.as_ref().ok_or_else(|| domain(HostMediaError::NotConnected))?.now()
                    .map_err(|_| domain(HostMediaError::NotConnected))?
            } else { 0 };
            let requested = tracks.clone().unwrap_or_else(|| state.sessions[&sid].snapshot.requested.clone());
            let actions = match &request {
                Mutation::Create(_) => {
                    state.group.as_mut().ok_or_else(host_failure)?.create_session(sid).map_err(domain)?;
                    Vec::new()
                }
                Mutation::Add(request) if !admission.duplicate => state.group.as_mut().ok_or_else(host_failure)?
                    .invite(sid, pid.ok_or_else(host_failure)?, request.peer.clone(), endpoints.ok_or_else(host_failure)?, &requested, now).map_err(domain)?,
                Mutation::Accept { incoming_id, .. } => state.group.as_mut().ok_or_else(host_failure)?
                    .accept(*incoming_id, sid, pid.ok_or_else(host_failure)?, &requested, now).map_err(domain)?,
                _ => Vec::new(),
            };
            let result = match &request {
                Mutation::Create(_) => MediaOperationResult::Session { session_id: sid },
                Mutation::Tracks(_) => MediaOperationResult::Tracks { session_id: sid },
                Mutation::Add(_) => MediaOperationResult::Participant { session_id: sid, participant_id: pid.ok_or_else(host_failure)? },
                Mutation::Accept { incoming_id, .. } => MediaOperationResult::Incoming { incoming_id: *incoming_id, session_id: sid, participant_id: pid.ok_or_else(host_failure)? },
            };
            state.operations.get_mut(&id).ok_or_else(host_failure)?.committing = Some(result.clone());
            let session = state.sessions.get_mut(&sid).ok_or_else(host_failure)?;
            session.visible = true;
            if let Some(tracks) = &tracks {
                session.snapshot.requested = tracks.clone();
                session.snapshot.actual = state::starting(tracks);
                session.intent_revision = revision;
            }
            if let Some(pid) = pid { session.participants.get_mut(&pid).ok_or_else(host_failure)?.visible = true; }
            if let Mutation::Accept { incoming_id, .. } = &request {
                state.incoming.get_mut(incoming_id).ok_or_else(host_failure)?.phase = IncomingPhase::Awaiting;
            }
            state.changed(sid);
            let effects = apply_actions_locked(&mut state, actions);
            Ok((result, effects))
        }).await?;
        if tracks.is_some() {
            let remaining = {
                let state = self.lock();
                state.operations.get(&id).ok_or_else(host_failure)?.deadline.saturating_duration_since(Instant::now())
            };
            let ready = {
                let state = self.lock();
                state.sessions.get(&sid).is_some_and(|session| session.live() && session.epoch == state.epoch)
                    && !state.closed && !state.fatal
                    && (cleanup || (state.permission_writes == 0 && self.current_authority(&state)))
            };
            let committed = if ready {
                self.command(MediaBackendCommand::CommitOperation { operation_id: id }, CancellationToken::default(), remaining).await
            } else { Err(host_failure()) };
            let (reply, cleanup_effects, deliver_effects) = {
                let mut state = self.lock();
                if let Some(session) = state.sessions.get_mut(&sid) { session.backend_opening = None; }
                let mut cleanup_effects = Vec::new();
                let deliver_effects = match committed {
                    Ok(MediaBackendResponse::LocalState { state: mut actual }) => {
                        if let Some(session) = state.sessions.get_mut(&sid).filter(|session| session.live() && session.intent_revision == revision) {
                            clamp_actual(&session.snapshot.requested, &mut actual);
                            session.snapshot.actual = actual;
                            state.changed(sid);
                        }
                        true
                    }
                    _ => {
                        cleanup_effects.extend(end_session_locked(&mut state, sid, MediaCallOutcome::HostFailed));
                        false
                    }
                };
                let reply = operation_reply(&state, &result);
                if let Some(operation) = state.operations.get_mut(&id) { operation.finish(reply.clone()); }
                (reply, cleanup_effects, deliver_effects)
            };
            self.dispatch_effects(cleanup_effects);
            if deliver_effects { self.dispatch_effects(effects); }
            // A failed backend commit still returns the owned terminal resource;
            // its retained decision remains Committed, never a false rollback.
            reply?;
        } else {
            {
                let mut state = self.lock();
                let reply = operation_reply(&state, &result);
                state.operations.get_mut(&id).ok_or_else(host_failure)?.finish(reply);
            }
            self.dispatch_effects(effects);
        }
        Ok(())
    }

    fn permission_requirements(&self, id: MediaOperationId, cleanup: bool) -> Result<Vec<(PermissionAuthorizationRequest, MediaConsentRequest)>> {
        if cleanup { return Ok(Vec::new()); }
        let state = self.lock();
        self.check_operation_locked(&state, id, false)?;
        let identity = &state.binding.as_ref().ok_or_else(host_failure)?.identity;
        let mut requests = vec![(PermissionAuthorizationRequest::Calling { network: identity.network, account: identity.account },
            MediaConsentRequest::Calling { network: identity.network, account: identity.account })];
        let operation = state.operations.get(&id).ok_or_else(host_failure)?;
        let tracks = operation.requested.as_ref().or_else(|| operation.session
            .and_then(|sid| state.sessions.get(&sid)).map(|session| &session.snapshot.requested));
        if let Some(tracks) = tracks {
            if tracks.microphone { requests.push((PermissionAuthorizationRequest::Device(HostDevicePermissionRequest::Microphone), MediaConsentRequest::Microphone)); }
            if tracks.camera { requests.push((PermissionAuthorizationRequest::Device(HostDevicePermissionRequest::Camera), MediaConsentRequest::Camera)); }
        }
        Ok(requests)
    }

    async fn consent(&self, id: MediaOperationId, request: &PermissionAuthorizationRequest, consent: MediaConsentRequest) -> Result<()> {
        let (cancel, remaining) = self.operation_window(id, false)?;
        let (snapshot, revision) = self.bounded(&cancel, remaining, false, async {
            let _guard = self.services.media_permission_gate.lock().await;
            let snapshot = self.permissions_service().authorization_snapshot(request).await.map_err(|_| host_failure())?;
            self.operation_window(id, false)?;
            Ok((snapshot, self.services.media_permission_revision()))
        }).await?;
        match snapshot.status {
            PermissionAuthorizationStatus::Authorized => return Ok(()),
            PermissionAuthorizationStatus::Denied => return Err(CallError::Denied),
            PermissionAuthorizationStatus::NotDetermined => {}
        }
        self.extend_for_consent(id)?;
        let (cancel, remaining) = self.operation_window(id, false)?;
        let answer = self.command(MediaBackendCommand::RequestConsent { operation_id: id, request: consent }, cancel, remaining).await?;
        let MediaBackendResponse::Consent { granted } = answer else { return Err(host_failure()); };
        let (cancel, remaining) = self.operation_window(id, false)?;
        self.bounded(&cancel, remaining, false, async {
            let _guard = self.services.media_permission_gate.lock().await;
            self.operation_window(id, false)?;
            if self.services.media_permission_revision() != revision { return Err(CallError::Denied); }
            // A genuine product answer cannot turn OS withdrawal into a stored
            // product refusal, nor overwrite a concurrent settings decision.
            if self.permissions_service().authorization_status(request).await.map_err(|_| host_failure())?
                != PermissionAuthorizationStatus::NotDetermined { return Err(CallError::Denied); }
            self.operation_window(id, false)?;
            if self.services.media_permission_revision() != revision { return Err(CallError::Denied); }
            // Dispatch accepts this permission decision, not the whole Media
            // operation. Later cancellation never compensates accepted consent.
            self.services.advance_media_permission_revision().map_err(|_| host_failure())?;
            let status = if granted { PermissionAuthorizationStatus::Authorized } else { PermissionAuthorizationStatus::Denied };
            if !self.permissions_service().set_authorization_status_if_unchanged(request, &snapshot, status).await.map_err(|_| host_failure())? {
                return Err(CallError::Denied);
            }
            if !granted {
                self.services.notify_media_permission_revoked(self.product_id(), request);
            }
            self.operation_window(id, false)?;
            if granted { Ok(()) } else { Err(CallError::Denied) }
        }).await
    }

    async fn read_permissions(&self, requirements: &[(PermissionAuthorizationRequest, MediaConsentRequest)]) -> Result<()> {
        let service = self.permissions_service();
        for (request, _) in requirements {
            if service.authorization_status(request).await.map_err(|_| host_failure())? != PermissionAuthorizationStatus::Authorized {
                return Err(CallError::Denied);
            }
        }
        Ok(())
    }

    async fn check_permissions(&self, id: MediaOperationId, cleanup: bool, requirements: &[(PermissionAuthorizationRequest, MediaConsentRequest)]) -> Result<()> {
        let (cancel, remaining) = self.operation_window(id, cleanup)?;
        self.bounded(&cancel, remaining, false, async {
            let _guard = self.services.media_permission_gate.lock().await;
            self.read_permissions(requirements).await?;
            self.operation_window(id, cleanup)?;
            Ok(())
        }).await
    }

    pub(super) async fn operation(&self, id: MediaOperationId) -> Result<MediaOperationSnapshot> {
        self.lock().operations.get(&id).map(|operation| operation.snapshot.clone())
            .ok_or_else(|| domain(HostMediaError::InvalidOperation))
    }

    pub(super) async fn cancel_operation(self: &Arc<Self>, id: MediaOperationId) -> Result<MediaOperationSnapshot> {
        let (completion, effects) = {
            let mut state = self.lock();
            if let Some(operation) = state.operations.get(&id) {
                if operation.committing.is_some() {
                    (operation.completion.clone(), Vec::new())
                } else {
                    (None, abort_operation_locked(&mut state, id, domain(HostMediaError::OperationCancelled)))
                }
            } else {
                if state.operations.len() >= OPERATION_BUDGET { return Err(exhausted(MediaResource::Operations)); }
                let epoch = state.epoch;
                state.operations.insert(id, Operation::tombstone(id, epoch));
                (None, Vec::new())
            }
        };
        self.dispatch_effects(effects);
        if let Some(completion) = completion { let _ = completion.await; }
        self.operation(id).await
    }

    pub(super) async fn refuse(self: &Arc<Self>, id: MediaIncomingId) -> Result<MediaIncomingResponse> {
        let effects = {
            let mut state = self.lock();
            let incoming = state.incoming.get(&id).ok_or_else(|| domain(HostMediaError::InvalidHandle))?;
            check_incoming(incoming, state.epoch)?;
            let actions = state.group.as_mut().map(|group| group.decline(id, GroupDecline::Refused)).unwrap_or_default();
            let effects = apply_actions_locked(&mut state, actions);
            state.resolve_incoming(id, MediaIncomingResolution::Refused);
            effects
        };
        self.dispatch_effects(effects);
        Ok(MediaIncomingResponse::Refused)
    }

    pub(super) async fn remove(self: &Arc<Self>, request: HostMediaRemoveParticipantRequest) -> Result<()> {
        let effects = {
            let mut state = self.lock();
            let session = state.session(&request.session_id)?;
            let participant = session.participants.get(&request.participant_id).filter(|participant| participant.visible)
                .ok_or_else(|| domain(HostMediaError::InvalidHandle))?;
            if !participant.live() { return Ok(()); }
            end_peer_locked(&mut state, request.session_id, request.participant_id, MediaCallOutcome::LocalEnded)
        };
        self.dispatch_effects(effects);
        Ok(())
    }

    pub(super) async fn end_session(self: &Arc<Self>, request: HostMediaEndSessionRequest) -> Result<()> {
        let effects = {
            let mut state = self.lock();
            if !state.session(&request.session_id)?.live() { return Ok(()); }
            end_session_locked(&mut state, request.session_id, MediaCallOutcome::LocalEnded)
        };
        self.dispatch_effects(effects);
        Ok(())
    }

    pub(super) async fn set_surfaces(self: &Arc<Self>, request: HostMediaSetSurfacesRequest) -> Result<HostMediaSetSurfacesResponse> {
        let started = Instant::now();
        let (completion, sender, epoch) = {
            let mut state = self.lock();
            state.ensure_open()?;
            let session = state.live_session(&request.session_id)?;
            if let Some(previous) = &session.layout {
                if request.layout_revision < previous.layout_revision {
                    return Err(domain(HostMediaError::StaleLayout { current_revision: previous.layout_revision }));
                }
                if request.layout_revision == previous.layout_revision && *previous != request {
                    return Err(domain(HostMediaError::InvalidSurface));
                }
            }
            check_viewport(&state, request.viewport_revision)?;
            if !self.current_authority(&state) { return Err(domain(HostMediaError::NotConnected)); }
            if state.permission_writes != 0 { return Err(CallError::Denied); }
            let capacity = state.capabilities.as_ref().ok_or_else(host_failure)?.max_surfaces_per_session;
            if request.surfaces.len() > usize::from(capacity) { return Err(domain(HostMediaError::InvalidSurface)); }
            let mut keys = BTreeSet::new();
            for surface in &request.surfaces {
                if !keys.insert(surface.surface_id) || !valid_rect(&surface.rect) || !valid_rect(&surface.clip) {
                    return Err(domain(HostMediaError::InvalidSurface));
                }
                if let MediaPictureSource::Remote { participant_id, .. } = &surface.source
                    && !session.participants.get(participant_id).is_some_and(|participant| participant.visible) {
                        return Err(domain(HostMediaError::InvalidHandle));
                    }
            }
            if session.layout.as_ref().is_some_and(|previous| previous.layout_revision == request.layout_revision) {
                (session.layout_completion.clone().ok_or_else(host_failure)?, None, state.epoch)
            } else {
                state.require_listener()?;
                let (sender, receiver) = oneshot::channel();
                let completion = async move { receiver.await.unwrap_or_else(|_| Err(host_failure())) }.boxed().shared();
                let session = state.sessions.get_mut(&request.session_id).ok_or_else(host_failure)?;
                session.layout = Some(request.clone());
                session.layout_completion = Some(completion.clone());
                (completion, Some(sender), state.epoch)
            }
        };
        if let Some(sender) = sender {
            let service = self.clone();
            let mut guard = JobGuard::new(Arc::downgrade(self));
            self.spawn(async move {
                let preflight = {
                    let state = service.lock();
                    if !service.current_authority(&state) { Err(domain(HostMediaError::NotConnected)) }
                    else { check_layout(&state, &request, epoch) }
                };
                let result = match preflight {
                    Err(error) => Err(error),
                    Ok(()) => {
                        let result = service.command(MediaBackendCommand::SetSurfaces {
                            session_id: request.session_id, viewport_revision: request.viewport_revision,
                            layout_revision: request.layout_revision, surfaces: request.surfaces.clone(),
                        }, CancellationToken::default(), (started + OPERATION_TIMEOUT).saturating_duration_since(Instant::now())).await;
                        let state = service.lock();
                        let current = if service.current_authority(&state) { check_layout(&state, &request, epoch) }
                            else { Err(domain(HostMediaError::NotConnected)) };
                        current.and_then(|()| match result {
                            Ok(MediaBackendResponse::Done) => Ok(HostMediaSetSurfacesResponse { layout_revision: request.layout_revision }),
                            Ok(_) => Err(host_failure()),
                            Err(error) => Err(error),
                        })
                    }
                };
                let _ = sender.send(result);
                guard.complete();
            }.boxed());
        }
        completion.await
    }
}

fn check_session_capacity(state: &State, limit: u16) -> Result<()> {
    if state.sessions.values().filter(|session| session.live()).count() >= usize::from(limit) {
        return Err(exhausted(MediaResource::Sessions));
    }
    Ok(())
}

fn check_participant_capacity(session: &Session, limit: u16) -> Result<()> {
    if session.participants.values().filter(|participant| participant.live()).count() >= usize::from(limit) {
        return Err(domain(HostMediaError::CapacityExceeded { limit }));
    }
    Ok(())
}

fn check_incoming(incoming: &state::Incoming, epoch: u64) -> Result<()> {
    if incoming.epoch != epoch || incoming.deadline <= Instant::now()
        || matches!(incoming.phase, IncomingPhase::Resolved(MediaIncomingResolution::Expired | MediaIncomingResolution::Cancelled)) {
        return Err(domain(HostMediaError::IncomingExpired));
    }
    if !matches!(incoming.phase, IncomingPhase::Open) { return Err(domain(HostMediaError::IncomingConsumed)); }
    Ok(())
}

fn clamp_actual(tracks: &MediaLocalTracks, actual: &mut MediaLocalState) {
    if !tracks.microphone { actual.microphone = MediaTrackState::Off; }
    if !tracks.camera { actual.camera = MediaTrackState::Off; actual.camera_kind = None; }
    if !tracks.screen { actual.screen = MediaTrackState::Off; }
}

fn valid_rect(rect: &MediaRect) -> bool {
    i32::try_from(i64::from(rect.x) + i64::from(rect.width)).is_ok()
        && i32::try_from(i64::from(rect.y) + i64::from(rect.height)).is_ok()
}

fn check_viewport(state: &State, revision: u64) -> Result<()> {
    let viewport = state.viewport.as_ref().ok_or_else(|| domain(HostMediaError::SurfaceUnavailable))?;
    if viewport.revision != revision { return Err(domain(HostMediaError::StaleViewport { current_revision: state.viewport_revision })); }
    Ok(())
}

fn check_layout(state: &State, request: &HostMediaSetSurfacesRequest, epoch: u64) -> Result<()> {
    state.ensure_open()?;
    if state.permission_writes != 0 { return Err(CallError::Denied); }
    let session = state.live_session(&request.session_id)?;
    if state.epoch != epoch { return Err(domain(HostMediaError::SessionEnded)); }
    check_viewport(state, request.viewport_revision)?;
    let latest = session.layout.as_ref().ok_or_else(|| domain(HostMediaError::InvalidSurface))?;
    if latest.layout_revision != request.layout_revision {
        return Err(domain(HostMediaError::StaleLayout { current_revision: latest.layout_revision }));
    }
    Ok(())
}
