//! Private V1 group control. The owner serializes this state machine and performs
//! its returned actions; neither transport nor capture is accessible here.

use parity_scale_codec::{Compact, Decode, Encode};
use rand_core::{OsRng, RngCore};
use truapi::v01::{
    HostMediaError, MediaAccount, MediaCallOutcome, MediaIncomingId,
    MediaIncomingResolution, MediaLocalTracks, MediaNetwork, MediaParticipantId,
    MediaPeer, MediaRemoteState, MediaResource, MediaSessionId, MediaTrackState,
};
use crate::platform::{MediaDescription, MediaDescriptionKind, MediaIceCandidate};
use zeroize::{Zeroize, Zeroizing};

use crate::host_logic::media_protocol::{
    MAX_ADVERTISEMENT_BYTES, MAX_PLAINTEXT_BYTES, MediaIdentity,
    VerifiedAdvertisement, decode_advertisement,
};
use crate::runtime::media_signaling::AuthenticatedMediaMessage;

const VERSION: u16 = 1;
const INVITATION_SECONDS: u64 = 60;
const ACCEPTANCE_SECONDS: u64 = 30;
const MAX_SESSIONS: usize = 256;
const MAX_PARTICIPANTS: usize = 2048;
const MAX_INCOMING: usize = 4096;
const MAX_PENDING: usize = 32;
const MAX_ENDPOINTS: usize = 32;
const MAX_PROPOSALS: usize = 32;
const MAX_SDP: usize = 64 * 1024;
const MAX_CANDIDATE: usize = 4096;
const MAX_MID: usize = 256;
const MAX_LINK_BUFFER: usize = 256 * 1024;
const MAX_BUFFER: usize = 1024 * 1024;
const MAX_LINK_MESSAGES: usize = 66;
const MAX_BUFFER_MESSAGES: usize = 256;

type Id = [u8; 32];
type EndpointKey = (Id, Id);

pub(super) struct GroupIncoming {
    pub incoming_id: MediaIncomingId,
    pub peer: MediaPeer,
    pub requested: MediaRemoteState,
    pub expires_at: u64,
    pub existing_session: Option<MediaSessionId>,
}

#[derive(Clone, Copy)]
pub(super) enum GroupDecline {
    Refused,
    Busy,
    Cancelled,
    Expired,
}

// No Debug: actions carry private descriptions, candidates and encrypted-control
// plaintext that must never enter diagnostics or public product events.
pub(super) enum GroupAction {
    Send { recipient: VerifiedAdvertisement, payload: Zeroizing<Vec<u8>> },
    Incoming(GroupIncoming),
    Resolved { incoming_id: MediaIncomingId, resolution: MediaIncomingResolution },
    StartPeer { session_id: MediaSessionId, participant_id: MediaParticipantId, offerer: bool },
    EndPeer { session_id: MediaSessionId, participant_id: MediaParticipantId, outcome: MediaCallOutcome },
    EndUnusedAcceptedSession { session_id: MediaSessionId },
    Description { session_id: MediaSessionId, participant_id: MediaParticipantId, description: MediaDescription },
    IceCandidate { session_id: MediaSessionId, participant_id: MediaParticipantId, candidate: MediaIceCandidate },
}

struct Session {
    id: MediaSessionId,
    group: Id,
    ended: bool,
    intent: MediaRemoteState,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LinkKind {
    Outgoing,
    Incoming,
    Introduced,
}

struct Endpoint {
    advertisement: VerifiedAdvertisement,
    rejected: Option<Reason>,
}

struct Incoming {
    id: MediaIncomingId,
    published: bool,
    resolved: bool,
    new_session: bool,
}

struct Link {
    group: Id,
    id: Id,
    kind: LinkKind,
    session: Option<MediaSessionId>,
    participant: Option<MediaParticipantId>,
    // Retained even when advertisements and queued media are released. A wire
    // link ID is never reused, even with a newly encrypted packet or certificate.
    peer: EndpointKey,
    remote: Option<VerifiedAdvertisement>,
    endpoints: Vec<Endpoint>,
    expires_at: u64,
    incoming: Option<Incoming>,
    proposed: bool,
    local_accepted: bool,
    remote_accepted: bool,
    commit_seen: bool,
    started: bool,
    ended: bool,
    buffered: Vec<Buffered>,
    buffered_bytes: usize,
}

struct Proposal {
    group: Id,
    introduction: Id,
    sender: VerifiedAdvertisement,
    expires_at: u64,
    tracks: MediaRemoteState,
}

struct PendingIntroduction {
    group: Id,
    id: Id,
    source: VerifiedAdvertisement,
    expires_at: u64,
    advertisement: Zeroizing<Vec<u8>>,
}

struct Buffered {
    sender: EndpointKey,
    value: BufferedValue,
}

enum BufferedValue {
    Description(MediaDescription),
    Candidate(MediaIceCandidate),
}

impl BufferedValue {
    fn bytes(&self) -> usize {
        match self {
            Self::Description(value) => value.sdp.len(),
            Self::Candidate(value) => value.candidate.len() + value.mid.as_ref().map_or(0, String::len),
        }
    }

    fn action(&mut self, session_id: MediaSessionId, participant_id: MediaParticipantId) -> GroupAction {
        match self {
            Self::Description(value) => GroupAction::Description {
                session_id, participant_id,
                description: MediaDescription { kind: value.kind, sdp: std::mem::take(&mut value.sdp) },
            },
            Self::Candidate(value) => GroupAction::IceCandidate {
                session_id, participant_id,
                candidate: MediaIceCandidate {
                    candidate: std::mem::take(&mut value.candidate),
                    mid: value.mid.take(), mline_index: value.mline_index,
                },
            },
        }
    }
}

impl Drop for BufferedValue {
    fn drop(&mut self) {
        match self {
            Self::Description(value) => value.sdp.zeroize(),
            Self::Candidate(value) => {
                value.candidate.zeroize();
                if let Some(mid) = &mut value.mid { mid.zeroize(); }
            }
        }
    }
}

pub(super) struct GroupEngine {
    local: VerifiedAdvertisement,
    sessions: Vec<Session>,
    links: Vec<Link>,
    proposals: Vec<Proposal>,
    introductions: Vec<PendingIntroduction>,
    incoming_issued: usize,
    participants_issued: usize,
    buffered_bytes: usize,
    buffered_messages: usize,
    renewal_pending: bool,
    // The transport supplies an effective monotonic UTC clock. Retaining the
    // high-water mark also prevents a caller's backward tick extending offers.
    now: u64,
}

impl GroupEngine {
    pub(super) fn new(local: VerifiedAdvertisement) -> Self {
        Self {
            local, sessions: Vec::new(), links: Vec::new(), proposals: Vec::new(),
            introductions: Vec::new(), incoming_issued: 0, participants_issued: 0,
            buffered_bytes: 0, buffered_messages: 0, renewal_pending: false, now: 0,
        }
    }

    pub(super) fn update_advertisement(&mut self, local: VerifiedAdvertisement) {
        if same_endpoint(&self.local, &local)
            && local.advertisement().fields.issued_at > self.local.advertisement().fields.issued_at
        {
            self.local = local;
            self.renewal_pending = true;
        }
    }

    pub(super) fn create_session(&mut self, session: MediaSessionId) -> Result<(), HostMediaError> {
        if self.sessions.iter().any(|value| value.id == session) {
            return Err(HostMediaError::InvalidState);
        }
        if self.sessions.len() >= MAX_SESSIONS {
            return Err(exhausted(MediaResource::Sessions));
        }
        let group = random_id().ok_or(HostMediaError::NotConnected)?;
        if self.sessions.iter().any(|value| value.group == group)
            || self.links.iter().any(|value| value.group == group)
        {
            return Err(HostMediaError::NotConnected);
        }
        self.sessions.push(Session { id: session, group, ended: false, intent: off() });
        Ok(())
    }

