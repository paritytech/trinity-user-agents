// @vitest-environment happy-dom

import { cleanup, render, screen, waitFor } from '@testing-library/react';
import { of } from 'rxjs';
import { afterEach, describe, expect, it } from 'vitest';

import { ipfsRawResource } from '@/domains/network';
import { type PersistedProduct, productsResource } from '@/domains/product';

import { ProductIdentity } from './ProductIdentity';

// No icon stub is needed: `ipfsRawResource`'s mock already answers null, which is
// the no-icon case.
const seedProduct = (displayName: string) => {
  const record = {
    baseName: 'dotli-wallet',
    displayName,
    description: '',
    icon: { cid: 'abc', format: 'png' },
    executables: {},
    pinned: false,
    createdAt: 1000,
    updatedAt: 1000,
  } as unknown as PersistedProduct;

  productsResource.instead(() => of([record]));
};

afterEach(() => {
  cleanup();
});

describe('ProductIdentity', () => {
  it('renders the answering product manifest name, not its identifier', async () => {
    seedProduct('Wallet');
    render(<ProductIdentity productId="dotli-wallet" />);

    expect(await screen.findByText('Wallet')).toBeInTheDocument();
    expect(screen.queryByText('dotli-wallet')).toBeNull();
  });

  it('falls back to the identifier when the manifest is not on hand', async () => {
    render(<ProductIdentity productId="dotli-wallet" />);

    await waitFor(() => expect(screen.getByText('dotli-wallet')).toBeInTheDocument());
  });

  it('renders the manifest icon with empty alt text, so the name is the only label', async () => {
    // The name already carries the identity; a duplicate alt would make every
    // candidate announce its product twice.
    seedProduct('Wallet');
    // Three zero bytes are exactly "AAAA" once base64-encoded.
    ipfsRawResource.instead(() => new Uint8Array([0, 0, 0]));
    const { container } = render(<ProductIdentity productId="dotli-wallet" />);

    await waitFor(() => expect(container.querySelector('img')).not.toBeNull());
    const image = container.querySelector('img');
    expect(image?.getAttribute('src')).toBe('data:image/png;base64,AAAA');
    expect(image?.getAttribute('alt')).toBe('');
  });
});
