import { useElementSize } from '@/shared/hooks';
import { TEST_IDS } from '@/shared/test-ids';
import { type ChatSession } from '@/domains/chat';
import { useOpenProductChatRoom } from '../hooks/useOpenProductChatRoom';
import { chatService } from '../service';

import { RoomList } from './partials/RoomList';

type Props = {
  // Max chats to show, derived from the dashboard widget size (small:2 / medium:4 / large:8).
  visibleCount: number;
  // Sessions + loading are owned by ChatWidgetContent (single source), which
  // also drives the block pulse via DashboardCardChrome's `isLoading`.
  sessions: ChatSession[];
  pending: boolean;
};

export const ChatWidget = ({ visibleCount, sessions, pending }: Props) => {
  const openChatRoom = useOpenProductChatRoom();

  // The visible items split the container height evenly to fill it without scrolling.
  // Before the first measurement (height 0) fall back to natural item height.
  const { ref, height } = useElementSize();
  const slotHeight = height > 0 ? height / visibleCount : undefined;
  // Density is size-driven (small → compact, medium/large → regular), not height-driven.
  const density = chatService.chatItemDensityForCount(visibleCount);

  return (
    <div data-testid={TEST_IDS.chatWidget} className="flex h-full w-full flex-col">
      <div ref={ref} className="min-h-0 flex-1 overflow-hidden">
        {pending ? null : (
          <RoomList
            sessions={sessions}
            selected={null}
            visibleCount={visibleCount}
            density={density}
            itemHeight={slotHeight}
            onSelect={session => openChatRoom(session.sessionId)}
          />
        )}
      </div>
    </div>
  );
};
