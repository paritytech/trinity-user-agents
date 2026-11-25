/**
 * Pure call-session machine: `CallStatus` transitions, media on/off state, the
 * session initializer, and the two-phase `state, event → { state, effects }`
 * reducer. All decisions live here so the whole choreography is unit-testable;
 * the runtime shell performs the I/O by interpreting `effects`. Bytes are
 * opaque — encode/decode is the shell's job, surfaced as already-encoded event
 * and effect payloads.
 *
 * Terminal statuses absorb all events; invalid/racing transitions are ignored
 * (return current) rather than throwing, because real-time signals arrive out
 * of order.
 */

import {
  type CallDirection,
  type CallEvent,
  type CallMediaChange,
  type CallMediaState,
  type CallPurpose,
  type CallSessionEffect,
  type CallSessionEvent,
  type CallSessionInit,
  type CallSessionState,
  type CallStatus,
} from './types';

type ReducerResult = { state: CallSessionState; effects: CallSessionEffect[] };

function initialCallStatus(direction: CallDirection): CallStatus {
  return direction === 'outgoing' ? 'requesting' : 'ringing';
}

function isTerminalCallStatus(status: CallStatus): boolean {
  return status === 'ended' || status === 'failed';
}

function nextCallStatus(current: CallStatus, event: CallEvent): CallStatus {
  if (isTerminalCallStatus(current)) return current;

  // `ended` / `failed` are reachable from every live status.
  if (event.type === 'ended' || event.type === 'declined') return 'ended';
  if (event.type === 'failed') return 'failed';

  switch (current) {
    case 'requesting':
      return event.type === 'peerAnswered' ? 'connecting' : current;
    case 'ringing':
      return event.type === 'accepted' ? 'connecting' : current;
    case 'connecting':
      return event.type === 'mediaConnected' ? 'connected' : current;
    case 'connected':
      return current;
    default:
      return current;
  }
}

function initialCallMediaState(purpose: CallPurpose): CallMediaState {
  return { cameraEnabled: purpose === 'video', microphoneEnabled: true };
}

function applyCallMediaChange(state: CallMediaState, change: CallMediaChange): CallMediaState {
  if (change.track === 'camera') {
    return { ...state, cameraEnabled: change.enabled };
  }
  return { ...state, microphoneEnabled: change.enabled };
}

function initCallSession(init: CallSessionInit): CallSessionState {
  return {
    status: initialCallStatus(init.direction),
    direction: init.direction,
    purpose: init.purpose,
    offerId: init.offerId,
    dataChannelOpen: false,
    media: initialCallMediaState(init.purpose),
    pendingUpgrade: 'idle',
  };
}

function unchanged(state: CallSessionState): ReducerResult {
  return { state, effects: [] };
}

