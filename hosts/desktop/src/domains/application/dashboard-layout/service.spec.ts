import { describe, expect, it } from 'vitest';

import { FAVORITES_FOLDER_ID } from './constants';
import { dashboardLayoutService } from './service';
import { type DashboardCard } from './types';

const widget = (productId: string): DashboardCard => ({
  i: productId,
  x: 0,
  y: 0,
  w: 1,
  h: 4,
  payload: { kind: 'product:widget', productId },
});

const favoritesFolder = (items: string[]): DashboardCard => ({
  i: FAVORITES_FOLDER_ID,
  x: 0,
  y: 0,
  w: 1,
  h: 4,
  payload: { kind: 'folder', items },
});

const nativeCard = (id: string): DashboardCard => ({
  i: id,
  x: 0,
  y: 0,
  w: 1,
  h: 4,
  payload: { kind: `native:${id}` },
});

const userFolder = (folderId: string, items: string[]): DashboardCard => ({
  i: folderId,
  x: 0,
  y: 0,
  w: 1,
  h: 4,
  payload: { kind: 'folder', items },
});

describe('dashboardLayoutService.stripLegacyTopLevelCardFromPages', () => {
  it('removes a same-id content card but keeps the favorites folder', () => {
    const pages: DashboardCard[][] = [[favoritesFolder(['my-app']), widget('my-app'), widget('other')]];

    const next = dashboardLayoutService.stripLegacyTopLevelCardFromPages(pages, 'my-app');

    expect(next[0]).toHaveLength(2);
    const favoritesCard = next[0]?.[0];
    expect(favoritesCard).toBeDefined();
    expect(dashboardLayoutService.asFolder(favoritesCard!)?.items).toEqual(['my-app']);
    const topLevel = next[0]!.filter(item => !dashboardLayoutService.isFolderCard(item));
    expect(topLevel.map(item => item.i)).toEqual(['other']);
  });
});

describe('dashboardLayoutService.removeCardFromPages', () => {
  it('removes only the top-level card when it coexists with a favourites entry', () => {
    const pages: DashboardCard[][] = [[favoritesFolder(['chat']), nativeCard('chat')]];

    const { pages: next, changed } = dashboardLayoutService.removeCardFromPages(pages, 0, 'chat');

    expect(changed).toBe(true);
    expect(next[0]!.map(item => item.i)).not.toContain('chat');
    const folder = next[0]?.find(item => item.i === FAVORITES_FOLDER_ID);
    expect(dashboardLayoutService.asFolder(folder!)?.items).toEqual(['chat']);
  });
});

describe('dashboardLayoutService.hasCardOnPages', () => {
  it('detects a card by id regardless of kind', () => {
    const pages: DashboardCard[][] = [[favoritesFolder(['coinflip'])], [nativeCard('chat')]];

    expect(dashboardLayoutService.hasCardOnPages(pages, 'chat')).toBe(true);
    expect(dashboardLayoutService.hasCardOnPages(pages, FAVORITES_FOLDER_ID)).toBe(true);
    expect(dashboardLayoutService.hasCardOnPages(pages, 'missing')).toBe(false);
  });
});

describe('dashboardLayoutService.findProductDashboardPlacement', () => {
  it('returns the top-level widget placement when present', () => {
    const pages: DashboardCard[][] = [[], [widget('my-app')]];

    expect(dashboardLayoutService.findProductDashboardPlacement(pages, 'my-app')).toEqual({ w: 1, h: 4, pageIndex: 1 });
  });

  it('detects a product living inside a user (non-favorites) folder', () => {
    const pages: DashboardCard[][] = [[userFolder('folder-1', ['my-app'])]];

    expect(dashboardLayoutService.findProductDashboardPlacement(pages, 'my-app')).toEqual({ w: 1, h: 4, pageIndex: 0 });
  });

  it('ignores favorites-folder membership so a favorite can still be added as a widget', () => {
    const pages: DashboardCard[][] = [[favoritesFolder(['my-app'])]];

    expect(dashboardLayoutService.findProductDashboardPlacement(pages, 'my-app')).toBeNull();
  });

  it('returns null when the product is neither a widget nor inside a folder', () => {
    const pages: DashboardCard[][] = [[widget('other')]];

    expect(dashboardLayoutService.findProductDashboardPlacement(pages, 'my-app')).toBeNull();
  });
});

