import { useEffect, useState } from 'react';

import { type ProductWorkerInstance } from '@/domains/product';
import { truapiRuntimeUseCase } from '@/aggregates/truapi-runtime';

import { productWorkerRegistry } from './state/registry';

/**
 * Declare that this surface needs `productId`'s worker running, for as long as it is
 * mounted.
 *
 * The core counts references and reports the level back to the host's demand watcher,
 * which starts and stops the executable. Nothing here waits for or observes the
 * resulting instance — read it with `useProductWorkerInstance` if you need it.
 */
export function useWorkerDemand(productId: string): void {
  useEffect(() => {
    // Both await `whenRuntimeReady`, which rejects when the core never boots. Dropping
    // the promise would raise one unhandled rejection per mounted holder on top of the
    // boot failure that already surfaced. There is no worker to run in that state, so
    // reporting it and moving on is the whole remedy.
    const report = (error: unknown) => console.error('[product-workers] worker demand not registered', { productId, error });

    void truapiRuntimeUseCase.acquireWorker(productId).catch(report);

    return () => void truapiRuntimeUseCase.releaseWorker(productId).catch(report);
  }, [productId]);
}

export function useProductWorkerInstance(productId: string): ProductWorkerInstance | null {
  const [instance, setInstance] = useState<ProductWorkerInstance | null>(() => productWorkerRegistry.get(productId));

  useEffect(() => {
    setInstance(productWorkerRegistry.get(productId));
    const sub = productWorkerRegistry.instance$(productId).subscribe(setInstance);

    return () => sub.unsubscribe();
  }, [productId]);

  return instance;
}