    pub(super) fn invite(
        &mut self, session: MediaSessionId, participant: MediaParticipantId,
        peer: MediaPeer, endpoints: Vec<VerifiedAdvertisement>,
        tracks: &MediaLocalTracks, now: u64,
    ) -> Result<Vec<GroupAction>, HostMediaError> {
        let now = now.max(self.now);
        let session_index = self.session_index(session)?;
        self.check_participant(participant)?;
        let local = &self.local.advertisement().fields;
        if peer.network.genesis_hash != local.network { return Err(HostMediaError::NetworkMismatch); }
        if peer.product_id != local.product_id { return Err(HostMediaError::ProductMismatch); }
        let MediaAccount::Sr25519(account) = peer.account;
        if endpoints.is_empty() { return Err(HostMediaError::NotConnected); }
        if endpoints.len() > MAX_ENDPOINTS { return Err(HostMediaError::InvalidPeer); }
        for (index, endpoint) in endpoints.iter().enumerate() {
            let fields = &endpoint.advertisement().fields;
            if !self.same_route(endpoint) || fields.account != account
                || key(endpoint) == key(&self.local) || fields.expires_at <= now
                || endpoints[..index].iter().any(|previous| key(previous) == key(endpoint))
            {
                return Err(HostMediaError::InvalidPeer);
            }
        }
        if self.links.iter().any(|link| !link.ended && link.session == Some(session)
            && (link.peer.0 == account || link.endpoints.iter().any(|endpoint| key(&endpoint.advertisement).0 == account)))
        {
            // The parent returns the existing participant before invoking us.
            return Err(HostMediaError::InvalidState);
        }
        let crossing: Vec<usize> = self.links.iter().enumerate().filter_map(|(index, link)| {
            (!link.ended && link.kind == LinkKind::Incoming
                && endpoints.iter().any(|endpoint| key(endpoint) == link.peer)).then_some(index)
        }).collect();
        if crossing.iter().any(|index| self.links[*index].local_accepted
            || key(&self.local) > self.links[*index].peer)
        {
            return Err(HostMediaError::InvalidState);
        }
        let id = random_id().ok_or(HostMediaError::NotConnected)?;
        if self.links.iter().any(|link| link.id == id) { return Err(HostMediaError::NotConnected); }
        let expires_at = now.checked_add(INVITATION_SECONDS).ok_or(HostMediaError::InvalidState)?;
        let group = self.sessions[session_index].group;
        let intent = intent(tracks);
        let mut actions = Vec::new();
        // No mutation occurs before every fallible admission check has passed.
        self.now = now;
        for index in crossing {
            self.finish(index, MediaCallOutcome::Busy, MediaIncomingResolution::Refused,
                Some(Reason::Busy), &mut actions);
        }
        for endpoint in &endpoints {
            send(&mut actions, endpoint, &Wire::Invite { group_id: group, link_id: id,
                expires_at, tracks: intent.clone() });
        }
        self.sessions[session_index].intent = intent;
        self.participants_issued += 1;
        self.links.push(Link {
            group, id, kind: LinkKind::Outgoing, session: Some(session), participant: Some(participant),
            peer: (account, [0; 32]), remote: None,
            endpoints: endpoints.into_iter().map(|advertisement| Endpoint { advertisement, rejected: None }).collect(),
            expires_at, incoming: None, proposed: true, local_accepted: true,
            remote_accepted: false, commit_seen: false, started: false, ended: false,
            buffered: Vec::new(), buffered_bytes: 0,
        });
        Ok(actions)
    }

    pub(super) fn accept(
        &mut self, incoming: MediaIncomingId, session: MediaSessionId,
        participant: MediaParticipantId, tracks: &MediaLocalTracks, now: u64,
    ) -> Result<Vec<GroupAction>, HostMediaError> {
        let now = now.max(self.now);
        let index = self.links.iter().position(|link| link.incoming.as_ref().is_some_and(|value| value.id == incoming))
            .ok_or(HostMediaError::InvalidHandle)?;
        let link = &self.links[index];
        if link.ended || now >= link.expires_at { return Err(HostMediaError::IncomingExpired); }
        if link.local_accepted || link.incoming.as_ref().is_none_or(|value| value.resolved || !value.published) {
            return Err(HostMediaError::IncomingConsumed);
        }
        self.check_participant(participant)?;
        let new_session = link.kind == LinkKind::Incoming;
        if new_session {
            if self.sessions.iter().any(|value| value.id == session || (!value.ended && value.group == link.group)) {
                return Err(HostMediaError::InvalidState);
            }
            if self.sessions.len() >= MAX_SESSIONS { return Err(exhausted(MediaResource::Sessions)); }
        } else {
            if link.session != Some(session) { return Err(HostMediaError::InvalidHandle); }
            self.session_index(session)?;
            if self.links.iter().enumerate().any(|(other_index, other)| other_index != index
                && !other.ended && other.session == Some(session) && other.peer == link.peer)
            {
                return Err(HostMediaError::InvalidState);
            }
        }
        let intent = intent(tracks);
        let group = link.group;
        let id = link.id;
        let deadline = now.checked_add(ACCEPTANCE_SECONDS).ok_or(HostMediaError::InvalidState)?;
        // Sending failure is handled by the owner, not turned into an uncommitted
        // operation after the following atomic logical acceptance.
        self.now = now;
        if new_session {
            self.sessions.push(Session { id: session, group, ended: false, intent: intent.clone() });
        } else if let Some(value) = self.sessions.iter_mut().find(|value| value.id == session) {
            value.intent = intent.clone();
        }
        self.participants_issued += 1;
        let link = &mut self.links[index];
        link.session = Some(session);
        link.participant = Some(participant);
        link.local_accepted = true;
        link.expires_at = link.expires_at.min(deadline);
        if let Some(value) = &mut link.incoming { value.new_session = new_session; }
        let mut actions = Vec::new();
        if let Some(remote) = &link.remote {
            send(&mut actions, remote, &Wire::Accept { group_id: group, link_id: id, tracks: intent });
        }
        self.maybe_start(index, &mut actions);
        Ok(actions)
    }

    pub(super) fn decline(&mut self, incoming: MediaIncomingId, reason: GroupDecline) -> Vec<GroupAction> {
        let mut actions = Vec::new();
        if let Some(index) = self.links.iter().position(|link| link.incoming.as_ref().is_some_and(|value| value.id == incoming))
            && !self.links[index].started {
                let (wire, outcome, resolution) = match reason {
                    GroupDecline::Refused => (Reason::Refused, MediaCallOutcome::Refused, MediaIncomingResolution::Refused),
                    GroupDecline::Busy => (Reason::Busy, MediaCallOutcome::Busy, MediaIncomingResolution::Refused),
                    GroupDecline::Cancelled => (Reason::Cancelled, MediaCallOutcome::LocalEnded, MediaIncomingResolution::Cancelled),
                    GroupDecline::Expired => (Reason::Unanswered, MediaCallOutcome::Unanswered, MediaIncomingResolution::Expired),
                };
                self.finish(index, outcome, resolution, Some(wire), &mut actions);
            }
        actions
    }

    pub(super) fn remove(&mut self, session: MediaSessionId, participant: MediaParticipantId) -> Vec<GroupAction> {
        let mut actions = Vec::new();
        if let Some(index) = self.links.iter().position(|link| link.session == Some(session) && link.participant == Some(participant)) {
            self.leave(index, &mut actions);
            self.finish(index, MediaCallOutcome::LocalEnded, MediaIncomingResolution::Cancelled, None, &mut actions);
        }
        actions
    }

    pub(super) fn end_session(&mut self, session: MediaSessionId) -> Vec<GroupAction> {
        let mut actions = Vec::new();
        let Some(value) = self.sessions.iter_mut().find(|value| value.id == session && !value.ended) else { return actions; };
        value.ended = true;
        let group = value.group;
        self.proposals.retain(|proposal| proposal.group != group);
        self.introductions.retain(|pending| pending.group != group);
        for index in 0..self.links.len() {
            if self.links[index].session == Some(session) {
                self.leave(index, &mut actions);
                self.finish(index, MediaCallOutcome::LocalEnded, MediaIncomingResolution::Cancelled, None, &mut actions);
            }
        }
        actions
    }