describe('sizeHintsToVariants', () => {
  it('maps heights to vertical variants, ignoring width', () => {
    expect(dashboardLayoutService.sizeHintsToVariants({ height: [1, 2, 4] })).toEqual(['small', 'medium', 'large']);
    expect(dashboardLayoutService.sizeHintsToVariants({ height: [1, 2, 4], width: 2 })).toEqual(['small', 'medium', 'large']);
  });

  it('returns only declared sizes', () => {
    expect(dashboardLayoutService.sizeHintsToVariants({ height: [2] })).toEqual(['medium']);
  });

  it('offers all four sizes only with height [0,1,2,4] and width 2', () => {
    expect(dashboardLayoutService.sizeHintsToVariants({ height: [0, 1, 2, 4], width: 2 })).toEqual([
      'small',
      'medium',
      'large',
      'horizontal',
    ]);
  });

  it('treats height 0 + width 2 as horizontal-only', () => {
    expect(dashboardLayoutService.sizeHintsToVariants({ height: [0], width: 2 })).toEqual(['horizontal']);
  });

  it('does not add horizontal without both a 0 height and width 2', () => {
    // 0 present but width missing/not 2
    expect(dashboardLayoutService.sizeHintsToVariants({ height: [0, 1] })).toEqual(['small']);
    expect(dashboardLayoutService.sizeHintsToVariants({ height: [0, 1], width: 1 })).toEqual(['small']);
    // width 2 but no 0 in height
    expect(dashboardLayoutService.sizeHintsToVariants({ height: [1, 2], width: 2 })).toEqual(['small', 'medium']);
  });

  it('returns empty for invalid (pixel) hints', () => {
    expect(dashboardLayoutService.sizeHintsToVariants({ height: [400], width: 360 })).toEqual([]);
  });

  it('returns empty when no height maps and horizontal does not apply', () => {
    expect(dashboardLayoutService.sizeHintsToVariants({ height: [0] })).toEqual([]);
    expect(dashboardLayoutService.sizeHintsToVariants({ height: [3] })).toEqual([]);
  });
});

describe('sizeHintsToLayoutRules', () => {
  it('derives resize bounds from all four sizes', () => {
    expect(dashboardLayoutService.sizeHintsToLayoutRules({ height: [0, 1, 2, 4], width: 2 })).toEqual({
      minH: 2,
      maxH: 8,
      minW: 1,
      maxW: 2,
      switchableSizes: ['small', 'medium', 'large', 'horizontal'],
    });
  });

  it('keeps width 1 when horizontal is not offered', () => {
    expect(dashboardLayoutService.sizeHintsToLayoutRules({ height: [1, 2, 4] })).toEqual({
      minH: 2,
      maxH: 8,
      minW: 1,
      maxW: 1,
      switchableSizes: ['small', 'medium', 'large'],
    });
  });

  it('locks a horizontal-only widget to a 2x4 footprint', () => {
    expect(dashboardLayoutService.sizeHintsToLayoutRules({ height: [0], width: 2 })).toEqual({
      minH: 4,
      maxH: 4,
      minW: 2,
      maxW: 2,
      switchableSizes: ['horizontal'],
    });
  });

  it('returns null for invalid hints', () => {
    expect(dashboardLayoutService.sizeHintsToLayoutRules({ height: [400], width: 360 })).toBeNull();
    expect(dashboardLayoutService.sizeHintsToLayoutRules({ height: [3] })).toBeNull();
  });
});

