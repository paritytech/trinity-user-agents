import { DASHBOARD_GRID_SNAP_Y_STEP, GRID_COLS } from '@/shared/components';

import {
  ALLOWED_WIDGET_HEIGHTS,
  DEFAULT_RESIZE_HANDLES,
  FAVORITES_FOLDER_ID,
  FOLDER_MIN_HEIGHT,
  HEIGHT_HINT_TO_VARIANT,
  HORIZONTAL_HEIGHT_MARKER,
  MAX_GRID_ROWS,
  MAX_VISIBLE_FAVORITES_BY_VARIANT,
  MAX_WIDGET_HEIGHT,
  MAX_WIDGET_WIDTH,
  SIZE_VARIANT_ORDER,
  WIDGET_VARIANT_GRID_SIZE,
} from './constants';
import {
  type DashboardCard,
  type DashboardCardLayoutRules,
  type DashboardCardPayload,
  type FolderCardPayload,
  type WidgetSizeHints,
  type WidgetSizeIconVariant,
} from './types';

type LayoutRect = { i: string; x: number; y: number; w: number; h: number };

// A folder payload is the `folder` kind carrying an `items` array. The opaque
// content branch of the union also permits `kind: 'folder'` structurally, so the
// `items` check is what actually distinguishes a well-formed folder.
function isFolderPayload(payload: DashboardCardPayload): payload is FolderCardPayload {
  return payload.kind === 'folder' && Array.isArray(payload.items);
}

function asFolder(card: DashboardCard): FolderCardPayload | null {
  return isFolderPayload(card.payload) ? card.payload : null;
}

// Folder is the one structural card kind the dashboard domain owns; every other
// card holds an opaque content payload the domain never interprets.
function isFolderCard(card: DashboardCard): card is DashboardCard & { payload: FolderCardPayload } {
  return isFolderPayload(card.payload);
}

// Content-agnostic presence check: does any page hold a card with this grid id?
function hasCardOnPages(pages: DashboardCard[][], cardId: string): boolean {
  return pages.some(page => page.some(item => item.i === cardId));
}

// Single page-scan kernel: the first card matching `predicate` with its page index.
function findCardOnPages(
  pages: DashboardCard[][],
  predicate: (card: DashboardCard) => boolean,
): { card: DashboardCard; pageIndex: number } | null {
  for (const [pageIndex, page] of pages.entries()) {
    const card = page.find(predicate);
    if (card) return { card, pageIndex };
  }
  return null;
}

// Current placement of a card by its grid id (`card.i`), regardless of kind.
function findCardPlacementById(pages: DashboardCard[][], cardId: string): { w: number; h: number; pageIndex: number } | null {
  const found = findCardOnPages(pages, card => card.i === cardId);
  return found ? { w: found.card.w, h: found.card.h, pageIndex: found.pageIndex } : null;
}

// Locates a top-level (non-folder) card by id across pages, returning its size
// and page. The dashboard treats such a card's content as opaque, so any
// non-folder card sharing the id is the product's standalone placement.
function findTopLevelCardPlacement(pages: DashboardCard[][], cardId: string): { w: number; h: number; pageIndex: number } | null {
  const found = findCardOnPages(pages, card => card.i === cardId && !isFolderCard(card));
  return found ? { w: found.card.w, h: found.card.h, pageIndex: found.pageIndex } : null;
}

// Locates a product inside a user (non-favorites) folder, returning the
// folder card's size and page. Favorites membership is intentionally ignored:
// a favorite can still be added as a standalone widget. Internal helper for
// `findProductDashboardPlacement` — not part of the public service surface.
function findProductFolderPlacement(
  pages: DashboardCard[][],
  productId: string,
): { w: number; h: number; pageIndex: number } | null {
  for (const [pageIndex, page] of pages.entries()) {
    for (const card of page) {
      if (card.i === FAVORITES_FOLDER_ID) continue;
      const folder = asFolder(card);
      if (folder?.items.includes(productId)) return { w: card.w, h: card.h, pageIndex };
    }
  }
  return null;
}

