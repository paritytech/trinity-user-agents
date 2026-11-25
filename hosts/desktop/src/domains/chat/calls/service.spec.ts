import { describe, expect, it } from 'vitest';

import { type ChatMessage, type ChatMessageStatus, type MessageContent } from '../session/types';

import { callService } from './service';
import { type OfferIdMap } from './types';

// ── Fixture helpers ────────────────────────────────────────────────────────────

function makeMessage(messageId: string, content: MessageContent, direction: 'incoming' | 'outgoing' = 'incoming'): ChatMessage {
  const status: ChatMessageStatus =
    direction === 'incoming' ? { direction: 'incoming', state: 'new' } : { direction: 'outgoing', state: 'sent' };

  return {
    messageId,
    sessionId: 'session-1',
    peer: { type: 'p2p', accountId: 'peer-account', name: 'Peer' },
    timestamp: Date.now(),
    content,
    status,
  };
}

const offerContent = (purpose?: 'audio' | 'video'): MessageContent => ({
  type: 'callSignal',
  signal: 'offer',
  ...(purpose !== undefined ? { purpose } : {}),
});

const answerContent = (offerMessageId: string): MessageContent => ({
  type: 'callSignal',
  signal: 'answer',
  offerMessageId,
});

const closedContent = (offerMessageId: string): MessageContent => ({
  type: 'callSignal',
  signal: 'closed',
  offerMessageId,
});

const textContent = (): MessageContent => ({ type: 'text', text: 'Hello' });

// ── isCallOffer ────────────────────────────────────────────────────────────────

describe('isCallOffer', () => {
  it('returns true for a callSignal offer message', () => {
    const message = makeMessage('msg-1', offerContent('audio'));
    expect(callService.isCallOffer(message)).toBe(true);
  });

  it('returns false for a callSignal answer', () => {
    const message = makeMessage('msg-2', answerContent('msg-1'));
    expect(callService.isCallOffer(message)).toBe(false);
  });

  it('returns false for a callSignal closed', () => {
    const message = makeMessage('msg-3', closedContent('msg-1'));
    expect(callService.isCallOffer(message)).toBe(false);
  });

  it('returns false for a non-callSignal message', () => {
    const message = makeMessage('msg-4', textContent());
    expect(callService.isCallOffer(message)).toBe(false);
  });
});

// ── offerPurpose ───────────────────────────────────────────────────────────────

describe('offerPurpose', () => {
  it('returns "audio" for an audio offer', () => {
    const message = makeMessage('msg-1', offerContent('audio'));
    expect(callService.offerPurpose(message)).toBe('audio');
  });

  it('returns "video" for a video offer', () => {
    const message = makeMessage('msg-1', offerContent('video'));
    expect(callService.offerPurpose(message)).toBe('video');
  });

  it('returns null when purpose is missing from an offer', () => {
    const message = makeMessage('msg-1', offerContent());
    expect(callService.offerPurpose(message)).toBeNull();
  });

  it('returns null for a callSignal answer (not an offer)', () => {
    const message = makeMessage('msg-2', answerContent('msg-1'));
    expect(callService.offerPurpose(message)).toBeNull();
  });

  it('returns null for a non-callSignal message', () => {
    const message = makeMessage('msg-3', textContent());
    expect(callService.offerPurpose(message)).toBeNull();
  });
});

// ── ringableIncomingOffers ─────────────────────────────────────────────────────

describe('ringableIncomingOffers', () => {
  it('returns an incoming offer that has no answer or closed', () => {
    const offer = makeMessage('offer-1', offerContent('audio'), 'incoming');
    expect(callService.ringableIncomingOffers([offer])).toEqual([offer]);
  });

  it('excludes an incoming offer that has a matching answer', () => {
    const offer = makeMessage('offer-1', offerContent('audio'), 'incoming');
    const answer = makeMessage('answer-1', answerContent('offer-1'), 'outgoing');
    expect(callService.ringableIncomingOffers([offer, answer])).toEqual([]);
  });

  it('excludes an incoming offer that has a matching closed signal', () => {
    const offer = makeMessage('offer-1', offerContent('video'), 'incoming');
    const closed = makeMessage('closed-1', closedContent('offer-1'), 'outgoing');
    expect(callService.ringableIncomingOffers([offer, closed])).toEqual([]);
  });

  it('excludes outgoing offers even when unresolved', () => {
    const offer = makeMessage('offer-1', offerContent('audio'), 'outgoing');
    expect(callService.ringableIncomingOffers([offer])).toEqual([]);
  });

  it('returns empty array when messages list is empty', () => {
    expect(callService.ringableIncomingOffers([])).toEqual([]);
  });

  it('returns empty array when no offer qualifies', () => {
    const text = makeMessage('msg-1', textContent(), 'incoming');
    expect(callService.ringableIncomingOffers([text])).toEqual([]);
  });

  it('returns only unresolved incoming offers from a mixed list', () => {
    const offer1 = makeMessage('offer-1', offerContent('audio'), 'incoming');
    const offer2 = makeMessage('offer-2', offerContent('video'), 'incoming');
    const answer = makeMessage('answer-1', answerContent('offer-1'), 'outgoing');
    const result = callService.ringableIncomingOffers([offer1, offer2, answer]);
    expect(result).toEqual([offer2]);
  });
});

// ── isIncomingOfferResolved ──────────────────────────────────────────────────────

