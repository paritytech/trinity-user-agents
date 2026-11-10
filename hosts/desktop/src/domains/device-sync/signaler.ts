/**
 * Drives offer/answer/candidate handshake between a `DeviceSessionChannel`
 * (signaling transport) and a `PeerConnection` (WebRTC) for one peer device.
 * Caller subscribes to `dataChannel$` for the ready sync DC.
 *
 * Wire shape: every message is a `SyncSignalingEnvelope { offerId, message }`.
 * The offerId correlates a single connection attempt across both peers; stale
 * answers / candidates redelivered by the persistent statement-store carry a
 * non-matching offerId and are dropped on receive — this kills the
 * stale-Answer poisoning that used to deadlock respawn cycles.
 *
 * Role rule (set by the caller): the peer with the smaller statementAccountId
 * (unsigned bytes compare) is the initiator. The initiator MINTS a fresh
 * offerId for each new local Offer. The acceptor ADOPTS the offerId from the
 * incoming Offer and replies with the same one. The lifecycle:
 *
 *   - Initiator:
 *       startup → mint offerId, send Offer{offerId}
 *       on receive Answer{matching offerId}      → apply + persist offerId
 *       on receive Answer{non-matching offerId}  → DROP silently
 *       on receive Candidates{matching offerId}  → apply
 *       on receive Reconnected{matching offerId} → bubble up via onResetRequest
 *
 *   - Acceptor:
 *       startup → wait
 *       on receive Offer{offerId} → adopt (set currentOfferId BEFORE answering
 *                                   so gathered ICE rides the right id) +
 *                                   persist + answer with same offerId
 *       on receive Candidates{matching offerId}  → apply
 *       on receive Reconnected{matching offerId} → bubble up via onResetRequest
 *
 * Reconnected is RESTART-ONLY (one-shot from the orchestrator at process start
 * when a persisted offerId exists). The signaler never sends Reconnected and
 * never re-posts Offers on Reconnected — the envelope makes that hack moot.
 */

import { type Observable, type Subscription } from 'rxjs';

import {
  type MinimalCandidate,
  MinimalCandidatesVecCodec,
  bufferIceCandidates,
  decodeMinimalSetup,
  encodeMinimalSetup,
  minimalToRtcCandidateInit,
  rtcCandidateToMinimal,
} from '@/shared/peer-channel';
import { type PeerConnection } from '@/shared/peer-channel';
import { type DeviceSessionChannel, type SyncSignalingEnvelope } from '@/domains/device-session';

type SignalerRole = 'initiator' | 'acceptor';

type SignalerParams = {
  session: DeviceSessionChannel;
  peerConnection: PeerConnection;
  role: SignalerRole;
  /**
   * A previous attempt's offerId that the peer may still be holding, from
   * persistence. Restart recovery: it tells the peer to dispose that attempt.
   *
   * The initiator sends it in the SAME statement as its fresh Offer — the peer
   * consumes a `Reconnected` together with whatever follows it in that request
   * (iOS `reconnection(in:)` → `(offerId, following)`), so splitting them makes
   * the peer reset against an empty `following` and then meet the Offer with a
   * connection it has just torn down. The acceptor has no Offer to pair it with
   * and sends it alone, which is the one legitimate lone-`Reconnected` case.
   *
   * Undefined when there is nothing to dispose, or on any spawn after the first
   * (the spec sends it once per process).
   */
  reconnectOfferId?: string;
  /**
   * Persists the agreed offerId for restart recovery. Fired:
   *   - acceptor: immediately upon ADOPTING an incoming Offer's offerId,
   *   - initiator: only after RECEIVING the matching Answer (proof the
   *     acceptor has the same offerId).
   * Wire-up: orchestrator writes the offerId to `deviceSyncRepository`.
   */
  onAcceptedOfferId?: (offerId: string) => void;
  /**
   * Bubbles up a `Reconnected` whose offerId matches our current attempt.
   * The orchestrator's contract: dispose this signaler + PC and spawn a
   * fresh attempt. The signaler does NOT dispose itself.
   */
  onResetRequest?: () => void;
};

export type SignalerHandle = {
  dataChannel$: Observable<RTCDataChannel>;
  close: () => void;
};

const newOfferId = (): string => crypto.randomUUID();

