import { useMemo } from 'react';

import { useRxState } from '@/shared/rxstate';
import { type ChatSession } from '@/domains/chat';
import { recentChats } from '../state/recentChats';

/**
 * Resolves the persisted recently-opened chat IDs against the live session list,
 * dropping IDs whose session no longer exists. Exposes the curate actions so the
 * search panel can add/remove/clear recents. Mirrors `browser`'s `useRecentSearches`.
 */
export const useRecentChats = (sessions: ChatSession[]) => {
  const [recentIds] = useRxState(recentChats.recent$);

  const recentSessions = useMemo(
    () => recentIds.map(id => sessions.find(s => s.sessionId === id)).filter((s): s is ChatSession => Boolean(s)),
    [recentIds, sessions],
  );

  return {
    recentSessions,
    addRecent: recentChats.addRecent,
    removeRecent: recentChats.removeRecent,
    clearRecent: recentChats.clearRecent,
  };
};
