import { x25519 } from '@noble/curves/ed25519.js';
import { Subject } from 'rxjs';
import { describe, expect, it } from 'vitest';

import { SyncSignalingEnvelopeCodec } from '@/shared/peer-channel';
import { SignalingStatementData } from '../schemas';
import { deviceSessionService } from '../service';
import { type SyncSignalingEnvelope } from '../types';

import { deviceSessionUseCase } from './session';

// The peer side of the pair, mirroring how `createChannel` builds its encryption.
const peerEncryption = (ownPriv: Uint8Array, peerPub: Uint8Array) =>
  deviceSessionService.createPayloadEncryption(deviceSessionService.deriveSharedSecret(ownPriv, peerPub));

/**
 * A (our side) ↔ B (peer) channel over a recording fake transport. Crypto,
 * codecs, and topic derivation are all real — only post/subscribe are faked.
 */
const makeHarness = () => {
  const aPriv = x25519.utils.randomSecretKey();
  const aPub = x25519.getPublicKey(aPriv);
  const bPriv = x25519.utils.randomSecretKey();
  const bPub = x25519.getPublicKey(bPriv);

  const posted: { topic: Uint8Array; data: Uint8Array; channel?: Uint8Array }[] = [];
  const incoming$ = new Subject<{ topic: Uint8Array; data: Uint8Array }>();

  const channel = deviceSessionUseCase.createChannel({
    ourDeviceEncPriv: aPriv,
    ourStatementAccountId: new Uint8Array(32).fill(0x01),
    peerDeviceEncPub: bPub,
    peerStatementAccountId: new Uint8Array(32).fill(0x02),
    post: (topic, data, statementChannel) => {
      posted.push({ topic, data, channel: statementChannel });
      return Promise.resolve();
    },
    subscribe: () => incoming$.asObservable(),
  });

  /** Decrypt one of our posted statements B-side and decode the envelope. */
  const decryptPosted = (data: Uint8Array) => SignalingStatementData.dec(peerEncryption(bPriv, aPub).decrypt(data));
  /** Encrypt an outer Request/Response B-side and push it onto our incoming subscription. */
  const receiveFromPeer = (outer: Uint8Array) => {
    incoming$.next({ topic: new Uint8Array(32), data: peerEncryption(bPriv, aPub).encrypt(outer) });
  };

  return { channel, posted, decryptPosted, receiveFromPeer };
};

const decryptRequest = (harness: ReturnType<typeof makeHarness>, data: Uint8Array) => {
  const envelope = harness.decryptPosted(data);
  if (envelope.tag !== 'Request') throw new Error('expected Request');
  return envelope.value;
};

const offer = (offerId: string, sdp: number[]): Uint8Array =>
  SyncSignalingEnvelopeCodec.enc({ offerId, message: { tag: 'Offer', value: { sdp: new Uint8Array(sdp) } } });
const candidates = (offerId: string, blob: number[]): Uint8Array =>
  SyncSignalingEnvelopeCodec.enc({ offerId, message: { tag: 'Candidates', value: { candidates: new Uint8Array(blob) } } });

