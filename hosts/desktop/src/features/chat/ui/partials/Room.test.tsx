// @vitest-environment happy-dom

import { render, screen } from '@testing-library/react';
import { type Observable, of } from 'rxjs';
import { describe, expect, it } from 'vitest';

import { TEST_IDS } from '@/shared/test-ids';
import { TranslationProvider } from '@/shared/translation';
import { type ChatSession, type RoomUserInput } from '@/domains/chat';

import { Room } from './Room';

function makeSession(userInput?: Observable<RoomUserInput>): ChatSession {
  // Tests are exempt from the no-`as` rule; we only need the observable surface.
  return {
    sessionId: 's1',
    roomId: 'room-1',
    name: of('Coinflip'),
    messages: of([]),
    lastMessage: of(null),
    unreadCount: of(0),
    participants: of([]),
    userInput,
    sendMessage: async () => {},
    markAsRead: async () => {},
    deleteSession: async () => {},
  } as unknown as ChatSession;
}

function renderRoom(session: ChatSession) {
  return render(
    <TranslationProvider>
      <Room session={session} />
    </TranslationProvider>,
  );
}

describe('Room', () => {
  it('shows the composer for a session without user input', async () => {
    renderRoom(makeSession());

    expect(await screen.findByTestId(TEST_IDS.chatMessageInput)).toBeInTheDocument();
  });

  it('hides the composer for a session whose user input is none', async () => {
    renderRoom(makeSession(of('none')));

    await screen.findByTestId(TEST_IDS.chatNoMessagesPlaceholder);
    expect(screen.queryByTestId(TEST_IDS.chatMessageInput)).toBeNull();
  });
});
