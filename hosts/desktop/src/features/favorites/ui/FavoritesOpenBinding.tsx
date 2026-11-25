import { useSideEffect } from '@/shared/di';
import { openFavoritesSideEffect } from '@/features/dashboard';
import { useOpenFavorites } from '../hooks/useOpenFavorites';

// Opens the fullscreen Favorites SPA when the folder widget's "View more" tile is
// pressed. The dashboard host owns the seam (`openFavoritesSideEffect`); this
// feature provides the navigation, so the dashboard needs no favourites-route knowledge.
export const FavoritesOpenBinding = () => {
  const openFavorites = useOpenFavorites();

  useSideEffect(openFavoritesSideEffect, () => openFavorites());

  return null;
};
