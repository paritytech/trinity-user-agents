import { ResultAsync } from 'neverthrow';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { FAVORITES_FOLDER_ID } from '../dashboard-layout/constants';
import { dashboardLayoutDb } from '../dashboard-layout/repository';
import { dashboardLayoutService } from '../dashboard-layout/service';
import { type DashboardCard, type DashboardLayout, type MainDashboardLayoutSnapshot } from '../dashboard-layout/types';

import { foldersUseCase } from './folders';

const savedLayout: DashboardLayout = {
  id: 'main',
  pages: [],
  activePageIndex: 0,
  updatedAt: 0,
};

const okSaveMainPages = () => ResultAsync.fromSafePromise<DashboardLayout, Error>(Promise.resolve(savedLayout));

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

const widgetCard = (id: string): DashboardCard => ({
  i: id,
  x: 0,
  y: 0,
  w: 2,
  h: 4,
  payload: { kind: 'product:widget', productId: id },
});

const okPages = (pages: DashboardCard[][]) => ResultAsync.fromSafePromise(Promise.resolve(pages));

const okMain = (pages: MainDashboardLayoutSnapshot['pages'] | null) =>
  ResultAsync.fromSafePromise<MainDashboardLayoutSnapshot | null, Error>(
    Promise.resolve(pages === null ? null : { pages, activePageIndex: 0 }),
  );

// The repository is a plain object, so its reads and writes are spied in place:
// nothing reaches Dexie, and the use case is the only real code on the path.
beforeEach(() => {
  vi.clearAllMocks();
  vi.spyOn(dashboardLayoutDb, 'getMain');
  vi.spyOn(dashboardLayoutDb, 'getMainPages');
  vi.spyOn(dashboardLayoutDb, 'saveMainPages');
  vi.mocked(dashboardLayoutDb.saveMainPages).mockReturnValue(okSaveMainPages());
});

describe('foldersUseCase.addToFavorites', () => {
  it('creates the favourites folder and adds the icon when none exists', async () => {
    vi.mocked(dashboardLayoutDb.getMain).mockReturnValue(okMain([[]]));

    const result = await foldersUseCase.addToFavorites('chat');

    expect(result.ok).toBe(true);
    const savedPages = vi.mocked(dashboardLayoutDb.saveMainPages).mock.calls[0]?.[0];
    const folder = savedPages?.flat().find(item => item.i === FAVORITES_FOLDER_ID);
    expect(dashboardLayoutService.asFolder(folder!)?.items).toEqual(['chat']);
  });

  it('rejects adding an icon already in favourites without saving', async () => {
    vi.mocked(dashboardLayoutDb.getMain).mockReturnValue(okMain([[favoritesFolder(['chat'])]]));

    const result = await foldersUseCase.addToFavorites('chat');

    expect(result.ok).toBe(false);
    expect(dashboardLayoutDb.saveMainPages).not.toHaveBeenCalled();
  });

  // A product's standalone widget and its favourites membership are independent
  // placements (mirror of removeItemFromFolder's coexistence guarantee). Adding
  // to favourites must NOT sweep a coexisting top-level product widget — the bug
  // that made `browse.dot` (the default seeded widget) vanish from the dashboard.
  it('keeps a coexisting top-level product widget when seeding the favourites folder', async () => {
    vi.mocked(dashboardLayoutDb.getMain).mockReturnValue(okMain([[widgetCard('browse.dot')]]));

    const result = await foldersUseCase.addToFavorites('browse.dot');

    expect(result.ok).toBe(true);
    const savedPages = vi.mocked(dashboardLayoutDb.saveMainPages).mock.calls[0]?.[0];
    expect(savedPages?.flat().find(item => item.i === 'browse.dot')?.payload.kind).toBe('product:widget');
    const folder = savedPages?.flat().find(item => item.i === FAVORITES_FOLDER_ID);
    expect(dashboardLayoutService.asFolder(folder!)?.items).toEqual(['browse.dot']);
  });

  it('keeps a coexisting top-level product widget when the favourites folder already exists', async () => {
    vi.mocked(dashboardLayoutDb.getMain).mockReturnValue(okMain([[widgetCard('browse.dot'), favoritesFolder(['a'])]]));

    const result = await foldersUseCase.addToFavorites('browse.dot');

    expect(result.ok).toBe(true);
    const savedPages = vi.mocked(dashboardLayoutDb.saveMainPages).mock.calls[0]?.[0];
    expect(savedPages?.flat().find(item => item.i === 'browse.dot')?.payload.kind).toBe('product:widget');
    const folder = savedPages?.flat().find(item => item.i === FAVORITES_FOLDER_ID);
    expect(dashboardLayoutService.asFolder(folder!)?.items).toEqual(['a', 'browse.dot']);
  });
});

describe('foldersUseCase.reorderFolderItems', () => {
  it('reorders the given ids in place, keeping other items in their slots', async () => {
    const pages: DashboardCard[][] = [[favoritesFolder(['chat', 'a', 'b'])]];
    vi.mocked(dashboardLayoutDb.getMainPages).mockReturnValue(okPages(pages));

    const ok = await foldersUseCase.reorderFolderItems(FAVORITES_FOLDER_ID, ['b', 'a']);

    expect(ok).toBe(true);
    const savedPages = vi.mocked(dashboardLayoutDb.saveMainPages).mock.calls[0]?.[0];
    const folder = savedPages?.[0]?.find(item => item.i === FAVORITES_FOLDER_ID);
    // 'chat' (outside the reordered subset) stays in slot 0; 'a'/'b' swap.
    expect(dashboardLayoutService.asFolder(folder!)?.items).toEqual(['chat', 'b', 'a']);
  });

  it('does not save when the resulting order is unchanged', async () => {
    const pages: DashboardCard[][] = [[favoritesFolder(['a', 'b', 'c'])]];
    vi.mocked(dashboardLayoutDb.getMainPages).mockReturnValue(okPages(pages));

    const ok = await foldersUseCase.reorderFolderItems(FAVORITES_FOLDER_ID, ['a', 'b', 'c']);

    expect(ok).toBe(false);
    expect(dashboardLayoutDb.saveMainPages).not.toHaveBeenCalled();
  });
});

describe('foldersUseCase.removeItemFromFolder', () => {
  it('removes only the favourites icon when a native widget of the same id coexists', async () => {
    const pages: DashboardCard[][] = [[favoritesFolder(['chat']), nativeCard('chat')]];
    vi.mocked(dashboardLayoutDb.getMainPages).mockReturnValue(okPages(pages));

    const removed = await foldersUseCase.removeItemFromFolder('chat');

    expect(removed).toBe(true);
    const savedPages = vi.mocked(dashboardLayoutDb.saveMainPages).mock.calls[0]?.[0];
    expect(savedPages?.[0]?.find(item => item.i === 'chat')?.payload.kind).toBe('native:chat');
    const folder = savedPages?.[0]?.find(item => item.i === FAVORITES_FOLDER_ID);
    expect(dashboardLayoutService.asFolder(folder!)).toMatchObject({ items: [] });
  });
});
