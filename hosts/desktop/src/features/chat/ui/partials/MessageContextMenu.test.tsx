// @vitest-environment happy-dom

import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { TEST_IDS } from '@/shared/test-ids';
import { TranslationProvider } from '@/shared/translation';
import { type ChatMessage } from '@/domains/chat';

import { MessageContextMenu } from './MessageContextMenu';

// Tests are exempt from the no-`as` rule; the menu reads only these fields.
const message = {
  messageId: 'm1',
  content: { type: 'text', text: 'hello' },
  peer: { name: 'Me' },
  timestamp: 1,
  status: { direction: 'outgoing', state: 'sent' },
} as unknown as ChatMessage;

describe('MessageContextMenu', () => {
  it('offers reply and edit when handlers are given', () => {
    render(
      <TranslationProvider>
        <MessageContextMenu
          message={message}
          position={{ x: 0, y: 0 }}
          onClose={vi.fn()}
          onReply={vi.fn()}
          onEdit={vi.fn()}
          onCopyText={vi.fn()}
        />
      </TranslationProvider>,
    );

    expect(screen.getByTestId(TEST_IDS.chatMessageContextMenuReply)).toBeInTheDocument();
    expect(screen.getByTestId(TEST_IDS.chatMessageContextMenuEdit)).toBeInTheDocument();
  });

  it('omits reply and edit when no handlers are given', () => {
    render(
      <TranslationProvider>
        <MessageContextMenu message={message} position={{ x: 0, y: 0 }} onClose={vi.fn()} onCopyText={vi.fn()} />
      </TranslationProvider>,
    );

    expect(screen.queryByTestId(TEST_IDS.chatMessageContextMenuReply)).toBeNull();
    expect(screen.queryByTestId(TEST_IDS.chatMessageContextMenuEdit)).toBeNull();
    expect(screen.getByTestId(TEST_IDS.chatMessageContextMenuCopy)).toBeInTheDocument();
  });
});
