// @vitest-environment happy-dom

import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { act } from 'react';
import { describe, expect, it } from 'vitest';

import { TEST_IDS } from '@/shared/test-ids';
import { TranslationProvider } from '@/shared/translation';
import { openAddToFavoritesDialog } from '../state/addToFavoritesDialog';

import { AddToFavoritesDialogHost } from './AddToFavoritesDialogHost';

// The real dialog renders: its catalog and layout reads are resources, so their
// builder mocks answer with an empty list and nothing reaches the chain.
describe('AddToFavoritesDialogHost', () => {
  it('mounts the dialog only while the feature state says it is open', async () => {
    render(
      <TranslationProvider>
        <AddToFavoritesDialogHost />
      </TranslationProvider>,
    );
    expect(screen.queryByTestId(TEST_IDS.addToFavoritesDialog)).not.toBeInTheDocument();

    act(() => openAddToFavoritesDialog());
    expect(await screen.findByTestId(TEST_IDS.addToFavoritesDialog)).toBeInTheDocument();

    await userEvent.keyboard('{Escape}');
    expect(screen.queryByTestId(TEST_IDS.addToFavoritesDialog)).not.toBeInTheDocument();
  });
});
