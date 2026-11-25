// @vitest-environment happy-dom

import { act, renderHook, waitFor } from '@testing-library/react';
import { Subject, of } from 'rxjs';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { useDisplayedProduct } from './hooks';
import { type PersistedProduct } from './repository';
import { chainResolveResource, productsResource } from './resource';
import { type Product } from './types';

function product(baseName: string): Product {
  return { baseName, displayName: 'App', description: '', icon: { cid: '', format: 'png' }, executables: {} };
}

function persisted(baseName: string): PersistedProduct {
  return { ...product(baseName), pinned: false, createdAt: 1000, updatedAt: 1000 };
}

afterEach(() => {
  vi.useRealTimers();
});

describe('useDisplayedProduct', () => {
  // The chain entry goes stale after a minute; a screen that mounts the product again
  // re-reads it, and must keep naming the product it already knows meanwhile.
  it('keeps an uncommitted product while its stale chain entry is re-read', async () => {
    vi.useFakeTimers({ toFake: ['Date'] });
    productsResource.instead(() => of([]));
    let reads = 0;
    chainResolveResource.instead(() => (reads++ === 0 ? product('a.dot') : new Promise<Product>(() => {})));
    const first = renderHook(() => useDisplayedProduct('a.dot'));
    await waitFor(() => expect(first.result.current.data?.baseName).toBe('a.dot'));
    first.unmount();

    vi.setSystemTime(Date.now() + 61_000);
    const { result } = renderHook(() => useDisplayedProduct('a.dot'));

    await waitFor(() => expect(reads).toBe(2));
    await waitFor(() => expect(result.current.data?.baseName).toBe('a.dot'));
    expect(result.current.pending).toBe(true);
  });

  it("never shows the previous identifier's product while a new one resolves", async () => {
    productsResource.instead(() => of([]));
    chainResolveResource.instead(({ identifier }) =>
      identifier === 'a.dot' ? product('a.dot') : new Promise<Product>(() => {}),
    );
    const seen: (string | undefined)[] = [];
    const { result, rerender } = renderHook(
      ({ id }: { id: string }) => {
        const state = useDisplayedProduct(id);
        seen.push(state.data?.baseName);

        return state;
      },
      { initialProps: { id: 'a.dot' } },
    );
    await waitFor(() => expect(result.current.data?.baseName).toBe('a.dot'));

    seen.length = 0;
    rerender({ id: 'b.dot' });

    await waitFor(() => expect(result.current.pending).toBe(true));
    expect(seen).not.toContain('a.dot');
  });

  // Committing drops the chain entry a render before the live DB row arrives. The gap
  // must not read as "no such product" — every screen showing it would tear down.
  it('keeps the product through its commit', async () => {
    const products = new Subject<PersistedProduct[]>();
    productsResource.instead(() => products);
    chainResolveResource.instead(() => product('a.dot'));
    const seen: { data: Product | null; pending: boolean }[] = [];
    const { result } = renderHook(() => {
      const state = useDisplayedProduct('a.dot');
      seen.push(state);

      return state;
    });
    act(() => products.next([]));
    await waitFor(() => expect(result.current.data?.baseName).toBe('a.dot'));

    seen.length = 0;
    act(() => chainResolveResource.invalidateAll());
    act(() => products.next([persisted('a.dot')]));

    expect(seen.map(state => state.data?.baseName)).not.toContain(undefined);
    expect(result.current.data?.baseName).toBe('a.dot');
  });
});
