/**
 * Call-session primitives and the functional-core state/event/effect types.
 * Pure data — no WebRTC, no chat coupling. The status vocabulary mirrors
 * Android `CallStatus` / iOS `ChatCallState`. Bytes fields (sdp, candidates)
 * are opaque payloads; encoding/decoding happens in the shell boundary layer.
 */

export type CallDirection = 'outgoing' | 'incoming';

export type CallPurpose = 'audio' | 'video';

export type CallStatus = 'requesting' | 'ringing' | 'connecting' | 'connected' | 'ended' | 'failed';

/**
 * Events that advance a call. `peerAnswered` = the far side answered our offer
 * (outgoing). `accepted` = the local user accepted an incoming call.
 * `mediaConnected` = the media connection reached `connected`. `declined` =
 * local user rejected an incoming call. `ended` = normal hangup by either
 * side. `failed` = ICE failure / timeout / unrecoverable error.
 */
export type CallEvent =
  | { type: 'peerAnswered' }
  | { type: 'accepted' }
  | { type: 'mediaConnected' }
  | { type: 'declined' }
  | { type: 'ended' }
  | { type: 'failed' };

/**
 * UI-facing media on/off state for a call. The wire transport of these facts is
 * the separate `MediaStateSignal` codec in `shared/peer-channel`. Video calls
 * open with the camera on; audio calls with it off. The mic is on by default
 * for both (the user mutes explicitly).
 */
export type CallMediaState = {
  cameraEnabled: boolean;
  microphoneEnabled: boolean;
};

export type CallMediaChange = {
  track: 'camera' | 'microphone';
  enabled: boolean;
};

// Consent-gated audio→video upgrade. 'outgoing' — we asked the peer and await
// their answer; 'incoming' — the peer asked us and the consent modal is up;
// 'idle' — no upgrade in flight.
export type PendingUpgrade = 'idle' | 'outgoing' | 'incoming';

export type CallSessionState = {
  status: CallStatus;
  direction: CallDirection;
  purpose: CallPurpose;
  offerId: string;
  dataChannelOpen: boolean;
  media: CallMediaState;
  pendingUpgrade: PendingUpgrade;
};

export type CallSessionEvent =
  | { type: 'localOfferEncoded'; sdp: Uint8Array }
  | { type: 'localAnswerEncoded'; sdp: Uint8Array }
  | { type: 'localCandidatesEncoded'; candidates: Uint8Array }
  | { type: 'accept' }
  | { type: 'decline' }
  | { type: 'end' }
  | { type: 'toggleMedia'; track: 'camera' | 'microphone'; enabled: boolean }
  // Consent-gated audio→video upgrade handshake (desktop↔desktop).
  | { type: 'requestVideoUpgrade' } // local user asked to switch → notify peer
  | { type: 'remoteUpgradeRequested' } // peer asked → raise the consent modal
  | { type: 'acceptVideoUpgrade' } // local user pressed "Switch" on the modal
  | { type: 'declineVideoUpgrade' } // local user pressed "Cancel" on the modal
  | { type: 'remoteUpgradeAccepted' } // peer agreed to our request
  | { type: 'remoteUpgradeDeclined' } // peer refused our request
  | { type: 'remoteAnswer'; offerId: string; sdp: Uint8Array }
  | { type: 'remoteCandidates'; offerId: string; candidates: Uint8Array }
  | { type: 'remoteClosed'; offerId: string }
  // Dismiss our ringing window silently; a `closed` would tear down the sibling's call.
  | { type: 'answeredElsewhere'; offerId: string }
  | { type: 'dataChannelOpen' }
  | { type: 'mediaConnected' }
  | { type: 'failed' };

export type CallSessionEffect =
  | { type: 'publishOffer'; offerId: string; purpose: CallPurpose; sdp: Uint8Array }
  | { type: 'publishAnswer'; offerId: string; sdp: Uint8Array }
  | { type: 'publishCandidates'; offerId: string; candidates: Uint8Array }
  | { type: 'publishClosed'; offerId: string }
  | { type: 'prepareAnswer' }
  | { type: 'applyRemoteAnswer'; sdp: Uint8Array }
  | { type: 'addRemoteCandidates'; candidates: Uint8Array }
  | { type: 'startMediaUpgrade' }
  // Consent handshake signals over the data channel (requester ↔ acceptor).
  | { type: 'sendUpgradeRequest' }
  | { type: 'sendUpgradeDecline' }
  // Requester side of an accepted upgrade: add camera + re-offer over the live
  // data channel (the in-band renegotiation).
  | { type: 'upgradeToVideo' }
  // Acceptor side of accepting: acquire the local camera track FIRST, then signal
  // acceptance to the peer — so by the time the requester re-offers, our video
  // sender exists and the answer is two-way (avoids a recv-only race).
  | { type: 'confirmVideoUpgrade' }
  | { type: 'sendMediaState'; track: 'camera' | 'microphone'; enabled: boolean }
  | { type: 'setLocalTrackEnabled'; track: 'camera' | 'microphone'; enabled: boolean }
  | { type: 'closePeerConnection' }
  // Terminal teardown of the call window itself. Emitted last on every terminal
  // transition so publishClosed/closePeerConnection run first. The shell delays
  // the actual window close briefly (mobile parity: ~1.5s terminal-state dwell)
  // so the closed signal can flush over the bridge and the user sees "Call ended".
  | { type: 'closeWindow' };

export type CallSessionInit = {
  direction: CallDirection;
  purpose: CallPurpose;
  offerId: string;
};