describe('dashboardLayoutService.applyCardResize', () => {
  const card = (i: string, x: number, y: number, h: number, w = 1): DashboardCard => ({
    i,
    x,
    y,
    w,
    h,
    payload: { kind: 'product:widget', productId: i },
  });

  it('leaves non-overlapping neighbours in place when a widget grows', () => {
    // A alone in col 0; B/C/D each alone in their own column. Growing A taller
    // does not intrude into any other column, so nobody should move.
    const pages: DashboardCard[][] = [[card('A', 0, 0, 4), card('B', 1, 0, 4), card('C', 2, 0, 4), card('D', 3, 0, 4)]];

    const result = dashboardLayoutService.applyCardResize(pages, 'A', { w: 1, h: 8 });

    expect(result).not.toBeNull();
    expect(result!.changed).toBe(true);
    const page = result!.pages[0]!;
    const byId = (id: string) => page.find(c => c.i === id)!;
    expect(byId('A').h).toBe(8);
    expect({ x: byId('B').x, y: byId('B').y }).toEqual({ x: 1, y: 0 });
    expect({ x: byId('C').x, y: byId('C').y }).toEqual({ x: 2, y: 0 });
    expect({ x: byId('D').x, y: byId('D').y }).toEqual({ x: 3, y: 0 });
  });

  it('relocates only the neighbour overlapped by the grown footprint', () => {
    // A (col 0, rows 0-4) grows to rows 0-8 and now overlaps B (col 0, rows 4-8).
    // B must move; C in another column stays put.
    const pages: DashboardCard[][] = [[card('A', 0, 0, 4), card('B', 0, 4, 4), card('C', 2, 0, 4)]];

    const result = dashboardLayoutService.applyCardResize(pages, 'A', { w: 1, h: 8 });

    expect(result).not.toBeNull();
    const page = result!.pages[0]!;
    const byId = (id: string) => page.find(c => c.i === id)!;
    expect(byId('A').h).toBe(8);
    expect({ x: byId('C').x, y: byId('C').y }).toEqual({ x: 2, y: 0 });
    // B relocated out of A's column, no longer overlapping A.
    const a = byId('A');
    const b = byId('B');
    const overlaps = a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;
    expect(overlaps).toBe(false);
  });
});

describe('dashboardLayoutService.findDropFit', () => {
  const card = (i: string, x: number, y: number, h: number, w = 1): DashboardCard => ({
    i,
    x,
    y,
    w,
    h,
    payload: { kind: 'product:widget', productId: i },
  });

  it('enters at the first column on the dragged row when moving to the next page', () => {
    // Empty page: a widget dragged to the next page (direction 1) enters at column 0,
    // keeping the row it was dragged at (bottom half here).
    expect(dashboardLayoutService.findDropFit([], 1, 4, 1, 4)).toEqual({ x: 0, y: 4 });
  });

  it('enters at the last column when moving to the previous page (mirror)', () => {
    // direction -1 enters from the right edge.
    expect(dashboardLayoutService.findDropFit([], 1, 4, -1, 0)).toEqual({ x: 3, y: 0 });
    // A width-2 widget enters at the last column it fits (cols-w).
    expect(dashboardLayoutService.findDropFit([], 2, 4, -1, 0)).toEqual({ x: 2, y: 0 });
  });

  it('snaps and clamps the dragged row to a valid slot', () => {
    expect(dashboardLayoutService.findDropFit([], 1, 4, 1, 7)).toEqual({ x: 0, y: 4 });
  });

  it('falls to the first free slot left→right when the entry cell is taken (direction 1)', () => {
    // Entry cell (col 0, row 0) is occupied; the next column-major fit is col 0 row 4.
    const items = [card('A', 0, 0, 4)];
    expect(dashboardLayoutService.findDropFit(items, 1, 4, 1, 0)).toEqual({ x: 0, y: 4 });
  });

  it('falls to the first free slot right→left when the entry cell is taken (direction -1)', () => {
    // Entry cell (col 3, row 0) is occupied; scanning right→left the next fit is col 3 row 4.
    const items = [card('A', 3, 0, 4)];
    expect(dashboardLayoutService.findDropFit(items, 1, 4, -1, 0)).toEqual({ x: 3, y: 4 });
  });

  it('spills to the adjacent column in the reading direction when the entry column is full', () => {
    // direction 1: col 0 full → col 1; direction -1: col 3 full → col 2.
    expect(dashboardLayoutService.findDropFit([card('A', 0, 0, 8)], 1, 4, 1, 0)).toEqual({ x: 1, y: 0 });
    expect(dashboardLayoutService.findDropFit([card('A', 3, 0, 8)], 1, 4, -1, 0)).toEqual({ x: 2, y: 0 });
  });

  it('returns null when the page cannot fit the card anywhere', () => {
    const items = [card('A', 0, 0, 8), card('B', 1, 0, 8), card('C', 2, 0, 8), card('D', 3, 0, 8)];
    expect(dashboardLayoutService.findDropFit(items, 1, 4, 1, 0)).toBeNull();
  });
});

