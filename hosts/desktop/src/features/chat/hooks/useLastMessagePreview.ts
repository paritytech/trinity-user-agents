import { useMemo } from 'react';
import { useObservable } from 'react-rx';
import { map, of, switchMap } from 'rxjs';

import { useTranslation } from '@/shared/translation';
import { type ChatSession } from '@/domains/chat';
import { type MessagePreview, EMPTY_PREVIEW, getMessagePreview, getMessagePreviewIcon, getPreview } from '../ui/helpers/message';

/**
 * The one-line preview a chat-list row shows for a session's last message.
 *
 * A call renders exactly as the conversation view renders it — the same icon
 * and the same state-aware title as `CallMessageBubble`. Deriving that state
 * needs the whole message array, so `session.messages` is subscribed to only
 * when the last message is actually a call; every other room stays on the
 * cheap `lastMessage`-only path.
 */
export function useLastMessagePreview(session: ChatSession): MessagePreview {
  const { t } = useTranslation();

  const preview$ = useMemo(
    () =>
      session.lastMessage.pipe(
        switchMap(last => {
          if (!last) return of(EMPTY_PREVIEW);
          if (last.content.type !== 'callSignal') {
            return of({ text: getMessagePreview(last), icon: getMessagePreviewIcon(last) });
          }

          return session.messages.pipe(map(messages => getPreview(last, messages, t)));
        }),
      ),
    [session, t],
  );

  return useObservable(preview$, EMPTY_PREVIEW);
}
