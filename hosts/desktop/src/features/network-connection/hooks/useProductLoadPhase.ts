import { useEffect, useState } from 'react';

import { CONNECTION_TIMEOUT_THRESHOLD_MS, SLOW_CONNECTION_THRESHOLD_MS } from '@/domains/network';

export type ProductLoadPhase = 'loading' | 'slow' | 'timeout';

/**
 * Mount time is taken as the load start, so this only reports the truth from a
 * component that exists solely while the product is loading.
 */
export const useProductLoadPhase = (): ProductLoadPhase => {
  const [phase, setPhase] = useState<ProductLoadPhase>('loading');

  useEffect(() => {
    const slowTimer = setTimeout(() => setPhase('slow'), SLOW_CONNECTION_THRESHOLD_MS);
    const timeoutTimer = setTimeout(() => setPhase('timeout'), CONNECTION_TIMEOUT_THRESHOLD_MS);

    return () => {
      clearTimeout(slowTimer);
      clearTimeout(timeoutTimer);
    };
  }, []);

  return phase;
};
