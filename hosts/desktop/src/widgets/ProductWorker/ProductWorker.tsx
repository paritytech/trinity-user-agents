import { memo } from 'react';

import { useWorkerDemand } from '@/aggregates/product-workers';

type ProductWorkerProps = {
  productId: string;
};

/**
 * Declares that a product's worker is wanted for as long as this is rendered.
 *
 * It no longer owns the worker's lifetime: the core counts references and the
 * aggregate's demand watcher runs and stops the executable. This is one reference,
 * held for the mount — so there is nothing to bind here and nothing to draw.
 */
export const ProductWorker = memo(({ productId }: ProductWorkerProps) => {
  useWorkerDemand(productId);

  return null;
});