    pub(super) fn receive(&mut self, message: AuthenticatedMediaMessage, now: u64) -> Vec<GroupAction> {
        self.now = self.now.max(now);
        let now = self.now;
        let mut actions = self.tick(now);
        if message.expires_at <= now || !self.same_route(&message.sender)
            || key(&message.sender) == key(&self.local)
        { return actions; }
        let Some(wire) = decode_wire(&message.plaintext) else { return actions; };
        match wire {
            Wire::Invite { group_id, link_id, expires_at, tracks } => {
                self.receive_invite(message.sender, group_id, link_id, expires_at, tracks, &mut actions);
            }
            Wire::Introduce { group_id, introduction_id, expires_at, peer_advertisement } => {
                self.receive_introduction(message.sender, group_id, introduction_id, expires_at, peer_advertisement, &mut actions);
            }
            Wire::Propose { group_id, introduction_id, expires_at, tracks } => {
                self.receive_proposal(message.sender, group_id, introduction_id, expires_at, tracks, &mut actions);
            }
            Wire::Accept { group_id, link_id, tracks: _ } => {
                if let Some(index) = self.route(&message.sender, group_id, link_id) {
                    self.receive_accept(index, message.sender, &mut actions);
                }
            }
            Wire::Commit { group_id, link_id } => {
                if let Some(index) = self.route(&message.sender, group_id, link_id) {
                    let link = &mut self.links[index];
                    if link.local_accepted && (link.kind == LinkKind::Incoming
                        || (link.kind == LinkKind::Introduced && key(&self.local) > link.peer))
                    {
                        link.commit_seen = true;
                        self.maybe_start(index, &mut actions);
                    }
                }
            }
            Wire::Refresh { group_id, link_id } => {
                // Certificate-only refresh is not a handshake, a new consent,
                // or a timer extension. Unaccepted/terminal links ignore it.
                if self.links.iter().any(|link| !link.ended && link.started
                    && link.group == group_id && link.id == link_id && link.peer == key(&message.sender))
                {
                    let _ = self.route(&message.sender, group_id, link_id);
                }
            }
            Wire::Resolve { group_id, link_id, reason } => {
                if let Some(index) = self.route(&message.sender, group_id, link_id) {
                    self.receive_resolve(index, &message.sender, reason, &mut actions);
                } else {
                    self.remember_early_end(&message.sender, group_id, link_id);
                }
            }
            Wire::Leave { group_id, link_id } => {
                if let Some(index) = self.route(&message.sender, group_id, link_id) {
                    if self.links[index].kind == LinkKind::Outgoing && !self.links[index].started {
                        self.receive_resolve(index, &message.sender, Reason::Cancelled, &mut actions);
                    } else {
                        self.finish(index, MediaCallOutcome::RemoteEnded, MediaIncomingResolution::Cancelled, None, &mut actions);
                    }
                } else {
                    self.remember_early_end(&message.sender, group_id, link_id);
                }
            }
            Wire::Description { group_id, link_id, description } => {
                if let Some(index) = self.route(&message.sender, group_id, link_id)
                    && self.links[index].local_accepted {
                        self.receive_media(index, key(&message.sender), BufferedValue::Description(MediaDescription {
                            kind: description.kind, sdp: description.sdp.to_owned(),
                        }), &mut actions);
                    }
            }
            Wire::IceCandidate { group_id, link_id, candidate } => {
                if let Some(index) = self.route(&message.sender, group_id, link_id)
                    && self.links[index].local_accepted {
                        self.receive_media(index, key(&message.sender), BufferedValue::Candidate(MediaIceCandidate {
                            candidate: candidate.candidate.to_owned(), mid: candidate.mid.map(str::to_owned),
                            mline_index: candidate.mline_index,
                        }), &mut actions);
                    }
            }
        }
        actions
    }

    pub(super) fn local_description(
        &mut self, session: MediaSessionId, participant: MediaParticipantId, mut description: MediaDescription,
    ) -> Vec<GroupAction> {
        let mut actions = Vec::new();
        if description.sdp.len() <= MAX_SDP
            && let Some(link) = self.started_link(session, participant)
                && let Some(remote) = &link.remote {
                    send(&mut actions, remote, &Wire::Description { group_id: link.group, link_id: link.id,
                        description: DescriptionWire { kind: description.kind, sdp: &description.sdp } });
                }
        description.sdp.zeroize();
        actions
    }

    pub(super) fn local_candidate(
        &mut self, session: MediaSessionId, participant: MediaParticipantId, mut candidate: MediaIceCandidate,
    ) -> Vec<GroupAction> {
        let mut actions = Vec::new();
        if candidate.candidate.len() <= MAX_CANDIDATE && candidate.mid.as_ref().is_none_or(|mid| mid.len() <= MAX_MID)
            && let Some(link) = self.started_link(session, participant)
                && let Some(remote) = &link.remote {
                    send(&mut actions, remote, &Wire::IceCandidate { group_id: link.group, link_id: link.id,
                        candidate: CandidateWire { candidate: &candidate.candidate, mid: candidate.mid.as_deref(), mline_index: candidate.mline_index } });
                }
        candidate.candidate.zeroize();
        if let Some(mid) = &mut candidate.mid { mid.zeroize(); }
        actions
    }

    /// Fence queued IO after removal, cancellation, or winner selection. Cleanup
    /// remains deliverable after the corresponding live link became a tombstone.
    pub(super) fn send_current(&self, recipient: &VerifiedAdvertisement, payload: &[u8]) -> bool {
        if !self.same_route(recipient) { return false; }
        let Some(wire) = decode_wire(payload) else { return false; };
        match &wire {
            Wire::Leave { .. } | Wire::Resolve { .. } => return true,
            Wire::Introduce { group_id, expires_at, peer_advertisement, .. } => {
                if *expires_at <= self.now { return false; }
                let Some(introduced) = introduced_advertisement(peer_advertisement, &self.local, self.now) else { return false; };
                let member = |endpoint: &VerifiedAdvertisement| self.links.iter().any(|link|
                    !link.ended && link.started && link.group == *group_id
                        && link.remote.as_ref().is_some_and(|remote| same_endpoint(remote, endpoint)));
                return key(recipient) != key(&introduced) && member(recipient) && member(&introduced);
            }
            _ => {}
        }
        let (group, id) = wire.ids();
        let Some(link) = self.links.iter().find(|link| !link.ended && link.group == group && link.id == id) else { return false; };
        if !link.started && link.expires_at <= self.now { return false; }
        let matches = if link.kind == LinkKind::Outgoing && link.remote.is_none() {
            link.endpoints.iter().any(|endpoint| endpoint.rejected.is_none()
                && same_endpoint(&endpoint.advertisement, recipient))
        } else {
            link.remote.as_ref().is_some_and(|remote| same_endpoint(remote, recipient))
        };
        if !matches { return false; }
        match wire {
            Wire::Invite { .. } => link.kind == LinkKind::Outgoing && !link.started && link.remote.is_none(),
            Wire::Propose { .. } => link.kind == LinkKind::Introduced,
            Wire::Accept { .. } => link.kind != LinkKind::Outgoing && link.local_accepted,
            Wire::Commit { .. } => link.started && (link.kind == LinkKind::Outgoing
                || (link.kind == LinkKind::Introduced && key(&self.local) < link.peer)),
            Wire::Description { .. } | Wire::IceCandidate { .. } | Wire::Refresh { .. } => link.started,
            Wire::Introduce { .. } | Wire::Leave { .. } | Wire::Resolve { .. } => false,
        }
    }

    /// Transport failure is scoped to the actual outbound link, not the peer's
    /// other sessions or an introduced edge owned by two different endpoints.
    pub(super) fn send_failed(&mut self, recipient: &VerifiedAdvertisement, payload: &[u8]) -> Vec<GroupAction> {
        let mut actions = Vec::new();
        let Some(wire) = decode_wire(payload) else { return actions; };
        if matches!(wire, Wire::Introduce { .. } | Wire::Refresh { .. } | Wire::Leave { .. } | Wire::Resolve { .. })
            || !self.send_current(recipient, payload)
        { return actions; }
        let (group, id) = wire.ids();
        let Some(index) = self.links.iter().position(|link| !link.ended && link.group == group && link.id == id) else { return actions; };
        if matches!(wire, Wire::Invite { .. }) {
            let link = &mut self.links[index];
            if let Some(endpoint) = link.endpoints.iter_mut().find(|endpoint| same_endpoint(&endpoint.advertisement, recipient)) {
                // Reuse terminal endpoint accounting without manufacturing a
                // wire response or consuming a new incoming/resource quota.
                endpoint.rejected = Some(Reason::Cancelled);
            }
            if link.endpoints.iter().any(|endpoint| endpoint.rejected.is_none()) { return actions; }
        } else {
            self.leave(index, &mut actions);
        }
        self.finish(index, MediaCallOutcome::ConnectivityLost, MediaIncomingResolution::Cancelled, None, &mut actions);
        actions
    }

