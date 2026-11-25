/**
 * Thin RTCPeerConnection wrapper for device-sync (data channel only, no media).
 * Initiator creates the DC up front; acceptor receives it via `datachannel`.
 * Local ICE goes out on `localCandidates$`; inbound via `addRemoteCandidate`.
 */

import { type Observable, BehaviorSubject, Subject } from 'rxjs';

import { type IceConfigParams, buildIceConfig } from './iceConfig';

export type PeerConnectionRole = 'initiator' | 'acceptor';

export type PeerConnectionParams = {
  role: PeerConnectionRole;
  dataChannelLabel: string;
  iceConfig: IceConfigParams;
};

export type PeerConnection = {
  role: PeerConnectionRole;
  createOffer: () => Promise<RTCSessionDescriptionInit>;
  createAnswer: () => Promise<RTCSessionDescriptionInit>;
  applyRemoteOffer: (offer: RTCSessionDescriptionInit) => Promise<void>;
  applyRemoteAnswer: (answer: RTCSessionDescriptionInit) => Promise<void>;
  /**
   * Rollback the most recent `setRemoteDescription({type:'answer'})` back to
   * `have-local-offer` so a fresh Answer can be applied. Local description
   * is preserved per WebRTC spec.
   */
  applyRemoteRollback: () => Promise<void>;
  addRemoteCandidate: (candidate: RTCIceCandidateInit) => Promise<void>;
  localCandidates$: Observable<RTCIceCandidate>;
  dataChannelOpen$: Observable<RTCDataChannel>;
  connectionState: () => RTCPeerConnectionState;
  signalingState: () => RTCSignalingState;
  /** Hot stream of `connectionState` transitions; emits current state on subscribe. */
  connectionState$: Observable<RTCPeerConnectionState>;
  close: () => void;
};

// Shared id counter so concurrent PCs are distinguishable in logs.
let nextPcId = 1;

export function createPeerConnection(params: PeerConnectionParams): PeerConnection {
  const pcId = nextPcId++;
  const config = buildIceConfig(params.iceConfig);

  const pc = new RTCPeerConnection(config);

  const localCandidates$ = new Subject<RTCIceCandidate>();
  const dataChannelOpen$ = new Subject<RTCDataChannel>();
  const connectionState$ = new BehaviorSubject<RTCPeerConnectionState>(pc.connectionState);

  pc.addEventListener('icecandidate', ev => {
    if (ev.candidate) {
      localCandidates$.next(ev.candidate);
    }
  });

  pc.addEventListener('connectionstatechange', () => {
    connectionState$.next(pc.connectionState);
  });

  function wireChannel(channel: RTCDataChannel): void {
    channel.addEventListener('error', ev => {
      // RTCErrorEvent is the actual shape, but its typing varies between TS
      // lib versions / DOM type roots; for diagnostic logging we only need
      // `error.message`, so probe the shape defensively rather than depend on
      // RTCErrorEvent being globally available at compile time.
      const detail =
        'error' in ev && ev.error && typeof ev.error === 'object' && 'message' in ev.error
          ? String(ev.error.message)
          : '(no detail)';
      console.error('WEBRTC [pc#%d] data channel ERROR: %s', pcId, detail);
    });
    if (channel.readyState === 'open') {
      // Already open by the time we attached the listener — surface immediately.
      // Skip the `open` listener: it would fire a second time on the same
      // channel and spawn a duplicate sync state machine downstream.
      dataChannelOpen$.next(channel);
      return;
    }
    channel.addEventListener('open', () => {
      dataChannelOpen$.next(channel);
    });
  }

  if (params.role === 'initiator') {
    wireChannel(pc.createDataChannel(params.dataChannelLabel));
  } else {
    pc.addEventListener('datachannel', ev => {
      wireChannel(ev.channel);
    });
  }

  return {
    role: params.role,
    createOffer: async () => {
      const offer = await pc.createOffer();
      await pc.setLocalDescription(offer);
      return offer;
    },
    createAnswer: async () => {
      const answer = await pc.createAnswer();
      await pc.setLocalDescription(answer);
      return answer;
    },
    applyRemoteOffer: async offer => {
      await pc.setRemoteDescription(offer);
    },
    applyRemoteAnswer: async answer => {
      await pc.setRemoteDescription(answer);
    },
    applyRemoteRollback: async () => {
      /* eslint-disable-next-line @typescript-eslint/consistent-type-assertions -- TS lib.dom doesn't expose 'rollback' as a valid RTCSdpType but it is per WebRTC spec */
      await pc.setRemoteDescription({ type: 'rollback' } as RTCSessionDescriptionInit);
    },
    addRemoteCandidate: async candidate => {
      await pc.addIceCandidate(candidate);
    },
    localCandidates$: localCandidates$.asObservable(),
    dataChannelOpen$: dataChannelOpen$.asObservable(),
    connectionState: () => pc.connectionState,
    signalingState: () => pc.signalingState,
    connectionState$: connectionState$.asObservable(),
    close: () => {
      pc.close();
      connectionState$.complete();
    },
  };
}
