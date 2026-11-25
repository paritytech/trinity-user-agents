import { useLocation, useRouter } from '@tanstack/react-router';
import { useEffect } from 'react';

import { useRxState } from '@/shared/rxstate';
import { truapiRuntime } from '@/aggregates/truapi-runtime';
import { navigationUseCase } from '../navigationUseCase';
import { browserTabs } from '../state/tabs';

/** Single selection→route subscriber for system tabs (chat/settings/dashboard). */
export const BrowserTabsNavigationBinding = () => {
  const [selectedId] = useRxState(browserTabs.selectedTabId$);
  const [tabs] = useRxState(browserTabs.tabs$);
  const router = useRouter();
  // Boot lands here on `/`, where the sync declines to move anyone (see
  // `navigationUseCase`). Depending on the flag rather than the pathname re-runs the sync
  // once — when the boot route resolves — so a restored tab is still reached, without
  // making later navigation re-trigger it and fight the user.
  const onBootRoute = useLocation({ select: ({ pathname }) => pathname === '/' });

  useEffect(() => {
    navigationUseCase.syncRouteToSelectedTab(router);
  }, [router, selectedId, onBootRoute]);

  useEffect(() => {
    navigationUseCase.navigateHomeWhenNoTabs(router);
  }, [router, tabs]);

  // Drop the tab selection when the session goes away. Compared against the
  // previous value rather than the current one so the pre-boot `null` auth state
  // (the core has not answered yet) never reads as a sign-out.
  useEffect(() => {
    let wasConnected = truapiRuntime.get().authState?.tag === 'Connected';

    const subscription = truapiRuntime.value$.subscribe(({ authState }) => {
      const isConnected = authState?.tag === 'Connected';
      if (wasConnected && !isConnected) {
        browserTabs.selectTab(null);
      }
      wasConnected = isConnected;
    });

    return () => subscription.unsubscribe();
  }, []);

  return null;
};