// Where a product currently lives on the dashboard for the add-widget modal:
// an existing top-level widget, or the user folder that already contains it.
// Used to gate the "Add" action so a product inside a folder can't also be
// added as a duplicate top-level widget.
function findProductDashboardPlacement(
  pages: DashboardCard[][],
  productId: string,
): { w: number; h: number; pageIndex: number } | null {
  return findTopLevelCardPlacement(pages, productId) ?? findProductFolderPlacement(pages, productId);
}

// Removes a legacy top-level duplicate of `cardId` while preserving a folder of
// the same id. A folder entry (e.g. a favourites icon) is an independent
// placement that must survive an add of a same-id card; any other top-level
// card sharing the id is swept so the new placement replaces it.
function stripLegacyTopLevelCardFromPages(pages: DashboardCard[][], cardId: string): DashboardCard[][] {
  let changed = false;
  const next = pages.map(page => {
    const filtered = page.filter(item => item.i !== cardId || isFolderCard(item));
    if (filtered.length !== page.length) {
      changed = true;
      return filtered;
    }
    return page;
  });
  return changed ? next : pages;
}

function snapLayoutY(y: number, h: number, maxRows: number, step: number = DASHBOARD_GRID_SNAP_Y_STEP): number {
  const maxY = maxRows - h;
  const snapped = Math.round(y / step) * step;
  return Math.max(0, Math.min(snapped, maxY));
}

function snapHeightToAllowed(height: number, allowedHeights: number[] = ALLOWED_WIDGET_HEIGHTS): number {
  return allowedHeights.reduce((prev, curr) => (Math.abs(curr - height) < Math.abs(prev - height) ? curr : prev));
}

function gridRectsOverlap(ax: number, ay: number, aw: number, ah: number, b: LayoutRect): boolean {
  return ax + aw > b.x && b.x + b.w > ax && ay + ah > b.y && b.y + b.h > ay;
}

// Row-major: scans left-to-right then top-to-bottom.
function findFirstFit(
  items: readonly LayoutRect[],
  w: number,
  h: number,
  gridCols = GRID_COLS,
  maxRows = MAX_GRID_ROWS,
): { x: number; y: number } | null {
  if (w < 1 || h < 1 || w > gridCols || h > maxRows) return null;

  for (let y = 0; y <= maxRows - h; y += DASHBOARD_GRID_SNAP_Y_STEP) {
    for (let x = 0; x <= gridCols - w; x++) {
      if (!items.some(item => gridRectsOverlap(x, y, w, h, item))) return { x, y };
    }
  }
  return null;
}

// Column-major: fills a column top-to-bottom before moving right.
function findColumnFit(
  items: readonly DashboardCard[],
  w: number,
  h: number,
  cols: number = GRID_COLS,
  maxRows: number = MAX_GRID_ROWS,
): { x: number; y: number } | null {
  if (w < 1 || h < 1 || w > cols || h > maxRows) return null;

  for (let x = 0; x <= cols - w; x++) {
    for (let y = 0; y <= maxRows - h; y += DASHBOARD_GRID_SNAP_Y_STEP) {
      if (!items.some(item => gridRectsOverlap(x, y, w, h, item))) return { x, y };
    }
  }
  return null;
}

// Places a card arriving from an adjacent page. The card enters from the page edge
// nearest the page it came from — moving to the next page (direction 1) it enters at
// the first column, moving to the previous page (direction -1) at the last column —
// and keeps the row (height) it was dragged at. If that entry cell is taken, it takes
// the first free slot scanning toward the far edge (left→right for direction 1,
// right→left for -1), column-major. Returns null when the page is full.
function findDropFit(
  items: readonly DashboardCard[],
  w: number,
  h: number,
  direction: 1 | -1,
  desiredRow: number,
  cols: number = GRID_COLS,
  maxRows: number = MAX_GRID_ROWS,
): { x: number; y: number } | null {
  if (w < 1 || h < 1 || w > cols || h > maxRows) return null;

  const lastX = cols - w;
  const entryCol = direction === 1 ? 0 : lastX;
  const y = snapLayoutY(desiredRow, h, maxRows);

  if (!items.some(item => gridRectsOverlap(entryCol, y, w, h, item))) return { x: entryCol, y };

  for (let i = 0; i <= lastX; i++) {
    const x = direction === 1 ? i : lastX - i;
    for (let row = 0; row <= maxRows - h; row += DASHBOARD_GRID_SNAP_Y_STEP) {
      if (!items.some(item => gridRectsOverlap(x, row, w, h, item))) return { x, y: row };
    }
  }

  return null;
}

