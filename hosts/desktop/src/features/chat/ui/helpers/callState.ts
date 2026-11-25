import { type ChatMessage } from '@/domains/chat';

// Mirrors iOS `Chat.CallState` (polkadot-app-ios-v2 ChatCallState.swift). A call
// produces one offer plus 0+ answer / ice / closed signals — the renderer needs
// a single derived state per offer.
export type CallState =
  | { kind: 'calling' }
  | { kind: 'active' }
  | { kind: 'finished'; durationMs: number }
  | { kind: 'cancelled'; ringDurationMs: number }
  | { kind: 'missed' };

export function deriveCallStates(messages: ChatMessage[]): Map<string, CallState> {
  type Bucket = {
    offer: ChatMessage;
    earliestAnswer?: ChatMessage;
    earliestClosed?: ChatMessage;
  };

  const buckets = new Map<string, Bucket>();

  // First pass — register offers.
  for (const msg of messages) {
    if (msg.content.type === 'callSignal' && msg.content.signal === 'offer') {
      buckets.set(msg.messageId, { offer: msg });
    }
  }

  // Second pass — fold answer / closed into their offer bucket.
  for (const msg of messages) {
    if (msg.content.type !== 'callSignal') continue;
    if (msg.content.signal === 'offer') continue;
    const offerId = msg.content.offerMessageId;
    if (!offerId) continue;
    const bucket = buckets.get(offerId);
    if (!bucket) continue;

    if (msg.content.signal === 'answer') {
      if (!bucket.earliestAnswer || msg.timestamp < bucket.earliestAnswer.timestamp) {
        bucket.earliestAnswer = msg;
      }
    } else if (msg.content.signal === 'closed') {
      if (!bucket.earliestClosed || msg.timestamp < bucket.earliestClosed.timestamp) {
        bucket.earliestClosed = msg;
      }
    }
    // ice is intentionally ignored — it carries no state-derivation signal.
  }

  const out = new Map<string, CallState>();
  for (const [offerId, { offer, earliestAnswer, earliestClosed }] of buckets) {
    if (!earliestClosed) {
      out.set(offerId, earliestAnswer ? { kind: 'active' } : { kind: 'calling' });
      continue;
    }

    if (earliestAnswer) {
      out.set(offerId, { kind: 'finished', durationMs: saturatingSubtract(earliestClosed.timestamp, earliestAnswer.timestamp) });
      continue;
    }

    if (offer.status.direction === 'outgoing') {
      out.set(offerId, { kind: 'cancelled', ringDurationMs: saturatingSubtract(earliestClosed.timestamp, offer.timestamp) });
    } else {
      out.set(offerId, { kind: 'missed' });
    }
  }

  return out;
}

/**
 * The `react-intl` key for a call's title. Returns the key rather than the
 * translated string so the mapping stays pure — the chat list and the call
 * bubble both translate it at their own call site.
 */
export function callTitleKey(state: CallState, isMe: boolean, isVideo: boolean): string {
  switch (state.kind) {
    case 'calling':
      if (isMe) return isVideo ? 'feature.chat.call.title.outgoingVideo' : 'feature.chat.call.title.outgoingVoice';

      return isVideo ? 'feature.chat.call.title.incomingVideo' : 'feature.chat.call.title.incomingVoice';
    case 'active':
      return isVideo ? 'feature.chat.call.title.ongoingVideo' : 'feature.chat.call.title.ongoingVoice';
    case 'finished':
      return isVideo ? 'feature.chat.call.title.video' : 'feature.chat.call.title.voice';
    case 'missed':
      return isVideo ? 'feature.chat.call.title.missedVideo' : 'feature.chat.call.title.missedVoice';
    case 'cancelled':
      return isVideo ? 'feature.chat.call.title.canceledVideo' : 'feature.chat.call.title.canceledVoice';
  }
}

function saturatingSubtract(a: number, b: number): number {
  return a >= b ? a - b : 0;
}
