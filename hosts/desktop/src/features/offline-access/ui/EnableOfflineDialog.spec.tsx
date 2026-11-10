// @vitest-environment happy-dom

import { fireEvent, render, screen } from '@testing-library/react';
import { of } from 'rxjs';
import { describe, expect, it, vi } from 'vitest';

import { TEST_IDS } from '@/shared/test-ids';
import { TranslationProvider } from '@/shared/translation';
import { type PersistedProduct, commitmentUseCase, productsResource } from '@/domains/product';

import { EnableOfflineDialog } from './EnableOfflineDialog';

// The pin write is a use-case method on a plain object, spied in place: `usePinProduct`
// is a `useAction` over it, so the real hook runs and the spy sees the call.
const pinProduct = vi.spyOn(commitmentUseCase, 'pinProduct').mockResolvedValue(null);

describe('EnableOfflineDialog', () => {
  it('pins the product on confirm', async () => {
    // eslint-disable-next-line @typescript-eslint/consistent-type-assertions -- test fixture, not production code
    const record = {
      baseName: 'a.dot',
      displayName: 'A',
      description: '',
      icon: { cid: '', format: 'png' },
      executables: {},
      pinned: false,
      createdAt: 1000,
      updatedAt: 1000,
    } as unknown as PersistedProduct;
    productsResource.instead(() => of([record]));

    render(
      <TranslationProvider>
        <EnableOfflineDialog productId="a.dot" onClose={() => {}} />
      </TranslationProvider>,
    );
    fireEvent.click(await screen.findByTestId(TEST_IDS.offlineAccessEnableConfirm));

    expect(pinProduct).toHaveBeenCalledWith('a.dot');
  });
});
