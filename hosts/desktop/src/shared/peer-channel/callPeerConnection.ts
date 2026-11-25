/**
 * Media-capable RTCPeerConnection wrapper for A/V calls (audio + video).
 * Mirrors peerConnection.ts (data-channel-only, device-sync) but adds
 * getUserMedia, addTrack, remoteTracks$, and mute/camera-toggle helpers.
 */

import { type Observable, BehaviorSubject, Subject } from 'rxjs';

import { type CallDeviceSelection } from '@/shared/call-media-devices';

import { type IceConfigParams, buildIceConfig } from './iceConfig';

export type CallPeerRole = 'initiator' | 'acceptor';
export type CallMediaPurpose = 'audio' | 'video';

export type CallPeerConnectionParams = {
  role: CallPeerRole;
  dataChannelLabel: string;
  iceConfig: IceConfigParams;
  // Current camera/mic device selection, read on demand at getUserMedia time.
  // Injected — this WebRTC primitive does not read the persisted preference
  // itself, so it stays free of the localStorage device concern; the caller
  // owns where the selection comes from.
  getDeviceSelection: () => CallDeviceSelection;
};

export type CallPeerConnection = {
  role: CallPeerRole;
  /** Creates an SDP offer, sets local description, returns the SDP text. */
  createOffer: () => Promise<string>;
  /** Creates an SDP answer, sets local description, returns the SDP text. */
  createAnswer: () => Promise<string>;
  /** Applies the remote offer SDP via setRemoteDescription({ type: 'offer', sdp }). */
  applyRemoteOffer: (sdp: string) => Promise<void>;
  /** Applies the remote answer SDP via setRemoteDescription({ type: 'answer', sdp }). */
  applyRemoteAnswer: (sdp: string) => Promise<void>;
  addRemoteCandidate: (candidate: RTCIceCandidateInit) => Promise<void>;
  /** Acquires getUserMedia and adds the tracks to the peer connection. */
  addLocalMedia: (purpose: CallMediaPurpose) => Promise<void>;
  /** Adds a camera track to an already-established call (audio→video upgrade),
   *  merging it into the existing local stream. The caller re-offers afterwards. */
  addLocalVideo: () => Promise<void>;
  /** Toggles the enabled flag on the corresponding local track. */
  setLocalTrackEnabled: (track: 'camera' | 'microphone', enabled: boolean) => void;
  /** Hot-swaps the live camera/microphone device: re-acquires the chosen device,
   *  replaceTrack()s the matching sender (no SDP renegotiation), stops the old
   *  track, and swaps it inside localStream so the self-view updates. No-op when
   *  there is no matching sender yet (e.g. camera before an audio→video upgrade). */
  switchDevice: (track: 'camera' | 'microphone', deviceId: Nullable<string>) => Promise<void>;
  /** Returns the local MediaStream, if one has been acquired. */
  localStream: () => Nullable<MediaStream>;
  localCandidates$: Observable<RTCIceCandidate>;
  dataChannelOpen$: Observable<RTCDataChannel>;
  /** Emits each remote track as it arrives (pc.ontrack). */
  remoteTracks$: Observable<MediaStreamTrack>;
  /** Hot stream of connectionState transitions; emits the current state on subscribe. */
  connectionState$: Observable<RTCPeerConnectionState>;
  close: () => void;
};

// Shared counter — keeps concurrent call PCs distinguishable in logs.
let nextCallPcId = 1;

// Build a getUserMedia audio/video constraint from a persisted selection.
// A chosen device uses `{ deviceId: { exact } }`; the system default uses `true`.
function trackConstraint(deviceId: Nullable<string>): MediaTrackConstraints | boolean {
  return deviceId === null ? true : { deviceId: { exact: deviceId } };
}

// getUserMedia that retries once without the deviceId constraint if the chosen
// device is gone (OverconstrainedError), so a stale selection never blocks a call.
async function getMediaWithFallback(constraints: MediaStreamConstraints): Promise<MediaStream> {
  try {
    return await navigator.mediaDevices.getUserMedia(constraints);
  } catch (err) {
    const isOverconstrained = err instanceof Error && err.name === 'OverconstrainedError';
    if (!isOverconstrained) throw err;
    console.warn('[CALL] selected device unavailable — falling back to system default');
    const fallback: MediaStreamConstraints = {
      audio: constraints.audio ? true : false,
      video: constraints.video ? true : false,
    };
    return navigator.mediaDevices.getUserMedia(fallback);
  }
}

