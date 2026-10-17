//! Unified [`Media`] trait.

use crate::versioned::media::{
    HostMediaAddParticipantError, HostMediaAddParticipantRequest, HostMediaAddParticipantResponse,
    HostMediaCancelOperationError, HostMediaCancelOperationRequest,
    HostMediaCancelOperationResponse, HostMediaCreateSessionError, HostMediaCreateSessionRequest,
    HostMediaCreateSessionResponse, HostMediaEndSessionError, HostMediaEndSessionRequest,
    HostMediaEndSessionResponse, HostMediaGetCapabilitiesError, HostMediaGetCapabilitiesRequest,
    HostMediaGetCapabilitiesResponse, HostMediaGetOperationError, HostMediaGetOperationRequest,
    HostMediaGetOperationResponse, HostMediaRemoveParticipantError,
    HostMediaRemoveParticipantRequest, HostMediaRemoveParticipantResponse,
    HostMediaRespondIncomingError, HostMediaRespondIncomingRequest,
    HostMediaRespondIncomingResponse, HostMediaSessionSubscribeError,
    HostMediaSessionSubscribeItem, HostMediaSessionSubscribeRequest, HostMediaSetLocalTracksError,
    HostMediaSetLocalTracksRequest, HostMediaSetLocalTracksResponse, HostMediaSetSurfacesError,
    HostMediaSetSurfacesRequest, HostMediaSetSurfacesResponse,
};
use crate::{CallContext, CallError, Subscription};
use crate::{wire, wire_trait};

/// Host-owned real-time media, eligible in both foreground App and Worker runtimes.
///
/// Ownership is immutable to the authenticated product/account/network authority
/// and runtime. Calling consent is distinct from browser WebRTC permission; local
/// microphone/camera additionally require their capture grants, and screen capture
/// always uses the trusted picker. Revoking any calling/microphone/camera grant or
/// losing identity ends affected sessions, including receive-only sessions. A host
/// indicator and hangup control remain visible until termination, even without a
/// product view. Host screen-stop ends only that track and fences old intent work.
///
/// Subscribe and receive the first snapshot before mutations. Known-resource
/// teardown, refusal, cancellation, and all-off intent remain possible after
/// subscription loss. Passive incoming delivery does not prompt, capture, or wake
/// a cold product. Dropping a listener never implicitly hangs up sessions.
///
/// Correlated mutations reserve finite capacity before effects. Identical requests
/// under one operation key join/replay; different requests conflict. A lost reply
/// is recovered through get_operation, never by retrying with a fresh key or
/// guessing from session enumeration. Host timeouts cancel pending work; a client
/// wait timeout alone is not host cancellation. No late callback may resurrect a
/// cancelled, superseded, or ended resource.
///
/// Prototype trait 218 (Contacts owns 20 on main) and these ordinals are assigned
/// on this implementation branch pending upstream review; they are not a claim of
/// globally reserved or shipped support.
#[wire_trait(id = 218)]
#[crate::async_trait]
pub trait Media: Send + Sync {
    /// Discover the complete service without permission, capture, or signaling.
    /// Hosts missing any mandatory V1 facility return framework Unsupported.
    ///
    /// ```ts
    /// const result = await truapi.media.getCapabilities();
    /// console.log("Media capabilities:", result);
    /// ```
    #[wire(id = 0)]
    async fn get_capabilities(
        &self,
        _cx: &CallContext,
        _request: HostMediaGetCapabilitiesRequest,
    ) -> Result<HostMediaGetCapabilitiesResponse, CallError<HostMediaGetCapabilitiesError>> {
        Err(CallError::Unsupported)
    }

    /// Subscribe runtime-wide, including before any session exists.
    /// The initial snapshot is atomic with event publication; future sequences
    /// strictly increase. Overflow interrupts with EventOverflow rather than
    /// dropping state. Resubscribe to recover a fresh authoritative snapshot.
    ///
    /// ```ts
    /// import { firstValueFrom, from } from "rxjs";
    ///
    /// const first = await firstValueFrom(from(truapi.media.sessionSubscribe()));
    /// assert(first.tag === "Snapshot", "snapshot required");
    /// console.log("current Media state:", first);
    /// ```
    #[wire(id = 1)]
    async fn session_subscribe(
        &self,
        _cx: &CallContext,
        _request: HostMediaSessionSubscribeRequest,
    ) -> Subscription<HostMediaSessionSubscribeItem, CallError<HostMediaSessionSubscribeError>>
    {
        Subscription::interrupted(CallError::Unsupported)
    }

