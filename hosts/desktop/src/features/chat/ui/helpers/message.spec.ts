import { describe, expect, it } from 'vitest';

import { type ChatMessage, type MessageContent } from '@/domains/chat';

import { getMessagePreview } from './message';

function productMessage(content: MessageContent): ChatMessage {
  // eslint-disable-next-line @typescript-eslint/consistent-type-assertions -- the preview reads only these fields of the double
  return {
    messageId: 'm1',
    sessionId: 's1',
    content,
    peer: { type: 'product', productId: 'demo.dot', name: 'Demo' },
    timestamp: 1,
    status: { direction: 'incoming', state: 'read' },
  } as unknown as ChatMessage;
}

describe('getMessagePreview', () => {
  it('previews a custom message by its alt', () => {
    const message = productMessage({ type: 'custom', messageType: 't', payload: new Uint8Array(), alt: 'Coin flip result' });

    expect(getMessagePreview(message)).toBe('Coin flip result');
  });

  it('falls back to Message for a custom message without alt', () => {
    const message = productMessage({ type: 'custom', messageType: 't', payload: new Uint8Array() });

    expect(getMessagePreview(message)).toBe('Message');
  });
});