// A saved dashboard exists when it has at least one page. Single source of truth
// for "is there a dashboard?", shared by the read hook (load vs empty) and the
// first-run seed (seed vs skip) so the two decisions can never drift apart.
function hasPages(pages: DashboardCard[][] | null | undefined): boolean {
  return (pages?.length ?? 0) > 0;
}

function ensurePages(pages: DashboardCard[][] | null): DashboardCard[][] {
  return pages && hasPages(pages) ? pages : [[]];
}

function findFavoritesFolder(pages: DashboardCard[][]): { pageIndex: number; item: DashboardCard } | null {
  for (let pageIndex = 0; pageIndex < pages.length; pageIndex++) {
    const page = pages[pageIndex] ?? [];
    const item = page.find(entry => isFolderCard(entry) && entry.i === FAVORITES_FOLDER_ID);
    if (item) return { pageIndex, item };
  }
  return null;
}

// Base names of the products currently living in the Favorites folder. The
// folder's `items` are the single source of truth for "is this a favorite",
// so reads across the app derive membership from here rather than tracking it
// separately.
function favoriteProductIds(pages: DashboardCard[][]): Set<string> {
  const found = findFavoritesFolder(pages);
  if (!found) return new Set();
  const folder = asFolder(found.item);
  if (!folder) return new Set();
  return new Set(folder.items);
}

// Every product the user placed on the dashboard, folder contents included. A
// product may be both placed and favorited, so the result is deduplicated while
// preserving first-seen order. Only `product:widget` cards name a product — a
// native card's `i` is a surface id, and emitting it would put a non-product
// identifier on the wire. Folder `items` is a bare string list that mixes both,
// so callers still have to intersect the result with the products they know.
function placedProductIds(pages: DashboardCard[][]): string[] {
  const ids = new Set<string>();
  for (const page of pages) {
    for (const item of page) {
      if (isFolderCard(item)) {
        for (const productId of item.payload.items) ids.add(productId);
        continue;
      }
      if (item.payload.kind !== 'product:widget') continue;
      ids.add(item.i);
    }
  }

  return [...ids];
}

function placeOnPages(
  pages: DashboardCard[][],
  card: DashboardCard,
  preferredPageIndex: number,
): { pages: DashboardCard[][]; pageIndex: number } {
  const searchOrder: number[] = [];
  const preferred = Math.max(0, Math.min(preferredPageIndex, pages.length - 1));
  if (pages.length > 0) {
    searchOrder.push(preferred);
    for (let i = 0; i < pages.length; i++) {
      if (i !== preferred) searchOrder.push(i);
    }
  }

  for (const index of searchOrder) {
    const position = findFirstFit(pages[index] ?? [], card.w, card.h);
    if (position) {
      const placed: DashboardCard = { ...card, x: position.x, y: position.y };
      const nextPages = pages.map((page, i) => (i === index ? [...page, placed] : page));
      return { pages: nextPages, pageIndex: index };
    }
  }

  const placed: DashboardCard = { ...card, x: 0, y: 0 };
  return { pages: [...pages, [placed]], pageIndex: pages.length };
}

