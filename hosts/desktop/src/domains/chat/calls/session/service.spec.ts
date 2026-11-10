import { describe, expect, it } from 'vitest';

import { callSessionService } from './service';

const {
  initialCallStatus,
  isTerminalCallStatus,
  nextCallStatus,
  initialCallMediaState,
  applyCallMediaChange,
  initCallSession,
  callSessionReducer,
} = callSessionService;

describe('initialCallStatus', () => {
  it('outgoing starts at requesting', () => {
    expect(initialCallStatus('outgoing')).toBe('requesting');
  });
  it('incoming starts at ringing', () => {
    expect(initialCallStatus('incoming')).toBe('ringing');
  });
});

describe('nextCallStatus — happy paths', () => {
  it('outgoing: requesting --peerAnswered--> connecting --mediaConnected--> connected', () => {
    let s = initialCallStatus('outgoing');
    s = nextCallStatus(s, { type: 'peerAnswered' });
    expect(s).toBe('connecting');
    s = nextCallStatus(s, { type: 'mediaConnected' });
    expect(s).toBe('connected');
  });
  it('incoming: ringing --accepted--> connecting --mediaConnected--> connected', () => {
    let s = initialCallStatus('incoming');
    s = nextCallStatus(s, { type: 'accepted' });
    expect(s).toBe('connecting');
    s = nextCallStatus(s, { type: 'mediaConnected' });
    expect(s).toBe('connected');
  });
});

describe('nextCallStatus — teardown', () => {
  it('declined from ringing ends the call', () => {
    expect(nextCallStatus('ringing', { type: 'declined' })).toBe('ended');
  });
  it('ended event from any live status ends the call', () => {
    for (const s of ['requesting', 'ringing', 'connecting', 'connected'] as const) {
      expect(nextCallStatus(s, { type: 'ended' })).toBe('ended');
    }
  });
  it('failed event from any live status fails the call', () => {
    for (const s of ['requesting', 'ringing', 'connecting', 'connected'] as const) {
      expect(nextCallStatus(s, { type: 'failed' })).toBe('failed');
    }
  });
});

describe('nextCallStatus — robustness', () => {
  it('terminal statuses absorb further events', () => {
    for (const ev of [{ type: 'accepted' }, { type: 'mediaConnected' }, { type: 'ended' }, { type: 'failed' }] as const) {
      expect(nextCallStatus('ended', ev)).toBe('ended');
      expect(nextCallStatus('failed', ev)).toBe('failed');
    }
  });
  it('ignores nonsensical transitions (accept while already connected)', () => {
    expect(nextCallStatus('connected', { type: 'accepted' })).toBe('connected');
  });
  it('ignores mediaConnected while still requesting (no peer answer yet)', () => {
    expect(nextCallStatus('requesting', { type: 'mediaConnected' })).toBe('requesting');
  });
});

describe('isTerminalCallStatus', () => {
  it('true for ended/failed, false otherwise', () => {
    expect(isTerminalCallStatus('ended')).toBe(true);
    expect(isTerminalCallStatus('failed')).toBe(true);
    expect(isTerminalCallStatus('connected')).toBe(false);
    expect(isTerminalCallStatus('requesting')).toBe(false);
  });
});

describe('initialCallMediaState', () => {
  it('audio call: mic on, camera off', () => {
    expect(initialCallMediaState('audio')).toEqual({ cameraEnabled: false, microphoneEnabled: true });
  });
  it('video call: mic on, camera on', () => {
    expect(initialCallMediaState('video')).toEqual({ cameraEnabled: true, microphoneEnabled: true });
  });
});

describe('applyCallMediaChange', () => {
  it('toggles the camera without touching the mic', () => {
    const start = initialCallMediaState('video');
    const next = applyCallMediaChange(start, { track: 'camera', enabled: false });
    expect(next).toEqual({ cameraEnabled: false, microphoneEnabled: true });
  });
  it('toggles the mic without touching the camera', () => {
    const start = initialCallMediaState('video');
    const next = applyCallMediaChange(start, { track: 'microphone', enabled: false });
    expect(next).toEqual({ cameraEnabled: true, microphoneEnabled: false });
  });
  it('returns a new object (does not mutate the input)', () => {
    const start = initialCallMediaState('video');
    const next = applyCallMediaChange(start, { track: 'camera', enabled: false });
    expect(next).not.toBe(start);
    expect(start).toEqual({ cameraEnabled: true, microphoneEnabled: true });
  });
});