export function createCallPeerConnection(params: CallPeerConnectionParams): CallPeerConnection {
  const pcId = nextCallPcId++;
  const { getDeviceSelection } = params;
  const config = { ...buildIceConfig(params.iceConfig), iceCandidatePoolSize: 8 };

  const pc = new RTCPeerConnection(config);

  const localCandidates$ = new Subject<RTCIceCandidate>();
  const dataChannelOpen$ = new Subject<RTCDataChannel>();
  const remoteTracks$ = new Subject<MediaStreamTrack>();
  const connectionState$ = new BehaviorSubject<RTCPeerConnectionState>(pc.connectionState);

  let localStream: Nullable<MediaStream> = null;

  pc.addEventListener('icecandidate', ev => {
    if (ev.candidate) {
      localCandidates$.next(ev.candidate);
    }
  });

  pc.addEventListener('connectionstatechange', () => {
    connectionState$.next(pc.connectionState);
  });

  pc.addEventListener('track', ev => {
    remoteTracks$.next(ev.track);
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
      console.error('WEBRTC CALL [pc#%d] data channel ERROR: %s', pcId, detail);
    });
    if (channel.readyState === 'open') {
      // Already open — surface immediately and skip the `open` listener to
      // avoid firing a second downstream event for the same channel.
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
      return offer.sdp ?? '';
    },

    createAnswer: async () => {
      const answer = await pc.createAnswer();
      await pc.setLocalDescription(answer);
      return answer.sdp ?? '';
    },

    applyRemoteOffer: async sdp => {
      await pc.setRemoteDescription({ type: 'offer', sdp });
    },

    applyRemoteAnswer: async sdp => {
      await pc.setRemoteDescription({ type: 'answer', sdp });
    },

    addRemoteCandidate: async candidate => {
      await pc.addIceCandidate(candidate);
    },

    addLocalMedia: async purpose => {
      let stream: MediaStream;
      const selection: CallDeviceSelection = getDeviceSelection();
      const constraints: MediaStreamConstraints = {
        audio: trackConstraint(selection.microphoneId),
        video: purpose === 'video' ? trackConstraint(selection.cameraId) : false,
      };
      try {
        stream = await getMediaWithFallback(constraints);
      } catch (err) {
        console.error('[CALL] getUserMedia FAILED (mic/camera permission or device?):', err);
        throw err;
      }
      localStream = stream;
      const kinds = stream
        .getTracks()
        .map(t => t.kind)
        .join('+');
      console.info('[CALL] getUserMedia ok — local tracks: %s; adding to pc', kinds || '(none)');
      for (const track of stream.getTracks()) {
        pc.addTrack(track, stream);
      }
      if (purpose === 'video') {
        // Best-effort bitrate cap on the video sender — not all Chromium builds
        // support setParameters on a freshly added sender, so we guard with try/catch.
        const videoSender = pc.getSenders().find(s => s.track?.kind === 'video');
        if (videoSender) {
          try {
            const sendParams = videoSender.getParameters();
            sendParams.encodings = [{ maxBitrate: 1_500_000 }];
            await videoSender.setParameters(sendParams);
          } catch (err) {
            console.debug('WEBRTC CALL [pc#%d] setParameters (video bitrate cap) failed: %o', pcId, err);
          }
        }
      }
    },

    addLocalVideo: async () => {
      let videoStream: MediaStream;
      const selection = getDeviceSelection();
      try {
        videoStream = await getMediaWithFallback({ video: trackConstraint(selection.cameraId) });
      } catch (err) {
        console.error('[CALL] getUserMedia(video) FAILED (camera permission or device?):', err);
        throw err;
      }
      const videoTrack = videoStream.getVideoTracks()[0];
      if (!videoTrack) return;
      // Merge into the existing local stream so the UI's self-view (which is bound
      // to that stream) picks the track up without re-attaching.
      const stream = localStream ?? videoStream;
      if (localStream) localStream.addTrack(videoTrack);
      localStream = stream;
      pc.addTrack(videoTrack, stream);
      const videoSender = pc.getSenders().find(s => s.track?.kind === 'video');
      if (videoSender) {
        try {
          const sendParams = videoSender.getParameters();
          sendParams.encodings = [{ maxBitrate: 1_500_000 }];
          await videoSender.setParameters(sendParams);
        } catch (err) {
          console.debug('WEBRTC CALL [pc#%d] setParameters (video bitrate cap) failed: %o', pcId, err);
        }
      }
    },

    setLocalTrackEnabled: (track, enabled) => {
      if (!localStream) return;
      const kind = track === 'microphone' ? 'audio' : 'video';
      for (const t of localStream.getTracks()) {
        if (t.kind === kind) {
          t.enabled = enabled;
        }
      }
    },

    switchDevice: async (track, deviceId) => {
      const kind = track === 'microphone' ? 'audio' : 'video';
      const sender = pc.getSenders().find(s => s.track?.kind === kind);
      if (!sender) {
        console.info('[CALL] switchDevice(%s): no %s sender yet — ignoring', track, kind);
        return;
      }

      const constraints: MediaStreamConstraints =
        kind === 'audio' ? { audio: trackConstraint(deviceId) } : { video: trackConstraint(deviceId) };
      const nextStream = await getMediaWithFallback(constraints);
      const nextTrack = nextStream.getTracks().find(t => t.kind === kind) ?? null;
      if (!nextTrack) {
        console.warn('[CALL] switchDevice(%s): acquired stream had no %s track', track, kind);
        return;
      }

      // Preserve the current enabled state (a muted mic must stay muted after swap).
      const previous = sender.track;
      if (previous) nextTrack.enabled = previous.enabled;

      await sender.replaceTrack(nextTrack);

      if (localStream) {
        if (previous) localStream.removeTrack(previous);
        localStream.addTrack(nextTrack);
      }
      if (previous) previous.stop();
      console.info('[CALL] switchDevice(%s) → replaced live %s track', track, kind);
    },

    localStream: () => localStream,

    localCandidates$: localCandidates$.asObservable(),
    dataChannelOpen$: dataChannelOpen$.asObservable(),
    remoteTracks$: remoteTracks$.asObservable(),
    connectionState$: connectionState$.asObservable(),

    close: () => {
      if (localStream) {
        for (const track of localStream.getTracks()) {
          track.stop();
        }
        localStream = null;
      }
      pc.close();
      connectionState$.complete();
      localCandidates$.complete();
      dataChannelOpen$.complete();
      remoteTracks$.complete();
    },
  };
}
