// @vitest-environment happy-dom

import { render, screen, waitFor } from '@testing-library/react';
import { of } from 'rxjs';
import { describe, expect, it } from 'vitest';

import { TEST_IDS } from '@/shared/test-ids';
import { type PersistedProduct, productsResource } from '@/domains/product';

import { PinIndicator } from './PinIndicator';

function seedProduct(pinned: boolean) {
  const record = {
    baseName: 'a.dot',
    displayName: 'A',
    description: '',
    icon: { cid: '', format: 'png' },
    executables: {},
    pinned,
    createdAt: 1000,
    updatedAt: 1000,
  } as unknown as PersistedProduct;

  productsResource.instead(() => of([record]));
}

describe('PinIndicator', () => {
  it('renders nothing when product is not pinned', async () => {
    seedProduct(false);
    const { container } = render(<PinIndicator productId="a.dot" />);
    // An empty container is also what an unsettled read looks like, so wait for the
    // resource to hold the row before concluding the icon stayed away.
    await waitFor(() => expect(Object.keys(productsResource.snapshot()).length).toBeGreaterThan(0));
    expect(container).toBeEmptyDOMElement();
  });

  it('renders the pin icon when pinned', async () => {
    seedProduct(true);
    render(<PinIndicator productId="a.dot" />);
    expect(await screen.findByTestId(TEST_IDS.offlineAccessPinIndicator)).toBeInTheDocument();
  });
});