describe('initCallSession', () => {
  it('outgoing video call starts requesting, camera on, DC closed', () => {
    expect(initCallSession({ direction: 'outgoing', purpose: 'video', offerId: 'o1' })).toEqual({
      status: 'requesting',
      direction: 'outgoing',
      purpose: 'video',
      offerId: 'o1',
      dataChannelOpen: false,
      media: { cameraEnabled: true, microphoneEnabled: true },
      pendingUpgrade: 'idle',
    });
  });
  it('incoming audio call starts ringing, camera off', () => {
    expect(initCallSession({ direction: 'incoming', purpose: 'audio', offerId: 'o2' })).toEqual({
      status: 'ringing',
      direction: 'incoming',
      purpose: 'audio',
      offerId: 'o2',
      dataChannelOpen: false,
      media: { cameraEnabled: false, microphoneEnabled: true },
      pendingUpgrade: 'idle',
    });
  });
});

const b = (n: number) => new Uint8Array([n]);
const outgoing = () => initCallSession({ direction: 'outgoing', purpose: 'video', offerId: 'o1' });
const incoming = () => initCallSession({ direction: 'incoming', purpose: 'audio', offerId: 'o1' });

describe('outgoing Phase-1', () => {
  it('localOfferEncoded → publishOffer, still requesting', () => {
    const r = callSessionReducer(outgoing(), { type: 'localOfferEncoded', sdp: b(1) });
    expect(r.state.status).toBe('requesting');
    expect(r.effects).toEqual([{ type: 'publishOffer', offerId: 'o1', purpose: 'video', sdp: b(1) }]);
  });
  it('remoteAnswer (matching offerId) → connecting + applyRemoteAnswer', () => {
    const r = callSessionReducer(outgoing(), { type: 'remoteAnswer', offerId: 'o1', sdp: b(2) });
    expect(r.state.status).toBe('connecting');
    expect(r.effects).toEqual([{ type: 'applyRemoteAnswer', sdp: b(2) }]);
  });
  it('remoteAnswer with a DIFFERENT offerId is ignored (multi-device safety)', () => {
    const s = outgoing();
    const r = callSessionReducer(s, { type: 'remoteAnswer', offerId: 'other', sdp: b(2) });
    expect(r.state).toBe(s);
    expect(r.effects).toEqual([]);
  });
  it('a DUPLICATE remoteAnswer is ignored once past requesting (no second applyRemoteAnswer)', () => {
    const connecting = callSessionReducer(outgoing(), { type: 'remoteAnswer', offerId: 'o1', sdp: b(2) }).state;
    const r = callSessionReducer(connecting, { type: 'remoteAnswer', offerId: 'o1', sdp: b(9) });
    expect(r.state).toBe(connecting);
    expect(r.effects).toEqual([]);
  });
  it('localCandidatesEncoded → publishCandidates', () => {
    const r = callSessionReducer(outgoing(), { type: 'localCandidatesEncoded', candidates: b(3) });
    expect(r.effects).toEqual([{ type: 'publishCandidates', offerId: 'o1', candidates: b(3) }]);
  });
  it('remoteCandidates (matching) → addRemoteCandidates; wrong offerId ignored', () => {
    expect(callSessionReducer(outgoing(), { type: 'remoteCandidates', offerId: 'o1', candidates: b(4) }).effects).toEqual([
      { type: 'addRemoteCandidates', candidates: b(4) },
    ]);
    expect(callSessionReducer(outgoing(), { type: 'remoteCandidates', offerId: 'x', candidates: b(4) }).effects).toEqual([]);
  });
});