    pub(super) fn tick(&mut self, now: u64) -> Vec<GroupAction> {
        self.now = self.now.max(now);
        let mut actions = Vec::new();
        self.proposals.retain(|proposal| proposal.expires_at > self.now);
        self.introductions.retain(|pending| pending.expires_at > self.now);
        for index in 0..self.links.len() {
            let link = &self.links[index];
            if !link.ended && !link.started && link.expires_at <= self.now {
                self.finish(index, MediaCallOutcome::Unanswered, MediaIncomingResolution::Expired,
                    Some(Reason::Unanswered), &mut actions);
            }
        }
        if self.renewal_pending {
            self.renewal_pending = false;
            for link in &self.links {
                if !link.ended && link.started
                    && let Some(remote) = &link.remote {
                        send(&mut actions, remote, &Wire::Refresh { group_id: link.group, link_id: link.id });
                    }
            }
        }
        actions
    }

    fn session_index(&self, session: MediaSessionId) -> Result<usize, HostMediaError> {
        let index = self.sessions.iter().position(|value| value.id == session).ok_or(HostMediaError::InvalidHandle)?;
        if self.sessions[index].ended { return Err(HostMediaError::SessionEnded); }
        Ok(index)
    }

    fn check_participant(&self, participant: MediaParticipantId) -> Result<(), HostMediaError> {
        if self.links.iter().any(|link| link.participant == Some(participant)) { return Err(HostMediaError::InvalidState); }
        if self.participants_issued >= MAX_PARTICIPANTS { return Err(exhausted(MediaResource::Participants)); }
        Ok(())
    }

    fn same_route(&self, remote: &VerifiedAdvertisement) -> bool {
        let local = &self.local.advertisement().fields;
        let remote = &remote.advertisement().fields;
        local.network == remote.network && local.product_id == remote.product_id
    }

    fn pending(&self) -> usize {
        self.links.iter().filter(|link| !link.ended && link.incoming.as_ref().is_some_and(|incoming| !incoming.resolved)).count()
    }

    fn incoming_id(&self) -> Option<Id> {
        if self.incoming_issued >= MAX_INCOMING || self.pending() >= MAX_PENDING { return None; }
        let id = random_id()?;
        (!self.links.iter().any(|link| link.incoming.as_ref().is_some_and(|incoming| incoming.id == id))).then_some(id)
    }

    fn route(&mut self, sender: &VerifiedAdvertisement, group: Id, id: Id) -> Option<usize> {
        let sender_key = key(sender);
        let index = self.links.iter().position(|link| !link.ended && link.group == group && link.id == id
            && (if link.kind == LinkKind::Outgoing && link.remote.is_none() {
                link.endpoints.iter().any(|endpoint| endpoint.rejected.is_none() && same_endpoint(&endpoint.advertisement, sender))
            } else {
                link.peer == sender_key && link.remote.as_ref().is_some_and(|remote| same_endpoint(remote, sender))
            }))?;
        let link = &mut self.links[index];
        refresh(&mut link.remote, sender);
        for endpoint in &mut link.endpoints {
            if same_endpoint(&endpoint.advertisement, sender)
                && sender.advertisement().fields.issued_at > endpoint.advertisement.advertisement().fields.issued_at
            {
                endpoint.advertisement = sender.clone();
            }
        }
        Some(index)
    }

    fn started_link(&self, session: MediaSessionId, participant: MediaParticipantId) -> Option<&Link> {
        self.links.iter().find(|link| !link.ended && link.started
            && link.session == Some(session) && link.participant == Some(participant))
    }

    fn known_link(&self, group: Id, id: Id) -> bool {
        self.links.iter().any(|link| link.group == group && link.id == id)
    }

    fn receive_invite(
        &mut self, sender: VerifiedAdvertisement, group: Id, id: Id, expires_at: u64,
        tracks: MediaRemoteState, actions: &mut Vec<GroupAction>,
    ) {
        if !valid_expiry(expires_at, self.now) || self.known_link(group, id) { return; }
        let Some(incoming_id) = self.incoming_id() else { return; };
        // Ordinary invitations never merge an existing local group/session.
        let duplicate_group = self.sessions.iter().any(|session| !session.ended && session.group == group);
        let crossing: Vec<usize> = self.links.iter().enumerate().filter_map(|(index, link)| {
            (!link.ended && !link.started && link.kind == LinkKind::Outgoing
                && link.endpoints.iter().any(|endpoint| same_endpoint(&endpoint.advertisement, &sender))).then_some(index)
        }).collect();
        let keep_outgoing = !crossing.is_empty() && key(&self.local) < key(&sender);
        let duplicate_peer = self.links.iter().any(|link| !link.ended && link.group == group
            && link.peer == key(&sender) && link.kind == LinkKind::Incoming);
        let reject = duplicate_group || keep_outgoing || duplicate_peer;
        if !reject {
            for index in crossing {
                self.leave(index, actions);
                self.finish(index, MediaCallOutcome::Busy, MediaIncomingResolution::Cancelled, None, actions);
            }
        }
        let peer = peer(&sender);
        let index = self.links.len();
        self.incoming_issued += 1;
        self.links.push(Link {
            group, id, kind: LinkKind::Incoming, session: None, participant: None,
            peer: key(&sender), remote: Some(sender), endpoints: Vec::new(), expires_at,
            incoming: Some(Incoming { id: incoming_id, published: !reject, resolved: false, new_session: false }),
            proposed: true, local_accepted: false, remote_accepted: true,
            commit_seen: false, started: false, ended: false, buffered: Vec::new(), buffered_bytes: 0,
        });
        if reject {
            self.finish(index, MediaCallOutcome::Busy, MediaIncomingResolution::Refused, Some(Reason::Busy), actions);
        } else {
            actions.push(GroupAction::Incoming(GroupIncoming {
                incoming_id, peer, requested: tracks, expires_at, existing_session: None,
            }));
        }
    }

    fn receive_introduction(
        &mut self, sender: VerifiedAdvertisement, group: Id, id: Id, expires_at: u64,
        advertisement: &[u8], actions: &mut Vec<GroupAction>,
    ) {
        if !valid_expiry(expires_at, self.now) || self.known_link(group, id) { return; }
        let Some(source) = self.links.iter().find(|link| !link.ended && link.local_accepted && link.group == group
            && link.remote.as_ref().is_some_and(|remote| same_endpoint(remote, &sender))) else { return; };
        if !source.started {
            // Commit and Introduce can cross in transport. Retain only bytes
            // from a locally accepted exact endpoint; the introduction is not
            // authorized or exposed until that source's winning handshake.
            if self.introductions.len() < MAX_PENDING
                && !self.introductions.iter().any(|pending| pending.group == group && pending.id == id
                    && key(&pending.source) == key(&sender))
            {
                self.introductions.push(PendingIntroduction {
                    group, id, source: sender, expires_at,
                    advertisement: Zeroizing::new(advertisement.to_vec()),
                });
            }
            return;
        }
        let Some(session) = source.session else { return; };
        let Ok(session_index) = self.session_index(session) else { return; };
        let Some(remote) = introduced_advertisement(advertisement, &self.local, self.now) else { return; };
        if key(&remote) == key(&self.local) || key(&remote) == key(&sender) { return; }
        // Several accepted members can introduce the same edge concurrently.
        // The lower endpoint chooses its first certified introduction. The
        // higher endpoint retains bounded alternatives until the lower's
        // matching Propose selects one; independent arrival order cannot split
        // the endpoints between two different link IDs.
        if self.links.iter().any(|link| !link.ended && link.session == Some(session) && link.peer == key(&remote)
            && (key(&self.local) < key(&remote) || link.kind != LinkKind::Introduced
                || link.proposed || link.local_accepted || link.started))
        { return; }
        let Some(incoming_id) = self.incoming_id() else { return; };
        let intent = self.sessions[session_index].intent.clone();
        let index = self.links.len();
        self.incoming_issued += 1;
        // Reserve the incoming tombstone on introduction admission, before an
        // out-of-order proposal can create effects. No public offer exists yet.
        self.links.push(Link {
            group, id, kind: LinkKind::Introduced, session: Some(session), participant: None,
            peer: key(&remote), remote: Some(remote), endpoints: Vec::new(), expires_at,
            incoming: Some(Incoming { id: incoming_id, published: false, resolved: false, new_session: false }),
            proposed: false, local_accepted: false, remote_accepted: false,
            commit_seen: false, started: false, ended: false, buffered: Vec::new(), buffered_bytes: 0,
        });
        if let Some(remote) = &self.links[index].remote {
            send(actions, remote, &Wire::Propose { group_id: group, introduction_id: id, expires_at, tracks: intent });
        }
        if let Some(position) = self.proposals.iter().position(|proposal| proposal.group == group
            && proposal.introduction == id && key(&proposal.sender) == self.links[index].peer)
        {
            let proposal = self.proposals.swap_remove(position);
            self.publish_proposal(index, proposal.sender, proposal.expires_at, proposal.tracks, actions);
        }
    }