function reflowFromPage(pages: DashboardCard[][], pageIndex: number, fixedItem: DashboardCard): DashboardCard[][] {
  const result: DashboardCard[][] = pages.slice(0, pageIndex);

  const sourcePage = pages[pageIndex] ?? [];

  const overlapsFixed = (item: DashboardCard) =>
    item.x < fixedItem.x + fixedItem.w &&
    fixedItem.x < item.x + item.w &&
    item.y < fixedItem.y + fixedItem.h &&
    fixedItem.y < item.y + item.h;

  // Minimal displacement: every card that does NOT overlap the resized card's new
  // footprint keeps its exact position. Only the cards the grown widget now covers
  // are relocated, so resizing one widget never reshuffles untouched neighbours.
  const targetPage: DashboardCard[] = [fixedItem];
  const displaced: DashboardCard[] = [];
  for (const item of sourcePage) {
    if (item.i === fixedItem.i) continue;
    if (overlapsFixed(item)) displaced.push(item);
    else targetPage.push(item);
  }

  displaced.sort((a, b) => a.y - b.y || a.x - b.x);

  let carry: DashboardCard[] = [];

  for (const item of displaced) {
    const position = findColumnFit(targetPage, item.w, item.h, GRID_COLS, MAX_GRID_ROWS);
    if (position) {
      targetPage.push({ ...item, x: position.x, y: position.y });
    } else {
      carry.push(item);
    }
  }

  result.push(targetPage);

  for (let i = pageIndex + 1; i < pages.length; i++) {
    const originalItems = (pages[i] ?? []).slice().sort((a, b) => a.x - b.x || a.y - b.y);
    const combined = [...carry, ...originalItems];

    const pageBuffer: DashboardCard[] = [];
    const nextCarry: DashboardCard[] = [];

    for (const item of combined) {
      const position = findColumnFit(pageBuffer, item.w, item.h, GRID_COLS, MAX_GRID_ROWS);
      if (position) {
        pageBuffer.push({ ...item, x: position.x, y: position.y });
      } else {
        nextCarry.push(item);
      }
    }

    result.push(pageBuffer);
    carry = nextCarry;
  }

  while (carry.length > 0) {
    const pageBuffer: DashboardCard[] = [];
    const nextCarry: DashboardCard[] = [];

    for (const item of carry) {
      const position = findColumnFit(pageBuffer, item.w, item.h, GRID_COLS, MAX_GRID_ROWS);
      if (position) {
        pageBuffer.push({ ...item, x: position.x, y: position.y });
      } else {
        nextCarry.push(item);
      }
    }

    if (pageBuffer.length === 0) break;

    result.push(pageBuffer);
    carry = nextCarry;
  }

  return result;
}

function compactAcrossPages(
  items: DashboardCard[],
  cols: number = GRID_COLS,
  maxRows: number = MAX_GRID_ROWS,
): DashboardCard[][] {
  if (items.length === 0) return [[]];

  const pages: DashboardCard[][] = [];
  let currentPage: DashboardCard[] = [];
  let columnHeights: number[] = new Array(cols).fill(0);

  const commitPage = () => {
    pages.push(currentPage);
    currentPage = [];
    columnHeights = new Array(cols).fill(0);
  };

  for (const item of items) {
    const width = Math.max(1, Math.min(item.w, cols));
    const height = Math.max(1, Math.min(item.h, maxRows));

    let bestX = -1;
    let bestBaseY = -1;

    for (let x = 0; x <= cols - width; x++) {
      let baseY = 0;
      for (let col = x; col < x + width; col++) {
        const colHeight = columnHeights[col] ?? 0;
        if (colHeight > baseY) baseY = colHeight;
      }
      if (baseY + height > maxRows) continue;

      if (baseY > bestBaseY || (baseY === bestBaseY && bestX === -1)) {
        bestBaseY = baseY;
        bestX = x;
      }
    }

    if (bestX === -1) {
      commitPage();
      bestX = 0;
      bestBaseY = 0;
    }

    currentPage.push({ ...item, x: bestX, y: bestBaseY, w: width, h: height });
    for (let col = bestX; col < bestX + width; col++) {
      columnHeights[col] = bestBaseY + height;
    }
  }

  if (currentPage.length > 0) pages.push(currentPage);
  if (pages.length === 0) pages.push([]);

  return pages;
}

// Defends against stale data from older app versions or invalid persisted state.
function clampCard(card: DashboardCard): DashboardCard {
  const minW = card.minW ?? 1;
  const maxW = Math.min(card.maxW ?? MAX_WIDGET_WIDTH, MAX_WIDGET_WIDTH);
  const w = Math.max(minW, Math.min(card.w, maxW));

  const minH = card.minH ?? 1;
  const maxH = Math.min(card.maxH ?? MAX_WIDGET_HEIGHT, MAX_WIDGET_HEIGHT, MAX_GRID_ROWS);
  const h = Math.max(minH, Math.min(card.h, maxH));

  const x = Math.max(0, Math.min(card.x, GRID_COLS - w));
  const y = snapLayoutY(card.y, h, MAX_GRID_ROWS);

  if (card.x === x && card.y === y && card.w === w && card.h === h) return card;
  return { ...card, x, y, w, h };
}