describe('incoming Phase-1', () => {
  it('accept → connecting + prepareAnswer', () => {
    const r = callSessionReducer(incoming(), { type: 'accept' });
    expect(r.state.status).toBe('connecting');
    expect(r.effects).toEqual([{ type: 'prepareAnswer' }]);
  });
  it('localAnswerEncoded → publishAnswer', () => {
    const accepted = callSessionReducer(incoming(), { type: 'accept' }).state;
    const r = callSessionReducer(accepted, { type: 'localAnswerEncoded', sdp: b(5) });
    expect(r.effects).toEqual([{ type: 'publishAnswer', offerId: 'o1', sdp: b(5) }]);
  });
  it('decline → ended + publishClosed + closePeerConnection + closeWindow', () => {
    const r = callSessionReducer(incoming(), { type: 'decline' });
    expect(r.state.status).toBe('ended');
    expect(r.effects).toEqual([
      { type: 'publishClosed', offerId: 'o1' },
      { type: 'closePeerConnection' },
      { type: 'closeWindow' },
    ]);
  });
  it('answeredElsewhere from ringing → ended + closePeerConnection + closeWindow, NO publishClosed', () => {
    const r = callSessionReducer(incoming(), { type: 'answeredElsewhere', offerId: 'o1' });
    expect(r.state.status).toBe('ended');
    expect(r.effects).toEqual([{ type: 'closePeerConnection' }, { type: 'closeWindow' }]);
  });
  it('answeredElsewhere for another offer is ignored (stale dismissal from an earlier call)', () => {
    const ringing = incoming();
    const r = callSessionReducer(ringing, { type: 'answeredElsewhere', offerId: 'other' });
    expect(r.state).toBe(ringing);
    expect(r.effects).toEqual([]);
  });
  it('answeredElsewhere is a no-op once we have locally accepted (connecting)', () => {
    const accepted = callSessionReducer(incoming(), { type: 'accept' }).state;
    const r = callSessionReducer(accepted, { type: 'answeredElsewhere', offerId: 'o1' });
    expect(r.state).toBe(accepted);
    expect(r.effects).toEqual([]);
  });
  it('answeredElsewhere is a no-op on a connected call', () => {
    const connecting = callSessionReducer(incoming(), { type: 'accept' }).state;
    const connected = callSessionReducer(connecting, { type: 'mediaConnected' }).state;
    const r = callSessionReducer(connected, { type: 'answeredElsewhere', offerId: 'o1' });
    expect(r.state).toBe(connected);
    expect(r.effects).toEqual([]);
  });
});

describe('Phase-2 + lifecycle', () => {
  it('dataChannelOpen → dataChannelOpen=true + startMediaUpgrade', () => {
    const connecting = callSessionReducer(outgoing(), { type: 'remoteAnswer', offerId: 'o1', sdp: b(2) }).state;
    const r = callSessionReducer(connecting, { type: 'dataChannelOpen' });
    expect(r.state.dataChannelOpen).toBe(true);
    expect(r.effects).toEqual([{ type: 'startMediaUpgrade' }]);
  });
  it('mediaConnected → connected', () => {
    const connecting = callSessionReducer(outgoing(), { type: 'remoteAnswer', offerId: 'o1', sdp: b(2) }).state;
    expect(callSessionReducer(connecting, { type: 'mediaConnected' }).state.status).toBe('connected');
  });
  it('toggleMedia updates media + emits sendMediaState & setLocalTrackEnabled', () => {
    const r = callSessionReducer(outgoing(), { type: 'toggleMedia', track: 'camera', enabled: false });
    expect(r.state.media.cameraEnabled).toBe(false);
    expect(r.effects).toEqual([
      { type: 'setLocalTrackEnabled', track: 'camera', enabled: false },
      { type: 'sendMediaState', track: 'camera', enabled: false },
    ]);
  });
  it('end → ended + publishClosed + closePeerConnection + closeWindow', () => {
    const r = callSessionReducer(outgoing(), { type: 'end' });
    expect(r.state.status).toBe('ended');
    expect(r.effects).toEqual([
      { type: 'publishClosed', offerId: 'o1' },
      { type: 'closePeerConnection' },
      { type: 'closeWindow' },
    ]);
  });
  it('remoteClosed (matching) → ended + closePeerConnection + closeWindow, no publishClosed', () => {
    const r = callSessionReducer(outgoing(), { type: 'remoteClosed', offerId: 'o1' });
    expect(r.state.status).toBe('ended');
    expect(r.effects).toEqual([{ type: 'closePeerConnection' }, { type: 'closeWindow' }]);
  });
  it('failed → failed + closePeerConnection + closeWindow', () => {
    const r = callSessionReducer(outgoing(), { type: 'failed' });
    expect(r.state.status).toBe('failed');
    expect(r.effects).toEqual([{ type: 'closePeerConnection' }, { type: 'closeWindow' }]);
  });
  it('terminal state ignores further events', () => {
    const ended = callSessionReducer(outgoing(), { type: 'end' }).state;
    const r = callSessionReducer(ended, { type: 'mediaConnected' });
    expect(r.state).toBe(ended);
    expect(r.effects).toEqual([]);
  });
});