    fn receive_proposal(
        &mut self, sender: VerifiedAdvertisement, group: Id, id: Id, expires_at: u64,
        tracks: MediaRemoteState, actions: &mut Vec<GroupAction>,
    ) {
        if !valid_expiry(expires_at, self.now) { return; }
        if let Some(index) = self.route(&sender, group, id) {
            self.publish_proposal(index, sender, expires_at, tracks, actions);
            return;
        }
        if self.known_link(group, id)
            || !self.sessions.iter().any(|session| !session.ended && session.group == group)
        { return; }
        // Knowing a group ID is not an introduction. This cache has no incoming
        // ID, peer authorization, backend work, or response carrying group data.
        if self.proposals.iter().any(|proposal| proposal.group == group && proposal.introduction == id
            && key(&proposal.sender) == key(&sender)) { return; }
        if self.proposals.len() >= MAX_PROPOSALS { return; }
        self.proposals.push(Proposal { group, introduction: id, sender, expires_at, tracks });
    }

    fn publish_proposal(
        &mut self, index: usize, sender: VerifiedAdvertisement, expires_at: u64,
        tracks: MediaRemoteState, actions: &mut Vec<GroupAction>,
    ) {
        let link = &mut self.links[index];
        if link.ended || link.kind != LinkKind::Introduced || link.proposed
            || !link.remote.as_ref().is_some_and(|remote| same_endpoint(remote, &sender))
        { return; }
        link.proposed = true;
        link.expires_at = link.expires_at.min(expires_at);
        refresh(&mut link.remote, &sender);
        let Some(incoming) = &mut link.incoming else { return; };
        incoming.published = true;
        if let Some(remote) = &link.remote {
            actions.push(GroupAction::Incoming(GroupIncoming {
                incoming_id: incoming.id, peer: peer(remote), requested: tracks,
                expires_at: link.expires_at, existing_session: link.session,
            }));
        }
        let group = link.group;
        let selected_peer = link.peer;
        for other in 0..self.links.len() {
            if other != index && self.links[other].group == group && self.links[other].peer == selected_peer
                && self.links[other].kind == LinkKind::Introduced && !self.links[other].proposed
            {
                self.finish(other, MediaCallOutcome::Busy, MediaIncomingResolution::Cancelled, None, actions);
            }
        }
    }

    fn receive_accept(&mut self, index: usize, sender: VerifiedAdvertisement, actions: &mut Vec<GroupAction>) {
        let link = &mut self.links[index];
        if link.started || link.remote_accepted && link.kind != LinkKind::Outgoing { return; }
        if link.kind == LinkKind::Outgoing {
            if link.remote.is_some() { return; }
            // This serialized assignment is the first-endpoint winning commit.
            link.peer = key(&sender);
            link.remote = Some(sender);
            link.remote_accepted = true;
            for endpoint in &link.endpoints {
                if key(&endpoint.advertisement) != link.peer {
                    send(actions, &endpoint.advertisement, &Wire::Resolve {
                        group_id: link.group, link_id: link.id, reason: Reason::Cancelled,
                    });
                }
            }
            link.endpoints.clear();
        } else if link.kind == LinkKind::Introduced {
            link.remote_accepted = true;
        refresh(&mut link.remote, &sender);
        } else {
            return;
        }
        self.maybe_start(index, actions);
    }

    fn maybe_start(&mut self, index: usize, actions: &mut Vec<GroupAction>) {
        let link = &self.links[index];
        if link.ended || link.started || !link.local_accepted || !link.remote_accepted { return; }
        let coordinator = link.kind == LinkKind::Outgoing
            || (link.kind == LinkKind::Introduced && key(&self.local) < link.peer);
        if !coordinator && !link.commit_seen { return; }
        let (Some(session), Some(participant), Some(remote)) = (link.session, link.participant, link.remote.as_ref()) else { return; };
        let group = link.group;
        // Generate all introductions before changing handshake state. Entropy
        // failure terminates the accepted identity rather than using predictable
        // IDs or claiming that a partial mesh was successfully established.
        let mut introductions = Vec::new();
        for other in &self.links {
            if other.ended || !other.started || other.session != Some(session) || other.peer == link.peer { continue; }
            let Some(advertisement) = &other.remote else { continue; };
            let Some(id) = random_id() else {
                self.leave(index, actions);
                self.finish(index, MediaCallOutcome::HostFailed, MediaIncomingResolution::Cancelled, None, actions);
                return;
            };
            introductions.push((id, advertisement.clone()));
        }
        let remote = remote.clone();
        let link = &mut self.links[index];
        if coordinator {
            send(actions, &remote, &Wire::Commit { group_id: group, link_id: link.id });
        }
        link.started = true;
        if let Some(incoming) = &mut link.incoming
            && incoming.published && !incoming.resolved {
                incoming.resolved = true;
                actions.push(GroupAction::Resolved { incoming_id: incoming.id, resolution: MediaIncomingResolution::Accepted });
            }
        actions.push(GroupAction::StartPeer { session_id: session, participant_id: participant, offerer: coordinator });
        self.buffered_bytes -= link.buffered_bytes;
        self.buffered_messages -= link.buffered.len();
        link.buffered_bytes = 0;
        for mut buffered in link.buffered.drain(..) {
            if buffered.sender == link.peer { actions.push(buffered.value.action(session, participant)); }
        }
        let expires_at = self.now.saturating_add(INVITATION_SECONDS);
        let remote_bytes = Zeroizing::new(remote.advertisement().encode());
        for (id, other) in introductions {
            let other_bytes = Zeroizing::new(other.advertisement().encode());
            send(actions, &remote, &Wire::Introduce { group_id: group, introduction_id: id, expires_at, peer_advertisement: &other_bytes });
            send(actions, &other, &Wire::Introduce { group_id: group, introduction_id: id, expires_at, peer_advertisement: &remote_bytes });
        }
        while let Some(position) = self.introductions.iter().position(|pending| pending.group == group
            && same_endpoint(&pending.source, &remote))
        {
            let pending = self.introductions.swap_remove(position);
            self.receive_introduction(pending.source, pending.group, pending.id, pending.expires_at,
                &pending.advertisement, actions);
        }
    }

