// @vitest-environment happy-dom

import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

// Stands in for the grid's drag interaction: clicking the button plays back the
// order dnd-kit would have produced, so the test pins the wiring, not the library.
const REORDERED_IDS = ['b', 'c', 'a'];

vi.mock('../folder/FolderGrid', () => ({
  FolderGrid: ({ onReorderItems }: { onReorderItems: (ids: string[]) => void }) => (
    <div data-testid="folder-grid">
      <button type="button" data-testid="reorder" onClick={() => onReorderItems(REORDERED_IDS)}>
        reorder
      </button>
    </div>
  ),
}));

import { TranslationProvider } from '@/shared/translation';
import { foldersUseCase as realFoldersUseCase } from '@/domains/application';
import { openAddToFavoritesSideEffect } from '../../di';

import { FolderCardContent } from './FolderCardContent';

// The two folder writes are spied in place; `dashboardLayoutService` stays real:
// `getFavoritesDisplay` is the overflow rule this file asserts, and a copy of it here
// would pass after the real one changed.
const foldersUseCase = {
  removeItemFromFolder: vi.spyOn(realFoldersUseCase, 'removeItemFromFolder'),
  reorderFolderItems: vi.spyOn(realFoldersUseCase, 'reorderFolderItems'),
};

const renderCard = () =>
  render(
    <TranslationProvider>
      <FolderCardContent cardId="folder-1" items={[]} isActivePage maxVisibleItems={6} iconSize="44" />
    </TranslationProvider>,
  );

const renderPopulatedCard = (isActivePage: boolean) =>
  render(
    <TranslationProvider>
      <FolderCardContent
        cardId="folder-1"
        items={['a', 'b', 'c']}
        isActivePage={isActivePage}
        maxVisibleItems={6}
        iconSize="44"
      />
    </TranslationProvider>,
  );

describe('FolderCardContent placeholders', () => {
  it('renders favorites empty placeholder and opens the Add-to-Favorites flow on action click', async () => {
    const user = userEvent.setup();
    const openAddToFavorites = vi.fn();
    const handler = { available: () => true, body: openAddToFavorites };
    openAddToFavoritesSideEffect.registerHandler(handler);

    try {
      renderCard();

      expect(screen.getByText('Save your favorite apps for quick access')).toBeTruthy();
      await user.click(screen.getByText('Browse Apps'));
      expect(openAddToFavorites).toHaveBeenCalledOnce();
    } finally {
      openAddToFavoritesSideEffect.removeHandler(handler);
    }
  });
});

describe('FolderCardContent reordering', () => {
  beforeEach(() => {
    foldersUseCase.reorderFolderItems.mockClear();
  });

  it('persists a widget reorder as folder item order — the same write the Favorites SPA uses', async () => {
    const user = userEvent.setup();
    renderPopulatedCard(true);

    await user.click(screen.getByTestId('reorder'));

    expect(foldersUseCase.reorderFolderItems).toHaveBeenCalledWith('folder-1', REORDERED_IDS);
  });

  it('ignores a reorder coming from a non-active dashboard page', async () => {
    const user = userEvent.setup();
    renderPopulatedCard(false);

    await user.click(screen.getByTestId('reorder'));

    expect(foldersUseCase.reorderFolderItems).not.toHaveBeenCalled();
  });
});
