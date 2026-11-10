/**
 * Device-to-device signaling channel: Observable wrapper around statement-store
 * post/subscribe with payload encryption applied transparently. Wire shape
 * matches Android's statement-store session layer (chat / video / sync share
 * it):
 *   plaintext  = SCALE(SignalingStatementData.Request { requestId, messages: [SCALE(SignalingMessage), …] })
 * where `messages` carries the FULL unacked batch (see `unackedSignals`), not
 * just the newest signal — each request statement replaces the previous one.
 *   ciphertext = ChaCha20-Poly1305(nonce || encrypted), see ../service.ts
 * On Request, channel emits inner SignalingMessage to `messages$` AND posts
 * back `Response { requestId, responseCode: 0 }`; without the ACK Android's
 * retry layer re-emits forever. Topics are directional (see ../service.ts).
 */

import { khash } from '@novasamatech/statement-store';
import { nanoid } from 'nanoid';
import { toHex } from 'polkadot-api/utils';
import { Subject } from 'rxjs';

import { SyncSignalingEnvelopeCodec } from '@/shared/peer-channel';
import { SignalingStatementData } from '../schemas';
import { deviceSessionService } from '../service';
import { type DeviceSessionChannel, type DeviceSessionDeps, type SyncSignalingEnvelope } from '../types';

const ACK_RESPONSE_CODE = 0;

const REQUEST_LABEL = new TextEncoder().encode('request');
const RESPONSE_LABEL = new TextEncoder().encode('response');

