/**
 * Live-call runtime (imperative shell). Owns the RTCPeerConnection, the bridge
 * adapter, media renegotiation, and the two-phase controller; drives all WebRTC
 * I/O by interpreting the pure `callSessionService` reducer's effects. Pushes a
 * flat `CallRuntimeState` snapshot to `onState` on every change so the React
 * surface (`CallWindowScreen`) stays a thin renderer.
 *
 * This is the call window's engine only — it never touches the chat transport
 * (Phase-1 signals relay over the MessagePort bridge to the main renderer, which
 * owns the on-chain send).
 */

import { type Subscription, firstValueFrom } from 'rxjs';

import { loadDeviceSelection } from '@/shared/call-media-devices';
import {
  type IceConfigParams,
  bufferIceCandidates,
  createCallPeerConnection,
  decodeCandidates,
  decodeMinimalSetup,
  encodeCandidates,
  encodeMinimalSetup,
} from '@/shared/peer-channel';
import { type CallSessionState, type MainToWindowMessage, callSessionService } from '@/domains/chat';

import { createBridgeAdapter } from './bridgeAdapter';
import { createCallController } from './callController';
import { type CallInit } from './callInitPort';
import { type MediaRenegotiationHandle, startMediaRenegotiation } from './mediaRenegotiation';

// Mobile parity (iOS ~1.5s / Android ~2s terminal-state dwell): after the call
// reaches a terminal status we keep the window up briefly so (a) the user sees
// "Call ended" and (b) the publishClosed signal has time to flush over the
// MessagePort bridge to the main renderer, which owns the on-chain send and
// completes it independently of this window's lifetime.
const TERMINAL_DWELL_MS = 1500;

export type CallRuntimeState = {
  callState: CallSessionState;
  remoteStream: Nullable<MediaStream>;
  remoteMicEnabled: boolean;
};

export type CallSession = {
  controller: ReturnType<typeof createCallController>;
  pc: ReturnType<typeof createCallPeerConnection>;
  getState: () => CallRuntimeState;
  dispose: () => void;
};

type CreateCallSessionParams = {
  callInit: CallInit;
  port: MessagePort;
  onState: (state: CallRuntimeState) => void;
};