    /// Create a ready session after calling and requested capture consent.
    /// No peer is connected or signaled. Local self-preview can start only after
    /// consent and stays in unreadable host composition; no view is required.
    ///
    /// ```ts
    /// import { connectable, firstValueFrom, from } from "rxjs";
    ///
    /// const events = connectable(from(truapi.media.sessionSubscribe()));
    /// const firstSnapshot = firstValueFrom(events);
    /// // Keep the host listener until the operation and cleanup finish.
    /// const listener = events.connect();
    /// try {
    ///   const first = await firstSnapshot;
    ///   assert(first.tag === "Snapshot", "snapshot required");
    ///   const operationId: `0x${string}` = `0x${crypto.getRandomValues(new Uint8Array(32)).toHex()}`;
    ///   const result = await truapi.media.createSession({
    ///     operationId, tracks: { microphone: false, camera: false, screen: false },
    ///   });
    ///   assert(result.isOk(), "createSession failed:", result);
    ///   console.log("ready session:", result.value.session);
    ///   await truapi.media.endSession({ sessionId: result.value.session.sessionId });
    /// } finally {
    ///   listener.unsubscribe();
    /// }
    /// ```
    #[wire(id = 2)]
    async fn create_session(
        &self,
        _cx: &CallContext,
        _request: HostMediaCreateSessionRequest,
    ) -> Result<HostMediaCreateSessionResponse, CallError<HostMediaCreateSessionError>> {
        Err(CallError::Unsupported)
    }

    /// Admit an invitation after peer, authority, consent, and capacity checks.
    /// Returns an inviting participant, not the remote answer. A duplicate live
    /// peer returns its participant without another capacity slot. Invitations
    /// expire in 60 seconds, then initial connection has a 30-second bound;
    /// identical retries never extend either deadline. One local plus five
    /// remote endpoints is the mandatory floor, not six remote participants.
    ///
    /// ```ts
    /// import { connectable, firstValueFrom, from } from "rxjs";
    ///
    /// const events = connectable(from(truapi.media.sessionSubscribe()));
    /// const firstSnapshot = firstValueFrom(events);
    /// // Keep the host listener until the operation and cleanup finish.
    /// const listener = events.connect();
    /// try {
    ///   const first = await firstSnapshot;
    ///   assert(first.tag === "Snapshot", "snapshot required");
    ///   const session = first.value.sessions.find(s => s.state !== "Ended" && s.participants.length > 0);
    ///   assert(session, "Start a session with an authenticated contact first");
    ///   // Re-adding this known peer demonstrates duplicate-peer admission.
    ///   const operationId: `0x${string}` = `0x${crypto.getRandomValues(new Uint8Array(32)).toHex()}`;
    ///   console.log(await truapi.media.addParticipant({
    ///     operationId, sessionId: session.sessionId, peer: session.participants[0].peer,
    ///   }));
    /// } finally {
    ///   listener.unsubscribe();
    /// }
    /// ```
    #[wire(id = 3)]
    async fn add_participant(
        &self,
        _cx: &CallContext,
        _request: HostMediaAddParticipantRequest,
    ) -> Result<HostMediaAddParticipantResponse, CallError<HostMediaAddParticipantError>> {
        Err(CallError::Unsupported)
    }

    /// Atomically decide an authenticated, expiring single-use incoming offer.
    /// Failed preconditions do not consume it; once acceptance/consent begins,
    /// cancellation or denial resolves it without replay. AcceptExisting must
    /// match the offer's existing session and cannot merge arbitrary calls.
    /// Refusal is cleanup and needs neither subscription nor operation quota.
    ///
    /// ```ts
    /// import { connectable, firstValueFrom, from } from "rxjs";
    ///
    /// const events = connectable(from(truapi.media.sessionSubscribe()));
    /// const firstSnapshot = firstValueFrom(events);
    /// // Keep the host listener until the operation and cleanup finish.
    /// const listener = events.connect();
    /// try {
    ///   const first = await firstSnapshot;
    ///   assert(first.tag === "Snapshot", "snapshot required");
    ///   const offer = first.value.incoming[0];
    ///   assert(offer, "An incoming offer is required");
    ///   console.log(await truapi.media.respondIncoming({
    ///     incomingId: offer.incomingId, decision: { tag: "Refuse" },
    ///   }));
    /// } finally {
    ///   listener.unsubscribe();
    /// }
    /// ```
    #[wire(id = 4)]
    async fn respond_incoming(
        &self,
        _cx: &CallContext,
        _request: HostMediaRespondIncomingRequest,
    ) -> Result<HostMediaRespondIncomingResponse, CallError<HostMediaRespondIncomingError>> {
        Err(CallError::Unsupported)
    }

