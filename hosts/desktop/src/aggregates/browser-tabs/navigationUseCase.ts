import { type RegisteredRouter } from '@tanstack/react-router';

import { pathnameMatchesSegment } from '@/shared/utils';

import { CHAT, FAVORITES } from './constants';
import { browserTabs } from './state/tabs';

const SYSTEM_TAB_ROUTES: Record<string, '/chat/{-$chatId}' | '/settings' | '/dashboard' | '/favorites'> = {
  [CHAT]: '/chat/{-$chatId}',
  [FAVORITES]: '/favorites',
  settings: '/settings',
  dashboard: '/dashboard',
};

/**
 * What the navigation use case needs from the app router. Supplied by the React
 * binding rather than imported: `@/router` is the composition root and imports every
 * feature, and an aggregate may not depend on features.
 */
type NavigationRouter = Pick<RegisteredRouter, 'state' | 'navigate'>;

export const isSystemTabType = (tabType: string): boolean => tabType in SYSTEM_TAB_ROUTES;

const routeSegmentForTabType = (tabType: string): string => `/${tabType}`;

/**
 * `/` is the boot route, and it is the only route that decides where the app goes from
 * the core's auth state. Its loader has to await the core, and both navigators below run
 * from a mount effect, so from `/` either one moves the user before that answer arrives
 * — and the redirect it pre-empts is the one that would have sent a signed-out user to
 * onboarding. A logout ends up here too: `reloadRenderer` resets the hash to `/` for
 * exactly that decision. Neither navigator has anything to say about `/`, so both wait.
 */
const isBootRoute = (pathname: string): boolean => pathname === '/';

/** Navigates to the route for the currently selected system tab (chat/settings/dashboard). */
const syncRouteToSelectedTab = (router: NavigationRouter): void => {
  const selectedId = browserTabs.selectedTabId$.get();
  if (selectedId === null) return;

  const tab = browserTabs.tabs$.get().find(t => t.id === selectedId);
  if (!tab) return;

  const to = SYSTEM_TAB_ROUTES[tab.type];
  if (!to) return;

  const pathname = router.state.location.pathname;
  if (isBootRoute(pathname)) return;
  if (pathnameMatchesSegment(pathname, routeSegmentForTabType(tab.type))) return;

  void router.navigate({ to });
};

/** When all tabs are closed, leave feature routes and show the dashboard home. */
const navigateHomeWhenNoTabs = (router: NavigationRouter): void => {
  if (browserTabs.tabs$.get().length > 0) return;

  const pathname = router.state.location.pathname;
  if (isBootRoute(pathname)) return;
  if (pathnameMatchesSegment(pathname, '/dashboard')) return;

  void router.navigate({ to: '/dashboard' });
};

export const navigationUseCase = {
  syncRouteToSelectedTab,
  navigateHomeWhenNoTabs,
};
