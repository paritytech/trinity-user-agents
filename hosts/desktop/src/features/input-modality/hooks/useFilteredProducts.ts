import { useMemo } from 'react';

import { usePersistedProducts, useRecentProductIds } from '@/domains/product';
import { inputModalityService } from '../service';

/** Binds the host's navigation suggestions to what the user has typed. */
export const useFilteredProducts = (query: string) => {
  const { data: allProducts } = usePersistedProducts();
  const { data: recentIds } = useRecentProductIds();

  return useMemo(() => inputModalityService.suggestProducts(query, allProducts, recentIds), [query, allProducts, recentIds]);
};