    fn receive_resolve(&mut self, index: usize, sender: &VerifiedAdvertisement, reason: Reason, actions: &mut Vec<GroupAction>) {
        let link = &mut self.links[index];
        if link.started { return; }
        if link.kind == LinkKind::Outgoing {
            let Some(endpoint) = link.endpoints.iter_mut().find(|endpoint| same_endpoint(&endpoint.advertisement, sender)) else { return; };
            endpoint.rejected = Some(reason);
            if link.endpoints.iter().any(|endpoint| endpoint.rejected.is_none()) { return; }
            let outcome = if link.endpoints.iter().any(|endpoint| endpoint.rejected == Some(Reason::Refused)) {
                MediaCallOutcome::Refused
            } else if link.endpoints.iter().any(|endpoint| endpoint.rejected == Some(Reason::Busy)) {
                MediaCallOutcome::Busy
            } else if link.endpoints.iter().any(|endpoint| endpoint.rejected == Some(Reason::Unanswered)) {
                MediaCallOutcome::Unanswered
            } else { MediaCallOutcome::RemoteEnded };
            self.finish(index, outcome, MediaIncomingResolution::Cancelled, None, actions);
        } else {
            let resolution = match reason {
                Reason::Cancelled if link.kind == LinkKind::Incoming => MediaIncomingResolution::AnsweredElsewhere,
                Reason::Unanswered => MediaIncomingResolution::Expired,
                Reason::Refused | Reason::Busy => MediaIncomingResolution::Refused,
                Reason::Cancelled => MediaIncomingResolution::Cancelled,
            };
            self.finish(index, reason.outcome(), resolution, None, actions);
        }
    }

    fn receive_media(&mut self, index: usize, sender: EndpointKey, mut value: BufferedValue, actions: &mut Vec<GroupAction>) {
        let link = &mut self.links[index];
        if link.started {
            if sender == link.peer
                && let (Some(session), Some(participant)) = (link.session, link.participant) {
                    actions.push(value.action(session, participant));
                }
            return;
        }
        let bytes = value.bytes();
        if link.buffered.len() >= MAX_LINK_MESSAGES || self.buffered_messages >= MAX_BUFFER_MESSAGES
            || link.buffered_bytes + bytes > MAX_LINK_BUFFER || self.buffered_bytes + bytes > MAX_BUFFER
        {
            self.leave(index, actions);
            self.finish(index, MediaCallOutcome::HostFailed, MediaIncomingResolution::Cancelled, None, actions);
            return;
        }
        link.buffered_bytes += bytes;
        self.buffered_bytes += bytes;
        self.buffered_messages += 1;
        link.buffered.push(Buffered { sender, value });
    }

    fn leave(&self, index: usize, actions: &mut Vec<GroupAction>) {
        let link = &self.links[index];
        if link.ended { return; }
        let wire = Wire::Leave { group_id: link.group, link_id: link.id };
        if let Some(remote) = &link.remote { send(actions, remote, &wire); }
        for endpoint in &link.endpoints {
            if endpoint.rejected.is_none() { send(actions, &endpoint.advertisement, &wire); }
        }
    }

    fn finish(
        &mut self, index: usize, outcome: MediaCallOutcome, resolution: MediaIncomingResolution,
        reason: Option<Reason>, actions: &mut Vec<GroupAction>,
    ) {
        let link = &mut self.links[index];
        if link.ended { return; }
        if let Some(reason) = reason {
            let wire = Wire::Resolve { group_id: link.group, link_id: link.id, reason };
            if let Some(remote) = &link.remote { send(actions, remote, &wire); }
            for endpoint in &link.endpoints {
                if endpoint.rejected.is_none() { send(actions, &endpoint.advertisement, &wire); }
            }
        }
        link.ended = true;
        if let Some(incoming) = &mut link.incoming
            && incoming.published && !incoming.resolved {
                incoming.resolved = true;
                actions.push(GroupAction::Resolved { incoming_id: incoming.id, resolution });
            }
        if let (Some(session_id), Some(participant_id)) = (link.session, link.participant) {
            actions.push(GroupAction::EndPeer { session_id, participant_id, outcome });
            if !link.started && resolution == MediaIncomingResolution::AnsweredElsewhere
                && link.incoming.as_ref().is_some_and(|incoming| incoming.new_session)
            {
                actions.push(GroupAction::EndUnusedAcceptedSession { session_id });
            }
        }
        self.buffered_bytes -= link.buffered_bytes;
        self.buffered_messages -= link.buffered.len();
        link.buffered_bytes = 0;
        link.buffered = Vec::new();
        link.remote = None;
        link.endpoints = Vec::new();
        self.proposals.retain(|proposal| proposal.group != link.group || proposal.introduction != link.id);
        self.introductions.retain(|pending| pending.group != link.group || key(&pending.source) != link.peer);
    }

    fn remember_early_end(&mut self, sender: &VerifiedAdvertisement, group: Id, id: Id) {
        // A cancellation can overtake its invitation/introduction. Reserve a
        // permanent, quota-counted tombstone, never a live peer or public offer.
        // Existing links (including other endpoints' links) cannot be overwritten.
        if self.known_link(group, id) || self.incoming_issued >= MAX_INCOMING { return; }
        self.incoming_issued += 1;
        self.links.push(Link {
            group, id, kind: LinkKind::Incoming, session: None, participant: None,
            peer: key(sender), remote: None, endpoints: Vec::new(), expires_at: self.now,
            incoming: None, proposed: false, local_accepted: false, remote_accepted: false,
            commit_seen: false, started: false, ended: true, buffered: Vec::new(), buffered_bytes: 0,
        });
    }
}

fn key(advertisement: &VerifiedAdvertisement) -> EndpointKey {
    let fields = &advertisement.advertisement().fields;
    (fields.account, fields.endpoint_id)
}

fn same_endpoint(left: &VerifiedAdvertisement, right: &VerifiedAdvertisement) -> bool {
    let left = &left.advertisement().fields;
    let right = &right.advertisement().fields;
    left.network == right.network && left.product_id == right.product_id
        && left.account == right.account && left.endpoint_id == right.endpoint_id
        && left.encryption_key == right.encryption_key && left.signing_key == right.signing_key
}

fn refresh(current: &mut Option<VerifiedAdvertisement>, next: &VerifiedAdvertisement) {
    if current.as_ref().is_some_and(|current| same_endpoint(current, next)
        && next.advertisement().fields.issued_at > current.advertisement().fields.issued_at)
    {
        *current = Some(next.clone());
    }
}

fn peer(advertisement: &VerifiedAdvertisement) -> MediaPeer {
    let fields = &advertisement.advertisement().fields;
    MediaPeer { network: MediaNetwork { genesis_hash: fields.network },
        product_id: fields.product_id.clone(), account: MediaAccount::Sr25519(fields.account) }
}

fn exhausted(resource: MediaResource) -> HostMediaError { HostMediaError::ResourceExhausted { resource } }

fn random_id() -> Option<Id> {
    let mut id = [0; 32];
    OsRng.try_fill_bytes(&mut id).ok()?;
    (id != [0; 32]).then_some(id)
}

fn valid_expiry(expires_at: u64, now: u64) -> bool {
    expires_at > now && expires_at - now <= INVITATION_SECONDS
}

fn intent(tracks: &MediaLocalTracks) -> MediaRemoteState {
    fn state(on: bool) -> MediaTrackState { if on { MediaTrackState::Starting } else { MediaTrackState::Off } }
    MediaRemoteState { microphone: state(tracks.microphone), camera: state(tracks.camera), screen: state(tracks.screen) }
}

fn off() -> MediaRemoteState {
    MediaRemoteState { microphone: MediaTrackState::Off, camera: MediaTrackState::Off, screen: MediaTrackState::Off }
}

#[derive(Clone, Copy, PartialEq, Eq, Encode, Decode)]
enum Reason { Refused, Busy, Cancelled, Unanswered }

impl Reason {
    fn outcome(self) -> MediaCallOutcome {
        match self {
            Self::Refused => MediaCallOutcome::Refused,
            Self::Busy => MediaCallOutcome::Busy,
            Self::Cancelled => MediaCallOutcome::RemoteEnded,
            Self::Unanswered => MediaCallOutcome::Unanswered,
        }
    }
}

#[derive(Encode)]
struct DescriptionWire<'a> { kind: MediaDescriptionKind, sdp: &'a str }

#[derive(Encode)]
struct CandidateWire<'a> { candidate: &'a str, mid: Option<&'a str>, mline_index: Option<u16> }

