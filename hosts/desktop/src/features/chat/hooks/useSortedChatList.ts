import { useMemo } from 'react';
import { useObservable } from 'react-rx';
import { combineLatest, map, of } from 'rxjs';

import { type ChatSession, type P2PChatRequest } from '@/domains/chat';

/**
 * One chat-list row. Rooms, outgoing requests, and the aggregated incoming-request
 * inbox all sort on the same timeline so a fresh request isn't stuck above an
 * older-but-more-recently-active room (or vice versa).
 */
export type ChatListEntry =
  | { kind: 'incoming'; timestamp: number; requests: P2PChatRequest[] }
  | { kind: 'outgoing'; timestamp: number; request: P2PChatRequest }
  | { kind: 'session'; timestamp: number; session: ChatSession };

const byNewest = (a: ChatListEntry, b: ChatListEntry) => b.timestamp - a.timestamp;

const requestEntries = (outgoingRequests: P2PChatRequest[], pendingRequests: P2PChatRequest[]): ChatListEntry[] => {
  const entries: ChatListEntry[] = outgoingRequests.map(request => ({ kind: 'outgoing', timestamp: request.timestamp, request }));
  if (pendingRequests.length > 0) {
    // The inbox sits at its newest request so it tracks the freshest incoming activity.
    const timestamp = Math.max(...pendingRequests.map(request => request.timestamp));
    entries.push({ kind: 'incoming', timestamp, requests: pendingRequests });
  }

  return entries;
};

/** Rooms + requests merged newest-first. A session's timestamp is its last message. */
export const useSortedChatList = (
  sessions: ChatSession[],
  outgoingRequests: P2PChatRequest[],
  pendingRequests: P2PChatRequest[],
): ChatListEntry[] => {
  const entries$ = useMemo(() => {
    const requests = requestEntries(outgoingRequests, pendingRequests);
    const sessions$ =
      sessions.length === 0
        ? of<ChatListEntry[]>([])
        : combineLatest(
            sessions.map(session =>
              session.lastMessage.pipe(map(msg => ({ kind: 'session' as const, timestamp: msg?.timestamp ?? 0, session }))),
            ),
          );

    return sessions$.pipe(map(sessionEntries => [...sessionEntries, ...requests].sort(byNewest)));
  }, [sessions, outgoingRequests, pendingRequests]);

  // Until the last-message streams emit, seed with sessions at timestamp 0 so nothing
  // flickers out of the list; the sort settles once the streams resolve.
  const fallback = useMemo<ChatListEntry[]>(
    () =>
      [
        ...sessions.map(session => ({ kind: 'session' as const, timestamp: 0, session })),
        ...requestEntries(outgoingRequests, pendingRequests),
      ].sort(byNewest),
    [sessions, outgoingRequests, pendingRequests],
  );

  return useObservable(entries$, fallback);
};
