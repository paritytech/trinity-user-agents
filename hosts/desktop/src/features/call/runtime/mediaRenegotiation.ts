/**
 * Phase-2 media upgrade, run entirely over the open WebRTC data channel (never
 * the statement-store bridge). Once the Phase-1 data channel is open, the
 * initiator adds local audio/video and sends a full (un-minimized) SDP offer as
 * a `PeerConnectionSignal` over the multiplexed data channel; the acceptor
 * applies it, adds its own media, and answers. Mid-call camera/mic toggles ride
 * a separate `MediaStateSignal` stream on the same channel. Mirrors iOS
 * `CallInitiator`/`CallAcceptor` + `DataChannelSignaler` and Android
 * `PeerChannelConnection.performMediaRenegotiation`.
 */

import {
  type CallMediaPurpose,
  type CallPeerConnection,
  type DataChannelPort,
  MediaStateSignalCodec,
  PeerConnectionSignalCodec,
  UpgradeConsentSignalCodec,
  WEBRTC_MEDIA_STATE_USE_CASE_ID,
  WEBRTC_RENEGOTIATION_USE_CASE_ID,
  WEBRTC_UPGRADE_CONSENT_USE_CASE_ID,
  createDataChannelSignaler,
} from '@/shared/peer-channel';

export type MediaRenegotiationParams = {
  channel: RTCDataChannel;
  pc: CallPeerConnection;
  role: 'initiator' | 'acceptor';
  purpose: CallMediaPurpose;
  onRemoteMediaState: (track: 'camera' | 'microphone', enabled: boolean) => void;
  // Audio→video consent handshake — the peer's request/answer arriving over the
  // data channel (desktop↔desktop).
  onUpgradeRequest: VoidFunction;
  onUpgradeAccept: VoidFunction;
  onUpgradeDecline: VoidFunction;
};

export type MediaRenegotiationHandle = {
  sendMediaState: (track: 'camera' | 'microphone', enabled: boolean) => void;
  // Consent-handshake sends (requester asks; acceptor answers).
  sendUpgradeRequest: VoidFunction;
  sendUpgradeAccept: VoidFunction;
  sendUpgradeDecline: VoidFunction;
  /** Audio→video upgrade: add a camera track to the live call and re-offer over
   *  the data channel (mobile-aligned — same call, no hang-up, no re-ring).
   *  Only fired by the requester once the peer has accepted. */
  requestVideoUpgrade: () => Promise<void>;
  dispose: VoidFunction;
};

function dataChannelPort(channel: RTCDataChannel): DataChannelPort {
  channel.binaryType = 'arraybuffer';
  return {
    // Copy into a fresh ArrayBuffer-backed view: RTCDataChannel.send's overload
    // wants ArrayBufferView<ArrayBuffer>, but codec output is typed
    // Uint8Array<ArrayBufferLike> (TS 5.7 typed-array generics).
    send: data => channel.send(new Uint8Array(data)),
    onMessage: handler => {
      const listener = (event: MessageEvent) => handler(new Uint8Array(event.data));
      channel.addEventListener('message', listener);
      return () => channel.removeEventListener('message', listener);
    },
  };
}

