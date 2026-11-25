// @vitest-environment happy-dom

import { render, screen, waitFor } from '@testing-library/react';
import { of } from 'rxjs';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { TranslationProvider } from '@/shared/translation';
import { chainResolveResource } from '@/domains/product';
// eslint-disable-next-line boundaries/dependencies -- the read behind the hook; the barrel exposes the hook but not the use case
import { interactionUseCase } from '@/domains/product/$usecase/interaction';

import { ProductListSettingsPage } from './ProductListSettingsPage';

// A permission-only row comes from the core's permission slots — an adapter, not a
// resource — so the use-case read behind the hook is spied in place.
const watchInteractedProducts = vi.spyOn(interactionUseCase, 'watchInteractedProducts').mockReturnValue(of([]));

// Nothing is committed, so every row falls through to the chain resolution, which
// is the read under test.
const resolveFromChain = vi.fn(({ identifier: _identifier }: { identifier: string }) => null);

const renderPage = () =>
  render(
    <TranslationProvider>
      <ProductListSettingsPage />
    </TranslationProvider>,
  );

describe('ProductListSettingsPage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resolveFromChain.mockClear();
    chainResolveResource.instead(resolveFromChain);
  });

  it('renders a permission-only row with the raw id as fallback label', async () => {
    watchInteractedProducts.mockReturnValue(of([{ kind: 'permissionOnly', productId: 'browsed.dot' }]));

    renderPage();

    await waitFor(() => expect(resolveFromChain).toHaveBeenCalledWith(expect.objectContaining({ identifier: 'browsed.dot' })));
    expect(screen.getAllByText('browsed.dot').length).toBeGreaterThan(0);
  });

  it('does not chain-resolve localhost permission-only entries', async () => {
    watchInteractedProducts.mockReturnValue(of([{ kind: 'permissionOnly', productId: 'localhost:5173' }]));

    renderPage();

    expect((await screen.findAllByText('localhost:5173')).length).toBeGreaterThan(0);
    expect(resolveFromChain).not.toHaveBeenCalled();
  });
});
