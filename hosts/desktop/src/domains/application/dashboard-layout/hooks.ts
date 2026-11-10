import { useCallback, useEffect, useRef, useState } from 'react';
import { type Layout } from 'react-grid-layout';

import { layoutInteractionSession } from '@/shared/components';
import { useRead } from '@/shared/hooks';
import { cardsUseCase } from '../$usecase/cards';

import { mainDashboardLayoutResource, saveMainActivePage, saveMainLayout } from './resource';
import { dashboardLayoutService } from './service';
import { type DashboardCard } from './types';

const EMPTY_FAVORITES: ReadonlySet<string> = new Set();

// Live set of product base names in the Favorites folder. Reads the shared
// main-layout resource, so a membership check (e.g. the product actions menu)
// rides the same cached subscription as the dashboard grid rather than opening
// its own subscription to the `dashboardLayouts` table.
export const useFavoriteProductIds = () => {
  return useRead(mainDashboardLayoutResource, {
    params: {},
    defaultValue: EMPTY_FAVORITES,
    map: snapshot => dashboardLayoutService.favoriteProductIds(snapshot?.pages ?? []),
  });
};

const EMPTY_PRODUCT_IDS: string[] = [];

// Every product placed on the dashboard — the input-routing context set for the
// dashboard screen. Rides the same cached main-layout subscription as the grid
// rather than opening its own. Folder items mix product base names with native
// grid ids, so the caller intersects this with the products it knows.
export const useDashboardProductIds = () => {
  return useRead(mainDashboardLayoutResource, {
    params: {},
    defaultValue: EMPTY_PRODUCT_IDS,
    map: snapshot => dashboardLayoutService.placedProductIds(snapshot?.pages ?? []),
  });
};

// Imperative setter for the main dashboard's active page. Used by entry points
// (e.g. the Home button) that jump to a page without mounting the full grid, so
// they reach the layout through the domain instead of the repository.
export const useSetMainActivePage = () =>
  useCallback((index: number) => {
    void saveMainActivePage(index);
  }, []);

