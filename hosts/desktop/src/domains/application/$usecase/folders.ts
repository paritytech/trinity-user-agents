import {
  DEFAULT_RESIZE_HANDLES,
  FAVORITES_FOLDER_ID,
  FOLDER_DEFAULT_HEIGHT,
  FOLDER_MIN_HEIGHT,
  MAX_WIDGET_HEIGHT,
} from '../dashboard-layout/constants';
import { dashboardLayoutDb } from '../dashboard-layout/repository';
import { dashboardLayoutService } from '../dashboard-layout/service';
import { type DashboardCard } from '../dashboard-layout/types';

// Locates a folder card by id across pages, returning the card and its page.
function findFolderById(pages: DashboardCard[][], folderId: string): { pageIndex: number; item: DashboardCard } | null {
  for (let pageIndex = 0; pageIndex < pages.length; pageIndex++) {
    const page = pages[pageIndex] ?? [];
    const item = page.find(entry => dashboardLayoutService.isFolderCard(entry) && entry.i === folderId);
    if (item) return { pageIndex, item };
  }
  return null;
}

// Adds an item to a folder's items, seeding the folder card if it does not yet
// exist. Seeding is only supported for the favourites folder — any other target
// folder must already exist (a user folder is created by the user, not implied
// by an add). Rejects when the item is already in the folder.
async function addItemToFolder(folderId: string, itemId: string): Promise<{ ok: boolean; pageIndex?: number }> {
  const main = await dashboardLayoutDb.getMain();
  if (main.isErr()) return { ok: false };
  const sourcePages = dashboardLayoutService.ensurePages(main.value?.pages ?? null);

  const existing = findFolderById(sourcePages, folderId);
  const existingFolder = existing ? dashboardLayoutService.asFolder(existing.item) : null;
  if (existingFolder?.items.includes(itemId)) return { ok: false };

  // No folder to seed into and not the favourites folder → nothing to do.
  if (!existing && folderId !== FAVORITES_FOLDER_ID) return { ok: false };

  // Adding to a folder mutates folder `items` only — it must NOT sweep a
  // coexisting top-level card of the same id. A product's standalone widget and
  // its favourites membership are independent placements (symmetric with
  // `removeItemFromFolder`); stripping here made `browse.dot` — the default
  // seeded widget whose card id equals its productId — vanish from the dashboard.
  let nextPages: DashboardCard[][];
  let pageIndex: number;
  if (existing && existingFolder) {
    const nextFolder: DashboardCard = {
      ...existing.item,
      payload: { ...existingFolder, items: [...existingFolder.items, itemId] },
    };
    nextPages = sourcePages.map((page, pi) =>
      pi === existing.pageIndex ? page.map(entry => (entry.i === folderId ? nextFolder : entry)) : page,
    );
    pageIndex = existing.pageIndex;
  } else {
    const newFolder: DashboardCard = {
      i: FAVORITES_FOLDER_ID,
      x: 0,
      y: 0,
      w: 1,
      h: FOLDER_DEFAULT_HEIGHT,
      minW: 1,
      maxW: 1,
      minH: FOLDER_MIN_HEIGHT,
      maxH: MAX_WIDGET_HEIGHT,
      resizeHandles: [...DEFAULT_RESIZE_HANDLES],
      payload: { kind: 'folder', items: [itemId] },
    };
    const preferred = main.value?.activePageIndex ?? 0;
    ({ pages: nextPages, pageIndex } = dashboardLayoutService.placeOnPages(sourcePages, newFolder, preferred));
  }

  const saveResult = await dashboardLayoutDb.saveMainPages(nextPages, pageIndex);
  return { ok: saveResult.isOk(), pageIndex: saveResult.isOk() ? pageIndex : undefined };
}

function addToFavorites(itemId: string): Promise<{ ok: boolean; pageIndex?: number }> {
  return addItemToFolder(FAVORITES_FOLDER_ID, itemId);
}

async function isIconInFavorites(iconId: string): Promise<boolean> {
  const main = await dashboardLayoutDb.getMain();
  if (main.isErr()) return false;
  const pages = dashboardLayoutService.ensurePages(main.value?.pages ?? null);
  return dashboardLayoutService.favoriteProductIds(pages).has(iconId);
}

async function removeItemFromFolder(itemId: string): Promise<boolean> {
  const result = await dashboardLayoutDb.getMainPages();
  if (result.isErr() || !result.value) return false;

  let changed = false;
  const nextPages = result.value.map(page =>
    page.map(item => {
      const folder = dashboardLayoutService.asFolder(item);
      if (!folder || !folder.items.includes(itemId)) return item;
      changed = true;
      return {
        ...item,
        payload: {
          ...folder,
          items: folder.items.filter(id => id !== itemId),
        },
      };
    }),
  );

  if (!changed) return false;
  const saveResult = await dashboardLayoutDb.saveMainPages(nextPages);
  return saveResult.isOk();
}

// Reorders a folder's `items` to match `orderedItemIds`. Only the ids present in
// `orderedItemIds` are reordered — in place, into the slots those ids currently
// occupy — so items outside the subset (native favourites, a search-hidden set,
// the overflow beyond a widget's visible cap) keep their slots. `items` order is
// the folder's only placement, so this is the single write both the Favorites SPA
// and the Favorites widget use for drag-and-drop.
async function reorderFolderItems(folderId: string, orderedItemIds: string[]): Promise<boolean> {
  const result = await dashboardLayoutDb.getMainPages();
  if (result.isErr() || !result.value) return false;

  const orderedSet = new Set(orderedItemIds);
  let changed = false;
  const nextPages = result.value.map(page =>
    page.map(item => {
      const folder = dashboardLayoutService.asFolder(item);
      if (!folder || item.i !== folderId) return item;

      let cursor = 0;
      const nextItems = folder.items.map(id => (orderedSet.has(id) ? (orderedItemIds[cursor++] ?? id) : id));
      if (nextItems.every((id, index) => id === folder.items[index])) return item;

      changed = true;
      return { ...item, payload: { ...folder, items: nextItems } };
    }),
  );

  if (!changed) return false;
  const saveResult = await dashboardLayoutDb.saveMainPages(nextPages);
  return saveResult.isOk();
}

export const foldersUseCase = {
  addItemToFolder,
  addToFavorites,
  isIconInFavorites,
  removeItemFromFolder,
  reorderFolderItems,
};
