// @vitest-environment happy-dom

import { RouterProvider, createMemoryHistory, createRootRoute, createRouter } from '@tanstack/react-router';
import { render, screen } from '@testing-library/react';
import { of } from 'rxjs';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { TEST_IDS } from '@/shared/test-ids';
import { TranslationProvider } from '@/shared/translation';
import { type ChatSession } from '@/domains/chat';

// useElementSize is the only @/shared/hooks symbol ChatWidget consumes; the mock
// is overridden per-test to simulate different measured container heights.
const useElementSizeMock = vi.fn();

import { ChatWidget } from './ChatWidget';

const GROUP_SENDER_NAME = 'Karl.99';

function makeGroupSession(id: string): ChatSession {
  const message = {
    messageId: `${id}-msg`,
    content: { type: 'text', text: 'Hey there! How can I assist you today?' },
    peer: { name: GROUP_SENDER_NAME },
    timestamp: Date.now(),
    status: { direction: 'incoming', state: 'read' },
  };

  // Tests are exempt from the no-`as` rule; we only need the observable surface.
  return {
    sessionId: id,
    roomId: `room-${id}`,
    name: of('Group chat'),
    messages: of([]),
    lastMessage: of(message),
    unreadCount: of(0),
    participants: of([{ name: 'a' }, { name: 'b' }]),
  } as unknown as ChatSession;
}

function makeCallSession(id: string, purpose: 'audio' | 'video'): ChatSession {
  const offer = {
    messageId: `${id}-offer`,
    content: { type: 'callSignal', signal: 'offer', purpose },
    peer: { name: GROUP_SENDER_NAME },
    timestamp: Date.now(),
    status: { direction: 'outgoing', state: 'sent' },
  };
  const answer = {
    messageId: `${id}-answer`,
    content: { type: 'callSignal', signal: 'answer', offerMessageId: offer.messageId },
    peer: { name: GROUP_SENDER_NAME },
    timestamp: offer.timestamp + 1000,
    status: { direction: 'incoming', state: 'read' },
  };
  const closed = {
    messageId: `${id}-closed`,
    content: { type: 'callSignal', signal: 'closed', offerMessageId: offer.messageId },
    peer: { name: GROUP_SENDER_NAME },
    timestamp: offer.timestamp + 5000,
    status: { direction: 'incoming', state: 'read' },
  };

  // Tests are exempt from the no-`as` rule; we only need the observable surface.
  return {
    sessionId: id,
    roomId: `room-${id}`,
    name: of('Call chat'),
    messages: of([offer, answer, closed]),
    // The domain guarantees this is the offer, not `closed` — see
    // chatMessageService.isStandaloneMessage.
    lastMessage: of(offer),
    unreadCount: of(0),
    participants: of([{ name: 'a' }]),
  } as unknown as ChatSession;
}

const ref = { current: null };

// `useOpenProductChatRoom` calls `useNavigate`, so the widget mounts inside a real router;
// nothing here navigates, a root route is enough.
async function renderWidget(visibleCount: number, sessions: ChatSession[] = [], pending = false) {
  const rootRoute = createRootRoute({
    component: () => (
      <TranslationProvider>
        <ChatWidget visibleCount={visibleCount} sessions={sessions} pending={pending} />
      </TranslationProvider>
    ),
  });
  const router = createRouter({ routeTree: rootRoute, history: createMemoryHistory({ initialEntries: ['/'] }) });
  // A RouterProvider renders nothing until the router has matched its first location.
  await router.load();

  return render(<RouterProvider router={router} />);
}

describe('ChatWidget', () => {
  beforeEach(() => {
    useElementSizeMock.mockReturnValue({ ref, height: 200 });
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  it('renders at most visibleCount items, clipping the rest', async () => {
    const sessions = [makeGroupSession('1'), makeGroupSession('2'), makeGroupSession('3')];

    await renderWidget(2, sessions);

    expect(screen.getAllByTestId(TEST_IDS.chatRoomItem)).toHaveLength(2);
  });

  it('does not render a scroll container', async () => {
    const { container } = await renderWidget(2, [makeGroupSession('1')]);

    expect(container.querySelector('.overflow-y-auto')).toBeNull();
    expect(container.querySelector('.overflow-hidden')).not.toBeNull();
  });

  it('shows the group-sender line at regular density (medium/large)', async () => {
    // visibleCount 4 = medium → regular layout keeps the group-sender line.
    await renderWidget(4, [makeGroupSession('1')]);

    expect(screen.getByText(GROUP_SENDER_NAME)).toBeTruthy();
  });

  it('hides the group-sender line at compact density (small)', async () => {
    // visibleCount 2 = small → compact layout drops the group-sender line.
    await renderWidget(2, [makeGroupSession('1')]);

    expect(screen.queryByText(GROUP_SENDER_NAME)).toBeNull();
  });

  it('renders no rooms while pending (the block pulse handles loading)', async () => {
    await renderWidget(4, [], true);

    // No skeleton rows: the whole widget block pulses via DashboardCardChrome.
    expect(screen.queryByTestId(TEST_IDS.chatRoomItem)).toBeNull();
  });

  it('previews a completed voice call with its call title', async () => {
    await renderWidget(4, [makeCallSession('s1', 'audio')]);

    expect(screen.getByText('Voice Call')).toBeTruthy();
  });

  it('previews a completed video call with its call title', async () => {
    await renderWidget(4, [makeCallSession('s1', 'video')]);

    expect(screen.getByText('Video Call')).toBeTruthy();
  });
});