export function useDashboardLayouts() {
  // One empty page until the first `read$` emission. The default dashboard names
  // a product, and a product id carries the network TLD this domain cannot reach
  // — so rather than invent one, the pre-load state holds no card at all and a
  // first-run write can never persist a name that resolves on no network.
  // Seeding is owned by `ensureDefaultDashboard` and arrives through this same
  // subscription.
  const [pages, setPages] = useState<DashboardCard[][]>([[]]);
  const [activePageIndex, setActivePageIndexState] = useState(0);
  const activePageIndexRef = useRef(0);
  const [isLoading, setIsLoading] = useState(true);

  useEffect(() => {
    const subscription = mainDashboardLayoutResource.read$({}).subscribe({
      next: snapshot => {
        if (snapshot && dashboardLayoutService.hasPages(snapshot.pages)) {
          if (layoutInteractionSession.drag || layoutInteractionSession.resize) {
            return;
          }

          const normalized = dashboardLayoutService.normalizeFavoritesFolder(snapshot.pages);
          let clampChanged = false;
          const clampedPages = normalized.map(page => {
            let pageChanged = false;
            const next = page.map(card => {
              const c = dashboardLayoutService.clampCard(card);
              if (c !== card) pageChanged = true;
              return c;
            });
            if (pageChanged) clampChanged = true;
            return pageChanged ? next : page;
          });
          const clamped = clampChanged ? clampedPages : normalized;
          const sanitized = clamped !== snapshot.pages;
          setPages(clamped);
          const nextActiveIndex = Math.min(snapshot.activePageIndex, clamped.length - 1);
          activePageIndexRef.current = nextActiveIndex;
          setActivePageIndexState(nextActiveIndex);
          if (sanitized) {
            saveMainLayout(clamped);
          }
          setIsLoading(false);
        } else {
          // No saved dashboard yet. Seeding is a first-run concern owned by
          // `ensureDefaultDashboard` at app bootstrap, not this view — the hook
          // just shows the default layout (its initial state) until the seed
          // write lands and re-emits through this same subscription.
          setIsLoading(false);
        }
      },
      error: err => {
        console.error('Error loading dashboard layout:', err);
        setIsLoading(false);
      },
    });

    return () => {
      subscription.unsubscribe();
    };
  }, []);

  const savePages = useCallback(async (newPages: DashboardCard[][], activeIndex?: number) => {
    const nextActiveIndex = activeIndex ?? activePageIndexRef.current;
    await saveMainLayout(newPages, nextActiveIndex);
  }, []);

  const updatePageAndSave = useCallback(
    (pageIndex: number, newLayout: DashboardCard[]) => {
      setPages(prev => {
        const next = prev.map((page, index) => (index === pageIndex ? newLayout : page));
        savePages(next);
        return next;
      });
    },
    [savePages],
  );

  const setActivePageIndex = useCallback((index: number) => {
    setActivePageIndexState(prev => {
      if (prev === index) return prev;
      activePageIndexRef.current = index;
      saveMainActivePage(index);
      return index;
    });
  }, []);

  const activeLayout = pages[activePageIndex] ?? [];

  const handleLayoutChange = useCallback(
    (newLayout: Layout): void => {
      const processedLayout = dashboardLayoutService.processLayoutChange(newLayout, activeLayout);
      if (processedLayout === null) return;

      updatePageAndSave(activePageIndex, processedLayout);
    },
    [activeLayout, activePageIndex, updatePageAndSave],
  );

  const handleAutolayout = useCallback(() => {
    setPages(prev => {
      const nextPages = dashboardLayoutService.autoLayout(prev);
      if (nextPages === prev) return prev;

      const nextActive = Math.min(activePageIndex, nextPages.length - 1);
      activePageIndexRef.current = nextActive;
      setActivePageIndexState(prevActive => (prevActive === nextActive ? prevActive : nextActive));
      savePages(nextPages, nextActive);
      return nextPages;
    });
  }, [activePageIndex, savePages]);

  // Card lifecycle (remove/resize) goes through the card use cases — the single
  // write path to the `dashboardLayouts` table. The use case persists and the
  // live `mainDashboardLayoutResource` subscription re-emits the new layout, so
  // there is no separate optimistic copy to keep in sync.
  const removeWidget = useCallback((productId: string) => cardsUseCase.removeCardFromLayout(productId), []);

  const resizeWidget = useCallback(
    (widgetId: string, size: { w: number; h: number }) => cardsUseCase.resizeCardToGridSize(widgetId, size),
    [],
  );

  // A folder is a first-class card, so removing it is removing its card.
  const removeFolder = useCallback((folderId: string) => cardsUseCase.removeCardFromLayout(folderId), []);

  const moveItemToAdjacentPage = useCallback(
    (itemId: string, direction: -1 | 1, dropRow?: number) => {
      setPages(prev => {
        const result = dashboardLayoutService.moveItemToAdjacentPage(prev, itemId, direction, dropRow);
        if (!result) return prev;

        const { nextPages, targetPageIndex } = result;
        activePageIndexRef.current = targetPageIndex;
        setActivePageIndexState(targetPageIndex);
        savePages(nextPages, targetPageIndex);
        return nextPages;
      });
    },
    [savePages],
  );

  const moveItemByPageDelta = useCallback(
    (itemId: string, delta: number, dropRow?: number) => {
      if (delta === 0) return;

      const sign = delta > 0 ? 1 : -1;
      const steps = Math.abs(delta);

      setPages(prev => {
        let current = prev;
        let targetPageIndex = activePageIndexRef.current;
        let moved = false;

        for (let step = 0; step < steps; step++) {
          // The dragged row only applies to the final landing page; intermediate
          // hops just relay the widget onward.
          const row = step === steps - 1 ? dropRow : undefined;
          const result = dashboardLayoutService.moveItemToAdjacentPage(current, itemId, sign, row);
          if (!result) break;
          current = result.nextPages;
          targetPageIndex = result.targetPageIndex;
          moved = true;
        }

        if (!moved) return prev;

        activePageIndexRef.current = targetPageIndex;
        setActivePageIndexState(targetPageIndex);
        savePages(current, targetPageIndex);
        return current;
      });
    },
    [savePages],
  );

  const moveItemToPrevPage = useCallback(
    (itemId: string, dropRow?: number) => {
      moveItemToAdjacentPage(itemId, -1, dropRow);
    },
    [moveItemToAdjacentPage],
  );

  const moveItemToNextPage = useCallback(
    (itemId: string, dropRow?: number) => {
      moveItemToAdjacentPage(itemId, 1, dropRow);
    },
    [moveItemToAdjacentPage],
  );

  return {
    pages,
    activePageIndex,
    setActivePageIndex,
    layout: activeLayout,
    isLoading,
    handleLayoutChange,
    handleAutolayout,
    removeWidget,
    resizeWidget,
    removeFolder,
    moveItemToPrevPage,
    moveItemToNextPage,
    moveItemByPageDelta,
  };
}