// RGL strips payload + min/max metadata; merge them back from `existing`.
function preserveCardMetadata(
  layoutItem: {
    i: string;
    x: number;
    y: number;
    w: number;
    h: number;
    minW?: number;
    maxW?: number;
    minH?: number;
    maxH?: number;
  },
  existing?: DashboardCard,
): DashboardCard {
  const minW = layoutItem.minW ?? existing?.minW ?? 1;
  const maxWBase = layoutItem.maxW ?? existing?.maxW ?? MAX_WIDGET_WIDTH;
  const maxW = Math.max(maxWBase, layoutItem.w, minW);
  const minH = layoutItem.minH ?? existing?.minH ?? 4;
  const maxH = layoutItem.maxH ?? existing?.maxH ?? MAX_WIDGET_HEIGHT;

  return {
    i: layoutItem.i,
    x: layoutItem.x,
    y: layoutItem.y,
    w: layoutItem.w,
    h: layoutItem.h,
    resizeHandles: existing?.resizeHandles || [...DEFAULT_RESIZE_HANDLES],
    minW,
    maxW,
    minH,
    maxH,
    // `existing` is always set in practice; the synthesized payload only keeps the type total.
    payload: existing?.payload ?? { kind: 'product:widget', productId: layoutItem.i },
  };
}

function normalizeFavoritesFolder(pagesInput: DashboardCard[][]): DashboardCard[][] {
  let firstFolderFound = false;
  let changed = false;
  const mergedIconIds: string[] = [];
  const seen = new Set<string>();

  for (const page of pagesInput) {
    for (const item of page) {
      const folder = asFolder(item);
      if (!folder) continue;
      for (const iconId of folder.items) {
        if (seen.has(iconId)) continue;
        seen.add(iconId);
        mergedIconIds.push(iconId);
      }
    }
  }

  const nextPages = pagesInput.map(page =>
    page.flatMap(item => {
      const folder = asFolder(item);
      if (!folder) return [item];

      if (!firstFolderFound) {
        firstFolderFound = true;
        const needsRename = item.i !== FAVORITES_FOLDER_ID;
        const needsMerge = folder.items.length !== mergedIconIds.length;
        const nextMinH = FOLDER_MIN_HEIGHT;
        const nextH = Math.max(item.h, nextMinH);
        const needsHeightFix = item.h !== nextH || (item.minH ?? 1) !== nextMinH || item.maxW !== 1;
        if (needsRename || needsMerge || needsHeightFix) changed = true;
        const merged: DashboardCard = {
          ...item,
          i: FAVORITES_FOLDER_ID,
          h: nextH,
          minH: nextMinH,
          maxW: 1,
          payload: { ...folder, items: mergedIconIds },
        };
        return [merged];
      }

      changed = true;
      return [];
    }),
  );

  return changed ? nextPages : pagesInput;
}