export function startMediaRenegotiation(params: MediaRenegotiationParams): MediaRenegotiationHandle {
  const signaler = createDataChannelSignaler(dataChannelPort(params.channel));

  // Local media is added exactly once — on the initiator's first offer, or on the
  // acceptor's first received offer. A later re-offer (audio→video upgrade) must
  // NOT re-acquire audio (that would double-add the mic track).
  let localMediaAdded = false;

  const unsubscribeRenegotiation = signaler.subscribe(WEBRTC_RENEGOTIATION_USE_CASE_ID, payload => {
    const signal = PeerConnectionSignalCodec.dec(payload);
    // Fire-and-forget: each branch is an independent async step. Errors are
    // surfaced through the pc's connectionState$ (→ 'failed') downstream.
    void handleRenegotiationSignal(signal);
  });

  async function handleRenegotiationSignal(signal: ReturnType<typeof PeerConnectionSignalCodec.dec>): Promise<void> {
    console.info('[CALL] media renegotiation signal: %s', signal.tag);
    if (signal.tag === 'offer') {
      await params.pc.applyRemoteOffer(signal.value);
      // First offer: add our own media. Subsequent offers (peer's audio→video
      // upgrade) only need an answer — we already have our tracks, and re-adding
      // would duplicate the audio sender.
      if (!localMediaAdded) {
        await params.pc.addLocalMedia(params.purpose);
        localMediaAdded = true;
      }
      const answer = await params.pc.createAnswer();
      console.info('[CALL] media: answered media offer (localMediaAdded=%s)', localMediaAdded);
      signaler.send(WEBRTC_RENEGOTIATION_USE_CASE_ID, PeerConnectionSignalCodec.enc({ tag: 'answer', value: answer }));
    } else if (signal.tag === 'answer') {
      await params.pc.applyRemoteAnswer(signal.value);
      console.info('[CALL] media: remote media answer applied');
    } else {
      for (const candidate of signal.value) {
        await params.pc.addRemoteCandidate({
          candidate: candidate.sdp,
          sdpMLineIndex: candidate.sdpMLineIndex,
          sdpMid: candidate.sdpMid ?? undefined,
        });
      }
    }
  }

  const unsubscribeMediaState = signaler.subscribe(WEBRTC_MEDIA_STATE_USE_CASE_ID, payload => {
    const state = MediaStateSignalCodec.dec(payload);
    params.onRemoteMediaState(state.tag === 'cameraEnabled' ? 'camera' : 'microphone', state.value);
  });

  const unsubscribeUpgradeConsent = signaler.subscribe(WEBRTC_UPGRADE_CONSENT_USE_CASE_ID, payload => {
    const signal = UpgradeConsentSignalCodec.dec(payload);
    console.info('[CALL] upgrade consent signal: %s', signal.tag);
    if (signal.tag === 'request') {
      params.onUpgradeRequest();
    } else if (signal.tag === 'accept') {
      params.onUpgradeAccept();
    } else {
      params.onUpgradeDecline();
    }
  });

  // The initiator kicks off the media offer. With BUNDLE the media m-lines reuse
  // the Phase-1 ICE transport, so no fresh candidate exchange is normally needed;
  // if an ICE restart is ever required for renegotiation, add a Phase-2 candidate
  // relay here (over WEBRTC_RENEGOTIATION_USE_CASE_ID) — verify against mobile.
  if (params.role === 'initiator') {
    void startInitiatorOffer();
  }

  async function startInitiatorOffer(): Promise<void> {
    console.info('[CALL] media: initiator adding local media (%s) + sending media offer', params.purpose);
    await params.pc.addLocalMedia(params.purpose);
    localMediaAdded = true;
    const offer = await params.pc.createOffer();
    signaler.send(WEBRTC_RENEGOTIATION_USE_CASE_ID, PeerConnectionSignalCodec.enc({ tag: 'offer', value: offer }));
    console.info('[CALL] media: media offer sent over data channel');
  }

  async function requestVideoUpgrade(): Promise<void> {
    console.info('[CALL] media: audio→video upgrade — adding camera track + re-offering');
    await params.pc.addLocalVideo();
    const offer = await params.pc.createOffer();
    signaler.send(WEBRTC_RENEGOTIATION_USE_CASE_ID, PeerConnectionSignalCodec.enc({ tag: 'offer', value: offer }));
    console.info('[CALL] media: video-upgrade offer sent over data channel');
  }

  function sendMediaState(track: 'camera' | 'microphone', enabled: boolean): void {
    const tag = track === 'camera' ? 'cameraEnabled' : 'microphoneEnabled';
    signaler.send(WEBRTC_MEDIA_STATE_USE_CASE_ID, MediaStateSignalCodec.enc({ tag, value: enabled }));
  }

  function sendUpgradeRequest(): void {
    signaler.send(WEBRTC_UPGRADE_CONSENT_USE_CASE_ID, UpgradeConsentSignalCodec.enc({ tag: 'request', value: undefined }));
  }

  function sendUpgradeAccept(): void {
    signaler.send(WEBRTC_UPGRADE_CONSENT_USE_CASE_ID, UpgradeConsentSignalCodec.enc({ tag: 'accept', value: undefined }));
  }

  function sendUpgradeDecline(): void {
    signaler.send(WEBRTC_UPGRADE_CONSENT_USE_CASE_ID, UpgradeConsentSignalCodec.enc({ tag: 'decline', value: undefined }));
  }

  function dispose(): void {
    unsubscribeRenegotiation();
    unsubscribeMediaState();
    unsubscribeUpgradeConsent();
  }

  return { sendMediaState, sendUpgradeRequest, sendUpgradeAccept, sendUpgradeDecline, requestVideoUpgrade, dispose };
}
