import { type ChatMessage, type MessageContent } from '../session/types';

import { type MainToWindowMessage, type WindowToMainMessage } from './schemas';
import { type CallPurpose } from './session/types';
import { type OfferIdMap } from './types';

function isCallOffer(message: ChatMessage): boolean {
  return message.content.type === 'callSignal' && message.content.signal === 'offer';
}

function offerPurpose(message: ChatMessage): CallPurpose | null {
  const content = message.content;
  if (content.type !== 'callSignal' || content.signal !== 'offer') return null;
  return content.purpose ?? null;
}

/**
 * Incoming call offers that should still ring: incoming direction, and no
 * `answer` or `closed` signal in `messages` references the offer's id yet.
 * Mirrors the offer/answer/closed fold in `features/chat/ui/helpers/callState.ts`
 * (deriveCallStates) but returns the offers to ring rather than a bubble state.
 */
function ringableIncomingOffers(messages: ChatMessage[]): ChatMessage[] {
  const resolved = new Set<string>();
  for (const message of messages) {
    const content = message.content;
    if (content.type !== 'callSignal') continue;
    if ((content.signal === 'answer' || content.signal === 'closed') && content.offerMessageId) {
      resolved.add(content.offerMessageId);
    }
  }

  const ringable: ChatMessage[] = [];
  for (const message of messages) {
    if (isCallOffer(message) && message.status.direction === 'incoming' && !resolved.has(message.messageId)) {
      ringable.push(message);
    }
  }
  return ringable;
}

/**
 * True when *any* `answer` or `closed` callSignal references `offerId` — including one this
 * device published. Deciding whether the resolution came from here is the caller's job.
 */
function isIncomingOfferResolved(messages: ChatMessage[], offerId: string): boolean {
  for (const message of messages) {
    const content = message.content;
    if (content.type !== 'callSignal') continue;
    if (content.signal !== 'answer' && content.signal !== 'closed') continue;
    if (content.offerMessageId === offerId) return true;
  }
  return false;
}

function reverseOfferId(map: OfferIdMap, chatMessageId: string): string | null {
  for (const [offerId, messageId] of map) {
    if (messageId === chatMessageId) return offerId;
  }
  return null;
}

/**
 * Translate a call-window message into the chat `MessageContent` to send,
 * resolving outgoing offerId → chatMessageId via `offerIdMap`. Returns null for
 * a kind that carries no chat signal.
 */
function translateWindowToMain(msg: WindowToMainMessage, offerIdMap: OfferIdMap): MessageContent | null {
  switch (msg.kind) {
    case 'publishOffer':
      // offerId ↔ messageId is stored by the caller once sendMessage resolves.
      return { type: 'callSignal', signal: 'offer', purpose: msg.purpose, sdp: msg.sdp };
    case 'publishAnswer': {
      const chatMessageId = offerIdMap.get(msg.offerId) ?? msg.offerId;
      return { type: 'callSignal', signal: 'answer', offerMessageId: chatMessageId, sdp: msg.sdp };
    }
    case 'publishCandidates': {
      const chatMessageId = offerIdMap.get(msg.offerId) ?? msg.offerId;
      return { type: 'callSignal', signal: 'ice', offerMessageId: chatMessageId, sdp: msg.candidates };
    }
    case 'publishClosed': {
      const chatMessageId = offerIdMap.get(msg.offerId) ?? msg.offerId;
      return { type: 'callSignal', signal: 'closed', offerMessageId: chatMessageId };
    }
  }
}

/**
 * Translate an inbound callSignal chat message into a message for the call
 * window, resolving chatMessageId → window offerId. Returns null when it is not
 * a relayable inbound signal.
 */
function translateInboundCallSignal(chatMessage: ChatMessage, offerIdMap: OfferIdMap): MainToWindowMessage | null {
  const content = chatMessage.content;
  if (content.type !== 'callSignal') return null;
  if (!content.offerMessageId) return null;

  const offerId = reverseOfferId(offerIdMap, content.offerMessageId) ?? content.offerMessageId;

  switch (content.signal) {
    case 'answer':
      if (!content.sdp) return null;
      return { kind: 'deliverAnswer', offerId, sdp: content.sdp };
    case 'ice':
      if (!content.sdp) return null;
      return { kind: 'deliverCandidates', offerId, candidates: content.sdp };
    case 'closed':
      return { kind: 'deliverClosed', offerId };
    default:
      return null;
  }
}

export const callService = {
  isCallOffer,
  offerPurpose,
  ringableIncomingOffers,
  isIncomingOfferResolved,
  translateWindowToMain,
  translateInboundCallSignal,
};