function applyCardResize(
  pages: DashboardCard[][],
  cardId: string,
  size: { w: number; h: number },
  options: { restrictToPageIndex?: number } = {},
): { pages: DashboardCard[][]; pageIndex: number; changed: boolean } | null {
  let pageIndex = -1;
  let current: DashboardCard | undefined;

  if (options.restrictToPageIndex !== undefined) {
    const page = pages[options.restrictToPageIndex] ?? [];
    const found = page.find(it => it.i === cardId);
    if (found) {
      pageIndex = options.restrictToPageIndex;
      current = found;
    }
  } else {
    for (let pi = 0; pi < pages.length; pi++) {
      const found = pages[pi]?.find(it => it.i === cardId);
      if (found) {
        pageIndex = pi;
        current = found;
        break;
      }
    }
  }

  if (!current || pageIndex < 0) return null;

  const minW = current.minW ?? 1;
  const maxW = Math.min(current.maxW ?? MAX_WIDGET_WIDTH, MAX_WIDGET_WIDTH);
  const nextW = Math.max(minW, Math.min(size.w, maxW));
  const maxH = Math.min(current.maxH ?? MAX_WIDGET_HEIGHT, MAX_WIDGET_HEIGHT, MAX_GRID_ROWS);
  const nextH = Math.max(1, Math.min(size.h, maxH));
  const nextX = Math.max(0, Math.min(current.x, GRID_COLS - nextW));
  const nextY = snapLayoutY(current.y, nextH, MAX_GRID_ROWS);

  if (current.w === nextW && current.h === nextH && current.x === nextX && current.y === nextY) {
    return { pages, pageIndex, changed: false };
  }

  const fixedItem: DashboardCard = {
    ...current,
    x: nextX,
    y: nextY,
    w: nextW,
    h: nextH,
    minH: nextH <= 1 ? 1 : Math.min(current.minH ?? nextH, nextH),
    maxW: Math.max(current.maxW ?? MAX_WIDGET_WIDTH, nextW),
  };

  const isShrinkOrSame = nextW <= current.w && nextH <= current.h;

  const nextPages = isShrinkOrSame
    ? pages.map((page, i) => (i === pageIndex ? page.map(it => (it.i === cardId ? fixedItem : it)) : page))
    : reflowFromPage(pages, pageIndex, fixedItem);

  return { pages: nextPages, pageIndex, changed: true };
}

// Removes a card by id from whichever page holds it, collapsing an emptied
// non-first page (page 0 is always kept) and shifting the active index to match.
// Pure counterpart to `applyCardResize` — the use case fetches, calls this, saves.
function removeCardFromPages(
  pages: DashboardCard[][],
  activePageIndex: number,
  cardId: string,
): { pages: DashboardCard[][]; activePageIndex: number; changed: boolean } {
  const pageIndex = pages.findIndex(page => page.some(item => item.i === cardId));
  if (pageIndex < 0) return { pages, activePageIndex, changed: false };

  const remaining = (pages[pageIndex] ?? []).filter(item => item.i !== cardId);
  const collapsePage = remaining.length === 0 && pageIndex !== 0 && pages.length > 1;

  const nextPages = collapsePage
    ? pages.filter((_, index) => index !== pageIndex)
    : pages.map((page, index) => (index === pageIndex ? remaining : page));

  // `collapsePage` guarantees `pageIndex >= 1`, so `activePageIndex - 1 >= 0`.
  const nextActiveIndex = collapsePage && activePageIndex >= pageIndex ? activePageIndex - 1 : activePageIndex;

  return { pages: nextPages, activePageIndex: nextActiveIndex, changed: true };
}

// Maps a widget's declared size hints to the supported size variants, primarily
// driven by `height`. `width` is optional and only gates horizontal. Returns an
// empty array when the hints don't map to ANY valid variant (e.g. pixel values
// like `{ height: [400], width: 360 }`) — callers treat that as invalid and
// reject the widget.
//
//   { height: [1, 2, 4] }                  → ['small', 'medium', 'large'] (width ignored)
//   { height: [0], width: 2 }              → ['horizontal']
//   { height: [0, 1, 2, 4], width: 2 }     → all four sizes
//
// Horizontal requires BOTH a `0` in `height` and `width === 2`; width alone (or
// a `0` without width 2) does not add it.
function sizeHintsToVariants(hints: WidgetSizeHints): WidgetSizeIconVariant[] {
  const variants = new Set<WidgetSizeIconVariant>();

  for (const height of hints.height) {
    const variant = HEIGHT_HINT_TO_VARIANT[height];
    if (variant) variants.add(variant);
  }

  if (hints.height.includes(HORIZONTAL_HEIGHT_MARKER) && hints.width === MAX_WIDGET_WIDTH) {
    variants.add('horizontal');
  }

  return SIZE_VARIANT_ORDER.filter(variant => variants.has(variant));
}

