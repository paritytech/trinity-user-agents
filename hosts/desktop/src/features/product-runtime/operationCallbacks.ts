import { type RequiredHostCallbacks } from '@parity/truapi-host';

type ProductOperationsCallbacks = Pick<RequiredHostCallbacks, 'productOperations'>;

/**
 * The host's ledger of a product's open operations.
 *
 * Bookkeeping only: it mints ids and remembers which are outstanding. It does NOT
 * touch `@/aggregates/product-workers`. An open operation becomes worker demand
 * inside the core, which reports it back on `subscribeWorkerDemand` — the stream
 * `productWorkersUseCase.watchDemand` already consumes. A second ledger here would
 * be a contradictory owner of the same fact.
 */
export function createProductOperationsCallbacks(): ProductOperationsCallbacks {
  const openByProduct = new Map<string, Set<number>>();

  // The smallest id this product is not currently holding. Upstream requires
  // uniqueness only among a product's OPEN operations and explicitly permits reuse
  // after one ends, so this stays bounded by concurrency rather than by session
  // length — a monotonic counter would climb for as long as the app runs.
  function nextId(open: Set<number>): number {
    let candidate = 0;
    while (open.has(candidate)) candidate++;

    return candidate;
  }

  return {
    productOperations: {
      // Neither method is `async`: both are synchronous bookkeeping, and an `async`
      // function with no `await` trips the `require-await` warning. Returning a
      // resolved promise satisfies the contract.
      beginOperation: (product, label) => {
        const open = openByProduct.get(product.productId) ?? new Set<number>();
        const id = nextId(open);

        open.add(id);
        openByProduct.set(product.productId, open);
        console.debug('[product-runtime] operation begun', { productId: product.productId, id, label });

        return Promise.resolve({ id });
      },

      endOperation: (product, id) => {
        const open = openByProduct.get(product.productId);
        if (open) {
          open.delete(id);
          if (open.size === 0) openByProduct.delete(product.productId);
        }

        return Promise.resolve();
      },
    },
  };
}