export function createCallSession({ callInit, port, onState }: CreateCallSessionParams): CallSession {
  const { launch, iceConfig }: { launch: CallInit['launch']; iceConfig: IceConfigParams } = callInit;

  const pc = createCallPeerConnection({
    role: launch.direction === 'outgoing' ? 'initiator' : 'acceptor',
    dataChannelLabel: 'call',
    iceConfig,
    // The feature owns device persistence; the pc reads the current selection
    // on demand at getUserMedia time via this callback.
    getDeviceSelection: loadDeviceSelection,
  });

  let reneg: Nullable<MediaRenegotiationHandle> = null;
  let mediaUpgradeRequested = false;
  const subscriptions: Subscription[] = [];

  // Latest snapshot pushed to the React surface. Seeded from the pure initial
  // session state; the controller's onStateChange (fired during creation below)
  // and remote-track subscriptions keep it current.
  let snapshot: CallRuntimeState = {
    callState: callSessionService.initCallSession(launch),
    remoteStream: null,
    remoteMicEnabled: true,
  };
  function push(next: Partial<CallRuntimeState>): void {
    snapshot = { ...snapshot, ...next };
    onState(snapshot);
  }

  const controller = createCallController({
    launch,
    sendToBridge: msg => adapter.send(msg),

    applyRemoteAnswer: bytes => {
      void (async () => {
        const { setupSdp, candidates } = decodeMinimalSetup(bytes);
        await pc.applyRemoteAnswer(setupSdp);
        for (const c of candidates) {
          await pc.addRemoteCandidate(c);
        }
      })().catch(err => {
        console.error('[call-window] applyRemoteAnswer failed', err);
        controller.onFailed();
      });
    },

    addRemoteCandidates: bytes => {
      // Trickle candidates are a bare Vec<MinimalCandidate> (mobile parity),
      // NOT a full MinimalSetup — decode with the candidate-only codec.
      void (async () => {
        const candidates = decodeCandidates(bytes);
        console.info('[CALL] addRemoteCandidates: %d candidate(s)', candidates.length);
        for (const c of candidates) {
          await pc.addRemoteCandidate(c);
        }
      })().catch(err => {
        console.error('[CALL] addRemoteCandidates failed', err);
      });
    },

    prepareAnswer: () => {
      void (async () => {
        const gathered = firstCandidateBatch();
        const answer = await pc.createAnswer();
        const bytes = encodeMinimalSetup(answer, await gathered);
        console.info('[CALL] answer created (%d bytes) — publishing', bytes.length);
        controller.onLocalAnswerEncoded(bytes);
      })().catch(err => {
        // Most likely cause: the remote offer was never applied (empty/failed
        // provideIncomingCall), so createAnswer() rejects. Without this the
        // answer silently never sends and the caller hangs in "Calling".
        console.error('[call-window] prepareAnswer failed — no answer sent', err);
        controller.onFailed();
      });
    },

    startMediaUpgrade: () => {
      // Actual upgrade starts once the data channel is open (see dataChannelOpen$ below).
      mediaUpgradeRequested = true;
    },

    sendUpgradeRequest: () => {
      if (!reneg) {
        console.warn('[CALL] upgrade request before media renegotiation is ready — ignoring');
        return;
      }
      reneg.sendUpgradeRequest();
    },

    sendUpgradeDecline: () => {
      reneg?.sendUpgradeDecline();
    },

    upgradeToVideo: () => {
      // Requester side of an accepted upgrade: add a camera track to the live call
      // and re-offer over the data channel (in-band — no hang-up, no re-ring).
      if (!reneg) {
        console.warn('[CALL] upgradeToVideo before media renegotiation is ready — ignoring');
        return;
      }
      void reneg.requestVideoUpgrade().catch(err => console.error('[CALL] video upgrade failed', err));
    },

    confirmVideoUpgrade: () => {
      // Acceptor side: acquire the camera FIRST, then signal acceptance. Ordering
      // matters — the peer re-offers as soon as it sees our accept, so our video
      // sender must already exist or the answer comes back recv-only (one-way).
      void (async () => {
        await pc.addLocalVideo();
        reneg?.sendUpgradeAccept();
      })().catch(err => console.error('[CALL] confirm video upgrade failed', err));
    },

    sendMediaState: (track, enabled) => {
      reneg?.sendMediaState(track, enabled);
    },

    setLocalTrackEnabled: (track, enabled) => {
      pc.setLocalTrackEnabled(track, enabled);
    },

    closePeerConnection: () => {
      pc.close();
      reneg?.dispose();
    },

    closeWindow: () => {
      // Delay so publishClosed flushes over the bridge and the terminal state is
      // visible, then close the standalone call BrowserWindow. window.close()
      // from a renderer closes the Electron window it belongs to.
      setTimeout(() => window.close(), TERMINAL_DWELL_MS);
    },

    onStateChange: state => {
      console.info('[CALL] window state → %s', state.status);
      push({ callState: state });
    },
  });

  // Bridge: inbound messages from the main renderer.
  // Note: adapter references controller, but controller's sendToBridge references adapter.
  // Break the cycle by declaring adapter after the controller closure.
  const adapter = createBridgeAdapter(port, (msg: MainToWindowMessage) => {
    if (msg.kind === 'provideIncomingCall') {
      void (async () => {
        console.info('[CALL] provideIncomingCall: offer sdp=%d bytes, offerId=%s', msg.sdp.length, msg.offerId);
        if (msg.sdp.length === 0) {
          throw new Error('empty offer SDP — IncomingCallDetector did not stash it, or the offer chat message carried no sdp');
        }
        const { setupSdp, candidates } = decodeMinimalSetup(msg.sdp);
        await pc.applyRemoteOffer(setupSdp);
        for (const c of candidates) {
          await pc.addRemoteCandidate(c);
        }
        console.info('[CALL] remote offer applied (%d candidates) — ready to answer', candidates.length);
      })().catch(err => {
        console.error('[call-window] provideIncomingCall failed', err);
        controller.onFailed();
      });
    } else {
      controller.onBridgeMessage(msg);
    }
  });

  // Trickle gathered ICE candidates in batches (mobile parity — see
  // `bufferIceCandidates`) as a bare Vec<MinimalCandidate>, NOT an empty-SDP
  // MinimalSetup, which the peer cannot parse as a candidate frame.
  subscriptions.push(
    pc.localCandidates$.pipe(bufferIceCandidates()).subscribe(batch => {
      if (batch.length === 0) return;
      controller.onLocalCandidatesEncoded(encodeCandidates(batch));
    }),
  );

  /**
   * First batch of local candidates, to embed in an offer/answer setup so the
   * peer can start connectivity checks from the very first signaling message
   * (Android `awaitInitialCandidates()` before `encodeSetup`).
   *
   * This is a SECOND subscription alongside the trickle one above, each with its
   * own window, so an embedded candidate is also sent once as a trickle frame —
   * harmless, `addIceCandidate` ignores a duplicate. The single-subscription
   * shape device-sync's signaler uses is the better one; adopting it here needs
   * the call state machine restructured, which is deliberately not done now.
   */
  function firstCandidateBatch(): Promise<RTCIceCandidate[]> {
    return firstValueFrom(pc.localCandidates$.pipe(bufferIceCandidates()));
  }

  subscriptions.push(
    pc.dataChannelOpen$.subscribe(channel => {
      console.info('[CALL] data channel OPEN (role=%s) — dispatching + starting media upgrade', pc.role);
      // Dispatch FIRST so the reducer's startMediaUpgrade effect sets
      // mediaUpgradeRequested; only then do we know to renegotiate media.
      controller.onDataChannelOpen();
      if (mediaUpgradeRequested) {
        reneg = startMediaRenegotiation({
          channel,
          pc,
          role: pc.role,
          purpose: launch.purpose === 'video' ? 'video' : 'audio',
          onRemoteMediaState: (track, enabled) => {
            // Surface the peer's mic state so the UI can show a "mic is off" pill.
            if (track === 'microphone') {
              push({ remoteMicEnabled: enabled });
            }
          },
          onUpgradeRequest: () => controller.onRemoteUpgradeRequested(),
          onUpgradeAccept: () => controller.onRemoteUpgradeAccepted(),
          onUpgradeDecline: () => controller.onRemoteUpgradeDeclined(),
        });
      }
    }),
  );

  subscriptions.push(
    pc.connectionState$.subscribe(state => {
      console.info('[CALL] pc connectionState → %s', state);
      if (state === 'connected') {
        controller.onMediaConnected();
      } else if (state === 'failed' || state === 'disconnected' || state === 'closed') {
        controller.onFailed();
      }
    }),
  );

  // publishRemote builds a FRESH MediaStream each time (new reference) so
  // ActiveCallScreen's `[remoteStream]` attach effect re-runs and (re-)attaches
  // the <audio> once the audio track is present — mutating one stream in place
  // keeps the same identity and the effect would not re-run. Accumulate every
  // track so an early audio track is never dropped ("video but no sound").
  const collectedTracks: MediaStreamTrack[] = [];
  subscriptions.push(
    pc.remoteTracks$.subscribe(track => {
      collectedTracks.push(track);
      console.info('[CALL] remote track received (kind=%s, id=%s) — %d total', track.kind, track.id, collectedTracks.length);
      push({ remoteStream: new MediaStream(collectedTracks) });
    }),
  );

  // Outgoing call: kick off the initial offer.
  if (launch.direction === 'outgoing') {
    void (async () => {
      const gathered = firstCandidateBatch();
      const offer = await pc.createOffer();
      const bytes = encodeMinimalSetup(offer, await gathered);
      console.info('[CALL] outgoing: offer created (%d bytes) — publishing, offerId=%s', bytes.length, launch.offerId);
      controller.onLocalOfferEncoded(bytes);
    })().catch(err => {
      console.error('[CALL] outgoing offer creation failed', err);
      controller.onFailed();
    });
  }

  return {
    controller,
    pc,
    getState: () => snapshot,
    dispose: () => {
      for (const sub of subscriptions) sub.unsubscribe();
      pc.close();
      reneg?.dispose();
    },
  };
}
