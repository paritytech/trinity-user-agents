import { describe, expect, it } from 'vitest';

import { type ChatMessage, type ReactionAggregate } from '@/domains/chat';

import { reactionKeysOnOwnMessages, shouldAutoScrollOnNewMessages } from './useScrollControls';

const message = (id: string, direction: 'outgoing' | 'incoming'): ChatMessage => ({
  messageId: id,
  sessionId: 's',
  peer: { type: 'p2p', accountId: 'acc', name: 'peer' },
  timestamp: 0,
  content: { type: 'text', text: 't' },
  status: direction === 'outgoing' ? { direction: 'outgoing', state: 'sent' } : { direction: 'incoming', state: 'seen' },
});

const aggregate = (emoji: string, reactors: { name: string; isMe: boolean }[]): ReactionAggregate => ({
  emoji,
  count: reactors.length,
  reactedByMe: reactors.some(r => r.isMe),
  reactors,
});

describe('reactionKeysOnOwnMessages', () => {
  it('includes reactions on the user own outgoing messages', () => {
    const messages = [message('m1', 'outgoing')];
    const reactions = new Map([['m1', [aggregate('👍', [{ name: 'Alice', isMe: false }])]]]);

    const keys = reactionKeysOnOwnMessages(messages, reactions);

    expect(keys.size).toBe(1);
    expect([...keys][0]).toContain('m1');
  });

  it('excludes reactions on incoming messages', () => {
    const messages = [message('m1', 'incoming')];
    const reactions = new Map([['m1', [aggregate('👍', [{ name: 'Alice', isMe: false }])]]]);

    expect(reactionKeysOnOwnMessages(messages, reactions).size).toBe(0);
  });

  it("counts the user's own reactions too (e.g. made on another device)", () => {
    const messages = [message('m1', 'outgoing')];
    const reactions = new Map([['m1', [aggregate('👍', [{ name: 'You', isMe: true }])]]]);

    expect(reactionKeysOnOwnMessages(messages, reactions).size).toBe(1);
  });

  it('keys each reactor+emoji distinctly, including the user own within a mixed aggregate', () => {
    const messages = [message('m1', 'outgoing')];
    const reactions = new Map([
      [
        'm1',
        [
          aggregate('👍', [
            { name: 'Alice', isMe: false },
            { name: 'You', isMe: true },
          ]),
          aggregate('🔥', [{ name: 'Bob', isMe: false }]),
        ],
      ],
    ]);

    expect(reactionKeysOnOwnMessages(messages, reactions).size).toBe(3);
  });
});

describe('shouldAutoScrollOnNewMessages', () => {
  it('scrolls on first open regardless of position', () => {
    expect(shouldAutoScrollOnNewMessages({ firstOpen: true, wasAtBottom: false, newestIsOutgoing: false })).toBe(true);
  });

  it('scrolls when the user was at the bottom', () => {
    expect(shouldAutoScrollOnNewMessages({ firstOpen: false, wasAtBottom: true, newestIsOutgoing: false })).toBe(true);
  });

  it('scrolls when the user sent the newest message', () => {
    expect(shouldAutoScrollOnNewMessages({ firstOpen: false, wasAtBottom: false, newestIsOutgoing: true })).toBe(true);
  });

  it('does not scroll for an incoming message while scrolled up', () => {
    expect(shouldAutoScrollOnNewMessages({ firstOpen: false, wasAtBottom: false, newestIsOutgoing: false })).toBe(false);
  });
});
