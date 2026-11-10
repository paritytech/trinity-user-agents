import { type RefObject, useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';

import { type ChatMessage, type ReactionAggregate } from '@/domains/chat';

// Within this many px of the bottom counts as "at the bottom" (autoscroll on, badge hidden).
const BOTTOM_THRESHOLD_PX = 80;

// Stable identity for a reaction across re-aggregation, so it's never double-counted as unread.
// Newline join is collision-safe: ids, emoji, and names are all single-line.
const reactionKey = (messageId: string, emoji: string, reactorName: string) => `${messageId}\n${emoji}\n${reactorName}`;

const messageIdOfKey = (key: string) => key.slice(0, key.indexOf('\n'));

// Own-device reactions are deliberately kept (not filtered by `isMe`): in multi-device a reaction made
// elsewhere is still news, and it only becomes "unread" once its message is off screen.
export function reactionKeysOnOwnMessages(messages: ChatMessage[], reactions: Map<string, ReactionAggregate[]>): Set<string> {
  const keys = new Set<string>();
  for (const message of messages) {
    if (message.status.direction !== 'outgoing') continue;
    const aggregates = reactions.get(message.messageId);
    if (!aggregates) continue;
    for (const aggregate of aggregates) {
      for (const reactor of aggregate.reactors) {
        keys.add(reactionKey(message.messageId, aggregate.emoji, reactor.name));
      }
    }
  }
  return keys;
}

// "Seen" once any part of the row overlaps the viewport — i.e. its reaction pill is on screen.
function isMessageVisible(container: HTMLElement, messageId: string): boolean {
  const el = container.querySelector(`[data-message-id="${CSS.escape(messageId)}"]`);
  if (!el) return false;
  const containerRect = container.getBoundingClientRect();
  const rect = el.getBoundingClientRect();
  return rect.bottom > containerRect.top && rect.top < containerRect.bottom;
}

// Otherwise the view stays put and new messages surface via the scroll-to-bottom badge instead.
export function shouldAutoScrollOnNewMessages({
  firstOpen,
  wasAtBottom,
  newestIsOutgoing,
}: {
  firstOpen: boolean;
  wasAtBottom: boolean;
  newestIsOutgoing: boolean;
}): boolean {
  return firstOpen || wasAtBottom || newestIsOutgoing;
}

type UseScrollControlsParams = {
  scrollRef: RefObject<HTMLDivElement | null>;
  messages: ChatMessage[];
  reactions: Map<string, ReactionAggregate[]>;
};

export const useScrollControls = ({ scrollRef, messages, reactions }: UseScrollControlsParams) => {
  const [isAtBottom, setIsAtBottom] = useState(true);
  const [unreadCount, setUnreadCount] = useState(0);
  // messageId -> set of unread reaction keys on that message.
  const [unreadReactions, setUnreadReactions] = useState<Map<string, Set<string>>>(() => new Map());

  const atBottomRef = useRef(true);
  const firstOpenRef = useRef(true);
  const rafRef = useRef<number | null>(null);
  // Reaction keys already accounted for (seen or already recorded as unread), so
  // a re-aggregation never re-triggers them.
  const processedReactionKeysRef = useRef<Set<string> | null>(null);
  const prevMessageIdsRef = useRef<Set<string>>(new Set());

  const scrollToBottom = useCallback(
    (behavior: ScrollBehavior = 'smooth') => {
      const el = scrollRef.current;
      if (!el) return;
      el.scrollTo({ top: el.scrollHeight, behavior });
    },
    [scrollRef],
  );

  const onScroll = useCallback(() => {
    if (rafRef.current !== null) return;
    rafRef.current = requestAnimationFrame(() => {
      rafRef.current = null;
      const el = scrollRef.current;
      if (!el) return;
      const distance = el.scrollHeight - el.scrollTop - el.clientHeight;
      const atBottom = distance < BOTTOM_THRESHOLD_PX;
      atBottomRef.current = atBottom;
      setIsAtBottom(prev => (prev === atBottom ? prev : atBottom));

      // Reactions become read once their message scrolls into view.
      setUnreadReactions(prev => {
        if (prev.size === 0) return prev;
        let changed = false;
        const next = new Map(prev);
        for (const messageId of prev.keys()) {
          if (isMessageVisible(el, messageId)) {
            next.delete(messageId);
            changed = true;
          }
        }
        return changed ? next : prev;
      });
    });
  }, [scrollRef]);

  useEffect(() => () => (rafRef.current !== null ? cancelAnimationFrame(rafRef.current) : undefined), []);

  // Reaching the bottom clears the new-message counter (unread reactions clear
  // per-message as each reacted message scrolls into view — see onScroll).
  useEffect(() => {
    if (!isAtBottom) return;
    setUnreadCount(prev => (prev === 0 ? prev : 0));
  }, [isAtBottom]);

  // Layout effect so scrollHeight already includes the freshly-rendered message.
  useLayoutEffect(() => {
    const firstOpen = firstOpenRef.current;
    // Stay "first open" until there is content to anchor to, so the initial jump
    // is a single instant scroll rather than a smooth one on the first batch.
    if (firstOpen && messages.length === 0) return;

    const prevIds = prevMessageIdsRef.current;
    const newMessages = messages.filter(m => !prevIds.has(m.messageId));
    prevMessageIdsRef.current = new Set(messages.map(m => m.messageId));

    if (firstOpen) {
      firstOpenRef.current = false;
      scrollToBottom('instant');
      return;
    }
    if (newMessages.length === 0) return;

    const newestNew = newMessages.reduce((latest, m) => (m.timestamp >= latest.timestamp ? m : latest));
    const shouldScroll = shouldAutoScrollOnNewMessages({
      firstOpen: false,
      wasAtBottom: atBottomRef.current,
      newestIsOutgoing: newestNew.status.direction === 'outgoing',
    });

    if (shouldScroll) {
      scrollToBottom('smooth');
    } else {
      setUnreadCount(prev => prev + newMessages.length);
    }
  }, [messages, scrollToBottom]);

  // A new reaction on one of my messages becomes unread unless its message is already on screen;
  // it clears when the message scrolls into view (onScroll) or on heart click.
  useEffect(() => {
    const currentKeys = reactionKeysOnOwnMessages(messages, reactions);

    if (processedReactionKeysRef.current === null) {
      // Snapshot reactions already present on open as seen.
      processedReactionKeysRef.current = currentKeys;
      return;
    }

    const processed = processedReactionKeysRef.current;
    const newKeys = [...currentKeys].filter(key => !processed.has(key));
    processedReactionKeysRef.current = currentKeys;

    const container = scrollRef.current;
    setUnreadReactions(prev => {
      if (prev.size === 0 && newKeys.length === 0) return prev;

      const next = new Map<string, Set<string>>();
      let changed = false;

      // Keep only still-present reactions — a removed reaction drops from the counter.
      for (const [messageId, set] of prev) {
        const kept = new Set([...set].filter(key => currentKeys.has(key)));
        if (kept.size !== set.size) changed = true;
        if (kept.size > 0) next.set(messageId, kept);
      }

      // Record newly-arrived reactions whose message is off screen.
      for (const key of newKeys) {
        const messageId = messageIdOfKey(key);
        if (container && isMessageVisible(container, messageId)) continue;
        const set = new Set(next.get(messageId) ?? []);
        if (!set.has(key)) {
          set.add(key);
          changed = true;
        }
        next.set(messageId, set);
      }

      return changed ? next : prev;
    });
  }, [messages, reactions, scrollRef]);

  const unreadReactionCount = useMemo(() => {
    let total = 0;
    for (const set of unreadReactions.values()) total += set.size;
    return total;
  }, [unreadReactions]);

  const scrollToOldestUnreadReaction = useCallback(() => {
    if (unreadReactions.size === 0) return;

    const timestampById = new Map(messages.map(m => [m.messageId, m.timestamp]));
    let oldestId: string | null = null;
    let oldestTimestamp = Number.POSITIVE_INFINITY;
    for (const messageId of unreadReactions.keys()) {
      const timestamp = timestampById.get(messageId);
      if (timestamp === undefined) continue;
      if (timestamp < oldestTimestamp) {
        oldestTimestamp = timestamp;
        oldestId = messageId;
      }
    }
    if (oldestId === null) return;

    const target = scrollRef.current?.querySelector(`[data-message-id="${CSS.escape(oldestId)}"]`);
    target?.scrollIntoView({ behavior: 'smooth', block: 'center' });

    const clearedId = oldestId;
    setUnreadReactions(prev => {
      if (!prev.has(clearedId)) return prev;
      const next = new Map(prev);
      next.delete(clearedId);
      return next;
    });
  }, [messages, unreadReactions, scrollRef]);

  return { isAtBottom, unreadCount, unreadReactionCount, onScroll, scrollToBottom, scrollToOldestUnreadReaction };
};