// Returns null when the hints are invalid (no supported variant) — the caller
// falls back to its own default rules.
function sizeHintsToLayoutRules(hints: WidgetSizeHints): DashboardCardLayoutRules | null {
  const switchableSizes = sizeHintsToVariants(hints);
  if (switchableSizes.length === 0) return null;

  // Derive the resize bounds from the footprints of the offered variants, so the
  // min/max envelope can never exclude a size the menu lets the user pick (e.g.
  // offering `horizontal` (h=4) while `maxH` stayed at 2).
  const sizes = switchableSizes.map(variant => WIDGET_VARIANT_GRID_SIZE[variant]);
  const widths = sizes.map(size => size.w);
  const heights = sizes.map(size => size.h);

  return {
    minH: Math.min(...heights),
    maxH: Math.max(...heights),
    minW: Math.min(...widths),
    maxW: Math.max(...widths),
    switchableSizes,
  };
}

function getVariantFromGridSize(w: number, h: number): WidgetSizeIconVariant {
  if (w === 2 && h === 4) return 'horizontal';
  if (h === 8) return 'large';
  if (h === 4) return 'medium';
  if (h === 2) return 'small';
  return 'small';
}

function getMaxVisibleFavorites(w: number, h: number): number {
  return MAX_VISIBLE_FAVORITES_BY_VARIANT[getVariantFromGridSize(w, h)];
}

// How many favourite tiles to render for a folder of `cap` capacity holding
// `itemCount` items, and whether a trailing "View more" tile is needed. When the
// items overflow the capacity, the last slot is given to "View more" (opening the
// Favorites SPA), so only `cap - 1` items are shown.
function getFavoritesDisplay(itemCount: number, cap: number): { visibleCount: number; hasViewMore: boolean } {
  if (itemCount <= cap) return { visibleCount: itemCount, hasViewMore: false };
  return { visibleCount: cap - 1, hasViewMore: true };
}

// Normalize a raw grid-layout change into persisted cards: preserve each card's
// metadata, snap height/y to the grid, enforce folder/widget min/max, and keep
// widget width fixed (only an explicit resize may change it). Returns `null`
// when the result would overflow the grid — the caller drops the change.
function processLayoutChange(
  newLayout: readonly {
    i: string;
    x: number;
    y: number;
    w: number;
    h: number;
    minW?: number;
    maxW?: number;
    minH?: number;
    maxH?: number;
  }[],
  activeLayout: DashboardCard[],
): DashboardCard[] | null {
  const processed = newLayout.map(layoutItem => {
    const existing = activeLayout.find(item => item.i === layoutItem.i);
    const preserved = preserveCardMetadata(layoutItem, existing);

    const isFolder = preserved.payload.kind === 'folder';
    preserved.h = snapHeightToAllowed(layoutItem.h);
    preserved.minH = isFolder ? FOLDER_MIN_HEIGHT : Math.min(preserved.minH ?? 1, preserved.h);
    preserved.maxW = isFolder ? 1 : Math.max(preserved.maxW ?? MAX_WIDGET_WIDTH, preserved.w);
    preserved.y = snapLayoutY(preserved.y, preserved.h, MAX_GRID_ROWS);
    // Drag/drop must never change widget width — only resizeWidget() may do that.
    // correctBounds() in react-grid-layout can set w = cols when x < 0, corrupting layout.
    if (existing !== undefined) {
      preserved.w = existing.w;
    }

    return preserved;
  });

  if (processed.some(item => item.y + item.h > MAX_GRID_ROWS)) {
    return null;
  }

  return processed;
}

