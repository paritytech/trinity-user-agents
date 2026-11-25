import { useCallback, useMemo } from 'react';

import { type ChatMessage, type ChatSession } from '../session/types';

import { reactionService } from './service';
import { type ReactionAggregate } from './types';

export function useMessageReactions(messages: ChatMessage[]): Map<string, ReactionAggregate[]> {
  return useMemo(() => reactionService.aggregateReactions(messages), [messages]);
}

export function useToggleReaction(session: ChatSession, messages: ChatMessage[]) {
  const reactions = useMessageReactions(messages);

  return useCallback(
    async (messageId: string, emoji: string) => {
      for (const content of reactionService.resolveToggle(reactions.get(messageId), messageId, emoji)) {
        await session.sendMessage(content);
      }
    },
    [session, reactions],
  );
}
