import { useMemo } from 'react';

import { useRxState } from '@/shared/rxstate';
import { usePersistedProducts } from '@/domains/product';
import { browserTabs } from '@/aggregates/browser-tabs';
import { useTruapiUserId } from '@/aggregates/truapi-runtime';
import { resolveTabProductIdTransformer } from '../di';

import { ResolvedProductWorker } from './ResolvedProductWorker';

export const WorkersManager = () => {
  const { data: products } = usePersistedProducts();
  const [tabs] = useRxState(browserTabs.tabs$);
  const accountId = useTruapiUserId() ?? '';

  // Invariant: a product tab's `id` is the product `baseName`, so tab-derived ids line
  // up with committed ones and the Set dedupes a product that is both.
  //
  // One list, not two: committing a browsed product used to move it between a
  // `browsed-*` list and a committed one, and the differing React keys remounted —
  // and disposed — the very worker awaiting that commit to answer its room
  // declaration. Here a commit changes the product's data, not its position, so the
  // holder keeps its reference and the core never sees demand drop.
  const productIds = useMemo(() => {
    const ids = new Set(products.map(product => product.baseName));

    for (const tab of tabs) {
      const productId = resolveTabProductIdTransformer(tab);
      if (productId) ids.add(productId);
    }

    return [...ids];
  }, [tabs, products]);

  return (
    <>
      {productIds.map(productId => (
        <ResolvedProductWorker key={`${productId}-${accountId}`} productId={productId} />
      ))}
    </>
  );
};
