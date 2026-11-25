import { pathnameMatchesSegment } from '@/shared/utils';

export const DASHBOARD_TAB_ID = 'dashboard';

/** Native system tab / dashboard grid id for chat — also the `/chat` route segment. */
export const CHAT = 'chat';

export const CHAT_PATH = `/${CHAT}` as const;

/** True for `/chat` and nested chat routes (e.g. `/chat/{id}`), not `/chatbot`. */
export const isChatPathname = (pathname: string): boolean => pathnameMatchesSegment(pathname, CHAT_PATH);

/** Native system tab id for the fullscreen Favorites SPA — also the `/favorites` route segment. */
export const FAVORITES = 'favorites';

export const FAVORITES_PATH = `/${FAVORITES}` as const;

/** True for `/favorites` and nested favorites routes. */
export const isFavoritesPathname = (pathname: string): boolean => pathnameMatchesSegment(pathname, FAVORITES_PATH);