export function startSignaler(params: SignalerParams): SignalerHandle {
  const { session, peerConnection: pc, role, reconnectOfferId, onAcceptedOfferId, onResetRequest } = params;
  const subscriptions: Subscription[] = [];

  /**
   * The offerId of THIS signaler's current connection attempt.
   *   - initiator: minted before sendOffer; immutable for this signaler's life.
   *   - acceptor: null until the first Offer is adopted, then set to that
   *     Offer's offerId (synchronously, before the Answer is built so any ICE
   *     candidates gathered while answering ride the correct id).
   */
  let currentOfferId: string | null = role === 'initiator' ? newOfferId() : null;

  // Set on close(). Guards the inbound handler: a batch is emitted synchronously
  // by channel.ts, so several handleIncoming calls can already be chained on
  // `inboundQueue` when an earlier one (a matching Reconnected) triggers a
  // reset → orchestrator close(). Without this, the already-queued handlers
  // would run applyRemoteOffer / addRemoteCandidate against a torn-down PC.
  let closed = false;

  // A reset is requested at most once per signaler. The persistent
  // statement-store can re-deliver the same Reconnected to a fresh
  // subscription; only the first may drive a respawn (otherwise a replay storm
  // could thrash respawn cycles).
  let resetRequested = false;
  function requestReset(): void {
    if (resetRequested) return;
    resetRequested = true;
    onResetRequest?.();
  }

  if (role === 'initiator') {
    void sendOffer(currentOfferId!);
  } else if (reconnectOfferId !== undefined) {
    // Acceptor: nothing to pair the Reconnected with — it exists precisely to
    // prompt the initiator to re-offer, so it goes out on its own.
    void session
      .send({ offerId: reconnectOfferId, message: { tag: 'Reconnected', value: undefined } })
      .catch(err =>
        console.warn('WEBRTC [signaler] reconnected send failed: %s', err instanceof Error ? err.message : String(err)),
      );
  }

  /**
   * Local candidates, batched (mobile parity — see `bufferIceCandidates`). The
   * FIRST batch rides the Offer/Answer setup; every later batch trickles as its
   * own Candidates frame, tagged with the current offerId so a peer that has
   * moved on to a newer attempt drops it.
   *
   * Subscribed at the send site, immediately BEFORE createOffer/createAnswer —
   * not at construction. `bufferTime` opens its first window on subscribe, so
   * subscribing here is what anchors that window to the moment gathering starts.
   * An acceptor that subscribed at construction would resolve its first batch
   * ~500ms later, long before the peer's Offer arrives, and would answer with an
   * empty setup — exactly the bug this fixes. Subscribing before the await also
   * closes the gap in which a candidate could fire and be dropped:
   * `localCandidates$` is a plain Subject and does not replay.
   *
   * Called once per signaler — an initiator reaches it through sendOffer, an
   * acceptor through sendAnswer, never both.
   */
  function startCandidateFlow(): Promise<RTCIceCandidate[]> {
    return new Promise<RTCIceCandidate[]>(resolve => {
      let isFirstBatch = true;
      const candidateSub = pc.localCandidates$.pipe(bufferIceCandidates()).subscribe({
        next: batch => {
          if (isFirstBatch) {
            isFirstBatch = false;
            resolve(batch);

            return;
          }
          sendCandidates(batch);
        },
      });
      subscriptions.push(candidateSub);
    });
  }

  function sendCandidates(batch: RTCIceCandidate[]): void {
    if (batch.length === 0) return;
    if (currentOfferId === null) {
      // Acceptor before it adopted an Offer — nothing to correlate with. The PC
      // only gathers after applyRemoteOffer, by which point we've already set
      // currentOfferId, so this is effectively unreachable.
      return;
    }

    const minimal: MinimalCandidate[] = [];
    for (const candidate of batch) {
      const encoded = rtcCandidateToMinimal(candidate);
      if (encoded) minimal.push(encoded);
    }
    if (minimal.length === 0) return;

    void session.send({
      offerId: currentOfferId,
      message: { tag: 'Candidates', value: { candidates: MinimalCandidatesVecCodec.enc(minimal) } },
    });
  }

  async function sendOffer(offerId: string): Promise<void> {
    try {
      const gathered = startCandidateFlow();
      const offer = await pc.createOffer();
      const setupBytes = encodeMinimalSetup(offer.sdp ?? '', await gathered);
      // close() can land while we wait out the batch window — an external
      // respawn runs independently of this send.
      if (closed) return;
      // One statement, Reconnected first: the peer disposes the old attempt and
      // adopts this Offer from the same request. See `reconnectOfferId`.
      const envelopes: SyncSignalingEnvelope[] = [];
      if (reconnectOfferId !== undefined) {
        envelopes.push({ offerId: reconnectOfferId, message: { tag: 'Reconnected', value: undefined } });
      }
      envelopes.push({ offerId, message: { tag: 'Offer', value: { sdp: setupBytes } } });
      await session.sendAll(envelopes);
    } catch (err) {
      console.error('WEBRTC [signaler] sendOffer failed: %s', err instanceof Error ? err.message : String(err));
    }
  }

  async function sendAnswer(offerId: string): Promise<void> {
    const gathered = startCandidateFlow();
    const answer = await pc.createAnswer();
    const setupBytes = encodeMinimalSetup(answer.sdp ?? '', await gathered);
    if (closed) return;
    await session.send({ offerId, message: { tag: 'Answer', value: { sdp: setupBytes } } });
  }

  // Apply a batch of remote ICE candidates, surviving individual failures — a
  // malformed/duplicate candidate must not abort the rest of the batch.
  async function addRemoteCandidates(candidates: RTCIceCandidateInit[], phase: 'initial' | 'trickle'): Promise<void> {
    for (const c of candidates) {
      // close() can land between awaits — an external respawn (handshake
      // timeout / connectionState failed) runs independently of the inbound
      // queue — so stop touching the torn-down PC the moment it does.
      if (closed) return;
      try {
        await pc.addRemoteCandidate(c);
      } catch (e) {
        console.warn('WEBRTC [signaler] addRemoteCandidate (%s) failed: %s', phase, e instanceof Error ? e.message : String(e));
      }
    }
  }

  async function handleIncoming(envelope: SyncSignalingEnvelope): Promise<void> {
    // Disposed mid-batch — drop the rest of the queue (see `closed` above).
    if (closed) return;
    const content = envelope.message;

    if (content.tag === 'Reconnected') {
      // Spec §5.4: dispose iff the envelope's offerId matches our current
      // active offerId. Otherwise ignore (stale / already replaced).
      if (currentOfferId !== null && envelope.offerId === currentOfferId) {
        requestReset();
      }
      return;
    }

    if (content.tag === 'Offer') {
      if (role !== 'acceptor') return;

      if (currentOfferId === null) {
        // First Offer for this signaler — adopt it. Set the id SYNCHRONOUSLY
        // before answering so any ICE candidate that fires during
        // createAnswer/setLocalDescription rides the correct id. currentOfferId
        // is now immutable for this signaler's life (see the other branches),
        // so trickle candidates can never be tagged with a later offer's id.
        currentOfferId = envelope.offerId;
        onAcceptedOfferId?.(envelope.offerId);
        const { setupSdp, candidates } = decodeMinimalSetup(content.value.sdp);
        await pc.applyRemoteOffer({ type: 'offer', sdp: setupSdp });
        // An external respawn may have closed us during applyRemoteOffer; don't
        // add candidates or build an Answer (createAnswer) against the dead PC.
        if (closed) return;
        await addRemoteCandidates(candidates, 'initial');
        if (closed) return;
        await sendAnswer(envelope.offerId);
        return;
      }

      if (envelope.offerId === currentOfferId) {
        // A duplicate of the attempt we already adopted (statement-store replay
        // under a fresh requestId). Never re-apply — setRemoteDescription on a
        // non-stable PC tears down in-flight DTLS.
        return;
      }

      // A different offerId means the initiator minted a fresh attempt (it
      // restarted or respawned). Our PC belongs to the dead attempt — even if
      // it still reports 'connected' (ICE hasn't timed out yet). Re-adopting in
      // place can't work (the PC can't be rebuilt here), so request a reset and
      // let the orchestrator respawn a fresh PC that adopts the new Offer.
      requestReset();
      return;
    }

    if (content.tag === 'Answer') {
      if (role !== 'initiator') return;
      // Reject stale Answers from previous attempts: the persistent
      // statement-store re-delivers the latest stored Answer to fresh
      // subscribers; if it's for a previous Offer, drop it. This single check
      // replaces the old rollback-and-reapply dance.
      if (envelope.offerId !== currentOfferId) {
        return;
      }
      const signalingState = pc.signalingState();
      if (signalingState !== 'have-local-offer') {
        return;
      }
      const { setupSdp, candidates } = decodeMinimalSetup(content.value.sdp);
      await pc.applyRemoteAnswer({ type: 'answer', sdp: setupSdp });
      await addRemoteCandidates(candidates, 'initial');
      // Initiator persists offerId only now — the Answer proves the acceptor
      // adopted the same id.
      onAcceptedOfferId?.(envelope.offerId);
      return;
    }

    if (content.tag === 'Candidates') {
      if (envelope.offerId !== currentOfferId) {
        return;
      }
      let decoded: ReturnType<typeof MinimalCandidatesVecCodec.dec>;
      try {
        decoded = MinimalCandidatesVecCodec.dec(content.value.candidates);
      } catch (e) {
        console.warn('WEBRTC [signaler] Candidates decode failed: %s', e instanceof Error ? e.message : String(e));
        return;
      }
      await addRemoteCandidates(decoded.map(minimalToRtcCandidateInit), 'trickle');
    }
  }

  // Serial inbound queue. Android batches multiple messages per wire frame;
  // running handleIncoming in parallel races the RTCPC state machine
  // ("Called in wrong state: stable" on consecutive applyRemoteOffer).
  let inboundQueue: Promise<void> = Promise.resolve();
  const messagesSub = session.messages$.subscribe({
    next: envelope => {
      inboundQueue = inboundQueue.then(() =>
        handleIncoming(envelope).catch(err => {
          console.error(
            'WEBRTC [signaler] handleIncoming(%s) failed: %s',
            envelope.message.tag,
            err instanceof Error ? err.message : String(err),
          );
        }),
      );
    },
  });
  subscriptions.push(messagesSub);

  return {
    dataChannel$: pc.dataChannelOpen$,
    close: () => {
      closed = true;
      for (const s of subscriptions) s.unsubscribe();
      session.close();
      pc.close();
    },
  };
}
