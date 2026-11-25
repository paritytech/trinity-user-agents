import { useSideEffect } from '@/shared/di';
import { openFavoriteItemSideEffect } from '@/features/dashboard';
import { useOpenChatTab } from '../hooks/useOpenChatTab';
import { CHAT } from '../tabs';

// Opens chat when its favourites-folder icon is pressed. Chat is native (no chain
// product), so the dashboard delegates the open via this side effect.
export const ChatFavoriteOpenBinding = () => {
  const openChatTab = useOpenChatTab();

  useSideEffect(openFavoriteItemSideEffect, ({ itemId }) => {
    if (itemId === CHAT) openChatTab();
  });

  return null;
};
