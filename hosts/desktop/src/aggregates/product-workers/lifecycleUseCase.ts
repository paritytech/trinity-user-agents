import { Observable } from 'rxjs';

import { type FetchResolver, type Product, type ProductWorkerInstance, createProductWorker } from '@/domains/product';
import { truapiRuntimeUseCase } from '@/aggregates/truapi-runtime';

import { chatDeliveryUseCase } from './chatDeliveryUseCase';
import { productWorkerRegistry } from './state/registry';

type CreateInstanceParams = {
  contenthash: string;
  files: Record<string, Uint8Array>;
  entrypoint: string;
  fetchResolver: FetchResolver;
  getProduct: () => Product;
};

/**
 * Emits the live `ProductWorkerInstance` for a (product, content) pair and
 * keeps it registered in the aggregate's registry. Unsubscribing disposes
 * the worker and unregisters it — the subscription IS the lifetime.
 */
function createInstance$({
  contenthash,
  files,
  entrypoint,
  fetchResolver,
  getProduct,
}: CreateInstanceParams): Observable<ProductWorkerInstance> {
  return new Observable<ProductWorkerInstance>(subscriber => {
    const baseName = getProduct().baseName;
    let instance: ProductWorkerInstance | null = null;
    let stopChatDelivery: VoidFunction | null = null;
    let cancelled = false;

    // The core gates its Chat trait on `Worker` by exact equality, so a worker that
    // opened under any other kind would be denied the chat it exists to serve.
    truapiRuntimeUseCase
      .createProvider({ productId: baseName, executionKind: 'Worker' })
      .then(coreProvider =>
        createProductWorker({
          productId: baseName,
          contenthash,
          files,
          entrypoint,
          fetchResolver,
          coreProvider,
        }),
      )
      .then(inst => {
        if (cancelled) {
          inst.dispose();
          return;
        }
        instance = inst;
        // Started here rather than by whatever renders the worker: delivery has to
        // live exactly as long as the instance, and this subscription is that lifetime.
        stopChatDelivery = chatDeliveryUseCase.start(baseName, inst.coreProvider);
        productWorkerRegistry.register(inst);
        subscriber.next(inst);
      })
      .catch(err => {
        console.error(`[ProductWorker] Failed to create worker for ${baseName}:`, err);
        subscriber.error(err);
      });

    return () => {
      cancelled = true;
      stopChatDelivery?.();
      if (instance) {
        productWorkerRegistry.unregister(instance);
        instance.dispose();
      }
    };
  });
}

export const lifecycleUseCase = {
  createInstance$,
};