function createChannel(deps: DeviceSessionDeps): DeviceSessionChannel {
  // X25519 shared secret (32B), used whole — CHAT-RFC-0004. Derived once per
  // channel and reused for both the topics and the payload encryption.
  const sharedSecret = deviceSessionService.deriveSharedSecret(deps.ourDeviceEncPriv, deps.peerDeviceEncPub);
  const encryption = deviceSessionService.createPayloadEncryption(sharedSecret);

  const outgoingTopic = deviceSessionService.deriveDeviceSessionTopic(
    sharedSecret,
    { accountId: deps.ourStatementAccountId },
    { accountId: deps.peerStatementAccountId },
  );
  const incomingTopic = deviceSessionService.deriveDeviceSessionTopic(
    sharedSecret,
    { accountId: deps.peerStatementAccountId },
    { accountId: deps.ourStatementAccountId },
  );

  // One statement per (account, channel) in the store: requests replace
  // requests, responses replace responses — matching Android's session layer
  // (`requestChannel(topic)` / `responseChannel(topic)`). Channel-less posts
  // would instead burn account slots until the store starts evicting our own
  // earlier statements (an Offer evicted by its own trailing ICE candidates
  // was exactly how a late-polling acceptor ended up never seeing an Offer).
  const requestChannel = khash(outgoingTopic, REQUEST_LABEL);
  const responseChannel = khash(outgoingTopic, RESPONSE_LABEL);

  // Android re-emits Request until it sees our Response. ACK every time
  // (to stop the loop) but forward inner message to out$ only once.
  const seenRequestIds = new Set<string>();

  // ── Outgoing unacked batch ────────────────────────────────────────────────
  // Because the new request statement REPLACES the previous one on
  // `requestChannel`, the latest statement must carry every signaling message
  // the peer hasn't acked yet — otherwise a peer that polls late (instead of
  // being live-subscribed) only ever sees the last message and loses the
  // rest (e.g. sees trailing ICE candidates but never the Offer). Identical
  // payloads are deduped so a re-posted Offer refreshes the batch instead of
  // duplicating inside it.
  type UnackedSignal = { signalId: string; bytes: Uint8Array; bytesHex: string };
  let unackedSignals: UnackedSignal[] = [];
  // requestId → signalIds that submission carried (ack drops exactly those).
  const requestCoverage = new Map<string, Set<string>>();

  async function postCiphertext(plaintext: Uint8Array, channel: Uint8Array): Promise<void> {
    const ciphertext = encryption.encrypt(plaintext);
    await deps.post(outgoingTopic, ciphertext, channel);
  }

  async function sendAck(requestId: string): Promise<void> {
    const plaintext = SignalingStatementData.enc({
      tag: 'Response',
      value: { requestId, responseCode: ACK_RESPONSE_CODE },
    });
    try {
      await postCiphertext(plaintext, responseChannel);
    } catch (e) {
      // Best-effort. Peer will re-emit, and our next ACK attempt may land.
      console.warn(
        '[device-session channel] failed to ACK requestId=%s: %s',
        requestId,
        e instanceof Error ? e.message : String(e),
      );
    }
  }

  const out$ = new Subject<SyncSignalingEnvelope>();
  const subscription = deps.subscribe(incomingTopic).subscribe({
    next: ({ data }) => {
      let plaintext: Uint8Array;
      try {
        plaintext = encryption.decrypt(data);
      } catch (e) {
        console.warn(
          '[device-session channel] decrypt FAILED on incomingTopic=%s dataLen=%d firstBytes=%s err=%s',
          toHex(incomingTopic),
          data.length,
          toHex(data.slice(0, Math.min(16, data.length))),
          e instanceof Error ? e.message : String(e),
        );
        return;
      }

      let envelope: ReturnType<typeof SignalingStatementData.dec>;
      try {
        envelope = SignalingStatementData.dec(plaintext);
      } catch (e) {
        console.warn(
          '[device-session channel] SignalingStatementData decode FAILED plaintextLen=%d prefix=%s err=%s',
          plaintext.length,
          toHex(plaintext.slice(0, Math.min(32, plaintext.length))),
          e instanceof Error ? e.message : String(e),
        );
        return;
      }

      if (envelope.tag === 'Request') {
        const { requestId, messages } = envelope.value;
        // ACK every Request — the peer's retry logic gates on us echoing
        // the requestId back, regardless of whether we've seen it before.
        void sendAck(requestId);

        if (seenRequestIds.has(requestId)) return;
        seenRequestIds.add(requestId);

        // A request carries the sender's full unacked batch. Decode each
        // envelope; then apply spec rule §4.5: "When multiple offer messages
        // arrive in a single request, only the LAST offer should be
        // processed." Earlier offers in the batch are superseded — we drop
        // them here so the signaler never adopts a stale offerId. All other
        // variants (answer / iceCandidates / reconnected) are emitted in
        // order; the signaler filters them by current offerId.
        const decoded: SyncSignalingEnvelope[] = [];
        for (const inner of messages) {
          let envelopeContent: SyncSignalingEnvelope;
          try {
            envelopeContent = SyncSignalingEnvelopeCodec.dec(inner);
          } catch (e) {
            console.warn(
              '[device-session channel] inner SyncSignalingEnvelope decode FAILED innerLen=%d prefix=%s err=%s',
              inner.length,
              toHex(inner.slice(0, Math.min(32, inner.length))),
              e instanceof Error ? e.message : String(e),
            );
            continue;
          }
          decoded.push(envelopeContent);
        }
        let lastOfferIdx = -1;
        for (let i = decoded.length - 1; i >= 0; i--) {
          if (decoded[i]!.message.tag === 'Offer') {
            lastOfferIdx = i;
            break;
          }
        }
        for (let i = 0; i < decoded.length; i++) {
          // Skip earlier Offers if a later Offer exists in the same batch.
          if (decoded[i]!.message.tag === 'Offer' && i !== lastOfferIdx) continue;
          out$.next(decoded[i]!);
        }
      }
      // Peer ack: that submission carried a snapshot of the unacked batch —
      // drop exactly those signals from it. Also the key delivery
      // discriminator: an ACK proves the peer RECEIVED our statement.
      if (envelope.tag === 'Response') {
        const covered = requestCoverage.get(envelope.value.requestId);
        if (covered === undefined) return;
        requestCoverage.delete(envelope.value.requestId);
        unackedSignals = unackedSignals.filter(s => !covered.has(s.signalId));
        if (unackedSignals.length === 0) requestCoverage.clear();
      }
    },
  });

  async function sendAll(envelopes: SyncSignalingEnvelope[]): Promise<void> {
    for (const envelope of envelopes) {
      const inner = SyncSignalingEnvelopeCodec.enc(envelope);
      const innerHex = toHex(inner);
      // Re-sent identical payloads refresh the batch rather than duplicating
      // inside it. Envelope-level dedupe is byte-exact, so the same Offer
      // re-sent with the same offerId collapses; an Offer with a NEW offerId
      // (initiator restarted) is treated as a new signal — exactly what we
      // want.
      if (!unackedSignals.some(s => s.bytesHex === innerHex)) {
        unackedSignals.push({ signalId: nanoid(), bytes: inner, bytesHex: innerHex });
      }
    }
    const requestId = nanoid();
    requestCoverage.set(requestId, new Set(unackedSignals.map(s => s.signalId)));
    const plaintext = SignalingStatementData.enc({
      tag: 'Request',
      value: { requestId, messages: unackedSignals.map(s => s.bytes) },
    });
    await postCiphertext(plaintext, requestChannel);
  }

  return {
    send: envelope => sendAll([envelope]),
    sendAll,
    messages$: out$.asObservable(),
    close: () => {
      subscription.unsubscribe();
    },
  };
}

export const deviceSessionUseCase = {
  createChannel,
};
