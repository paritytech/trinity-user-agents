import { useNavigate } from '@tanstack/react-router';
import { useCallback } from 'react';

import { FAVORITES, browserTabs } from '@/aggregates/browser-tabs';

// Opens the fullscreen Favorites SPA as a system browser tab (mirrors chat): it
// materializes + selects the Favorites tab so it shows in the tab strip, then
// navigates to its route.
export const useOpenFavorites = () => {
  const navigate = useNavigate();

  return useCallback(() => {
    browserTabs.addTab({ id: FAVORITES, type: FAVORITES, deeplink: '' }, { persistable: true });
    browserTabs.selectTab(FAVORITES);
    void navigate({ to: '/favorites' });
  }, [navigate]);
};