// Variant order and field order are the frozen private V1 SCALE profile.
// Borrowed length-bearing fields ensure routing rejects unknown peers without
// allocating descriptions, candidates, or embedded certificates.
#[derive(Encode)]
enum Wire<'a> {
    Invite { group_id: Id, link_id: Id, expires_at: u64, tracks: MediaRemoteState },
    Accept { group_id: Id, link_id: Id, tracks: MediaRemoteState },
    Commit { group_id: Id, link_id: Id },
    Resolve { group_id: Id, link_id: Id, reason: Reason },
    Introduce { group_id: Id, introduction_id: Id, expires_at: u64, peer_advertisement: &'a [u8] },
    Propose { group_id: Id, introduction_id: Id, expires_at: u64, tracks: MediaRemoteState },
    Description { group_id: Id, link_id: Id, description: DescriptionWire<'a> },
    IceCandidate { group_id: Id, link_id: Id, candidate: CandidateWire<'a> },
    Leave { group_id: Id, link_id: Id },
    Refresh { group_id: Id, link_id: Id },
}

impl Wire<'_> {
    fn ids(&self) -> (Id, Id) {
        match self {
            Self::Invite { group_id, link_id, .. } | Self::Accept { group_id, link_id, .. }
            | Self::Commit { group_id, link_id } | Self::Resolve { group_id, link_id, .. }
            | Self::Description { group_id, link_id, .. } | Self::IceCandidate { group_id, link_id, .. }
            | Self::Leave { group_id, link_id } | Self::Refresh { group_id, link_id } => (*group_id, *link_id),
            Self::Introduce { group_id, introduction_id, .. } | Self::Propose { group_id, introduction_id, .. }
                => (*group_id, *introduction_id),
        }
    }
}

fn send(actions: &mut Vec<GroupAction>, recipient: &VerifiedAdvertisement, wire: &Wire<'_>) {
    let mut payload = Zeroizing::new(Vec::with_capacity(2 + wire.encoded_size()));
    VERSION.encode_to(&mut *payload);
    wire.encode_to(&mut *payload);
    actions.push(GroupAction::Send { recipient: recipient.clone(), payload });
}

fn decode<T: Decode>(input: &mut &[u8]) -> Option<T> { T::decode(input).ok() }

fn bounded_bytes<'a>(input: &mut &'a [u8], limit: usize) -> Option<&'a [u8]> {
    let length = decode::<Compact<u32>>(input)?.0 as usize;
    if length > limit || length > input.len() { return None; }
    let (value, remaining) = input.split_at(length);
    *input = remaining;
    Some(value)
}

fn bounded_string<'a>(input: &mut &'a [u8], limit: usize) -> Option<&'a str> {
    std::str::from_utf8(bounded_bytes(input, limit)?).ok()
}

fn decode_intent(input: &mut &[u8]) -> Option<MediaRemoteState> {
    fn track(input: &mut &[u8]) -> Option<MediaTrackState> {
        match decode::<u8>(input)? { 0 => Some(MediaTrackState::Off), 1 => Some(MediaTrackState::Starting), _ => None }
    }
    Some(MediaRemoteState { microphone: track(input)?, camera: track(input)?, screen: track(input)? })
}

fn decode_wire(bytes: &[u8]) -> Option<Wire<'_>> {
    if bytes.len() > MAX_PLAINTEXT_BYTES { return None; }
    let mut input = bytes;
    if decode::<u16>(&mut input)? != VERSION { return None; }
    let ordinal = decode::<u8>(&mut input)?;
    let group_id: Id = decode(&mut input)?;
    let id: Id = decode(&mut input)?;
    if group_id == [0; 32] || id == [0; 32] { return None; }
    let wire = match ordinal {
        0 => Wire::Invite { group_id, link_id: id, expires_at: decode(&mut input)?, tracks: decode_intent(&mut input)? },
        1 => Wire::Accept { group_id, link_id: id, tracks: decode_intent(&mut input)? },
        2 => Wire::Commit { group_id, link_id: id },
        3 => Wire::Resolve { group_id, link_id: id, reason: decode(&mut input)? },
        4 => Wire::Introduce { group_id, introduction_id: id, expires_at: decode(&mut input)?,
            peer_advertisement: bounded_bytes(&mut input, MAX_ADVERTISEMENT_BYTES)? },
        5 => Wire::Propose { group_id, introduction_id: id, expires_at: decode(&mut input)?, tracks: decode_intent(&mut input)? },
        6 => Wire::Description { group_id, link_id: id, description: DescriptionWire {
            kind: decode(&mut input)?, sdp: bounded_string(&mut input, MAX_SDP)?,
        } },
        7 => {
            let candidate = bounded_string(&mut input, MAX_CANDIDATE)?;
            let mid = match decode::<u8>(&mut input)? {
                0 => None, 1 => Some(bounded_string(&mut input, MAX_MID)?), _ => return None,
            };
            let mline_index = decode::<Option<u16>>(&mut input)?;
            Wire::IceCandidate { group_id, link_id: id, candidate: CandidateWire { candidate, mid, mline_index } }
        }
        8 => Wire::Leave { group_id, link_id: id },
        9 => Wire::Refresh { group_id, link_id: id },
        _ => return None,
    };
    input.is_empty().then_some(wire)
}