// Move a card to the page `direction` away (−1 prev / +1 next), entering at the
// nearest edge at `dropRow` (falling back to the card's own row). Pure page
// shuffle: returns the rebuilt pages + the landing page index, or `null` when
// the card isn't found or there's no previous page.
function moveItemToAdjacentPage(
  pages: DashboardCard[][],
  itemId: string,
  direction: -1 | 1,
  dropRow?: number,
): { nextPages: DashboardCard[][]; targetPageIndex: number } | null {
  const sourcePageIndex = pages.findIndex(page => page.some(item => item.i === itemId));
  if (sourcePageIndex === -1) return null;

  const sourcePage = pages[sourcePageIndex] ?? [];
  const itemToMove = sourcePage.find(item => item.i === itemId);
  if (!itemToMove) return null;

  let targetPageIndex = sourcePageIndex + direction;
  if (targetPageIndex < 0) return null;

  const nextPages = pages.map(page => [...page]);
  while (targetPageIndex >= nextPages.length) {
    nextPages.push([]);
  }

  nextPages[sourcePageIndex] = sourcePage.filter(item => item.i !== itemId);

  if (nextPages[sourcePageIndex].length === 0 && sourcePageIndex !== 0 && nextPages.length > 1) {
    nextPages.splice(sourcePageIndex, 1);
    if (sourcePageIndex < targetPageIndex) {
      targetPageIndex -= 1;
    }
  }

  const targetPage = nextPages[targetPageIndex] ?? [];
  // Enter from the page edge nearest the source page, at the dragged row; fall
  // back to the dragged item's own row when the drop row is unknown.
  const targetPosition = findDropFit(targetPage, itemToMove.w, itemToMove.h, direction, dropRow ?? itemToMove.y);

  if (targetPosition) {
    targetPage.push({ ...itemToMove, x: targetPosition.x, y: targetPosition.y });
    nextPages[targetPageIndex] = targetPage;
  } else {
    const overflowPageIndex = targetPageIndex + 1;
    while (overflowPageIndex >= nextPages.length) {
      nextPages.push([]);
    }
    nextPages[overflowPageIndex] = [{ ...itemToMove, x: 0, y: 0 }];
    targetPageIndex = overflowPageIndex;
  }

  return { nextPages, targetPageIndex };
}

// Re-pack every card across pages in reading order (top-to-bottom, left-to-right)
// and compact into the minimum number of pages. Returns the input unchanged when
// there are no cards.
function autoLayout(pages: DashboardCard[][]): DashboardCard[][] {
  const flatInReadingOrder: DashboardCard[] = [];
  for (const page of pages) {
    const sorted = [...page].sort((a, b) => a.y - b.y || a.x - b.x);
    flatInReadingOrder.push(...sorted);
  }

  if (flatInReadingOrder.length === 0) return pages;

  const compactedPages = compactAcrossPages(flatInReadingOrder);
  const originalsById = new Map(flatInReadingOrder.map(item => [item.i, item]));

  return compactedPages.map(page =>
    page.map(item => {
      const original = originalsById.get(item.i);
      return original ? { ...original, x: item.x, y: item.y, w: item.w, h: item.h } : item;
    }),
  );
}

/**
 * The dashboard a brand-new user starts with — a single product widget.
 *
 * Takes the product id rather than baking one in: the card key and its payload
 * ARE the product's identity, so they must carry the network's TLD, which this
 * domain cannot reach. Persisted by `seedDefaultMainLayout`.
 */
function defaultPages(productId: string): DashboardCard[][] {
  return [
    [
      {
        i: productId,
        x: 1,
        y: 2,
        w: 2,
        h: 4,
        minW: 1,
        maxW: MAX_WIDGET_WIDTH,
        minH: 4,
        maxH: MAX_WIDGET_HEIGHT,
        resizeHandles: [...DEFAULT_RESIZE_HANDLES],
        payload: { kind: 'product:widget', productId },
      },
    ],
  ];
}

export const dashboardLayoutService = {
  defaultPages,
  sizeHintsToVariants,
  sizeHintsToLayoutRules,
  getVariantFromGridSize,
  getMaxVisibleFavorites,
  getFavoritesDisplay,
  asFolder,
  isFolderCard,
  placedProductIds,
  hasCardOnPages,
  findCardPlacementById,
  findProductDashboardPlacement,
  stripLegacyTopLevelCardFromPages,
  snapLayoutY,
  snapHeightToAllowed,
  findFirstFit,
  findColumnFit,
  findDropFit,
  hasPages,
  ensurePages,
  findFavoritesFolder,
  favoriteProductIds,
  placeOnPages,
  reflowFromPage,
  compactAcrossPages,
  clampCard,
  preserveCardMetadata,
  normalizeFavoritesFolder,
  applyCardResize,
  removeCardFromPages,
  processLayoutChange,
  moveItemToAdjacentPage,
  autoLayout,
};