describe('audio→video upgrade consent handshake', () => {
  // A connected audio call with the data channel open — the only state in which
  // an upgrade may start on either side.
  const connectedAudio = () => callSessionReducer(incoming(), { type: 'dataChannelOpen' }).state;

  describe('requester side', () => {
    it('requestVideoUpgrade → pendingUpgrade=outgoing + sendUpgradeRequest, no media change yet', () => {
      const r = callSessionReducer(connectedAudio(), { type: 'requestVideoUpgrade' });
      expect(r.state.pendingUpgrade).toBe('outgoing');
      expect(r.state.purpose).toBe('audio');
      expect(r.state.media.cameraEnabled).toBe(false);
      expect(r.effects).toEqual([{ type: 'sendUpgradeRequest' }]);
    });

    it('requestVideoUpgrade is ignored before the data channel is open', () => {
      const s = incoming(); // dataChannelOpen=false
      const r = callSessionReducer(s, { type: 'requestVideoUpgrade' });
      expect(r.state).toBe(s);
      expect(r.effects).toEqual([]);
    });

    it('requestVideoUpgrade is ignored when an upgrade is already in flight', () => {
      const outgoingPending = callSessionReducer(connectedAudio(), { type: 'requestVideoUpgrade' }).state;
      const r = callSessionReducer(outgoingPending, { type: 'requestVideoUpgrade' });
      expect(r.state).toBe(outgoingPending);
      expect(r.effects).toEqual([]);
    });

    it('remoteUpgradeAccepted → purpose=video, camera on, upgradeToVideo + sendMediaState', () => {
      const outgoingPending = callSessionReducer(connectedAudio(), { type: 'requestVideoUpgrade' }).state;
      const r = callSessionReducer(outgoingPending, { type: 'remoteUpgradeAccepted' });
      expect(r.state.pendingUpgrade).toBe('idle');
      expect(r.state.purpose).toBe('video');
      expect(r.state.media.cameraEnabled).toBe(true);
      expect(r.effects).toEqual([{ type: 'upgradeToVideo' }, { type: 'sendMediaState', track: 'camera', enabled: true }]);
    });

    it('remoteUpgradeDeclined → clears pending, stays audio, no effects', () => {
      const outgoingPending = callSessionReducer(connectedAudio(), { type: 'requestVideoUpgrade' }).state;
      const r = callSessionReducer(outgoingPending, { type: 'remoteUpgradeDeclined' });
      expect(r.state.pendingUpgrade).toBe('idle');
      expect(r.state.purpose).toBe('audio');
      expect(r.effects).toEqual([]);
    });

    it('remoteUpgradeAccepted with no pending outgoing request is ignored', () => {
      const s = connectedAudio();
      const r = callSessionReducer(s, { type: 'remoteUpgradeAccepted' });
      expect(r.state).toBe(s);
      expect(r.effects).toEqual([]);
    });
  });

  describe('acceptor side', () => {
    it('remoteUpgradeRequested → pendingUpgrade=incoming, no effects (modal only)', () => {
      const r = callSessionReducer(connectedAudio(), { type: 'remoteUpgradeRequested' });
      expect(r.state.pendingUpgrade).toBe('incoming');
      expect(r.state.purpose).toBe('audio');
      expect(r.effects).toEqual([]);
    });

    it('acceptVideoUpgrade → purpose=video, camera on, sendUpgradeAccept + acquireLocalVideo + sendMediaState', () => {
      const incomingPending = callSessionReducer(connectedAudio(), { type: 'remoteUpgradeRequested' }).state;
      const r = callSessionReducer(incomingPending, { type: 'acceptVideoUpgrade' });
      expect(r.state.pendingUpgrade).toBe('idle');
      expect(r.state.purpose).toBe('video');
      expect(r.state.media.cameraEnabled).toBe(true);
      expect(r.effects).toEqual([{ type: 'confirmVideoUpgrade' }, { type: 'sendMediaState', track: 'camera', enabled: true }]);
    });

    it('declineVideoUpgrade → clears pending, stays audio, sendUpgradeDecline', () => {
      const incomingPending = callSessionReducer(connectedAudio(), { type: 'remoteUpgradeRequested' }).state;
      const r = callSessionReducer(incomingPending, { type: 'declineVideoUpgrade' });
      expect(r.state.pendingUpgrade).toBe('idle');
      expect(r.state.purpose).toBe('audio');
      expect(r.effects).toEqual([{ type: 'sendUpgradeDecline' }]);
    });

    it('acceptVideoUpgrade with no incoming request is ignored', () => {
      const s = connectedAudio();
      const r = callSessionReducer(s, { type: 'acceptVideoUpgrade' });
      expect(r.state).toBe(s);
      expect(r.effects).toEqual([]);
    });

    it('remoteUpgradeRequested is ignored on a video call (already video)', () => {
      const connectedVideo = callSessionReducer(outgoing(), { type: 'dataChannelOpen' }).state;
      const r = callSessionReducer(connectedVideo, { type: 'remoteUpgradeRequested' });
      expect(r.state.pendingUpgrade).toBe('idle');
      expect(r.effects).toEqual([]);
    });
  });
});