fn introduced_advertisement(bytes: &[u8], local: &VerifiedAdvertisement, now: u64) -> Option<VerifiedAdvertisement> {
    // Read only the bounded identity prefix to construct an exact verification
    // target. The normal certificate verifier then checks every field, canonical
    // product identity, account signature, key, time bound and trailing byte.
    if bytes.len() > MAX_ADVERTISEMENT_BYTES { return None; }
    let mut input = bytes;
    if decode::<u16>(&mut input)? != VERSION { return None; }
    let network: Id = decode(&mut input)?;
    let product = bounded_string(&mut input, 255)?;
    let account: Id = decode(&mut input)?;
    let fields = &local.advertisement().fields;
    if network != fields.network || product != fields.product_id { return None; }
    let expected = MediaIdentity { network, product_id: fields.product_id.clone(), account };
    decode_advertisement(bytes, &expected, now).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host_logic::media_protocol::{ACCOUNT_SIGNING_CONTEXT, UnsignedAdvertisement};
    use hpke::{Kem, Serializable, kem::X25519HkdfSha256};
    use schnorrkel::{ExpansionMode, MiniSecretKey};

    const NOW: u64 = 1_800_000_000;

    fn advertisement(account_seed: u8, endpoint_seed: u8) -> VerifiedAdvertisement {
        let account = MiniSecretKey::from_bytes(&[account_seed; 32]).unwrap()
            .expand_to_keypair(ExpansionMode::Ed25519);
        let signing = MiniSecretKey::from_bytes(&[endpoint_seed; 32]).unwrap()
            .expand_to_keypair(ExpansionMode::Ed25519);
        let (_, encryption) = X25519HkdfSha256::derive_keypair(&[endpoint_seed; 32]);
        let fields = UnsignedAdvertisement {
            version: 1, network: [7; 32], product_id: "vox.dot".into(),
            account: account.public.to_bytes(), endpoint_id: [endpoint_seed; 32],
            encryption_key: encryption.to_bytes().into(), signing_key: signing.public.to_bytes(),
            issued_at: NOW, expires_at: NOW + 600,
        };
        let signature = account.sign_simple(ACCOUNT_SIGNING_CONTEXT, &fields.account_signing_input()).to_bytes();
        fields.authenticate(signature, NOW).unwrap()
    }

    fn tracks() -> MediaLocalTracks {
        MediaLocalTracks { microphone: false, camera: false, screen: false,
            camera_preference: None, audio_preference: None }
    }

    // This is the already-authenticated group boundary, not a replacement for
    // the transport/HPKE tests. Only messages addressed to this endpoint arrive.
    fn relay(sender: &VerifiedAdvertisement, actions: &[GroupAction], receiver: &mut GroupEngine) -> Vec<GroupAction> {
        let mut output = Vec::new();
        for action in actions {
            if let GroupAction::Send { recipient, payload } = action
                && key(recipient) == key(&receiver.local) {
                    output.extend(receiver.receive(AuthenticatedMediaMessage {
                        sender: sender.clone(), expires_at: NOW + 60,
                        plaintext: payload.clone(),
                    }, NOW));
                }
        }
        output
    }

    fn incoming(actions: &[GroupAction]) -> MediaIncomingId {
        actions.iter().find_map(|action| match action {
            GroupAction::Incoming(offer) => Some(offer.incoming_id),
            _ => None,
        }).expect("an authenticated incoming offer")
    }

    fn starts(actions: &[GroupAction], session: Id, participant: Id, offerer: bool) -> bool {
        actions.iter().any(|action| matches!(action, GroupAction::StartPeer {
            session_id, participant_id, offerer: actual,
        } if *session_id == session && *participant_id == participant && *actual == offerer))
    }

    fn no_start(actions: &[GroupAction]) {
        assert!(!actions.iter().any(|action| matches!(action, GroupAction::StartPeer { .. })));
    }

    #[test]
    fn only_first_answering_endpoint_receives_commit_and_media() {
        let a = advertisement(1, 11);
        let b1 = advertisement(2, 21);
        let b2 = advertisement(2, 22);
        let mut caller = GroupEngine::new(a.clone());
        let mut first_device = GroupEngine::new(b1.clone());
        let mut second_device = GroupEngine::new(b2.clone());
        caller.create_session([10; 32]).unwrap();
        let invitations = caller.invite([10; 32], [11; 32], peer(&b1),
            vec![b1.clone(), b2.clone()], &tracks(), NOW).unwrap();
        let first_offer = incoming(&relay(&a, &invitations, &mut first_device));
        let second_offer = incoming(&relay(&a, &invitations, &mut second_device));
        let first_answer = first_device.accept(first_offer, [20; 32], [21; 32], &tracks(), NOW).unwrap();
        let second_answer = second_device.accept(second_offer, [30; 32], [31; 32], &tracks(), NOW).unwrap();
        no_start(&first_answer);
        no_start(&second_answer);

        let commit = relay(&b2, &second_answer, &mut caller);
        assert!(starts(&commit, [10; 32], [11; 32], true));
        no_start(&relay(&b1, &first_answer, &mut caller));
        let loser = relay(&a, &commit, &mut first_device);
        assert!(loser.iter().any(|action| matches!(action, GroupAction::Resolved {
            incoming_id, resolution: MediaIncomingResolution::AnsweredElsewhere,
        } if *incoming_id == first_offer)));
        assert!(loser.iter().any(|action| matches!(action,
            GroupAction::EndUnusedAcceptedSession { session_id } if *session_id == [20; 32])));
        no_start(&loser);
        assert!(starts(&relay(&a, &commit, &mut second_device), [30; 32], [31; 32], false));

        let description = caller.local_description([10; 32], [11; 32],
            MediaDescription { kind: MediaDescriptionKind::Offer, sdp: "private-offer".into() });
        assert!(!relay(&a, &description, &mut first_device).iter()
            .any(|action| matches!(action, GroupAction::Description { .. })));
        assert!(relay(&a, &description, &mut second_device).iter().any(|action|
            matches!(action, GroupAction::Description { participant_id, .. } if *participant_id == [31; 32])));
    }

    #[test]
    fn crossed_invitations_keep_one_direction_without_implicit_acceptance() {
        let mut low = advertisement(3, 31);
        let mut high = advertisement(4, 41);
        if key(&low) > key(&high) { std::mem::swap(&mut low, &mut high); }
        let mut lower = GroupEngine::new(low.clone());
        let mut higher = GroupEngine::new(high.clone());
        lower.create_session([10; 32]).unwrap();
        higher.create_session([20; 32]).unwrap();
        let lower_invite = lower.invite([10; 32], [11; 32], peer(&high), vec![high.clone()], &tracks(), NOW).unwrap();
        let higher_invite = higher.invite([20; 32], [21; 32], peer(&low), vec![low.clone()], &tracks(), NOW).unwrap();
        let rejection = relay(&high, &higher_invite, &mut lower);
        assert!(!rejection.iter().any(|action| matches!(action, GroupAction::Incoming(_))));
        let offer = relay(&low, &lower_invite, &mut higher);
        let offer_id = incoming(&offer);
        assert!(offer.iter().any(|action| matches!(action, GroupAction::EndPeer {
            participant_id, outcome: MediaCallOutcome::Busy, ..
        } if *participant_id == [21; 32])));
        no_start(&offer);
        no_start(&relay(&low, &rejection, &mut higher));
        no_start(&relay(&high, &offer, &mut lower));

        let accept = higher.accept(offer_id, [30; 32], [31; 32], &tracks(), NOW).unwrap();
        no_start(&accept);
        let commit = relay(&high, &accept, &mut lower);
        assert!(starts(&commit, [10; 32], [11; 32], true));
        assert!(starts(&relay(&low, &commit, &mut higher), [30; 32], [31; 32], false));
    }

    #[test]
    fn introduced_edge_requires_both_decisions_and_refusal_preserves_existing_links() {
        let a = advertisement(5, 51);
        let b = advertisement(6, 61);
        let c = advertisement(7, 71);
        let mut hub = GroupEngine::new(a.clone());
        let mut left = GroupEngine::new(b.clone());
        let mut right = GroupEngine::new(c.clone());
        hub.create_session([10; 32]).unwrap();
        let invite_b = hub.invite([10; 32], [11; 32], peer(&b), vec![b.clone()], &tracks(), NOW).unwrap();
        let offer_b = incoming(&relay(&a, &invite_b, &mut left));
        let accept_b = left.accept(offer_b, [20; 32], [21; 32], &tracks(), NOW).unwrap();
        let commit_b = relay(&b, &accept_b, &mut hub);
        assert!(starts(&relay(&a, &commit_b, &mut left), [20; 32], [21; 32], false));
        let invite_c = hub.invite([10; 32], [12; 32], peer(&c), vec![c.clone()], &tracks(), NOW).unwrap();
        let offer_c = incoming(&relay(&a, &invite_c, &mut right));
        let accept_c = right.accept(offer_c, [30; 32], [31; 32], &tracks(), NOW).unwrap();
        let introductions = relay(&c, &accept_c, &mut hub);
        let proposal_b = relay(&a, &introductions, &mut left);
        let proposal_c = relay(&a, &introductions, &mut right);
        let question_b = relay(&c, &proposal_c, &mut left);
        let question_c = relay(&b, &proposal_b, &mut right);
        no_start(&question_b);
        no_start(&question_c);
        assert!(question_b.iter().any(|action| matches!(action, GroupAction::Incoming(offer)
            if offer.existing_session == Some([20; 32]) && offer.peer == peer(&c))));
        assert!(question_c.iter().any(|action| matches!(action, GroupAction::Incoming(offer)
            if offer.existing_session == Some([30; 32]) && offer.peer == peer(&b))));

        let edge_b = incoming(&question_b);
        let edge_c = incoming(&question_c);
        let one_answer = left.accept(edge_b, [20; 32], [22; 32], &tracks(), NOW).unwrap();
        no_start(&one_answer);
        no_start(&relay(&b, &one_answer, &mut right));
        let refusal = right.decline(edge_c, GroupDecline::Refused);
        let resolved = relay(&c, &refusal, &mut left);
        no_start(&resolved);
        assert!(resolved.iter().any(|action| matches!(action, GroupAction::Resolved {
            incoming_id, resolution: MediaIncomingResolution::Refused,
        } if *incoming_id == edge_b)));
        let still_b = hub.local_description([10; 32], [11; 32],
            MediaDescription { kind: MediaDescriptionKind::Offer, sdp: "private-b".into() });
        let still_c = hub.local_description([10; 32], [12; 32],
            MediaDescription { kind: MediaDescriptionKind::Offer, sdp: "private-c".into() });
        assert!(relay(&a, &still_b, &mut left).iter().any(|action|
            matches!(action, GroupAction::Description { participant_id, .. } if *participant_id == [21; 32])));
        assert!(relay(&a, &still_c, &mut right).iter().any(|action|
            matches!(action, GroupAction::Description { participant_id, .. } if *participant_id == [31; 32])));
    }
}