describe('isIncomingOfferResolved', () => {
  it('is true when an answer references the offer', () => {
    const answer = makeMessage('answer-1', answerContent('offer-1'), 'outgoing');
    expect(callService.isIncomingOfferResolved([answer], 'offer-1')).toBe(true);
  });

  it('is true when a closed references the offer', () => {
    const closed = makeMessage('closed-1', closedContent('offer-1'), 'outgoing');
    expect(callService.isIncomingOfferResolved([closed], 'offer-1')).toBe(true);
  });

  it('is false when only the offer is present', () => {
    const offer = makeMessage('offer-1', offerContent('audio'), 'incoming');
    expect(callService.isIncomingOfferResolved([offer], 'offer-1')).toBe(false);
  });

  it('is false when the resolving signal references a different offer', () => {
    const answer = makeMessage('answer-1', answerContent('offer-2'), 'outgoing');
    expect(callService.isIncomingOfferResolved([answer], 'offer-1')).toBe(false);
  });

  it('is false for unrelated messages', () => {
    const text = makeMessage('msg-1', textContent(), 'incoming');
    expect(callService.isIncomingOfferResolved([text], 'offer-1')).toBe(false);
  });

  it('is false for an empty message list', () => {
    expect(callService.isIncomingOfferResolved([], 'offer-1')).toBe(false);
  });
});

// ── translateWindowToMain ───────────────────────────────────────────────────────

describe('translateWindowToMain', () => {
  it('publishOffer → callSignal offer with purpose and sdp', () => {
    const map: OfferIdMap = new Map();
    const sdp = new Uint8Array([1, 2, 3]);
    const result = callService.translateWindowToMain({ kind: 'publishOffer', offerId: 'o1', purpose: 'audio', sdp }, map);
    expect(result).toEqual({ type: 'callSignal', signal: 'offer', purpose: 'audio', sdp });
  });

  it('publishAnswer uses offerIdMap to find chatMessageId', () => {
    const map: OfferIdMap = new Map([['o1', 'chatMsg1']]);
    const sdp = new Uint8Array([4, 5]);
    const result = callService.translateWindowToMain({ kind: 'publishAnswer', offerId: 'o1', sdp }, map);
    expect(result).toEqual({ type: 'callSignal', signal: 'answer', offerMessageId: 'chatMsg1', sdp });
  });

  it('publishAnswer falls back to offerId when not in map', () => {
    const map: OfferIdMap = new Map();
    const sdp = new Uint8Array([4, 5]);
    const result = callService.translateWindowToMain({ kind: 'publishAnswer', offerId: 'o1', sdp }, map);
    expect(result).toEqual({ type: 'callSignal', signal: 'answer', offerMessageId: 'o1', sdp });
  });

  it('publishClosed uses offerIdMap', () => {
    const map: OfferIdMap = new Map([['o2', 'chatMsg2']]);
    const result = callService.translateWindowToMain({ kind: 'publishClosed', offerId: 'o2' }, map);
    expect(result).toEqual({ type: 'callSignal', signal: 'closed', offerMessageId: 'chatMsg2' });
  });

  it('publishCandidates uses candidates as sdp', () => {
    const map: OfferIdMap = new Map([['o1', 'chatMsg1']]);
    const candidates = new Uint8Array([7, 8, 9]);
    const result = callService.translateWindowToMain({ kind: 'publishCandidates', offerId: 'o1', candidates }, map);
    expect(result).toEqual({ type: 'callSignal', signal: 'ice', offerMessageId: 'chatMsg1', sdp: candidates });
  });
});

// ── translateInboundCallSignal ──────────────────────────────────────────────────

describe('translateInboundCallSignal', () => {
  it('returns null for non-callSignal messages', () => {
    const map: OfferIdMap = new Map();
    const msg = makeMessage('msg-1', textContent());
    expect(callService.translateInboundCallSignal(msg, map)).toBeNull();
  });

  it('returns null for offer signals (no offerMessageId)', () => {
    const map: OfferIdMap = new Map();
    const msg = makeMessage('msg-1', offerContent('audio'));
    expect(callService.translateInboundCallSignal(msg, map)).toBeNull();
  });

  it('answer with sdp → deliverAnswer, resolving offerId via reverse map', () => {
    const map: OfferIdMap = new Map([['windowOffer1', 'chatMsg1']]);
    const sdp = new Uint8Array([1, 2]);
    const msg = makeMessage('chatMsg1', { type: 'callSignal', signal: 'answer', offerMessageId: 'chatMsg1', sdp });
    expect(callService.translateInboundCallSignal(msg, map)).toEqual({ kind: 'deliverAnswer', offerId: 'windowOffer1', sdp });
  });

  it('closed → deliverClosed, fallback offerId when not in map', () => {
    const map: OfferIdMap = new Map();
    const msg = makeMessage('chatMsg99', { type: 'callSignal', signal: 'closed', offerMessageId: 'chatMsg99' });
    expect(callService.translateInboundCallSignal(msg, map)).toEqual({ kind: 'deliverClosed', offerId: 'chatMsg99' });
  });

  it('ice → deliverCandidates', () => {
    const map: OfferIdMap = new Map([['windowOffer1', 'chatMsg1']]);
    const sdp = new Uint8Array([5, 6]);
    const msg = makeMessage('chatMsg1', { type: 'callSignal', signal: 'ice', offerMessageId: 'chatMsg1', sdp });
    expect(callService.translateInboundCallSignal(msg, map)).toEqual({
      kind: 'deliverCandidates',
      offerId: 'windowOffer1',
      candidates: sdp,
    });
  });
});