    /// Remove an owned endpoint without ending the other participants or session.
    /// Repeated removal of a previously owned terminal capability succeeds without
    /// new events, permission, subscription, quota, or successful network exchange.
    ///
    /// ```ts
    /// import { connectable, firstValueFrom, from } from "rxjs";
    ///
    /// const events = connectable(from(truapi.media.sessionSubscribe()));
    /// const firstSnapshot = firstValueFrom(events);
    /// // Keep the host listener until the operation and cleanup finish.
    /// const listener = events.connect();
    /// try {
    ///   const first = await firstSnapshot;
    ///   assert(first.tag === "Snapshot", "snapshot required");
    ///   const session = first.value.sessions.find(s => s.participants.some(p => p.state !== "Left"));
    ///   assert(session, "A session with a live participant is required");
    ///   const participant = session.participants.find(p => p.state !== "Left")!;
    ///   console.log(await truapi.media.removeParticipant({
    ///     sessionId: session.sessionId, participantId: participant.participantId,
    ///   }));
    /// } finally {
    ///   listener.unsubscribe();
    /// }
    /// ```
    #[wire(id = 5)]
    async fn remove_participant(
        &self,
        _cx: &CallContext,
        _request: HostMediaRemoveParticipantRequest,
    ) -> Result<HostMediaRemoveParticipantResponse, CallError<HostMediaRemoveParticipantError>>
    {
        Err(CallError::Unsupported)
    }

    /// Replace complete capture intent, ordered at admission before async consent.
    /// New intent supersedes older uncommitted work with InvalidState; its consent,
    /// picker, and device work must stop, and late capture must be released. If the
    /// newer intent is denied, retain the last committed intent, not the superseded
    /// request. All-off intent is allowed without a listener. Picker cancellation
    /// leaves the last accepted intent unchanged; committed Off stops sending.
    ///
    /// ```ts
    /// import { connectable, firstValueFrom, from } from "rxjs";
    ///
    /// const events = connectable(from(truapi.media.sessionSubscribe()));
    /// const firstSnapshot = firstValueFrom(events);
    /// // Keep the host listener until the operation and cleanup finish.
    /// const listener = events.connect();
    /// try {
    ///   const first = await firstSnapshot;
    ///   assert(first.tag === "Snapshot", "snapshot required");
    ///   const session = first.value.sessions.find(s => s.state !== "Ended");
    ///   assert(session, "Start a session first");
    ///   const operationId: `0x${string}` = `0x${crypto.getRandomValues(new Uint8Array(32)).toHex()}`;
    ///   console.log(await truapi.media.setLocalTracks({
    ///     operationId, sessionId: session.sessionId,
    ///     tracks: { microphone: false, camera: false, screen: false },
    ///   }));
    /// } finally {
    ///   listener.unsubscribe();
    /// }
    /// ```
    #[wire(id = 6)]
    async fn set_local_tracks(
        &self,
        _cx: &CallContext,
        _request: HostMediaSetLocalTracksRequest,
    ) -> Result<HostMediaSetLocalTracksResponse, CallError<HostMediaSetLocalTracksError>> {
        Err(CallError::Unsupported)
    }