describe('deviceSessionUseCase.createChannel', () => {
  it('wraps an outbound SyncSignalingEnvelope in SignalingStatementData.Request, then encrypts', async () => {
    const harness = makeHarness();

    await harness.channel.send({ offerId: 'oid-1', message: { tag: 'Offer', value: { sdp: new Uint8Array([1]) } } });

    expect(harness.posted).toHaveLength(1);
    expect(Array.from(harness.posted[0]!.data)).not.toEqual([1]);

    const request = decryptRequest(harness, harness.posted[0]!.data);
    expect(request.requestId.length).toBeGreaterThan(0);
    expect(request.messages).toHaveLength(1);
    const decoded = SyncSignalingEnvelopeCodec.dec(request.messages[0]!);
    expect(decoded.offerId).toBe('oid-1');
    expect(decoded.message.tag).toBe('Offer');
    harness.channel.close();
  });

  it('decrypts inbound Request, emits the inner envelope, and ACKs with a matching Response', async () => {
    const harness = makeHarness();

    const received: SyncSignalingEnvelope[] = [];
    harness.channel.messages$.subscribe(m => received.push(m));

    const requestId = 'test-req-123';
    harness.receiveFromPeer(
      SignalingStatementData.enc({ tag: 'Request', value: { requestId, messages: [offer('oid-9', [7])] } }),
    );

    await new Promise(r => setTimeout(r, 10));

    expect(received).toHaveLength(1);
    expect(received[0]!.offerId).toBe('oid-9');
    expect(received[0]!.message.tag).toBe('Offer');

    expect(harness.posted).toHaveLength(1);
    const ackEnvelope = harness.decryptPosted(harness.posted[0]!.data);
    expect(ackEnvelope.tag).toBe('Response');
    if (ackEnvelope.tag !== 'Response') throw new Error('unreachable');
    expect(ackEnvelope.value.requestId).toBe(requestId);
    expect(ackEnvelope.value.responseCode).toBe(0);

    harness.channel.close();
  });

  it('on a batch with multiple Offers, emits only the LAST offer (spec §4.5)', async () => {
    const harness = makeHarness();

    const received: SyncSignalingEnvelope[] = [];
    harness.channel.messages$.subscribe(m => received.push(m));

    harness.receiveFromPeer(
      SignalingStatementData.enc({
        tag: 'Request',
        value: {
          requestId: 'batch-1',
          // stale offer, then candidates for it, then the FRESH offer
          messages: [offer('old', [1]), candidates('old', [2]), offer('new', [3])],
        },
      }),
    );
    await new Promise(r => setTimeout(r, 10));

    // The stale offer is dropped; candidates + the last offer are emitted in order.
    const offers = received.filter(m => m.message.tag === 'Offer');
    expect(offers).toHaveLength(1);
    expect(offers[0]!.offerId).toBe('new');
    // Candidates message is still delivered (not an Offer, never superseded).
    expect(received.map(m => m.message.tag)).toContain('Candidates');

    harness.channel.close();
  });

  it('batches unacked signals into each request statement and drops them once a covering ACK arrives', async () => {
    const harness = makeHarness();

    await harness.channel.send({ offerId: 'oid', message: { tag: 'Offer', value: { sdp: new Uint8Array([1]) } } });
    await harness.channel.send({ offerId: 'oid', message: { tag: 'Candidates', value: { candidates: new Uint8Array([2]) } } });

    expect(harness.posted).toHaveLength(2);
    expect(harness.posted[0]!.channel).toBeDefined();
    expect(harness.posted[1]!.channel).toEqual(harness.posted[0]!.channel);
    const second = decryptRequest(harness, harness.posted[1]!.data);
    expect(second.messages).toHaveLength(2);
    expect(SyncSignalingEnvelopeCodec.dec(second.messages[0]!).message.tag).toBe('Offer');
    expect(SyncSignalingEnvelopeCodec.dec(second.messages[1]!).message.tag).toBe('Candidates');

    harness.receiveFromPeer(
      SignalingStatementData.enc({ tag: 'Response', value: { requestId: second.requestId, responseCode: 0 } }),
    );
    await new Promise(r => setTimeout(r, 10));

    await harness.channel.send({ offerId: 'oid', message: { tag: 'Candidates', value: { candidates: new Uint8Array([3]) } } });
    const third = decryptRequest(harness, harness.posted[2]!.data);
    expect(third.messages).toHaveLength(1);
    expect(SyncSignalingEnvelopeCodec.dec(third.messages[0]!).message.tag).toBe('Candidates');

    harness.channel.close();
  });

  it('re-sending an identical envelope refreshes the statement without duplicating it inside the batch', async () => {
    const harness = makeHarness();

    await harness.channel.send({ offerId: 'oid', message: { tag: 'Offer', value: { sdp: new Uint8Array([1]) } } });
    await harness.channel.send({ offerId: 'oid', message: { tag: 'Offer', value: { sdp: new Uint8Array([1]) } } });

    const second = decryptRequest(harness, harness.posted[1]!.data);
    expect(second.messages).toHaveLength(1);

    harness.channel.close();
  });

  it('drops a duplicate Request with the same requestId, but ACKs every retry', async () => {
    const harness = makeHarness();

    const received: SyncSignalingEnvelope[] = [];
    harness.channel.messages$.subscribe(m => received.push(m));

    const outer = SignalingStatementData.enc({
      tag: 'Request',
      value: {
        requestId: 'dup-1',
        messages: [SyncSignalingEnvelopeCodec.enc({ offerId: 'oid', message: { tag: 'Reconnected', value: undefined } })],
      },
    });
    harness.receiveFromPeer(outer);
    harness.receiveFromPeer(outer);
    await new Promise(r => setTimeout(r, 10));

    expect(received).toHaveLength(1);
    expect(harness.posted).toHaveLength(2);

    harness.channel.close();
  });
});