describe('getVariantFromGridSize', () => {
  it('maps grid sizes to variants', () => {
    expect(dashboardLayoutService.getVariantFromGridSize(1, 2)).toBe('small');
    expect(dashboardLayoutService.getVariantFromGridSize(1, 4)).toBe('medium');
    expect(dashboardLayoutService.getVariantFromGridSize(1, 8)).toBe('large');
    expect(dashboardLayoutService.getVariantFromGridSize(2, 4)).toBe('horizontal');
  });

  it('falls back to small for unknown sizes', () => {
    expect(dashboardLayoutService.getVariantFromGridSize(3, 3)).toBe('small');
  });
});

describe('getMaxVisibleFavorites', () => {
  it('caps small at 3 (1 row × 3 cols)', () => {
    expect(dashboardLayoutService.getMaxVisibleFavorites(1, 2)).toBe(3);
  });

  it('caps medium at 9 (3 rows × 3 cols)', () => {
    expect(dashboardLayoutService.getMaxVisibleFavorites(1, 4)).toBe(9);
  });

  it('caps large at 15 (5 rows × 3 cols)', () => {
    expect(dashboardLayoutService.getMaxVisibleFavorites(1, 8)).toBe(15);
  });
});

describe('getFavoritesDisplay', () => {
  it('shows all items and no "View more" when within capacity', () => {
    expect(dashboardLayoutService.getFavoritesDisplay(6, 6)).toEqual({ visibleCount: 6, hasViewMore: false });
    expect(dashboardLayoutService.getFavoritesDisplay(3, 6)).toEqual({ visibleCount: 3, hasViewMore: false });
  });

  it('reserves the last slot for "View more" when items overflow capacity', () => {
    // cap 6, 7 items → show 5 + View more.
    expect(dashboardLayoutService.getFavoritesDisplay(7, 6)).toEqual({ visibleCount: 5, hasViewMore: true });
    // cap 12, 13 items → show 11 + View more.
    expect(dashboardLayoutService.getFavoritesDisplay(13, 12)).toEqual({ visibleCount: 11, hasViewMore: true });
  });
});

describe('dashboardLayoutService.findCardPlacementById', () => {
  it('returns the size and page of a card matching the id, regardless of kind', () => {
    const pages: DashboardCard[][] = [[widget('app.dot')], [nativeCard('chat')]];

    expect(dashboardLayoutService.findCardPlacementById(pages, 'chat')).toEqual({ w: 1, h: 4, pageIndex: 1 });
  });

  it('returns null when no card matches the id', () => {
    const pages: DashboardCard[][] = [[widget('app.dot')]];

    expect(dashboardLayoutService.findCardPlacementById(pages, 'missing')).toBeNull();
  });
});