    /// Queue one atomic complete session layout on the authorized runtime viewport.
    /// Validate every handle, rectangle, key, bound, and revision before changing
    /// anything. Identical current-revision retries succeed, conflicting reuse is
    /// InvalidSurface, and older layouts are StaleLayout. Viewport changes discard
    /// the whole stale queued set; detach clears all runtime layouts. Empty sets
    /// clear pictures. No attachment yields SurfaceUnavailable, also for workers.
    /// Host clipping may reduce but never expand the submitted visible region;
    /// pictures remain unreadable siblings below trusted UI, not product textures.
    ///
    /// ```ts
    /// import { connectable, firstValueFrom, from } from "rxjs";
    ///
    /// const events = connectable(from(truapi.media.sessionSubscribe()));
    /// const firstSnapshot = firstValueFrom(events);
    /// // Keep the host listener until the operation and cleanup finish.
    /// const listener = events.connect();
    /// try {
    ///   const first = await firstSnapshot;
    ///   assert(first.tag === "Snapshot", "snapshot required");
    ///   const viewport = first.value.viewport;
    ///   assert(viewport, "An attached App viewport is required");
    ///   const operationId: `0x${string}` = `0x${crypto.getRandomValues(new Uint8Array(32)).toHex()}`;
    ///   const created = await truapi.media.createSession({
    ///     operationId, tracks: { microphone: false, camera: false, screen: false },
    ///   });
    ///   assert(created.isOk(), "createSession failed:", created);
    ///   const sessionId = created.value.session.sessionId;
    ///   try {
    ///     console.log(await truapi.media.setSurfaces({
    ///       sessionId, viewportRevision: viewport.revision, layoutRevision: 1n, surfaces: [],
    ///     }));
    ///   } finally {
    ///     await truapi.media.endSession({ sessionId });
    ///   }
    /// } finally {
    ///   listener.unsubscribe();
    /// }
    /// ```
    #[wire(id = 7)]
    async fn set_surfaces(
        &self,
        _cx: &CallContext,
        _request: HostMediaSetSurfacesRequest,
    ) -> Result<HostMediaSetSurfacesResponse, CallError<HostMediaSetSurfacesError>> {
        Err(CallError::Unsupported)
    }

    /// End an owned session, linearizing before concurrent track/layout mutations.
    /// Release capture, playback, decoders, signaling, and surfaces, resolve pending
    /// offers, and emit one terminal snapshot. Repeated known teardown succeeds
    /// without permission or new quota. Tombstones last until runtime destruction;
    /// random and unowned handles both return InvalidHandle without ownership leaks.
    ///
    /// ```ts
    /// import { connectable, firstValueFrom, from } from "rxjs";
    ///
    /// const events = connectable(from(truapi.media.sessionSubscribe()));
    /// const firstSnapshot = firstValueFrom(events);
    /// // Keep the host listener until the operation and cleanup finish.
    /// const listener = events.connect();
    /// try {
    ///   const first = await firstSnapshot;
    ///   assert(first.tag === "Snapshot", "snapshot required");
    ///   const session = first.value.sessions.find(s => s.state !== "Ended");
    ///   assert(session, "Start a session first");
    ///   console.log(await truapi.media.endSession({ sessionId: session.sessionId }));
    /// } finally {
    ///   listener.unsubscribe();
    /// }
    /// ```
    #[wire(id = 8)]
    async fn end_session(
        &self,
        _cx: &CallContext,
        _request: HostMediaEndSessionRequest,
    ) -> Result<HostMediaEndSessionResponse, CallError<HostMediaEndSessionError>> {
        Err(CallError::Unsupported)
    }

    /// Recover the exact authoritative operation outcome after a lost reply.
    /// Unknown keys return InvalidOperation; discovery of another runtime's keys
    /// is never possible. Does not require an active listener or new consent.
    ///
    /// ```ts
    /// const operationId: `0x${string}` = `0x${crypto.getRandomValues(new Uint8Array(32)).toHex()}`;
    /// // Cancel before admission, then recover that exact retained outcome.
    /// const cancelled = await truapi.media.cancelOperation({ operationId });
    /// assert(cancelled.isOk(), "cancelOperation failed:", cancelled);
    /// console.log(await truapi.media.getOperation({ operationId }));
    /// ```
    #[wire(id = 9)]
    async fn get_operation(
        &self,
        _cx: &CallContext,
        _request: HostMediaGetOperationRequest,
    ) -> Result<HostMediaGetOperationResponse, CallError<HostMediaGetOperationError>> {
        Err(CallError::Unsupported)
    }

    /// Cancel pending work authoritatively, including consent and late capture.
    /// Linearizes against commit: if commit won, return Committed without undoing
    /// or lying about it. Cancelling an unknown key reserves a Cancelled tombstone
    /// so a delayed original request cannot act; budget exhaustion precedes effects.
    /// Known cancellation needs no subscription, new quota, or permission prompt.
    ///
    /// ```ts
    /// const operationId: `0x${string}` = `0x${crypto.getRandomValues(new Uint8Array(32)).toHex()}`;
    /// // A delayed mutation with this key is now prevented from taking effect.
    /// console.log(await truapi.media.cancelOperation({ operationId }));
    /// ```
    #[wire(id = 10)]
    async fn cancel_operation(
        &self,
        _cx: &CallContext,
        _request: HostMediaCancelOperationRequest,
    ) -> Result<HostMediaCancelOperationResponse, CallError<HostMediaCancelOperationError>> {
        Err(CallError::Unsupported)
    }
}
