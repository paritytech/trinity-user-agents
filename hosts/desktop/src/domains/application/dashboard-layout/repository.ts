import { ResultAsync } from 'neverthrow';
import { type ResizeHandleAxis } from 'react-grid-layout';
import { type Observable, map } from 'rxjs';

import { type DashboardLayoutItemRow, type DashboardLayoutRow, database, streamTable } from '@/shared/database';
import { toError } from '@/shared/utils';

import { parseDashboardCardPayload } from './schemas';
import { type DashboardCard, type DashboardCardPayload, type DashboardLayout, type MainDashboardLayoutSnapshot } from './types';

// Legacy on-disk shapes — pre-card-refactor. Read-only converters below
// hydrate them into `DashboardCard`s on every read; new writes always use the
// card shape.
// LegacyLayoutItem mirrors DashboardLayoutItemRow exactly so that
// StoredLayout = DashboardLayoutRow is structurally compatible.
type LegacyLayoutItem = DashboardLayoutItemRow;

type StoredLayout = DashboardLayoutRow;

const table = database.dashboardLayouts;

// Legacy 1×1 product-icon rows pre-date the favourites folder. They are no
// longer a recognised top-level card kind: a stored card whose payload kind is
// `product:icon` is migrated out on read (dropped here) so it never surfaces as
// a ghost top-level card. Its membership now lives only in the favourites
// folder, which persists independently.
const LEGACY_PRODUCT_ICON_KIND = 'product:icon';

function isLegacyProductIconRow(item: LegacyLayoutItem): boolean {
  return item.payload?.kind === LEGACY_PRODUCT_ICON_KIND;
}

// Synthesize a payload from a legacy item that pre-dates the card-refactor.
// Native chat used to live in the products table with `i === 'chat'`; that's
// the only `'widget'` row we recognize as native today.
function payloadFromLegacy(item: LegacyLayoutItem): DashboardCardPayload {
  if (item.payload) {
    const parsed = parseDashboardCardPayload(item.payload);
    if (parsed) return parsed;
  }
  if (item.type === 'folder') {
    return { kind: 'folder', items: item.folderItems ?? [] };
  }
  if (item.i === 'chat') return { kind: 'native:chat' };
  return { kind: 'product:widget', productId: item.i };
}

const RESIZE_HANDLE_AXES = new Set<string>(['s', 'w', 'e', 'n', 'sw', 'nw', 'se', 'ne']);

function isResizeHandleAxis(value: string): value is ResizeHandleAxis {
  return RESIZE_HANDLE_AXES.has(value);
}

function legacyToCard(item: LegacyLayoutItem): DashboardCard {
  return {
    i: item.i,
    x: item.x,
    y: item.y,
    w: item.w,
    h: item.h,
    minW: item.minW,
    maxW: item.maxW,
    minH: item.minH,
    maxH: item.maxH,
    resizeHandles: item.resizeHandles?.filter(isResizeHandleAxis),
    payload: payloadFromLegacy(item),
  };
}

function rowsToCards(rows: LegacyLayoutItem[]): DashboardCard[] {
  return rows.filter(row => !isLegacyProductIconRow(row)).map(legacyToCard);
}

function resolvePages(layout: StoredLayout | null | undefined): DashboardCard[][] | null {
  if (!layout) return null;
  if (layout.pages && layout.pages.length > 0) return layout.pages.map(rowsToCards);
  if (layout.items && layout.items.length > 0) return [rowsToCards(layout.items)];
  return null;
}

function clampIndex(index: number | undefined, pageCount: number): number {
  if (!index || index < 0) return 0;
  if (pageCount <= 0) return 0;
  return Math.min(index, pageCount - 1);
}

function resolveMain(layout: StoredLayout | null | undefined): MainDashboardLayoutSnapshot | null {
  const pages = resolvePages(layout);
  if (!pages) return null;
  return { pages, activePageIndex: clampIndex(layout?.activePageIndex, pages.length) };
}

export const dashboardLayoutDb = {
  getMainLayout(): ResultAsync<DashboardLayout | null, Error> {
    return ResultAsync.fromPromise(
      table.get('main').then((stored): DashboardLayout | null => {
        if (!stored) return null;
        const pages = resolvePages(stored) ?? [];
        return {
          id: stored.id,
          pages,
          activePageIndex: clampIndex(stored.activePageIndex, pages.length),
          updatedAt: stored.updatedAt,
        };
      }),
      toError,
    );
  },

  getMainPages(): ResultAsync<DashboardCard[][] | null, Error> {
    return ResultAsync.fromPromise(table.get('main').then(resolvePages), toError);
  },

  getMain(): ResultAsync<MainDashboardLayoutSnapshot | null, Error> {
    return ResultAsync.fromPromise(table.get('main').then(resolveMain), toError);
  },

  saveMainPages(pages: DashboardCard[][], activePageIndex?: number): ResultAsync<DashboardLayout, Error> {
    return ResultAsync.fromPromise(
      (async () => {
        const current = await table.get('main');
        const nextIndex = clampIndex(activePageIndex ?? current?.activePageIndex, pages.length);
        const layout: DashboardLayout = {
          id: 'main',
          pages,
          activePageIndex: nextIndex,
          updatedAt: Date.now(),
        };
        await table.put(layout);
        return layout;
      })(),
      toError,
    );
  },

  saveMainActivePageIndex(activePageIndex: number): ResultAsync<number, Error> {
    return ResultAsync.fromPromise(
      (async () => {
        const current = await table.get('main');
        const pageCount = current?.pages?.length ?? (current?.items ? 1 : 0);
        const nextIndex = clampIndex(activePageIndex, pageCount);
        if (current?.activePageIndex === nextIndex) return nextIndex;
        await table.update('main', { activePageIndex: nextIndex, updatedAt: Date.now() });
        return nextIndex;
      })(),
      toError,
    );
  },

  subscribeToMain(): Observable<MainDashboardLayoutSnapshot | null> {
    return streamTable(table, t => t.get('main')).pipe(map(layout => resolveMain(layout ?? null)));
  },
};
