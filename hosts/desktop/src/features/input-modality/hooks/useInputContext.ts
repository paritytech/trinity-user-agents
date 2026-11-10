import { useLocation } from '@tanstack/react-router';
import { useMemo } from 'react';
import { useObservable } from 'react-rx';

import { useDashboardProductIds } from '@/domains/application';
import { useUserProductRooms } from '@/domains/chat';
import { usePersistedProducts } from '@/domains/product';
import { browserTabs, isSystemTabType } from '@/aggregates/browser-tabs';
import { useTruapiUserId } from '@/aggregates/truapi-runtime';
import { inputModalityService } from '../service';

/** The product ids the current screen contributes, live. */
export const useInputContext = (): string[] => {
  const { pathname } = useLocation();
  const selectedTab = useObservable(browserTabs.selectedTab$, null);
  const { data: placedIds } = useDashboardProductIds();
  const { data: rooms } = useUserProductRooms(useTruapiUserId());
  const { data: installed } = usePersistedProducts();

  const dashboardProductIds = useMemo(
    () =>
      inputModalityService.installedAmong(
        placedIds,
        installed.map(product => product.baseName),
      ),
    [placedIds, installed],
  );

  const roomProductIds = useMemo(() => inputModalityService.distinctProductIds(rooms), [rooms]);

  // A product tab's id is the product identifier; a system tab (chat, settings,
  // favorites, dashboard) names a surface, not a product.
  const screenProductId = selectedTab && !isSystemTabType(selectedTab.type) ? selectedTab.id : null;

  return useMemo(
    () => inputModalityService.contextProductIds({ pathname, screenProductId, dashboardProductIds, roomProductIds }),
    [pathname, screenProductId, dashboardProductIds, roomProductIds],
  );
};
