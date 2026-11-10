import { FAVORITES, useTabRouteBinding } from '@/aggregates/browser-tabs';

const favoritesTab = { id: FAVORITES, type: FAVORITES, deeplink: '' };

// Route → tab: materialize + select the Favorites tab whenever the user lands on
// `/favorites` (deep link, back/forward), so the tab strip stays in sync with the
// route even when the SPA is opened without going through the toolbar button.
export const FavoritesTabBinding = () => {
  useTabRouteBinding({ segment: '/favorites', tab: favoritesTab, touchAlive: true });

  return null;
};
