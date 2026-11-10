import { describe, expect, it } from 'vitest';

import { type CallSignalContent, type ChatMessage } from '@/domains/chat';

import { callTitleKey, deriveCallStates } from './callState';

const peer: ChatMessage['peer'] = { type: 'p2p', accountId: 'peer-acct', name: 'Peer' };

function msg(
  messageId: string,
  timestamp: number,
  content: CallSignalContent,
  direction: 'incoming' | 'outgoing' = 'incoming',
): ChatMessage {
  return {
    messageId,
    sessionId: 'session-1',
    peer,
    timestamp,
    content,
    status: direction === 'incoming' ? { direction: 'incoming', state: 'new' } : { direction: 'outgoing', state: 'sent' },
  };
}

describe('deriveCallStates', () => {
  it('offer alone → calling', () => {
    const out = deriveCallStates([msg('o1', 0, { type: 'callSignal', signal: 'offer', purpose: 'audio' })]);
    expect(out.get('o1')).toEqual({ kind: 'calling' });
  });

  it('offer + answer → active', () => {
    const out = deriveCallStates([
      msg('o1', 0, { type: 'callSignal', signal: 'offer', purpose: 'audio' }),
      msg('a1', 500, { type: 'callSignal', signal: 'answer', offerMessageId: 'o1' }),
    ]);
    expect(out.get('o1')).toEqual({ kind: 'active' });
  });

  it('offer + answer + closed → finished with closed-answer duration', () => {
    const out = deriveCallStates([
      msg('o1', 0, { type: 'callSignal', signal: 'offer', purpose: 'audio' }),
      msg('a1', 1_000, { type: 'callSignal', signal: 'answer', offerMessageId: 'o1' }),
      msg('c1', 5_500, { type: 'callSignal', signal: 'closed', offerMessageId: 'o1' }),
    ]);
    expect(out.get('o1')).toEqual({ kind: 'finished', durationMs: 4_500 });
  });

  it('incoming offer + closed (no answer) → missed', () => {
    const out = deriveCallStates([
      msg('o1', 0, { type: 'callSignal', signal: 'offer', purpose: 'video' }, 'incoming'),
      msg('c1', 2_000, { type: 'callSignal', signal: 'closed', offerMessageId: 'o1' }),
    ]);
    expect(out.get('o1')).toEqual({ kind: 'missed' });
  });

  it('outgoing offer + closed (no answer) → cancelled with ring duration', () => {
    const out = deriveCallStates([
      msg('o1', 1_000, { type: 'callSignal', signal: 'offer', purpose: 'audio' }, 'outgoing'),
      msg('c1', 4_000, { type: 'callSignal', signal: 'closed', offerMessageId: 'o1' }),
    ]);
    expect(out.get('o1')).toEqual({ kind: 'cancelled', ringDurationMs: 3_000 });
  });

  it('ICE candidates do not affect state', () => {
    const out = deriveCallStates([
      msg('o1', 0, { type: 'callSignal', signal: 'offer', purpose: 'audio' }),
      msg('i1', 100, { type: 'callSignal', signal: 'ice', offerMessageId: 'o1' }),
      msg('i2', 200, { type: 'callSignal', signal: 'ice', offerMessageId: 'o1' }),
    ]);
    expect(out.get('o1')).toEqual({ kind: 'calling' });
  });

  it('two independent calls in one session — keyed by offer messageId', () => {
    const out = deriveCallStates([
      msg('o1', 0, { type: 'callSignal', signal: 'offer', purpose: 'audio' }, 'outgoing'),
      msg('c1', 1_000, { type: 'callSignal', signal: 'closed', offerMessageId: 'o1' }),
      msg('o2', 2_000, { type: 'callSignal', signal: 'offer', purpose: 'video' }, 'incoming'),
      msg('a2', 2_500, { type: 'callSignal', signal: 'answer', offerMessageId: 'o2' }),
    ]);
    expect(out.get('o1')).toEqual({ kind: 'cancelled', ringDurationMs: 1_000 });
    expect(out.get('o2')).toEqual({ kind: 'active' });
  });

  it('picks the earliest answer / closed when duplicates exist', () => {
    const out = deriveCallStates([
      msg('o1', 0, { type: 'callSignal', signal: 'offer', purpose: 'audio' }),
      msg('a-late', 5_000, { type: 'callSignal', signal: 'answer', offerMessageId: 'o1' }),
      msg('a-early', 1_000, { type: 'callSignal', signal: 'answer', offerMessageId: 'o1' }),
      msg('c-late', 9_000, { type: 'callSignal', signal: 'closed', offerMessageId: 'o1' }),
      msg('c-early', 6_000, { type: 'callSignal', signal: 'closed', offerMessageId: 'o1' }),
    ]);
    expect(out.get('o1')).toEqual({ kind: 'finished', durationMs: 5_000 });
  });

  it('ignores signals whose offerMessageId references an absent offer', () => {
    const out = deriveCallStates([msg('a-orphan', 1_000, { type: 'callSignal', signal: 'answer', offerMessageId: 'missing' })]);
    expect(out.size).toBe(0);
  });
});

describe('callTitleKey', () => {
  it('distinguishes outgoing from incoming while the call is still ringing', () => {
    expect(callTitleKey({ kind: 'calling' }, true, false)).toBe('feature.chat.call.title.outgoingVoice');
    expect(callTitleKey({ kind: 'calling' }, true, true)).toBe('feature.chat.call.title.outgoingVideo');
    expect(callTitleKey({ kind: 'calling' }, false, false)).toBe('feature.chat.call.title.incomingVoice');
    expect(callTitleKey({ kind: 'calling' }, false, true)).toBe('feature.chat.call.title.incomingVideo');
  });

  it('ignores direction once the call is connected, finished, missed or cancelled', () => {
    expect(callTitleKey({ kind: 'active' }, true, false)).toBe('feature.chat.call.title.ongoingVoice');
    expect(callTitleKey({ kind: 'active' }, false, true)).toBe('feature.chat.call.title.ongoingVideo');
    expect(callTitleKey({ kind: 'finished', durationMs: 1000 }, true, false)).toBe('feature.chat.call.title.voice');
    expect(callTitleKey({ kind: 'finished', durationMs: 1000 }, false, true)).toBe('feature.chat.call.title.video');
    expect(callTitleKey({ kind: 'missed' }, false, false)).toBe('feature.chat.call.title.missedVoice');
    expect(callTitleKey({ kind: 'missed' }, false, true)).toBe('feature.chat.call.title.missedVideo');
    expect(callTitleKey({ kind: 'cancelled', ringDurationMs: 500 }, true, false)).toBe('feature.chat.call.title.canceledVoice');
    expect(callTitleKey({ kind: 'cancelled', ringDurationMs: 500 }, true, true)).toBe('feature.chat.call.title.canceledVideo');
  });
});
