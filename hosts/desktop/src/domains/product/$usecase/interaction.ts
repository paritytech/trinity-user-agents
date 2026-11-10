import { type Observable, catchError, combineLatest, debounceTime, from, map, of, startWith } from 'rxjs';

import { DEFAULT_DOTNS_TLD } from '../dotns/constants';
import { dotNsService } from '../dotns/service';
import { productsResource } from '../product/resource';
import { type Product } from '../product/types';

import { dotNsUseCase } from './dotns';
import { permissionsUseCase } from './permissions';

// Use-case-local composition type (multi-module: product + permissions + alias-permissions).
// A discriminated union per interacted product: committed entries carry the
// resolved `Product`; permission-only entries carry just the stored raw id
// (display resolution is the consumer's concern — see `useDisplayedProduct`).
export type InteractedProduct = { kind: 'committed'; product: Product } | { kind: 'permissionOnly'; productId: string };

// Ids of interacted products that are NOT committed — every product the core holds
// a permission slot for — deduped and sorted for a deterministic order.
//
// A slot is keyed by whatever product id the core was handed, which is the raw
// webview identifier, while a committed `baseName` is always `baseNameOf()`-
// normalized. Membership is compared on the normalized form, but the *raw id* is
// emitted: detail-page lookups key on it.
//
// The "row with no standing decision" case that used to need filtering is gone with
// the local store: the core clears a slot when a decision is reset, so a slot's
// existence IS a decision.
function collectPermissionOnlyIds(products: Product[], permissionProductIds: string[], tld: string): string[] {
  const committed = new Set(products.map(product => dotNsService.baseNameOf(product.baseName, tld)));
  const idByBaseName = new Map<string, string>();

  function add(rawId: string) {
    const baseName = dotNsService.baseNameOf(rawId, tld);
    if (committed.has(baseName)) return;
    if (!idByBaseName.has(baseName)) idByBaseName.set(baseName, rawId);
  }

  for (const productId of permissionProductIds) {
    add(productId);
  }

  return [...idByBaseName.values()].sort((a, b) => a.localeCompare(b));
}

// A permission store is supplementary to the committed-products list: it must
// not hold the whole list back (combineLatest waits for every source's first
// value) nor collapse it (one corrupt row erroring the stream would otherwise
// error the combined stream). Seed with [] and degrade to [] on error.
function supplementary<T>(stream$: Observable<T[]>): Observable<T[]> {
  return stream$.pipe(
    catchError(() => of<T[]>([])),
    startWith<T[]>([]),
  );
}

// Live-updating read over three same-domain stores: committed products plus
// every product that has any stored permission decision (device/remote row or
// an alias grant as requester), regardless of status.
function watchInteractedProducts(): Observable<InteractedProduct[]> {
  return combineLatest([
    productsResource.read$({}),
    supplementary(permissionsUseCase.watchProductsWithPermissions()),
    // Same contract as `supplementary` above, for a promise source: seeded so the
    // list is not held back by a chain round trip, and caught so a failed read
    // cannot error the combined stream. The seed is only ever visible for the
    // first frame, and only affects id normalization, never what is emitted.
    from(dotNsUseCase.getActiveTld()).pipe(
      startWith(DEFAULT_DOTNS_TLD),
      catchError(() => of(DEFAULT_DOTNS_TLD)),
    ),
  ]).pipe(
    // Coalesce the synchronous seed/value burst (combineLatest glitch frames)
    // into one emission carrying every source's settled value.
    debounceTime(0),
    map(([products, permissionProductIds, tld]) => {
      // Committed entries first, then the sorted permission-only ids — a stable
      // order so consumers can render the union as-is without re-sorting.
      const committed = products.map((product): InteractedProduct => ({ kind: 'committed', product }));
      const permissionOnly = collectPermissionOnlyIds(products, permissionProductIds, tld).map(
        (productId): InteractedProduct => ({ kind: 'permissionOnly', productId }),
      );

      return [...committed, ...permissionOnly];
    }),
  );
}

export const interactionUseCase = {
  watchInteractedProducts,
};