function callSessionReducer(state: CallSessionState, event: CallSessionEvent): ReducerResult {
  if (isTerminalCallStatus(state.status)) return unchanged(state);

  switch (event.type) {
    case 'localOfferEncoded':
      return { state, effects: [{ type: 'publishOffer', offerId: state.offerId, purpose: state.purpose, sdp: event.sdp }] };

    case 'localAnswerEncoded':
      return { state, effects: [{ type: 'publishAnswer', offerId: state.offerId, sdp: event.sdp }] };

    case 'localCandidatesEncoded':
      return { state, effects: [{ type: 'publishCandidates', offerId: state.offerId, candidates: event.candidates }] };

    case 'accept':
      return {
        state: { ...state, status: nextCallStatus(state.status, { type: 'accepted' }) },
        effects: [{ type: 'prepareAnswer' }],
      };

    case 'decline':
    case 'end':
      return {
        state: { ...state, status: nextCallStatus(state.status, { type: 'ended' }) },
        effects: [{ type: 'publishClosed', offerId: state.offerId }, { type: 'closePeerConnection' }, { type: 'closeWindow' }],
      };

    case 'toggleMedia':
      return {
        state: { ...state, media: applyCallMediaChange(state.media, { track: event.track, enabled: event.enabled }) },
        effects: [
          { type: 'setLocalTrackEnabled', track: event.track, enabled: event.enabled },
          { type: 'sendMediaState', track: event.track, enabled: event.enabled },
        ],
      };

    case 'requestVideoUpgrade':
      // Ask the peer to switch an audio call to video. Only valid on a connected
      // audio call whose data channel is open (the handshake + renegotiation ride
      // that channel) with no upgrade already in flight. No local media changes
      // yet — the camera turns on only once the peer accepts.
      if (state.purpose !== 'audio' || !state.dataChannelOpen || state.pendingUpgrade !== 'idle') return unchanged(state);
      return { state: { ...state, pendingUpgrade: 'outgoing' }, effects: [{ type: 'sendUpgradeRequest' }] };

    case 'remoteUpgradeRequested':
      // Peer wants to switch to video — raise the consent modal. Ignore unless
      // we're on a plain audio call with nothing else pending (a request that
      // races our own is dropped; the peer can retry).
      if (state.purpose !== 'audio' || !state.dataChannelOpen || state.pendingUpgrade !== 'idle') return unchanged(state);
      return { state: { ...state, pendingUpgrade: 'incoming' }, effects: [] };

    case 'acceptVideoUpgrade':
      // We agreed (pressed "Switch"). Flip to video + enable the camera, tell the
      // peer, and add our own camera track locally — the peer's follow-up re-offer
      // negotiates it into two-way video.
      if (state.pendingUpgrade !== 'incoming') return unchanged(state);
      return {
        state: {
          ...state,
          pendingUpgrade: 'idle',
          purpose: 'video',
          media: applyCallMediaChange(state.media, { track: 'camera', enabled: true }),
        },
        effects: [{ type: 'confirmVideoUpgrade' }, { type: 'sendMediaState', track: 'camera', enabled: true }],
      };

    case 'declineVideoUpgrade':
      // We refused (pressed "Cancel"). Tell the peer; nothing else changed.
      if (state.pendingUpgrade !== 'incoming') return unchanged(state);
      return { state: { ...state, pendingUpgrade: 'idle' }, effects: [{ type: 'sendUpgradeDecline' }] };

    case 'remoteUpgradeAccepted':
      // Peer agreed to our request. Now add the camera + re-offer (the in-band
      // upgrade), flip to video, enable the camera.
      if (state.pendingUpgrade !== 'outgoing') return unchanged(state);
      return {
        state: {
          ...state,
          pendingUpgrade: 'idle',
          purpose: 'video',
          media: applyCallMediaChange(state.media, { track: 'camera', enabled: true }),
        },
        effects: [{ type: 'upgradeToVideo' }, { type: 'sendMediaState', track: 'camera', enabled: true }],
      };

    case 'remoteUpgradeDeclined':
      // Peer refused our request — clear the waiting state; call stays audio.
      if (state.pendingUpgrade !== 'outgoing') return unchanged(state);
      return { state: { ...state, pendingUpgrade: 'idle' }, effects: [] };

    case 'remoteAnswer':
      if (event.offerId !== state.offerId) return unchanged(state);
      // Apply the answer exactly once. A duplicate/re-delivered answer after we
      // have already left 'requesting' must NOT re-run applyRemoteAnswer — a
      // second setRemoteDescription(answer) on a stable connection throws and
      // breaks the call.
      if (state.status !== 'requesting') return unchanged(state);
      return {
        state: { ...state, status: nextCallStatus(state.status, { type: 'peerAnswered' }) },
        effects: [{ type: 'applyRemoteAnswer', sdp: event.sdp }],
      };

    case 'remoteCandidates':
      if (event.offerId !== state.offerId) return unchanged(state);
      return { state, effects: [{ type: 'addRemoteCandidates', candidates: event.candidates }] };

    case 'remoteClosed':
      if (event.offerId !== state.offerId) return unchanged(state);
      return {
        state: { ...state, status: nextCallStatus(state.status, { type: 'ended' }) },
        effects: [{ type: 'closePeerConnection' }, { type: 'closeWindow' }],
      };

    case 'answeredElsewhere':
      // No `closed` on the wire — it would reach the peer and tear down the call the
      // sibling just established. Guarded to `ringing` so a race against a local accept
      // can never drop our own call, and to `offerId` so a stale dismissal from an
      // earlier call can never dismiss this one.
      if (event.offerId !== state.offerId) return unchanged(state);
      if (state.status !== 'ringing') return unchanged(state);
      return {
        state: { ...state, status: nextCallStatus(state.status, { type: 'ended' }) },
        effects: [{ type: 'closePeerConnection' }, { type: 'closeWindow' }],
      };

    case 'dataChannelOpen':
      return { state: { ...state, dataChannelOpen: true }, effects: [{ type: 'startMediaUpgrade' }] };

    case 'mediaConnected':
      return { state: { ...state, status: nextCallStatus(state.status, { type: 'mediaConnected' }) }, effects: [] };

    case 'failed':
      return {
        state: { ...state, status: nextCallStatus(state.status, { type: 'failed' }) },
        effects: [{ type: 'closePeerConnection' }, { type: 'closeWindow' }],
      };

    default:
      return unchanged(state);
  }
}

export const callSessionService = {
  initialCallStatus,
  isTerminalCallStatus,
  nextCallStatus,
  initialCallMediaState,
  applyCallMediaChange,
  initCallSession,
  callSessionReducer,
};