describe('dashboardLayoutService.moveItemToAdjacentPage', () => {
  it('moves a card to the next page and reports the landing page index', () => {
    const pages: DashboardCard[][] = [[widget('a'), widget('b')], []];

    const result = dashboardLayoutService.moveItemToAdjacentPage(pages, 'a', 1);

    expect(result).not.toBeNull();
    expect(result!.targetPageIndex).toBe(1);
    expect(result!.nextPages[0]!.map(c => c.i)).toEqual(['b']);
    expect(result!.nextPages[1]!.map(c => c.i)).toEqual(['a']);
  });

  it('returns null when the card is not found', () => {
    expect(dashboardLayoutService.moveItemToAdjacentPage([[widget('a')]], 'missing', 1)).toBeNull();
  });

  it('returns null when moving before the first page', () => {
    expect(dashboardLayoutService.moveItemToAdjacentPage([[widget('a')]], 'a', -1)).toBeNull();
  });
});

describe('dashboardLayoutService.autoLayout', () => {
  it('returns the same reference when there are no cards', () => {
    const pages: DashboardCard[][] = [[], []];
    expect(dashboardLayoutService.autoLayout(pages)).toBe(pages);
  });

  it('re-packs cards while preserving their payloads and ids', () => {
    const pages: DashboardCard[][] = [[widget('a')], [widget('b')]];

    const result = dashboardLayoutService.autoLayout(pages);
    const ids = result
      .flat()
      .map(c => c.i)
      .sort();

    expect(ids).toEqual(['a', 'b']);
    expect(result.flat().every(c => c.payload.kind === 'product:widget')).toBe(true);
  });
});

describe('dashboardLayoutService.processLayoutChange', () => {
  it('keeps a widget width fixed on drag (only resize may change width)', () => {
    const active: DashboardCard[] = [{ ...widget('a'), w: 1 }];
    const raw = [{ i: 'a', x: 0, y: 0, w: 2, h: 4 }]; // grid tried to widen it

    const processed = dashboardLayoutService.processLayoutChange(raw, active);

    expect(processed).not.toBeNull();
    expect(processed![0]!.w).toBe(1);
  });

  it('snaps a non-allowed height to an allowed one', () => {
    const raw = [{ i: 'a', x: 0, y: 0, w: 1, h: 3 }]; // 3 is not an allowed widget height

    const processed = dashboardLayoutService.processLayoutChange(raw, [widget('a')]);

    expect(processed).not.toBeNull();
    expect(processed![0]!.h).not.toBe(3);
  });
});

describe('placedProductIds', () => {
  it('collects widgets and folder items across every page', () => {
    const pages = [[widget('wallet'), favoritesFolder(['notes', 'gallery'])], [widget('governance')]];

    expect(dashboardLayoutService.placedProductIds(pages)).toEqual(['wallet', 'notes', 'gallery', 'governance']);
  });

  it('skips a placed card that is not a product', () => {
    // `native:chat` and friends are real placed cards whose `i` is not a product
    // identifier. Emitting one would put a non-product id on the wire.
    expect(dashboardLayoutService.placedProductIds([[nativeCard('chat'), widget('wallet')]])).toEqual(['wallet']);
  });

  it('still emits a favourited native id, which the caller must filter', () => {
    // A folder's `items` is a bare string list mixing product base names with
    // native grid ids, so this layer cannot tell them apart. `useInputContext`
    // intersects with the installed products; this pins that boundary.
    expect(dashboardLayoutService.placedProductIds([[favoritesFolder(['chat', 'wallet'])]])).toEqual(['chat', 'wallet']);
  });

  it('deduplicates a product that is both placed and favorited', () => {
    expect(dashboardLayoutService.placedProductIds([[widget('wallet'), favoritesFolder(['wallet'])]])).toEqual(['wallet']);
  });

  it('returns nothing for an empty dashboard', () => {
    expect(dashboardLayoutService.placedProductIds([])).toEqual([]);
  });
});
