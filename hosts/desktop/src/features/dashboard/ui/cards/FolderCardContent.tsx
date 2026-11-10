import { useCallback, useMemo, useState } from 'react';

import { WidgetPlaceholder } from '@/shared/components';
import { useTranslation } from '@/shared/translation';
import { dashboardLayoutService, foldersUseCase } from '@/domains/application';
import { openAddToFavoritesSideEffect, openFavoriteItemSideEffect, openFavoritesSideEffect } from '../../di';
import { type FolderItemIconSize } from '../../types';
import { FolderGrid } from '../folder/FolderGrid';

type Props = {
  cardId: string;
  items: string[];
  isActivePage: boolean;
  maxVisibleItems: number;
  iconSize: FolderItemIconSize;
};

// Folder body — the grid of icon-sized shortcuts. The surrounding card frame
// (topbar with FolderIcon + label + menu) comes from `DashboardCardChrome`;
// this component renders only what lives inside the body. The per-item icon+label
// and the open action are content-generic: rendering goes through
// `folderItemContentTransformer` (per cell) and opening through
// `openFavoriteItemSideEffect`, so the dashboard host stays product-agnostic.
export const FolderCardContent = ({ cardId, items, isActivePage, maxVisibleItems, iconSize }: Props) => {
  const { t } = useTranslation();
  const [openMenuId, setOpenMenuId] = useState<string | null>(null);

  const handleOpen = useCallback((itemId: string) => {
    void openFavoriteItemSideEffect.apply({ itemId });
  }, []);

  // Cap to what fits the current widget size. On overflow the last slot becomes a
  // "View more" tile (opening the Favorites SPA), so one fewer item is shown. Extra
  // favourites stay saved — the `items` source is untouched.
  const { visibleCount, hasViewMore } = dashboardLayoutService.getFavoritesDisplay(items.length, maxVisibleItems);
  // Memoized: the grid keeps an optimistic copy of this order and reconciles against
  // it, so handing it a fresh array every render would re-run that reconciliation.
  const visibleIds = useMemo(() => items.slice(0, visibleCount), [items, visibleCount]);

  const handleOpenViewMore = () => {
    void openFavoritesSideEffect.apply(undefined);
  };

  const handleBrowseApps = () => {
    if (!isActivePage) return;
    void openAddToFavoritesSideEffect.apply(undefined);
  };

  const handleRemove = (widgetId: string) => {
    if (!isActivePage) return;
    void foldersUseCase.removeItemFromFolder(widgetId);
  };

  // Item order IS the placement, so a widget drag writes the same order the
  // Favorites SPA reads and writes. Only the visible prefix is passed; the use case
  // reorders that subset in place, leaving overflow items untouched.
  const handleReorderItems = (orderedItemIds: string[]) => {
    if (!isActivePage) return;
    void foldersUseCase.reorderFolderItems(cardId, orderedItemIds);
  };

  if (items.length === 0) {
    return (
      <WidgetPlaceholder
        message={t('feature.dashboard.favorites.emptyPlaceholder')}
        actionLabel={t('feature.dashboard.favorites.browseApps')}
        onAction={handleBrowseApps}
      />
    );
  }

  return (
    <div className="h-full w-full overflow-hidden p-2">
      <FolderGrid
        folderId={cardId}
        items={visibleIds}
        maxVisibleItems={maxVisibleItems}
        iconSize={iconSize}
        hasViewMore={hasViewMore}
        openMenuId={openMenuId}
        onMenuOpenChange={(menuId, open) => setOpenMenuId(prev => (open ? menuId : prev === menuId ? null : prev))}
        onOpenWidget={handleOpen}
        onRemoveWidget={handleRemove}
        onReorderItems={handleReorderItems}
        onOpenViewMore={handleOpenViewMore}
      />
    </div>
  );
};
