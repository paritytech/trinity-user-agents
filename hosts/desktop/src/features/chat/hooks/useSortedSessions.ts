import { useMemo } from 'react';
import { useObservable } from 'react-rx';
import { combineLatest, map, of } from 'rxjs';

import { type ChatSession } from '@/domains/chat';

/** Sessions newest-activity-first. Shared by the chat list and search recents so both agree on "recent". */
export const useSortedSessions = (sessions: ChatSession[]): ChatSession[] => {
  const sorted$ = useMemo(() => {
    if (sessions.length === 0) return of<ChatSession[]>([]);

    return combineLatest(
      sessions.map(s => s.lastMessage.pipe(map(msg => ({ session: s, timestamp: msg?.timestamp ?? 0 })))),
    ).pipe(map(items => [...items].sort((a, b) => b.timestamp - a.timestamp).map(i => i.session)));
  }, [sessions]);

  return useObservable(sorted$, sessions);
};
