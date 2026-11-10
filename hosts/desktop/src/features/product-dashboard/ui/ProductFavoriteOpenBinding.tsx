import { useSideEffect } from '@/shared/di';
import { openFavoriteItemSideEffect } from '@/features/dashboard';
import { useOpenProductSurface } from '../hooks/useOpenProductSurface';

// Registers the product open handler for the dashboard's favourites-folder open
// seam. Every folder item opens via `openFavoriteItemSideEffect`; this handler
// resolves the chain product for the id and opens its surface. It no-ops for
// non-products since `resolveProduct` returns null, so native/chat handlers
// claim their own ids.
export const ProductFavoriteOpenBinding = () => {
  const openProduct = useOpenProductSurface();

  useSideEffect(openFavoriteItemSideEffect, ({ itemId }) => {
    void openProduct(itemId);
  });

  return null;
};
