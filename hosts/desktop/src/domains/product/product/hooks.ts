import { useMemo } from 'react';

import { dependentRead, useRead } from '@/shared/hooks';
import { nonNullable } from '@/shared/utils';
import { useActiveEnvironment } from '@/domains/application';
import { useDotNsTld } from '../dotns/hooks';

import { chainResolveCacheKey, chainResolveResource, productsResource } from './resource';
import { type Product } from './types';

// Internal: resolve an identifier purely from the chain into memory (no DB).
// `useDisplayedProduct` is the public entry point; this is its fallback for
// identifiers with no committed row.
//
// The environment is resolved here, in the React binding, and passed into the
// resource as a parameter — the resource reads gateways only, never a use case.
function useChainResolvedProduct(identifier: Nullable<string>) {
  const { data: environment } = useActiveEnvironment();
  // Gated on the settled TLD, not just a usable one: a resolve fired under the
  // `.dot` fallback namehashes the wrong root on any other network, and caches
  // the miss under a key nothing revisits. A failed read gates the same way — it
  // leaves that same fallback in `data`. This read also carries the environment's
  // own wait and failure, so it is the single dependency below.
  const tld = useDotNsTld();
  const tldSettled = !tld.pending && tld.error === null;

  const params = identifier && environment && tldSettled ? { identifier, environment, tld: tld.data } : null;
  const key = params ? chainResolveCacheKey(params.environment, params.identifier, params.tld) : null;

  const result = useRead(chainResolveResource, {
    params,
    // eslint-disable-next-line @typescript-eslint/consistent-type-assertions -- narrowing null to typed defaultValue
    defaultValue: null as ChainProjection | null,
    // Tagged with the key it came from: `useRead` keeps its last value while a read is
    // re-running or its entry has left the cache, and only the key says whether that
    // value still names this identifier. `undefined` for an entry that is not cached,
    // so `useRead` can tell it was dropped from one that resolved to `null` (not on chain).
    map: (cache, { environment: env, identifier: id, tld: suffix }) => {
      const entryKey = chainResolveCacheKey(env, id, suffix);

      return entryKey in cache ? { key: entryKey, product: cache[entryKey] ?? null } : undefined;
    },
  });

  // `enabled` is what the caller asked for: with `identifier === null` there is
  // nothing to wait on, and claiming pending would stall `useDisplayedProduct` for
  // a product already served from the DB — or, worse, report the TLD's failure
  // against a localhost webview that needs no dotNS at all.
  const read = dependentRead(result, tld, { enabled: nonNullable(identifier) });

  return { ...read, data: read.data?.key === key ? read.data.product : null };
}

type ChainProjection = { key: string; product: Product | null };

// Every row in `products` is a committed (installed) product, so "all products"
// and "installed products" are the same set — one hook covers both.
export const usePersistedProducts = () => {
  return useRead(productsResource, {
    params: {},
    defaultValue: [],
  });
};

export const useIsProductInstalled = (productId: string | null) => {
  const { data: products } = usePersistedProducts();

  return useMemo(() => {
    if (!productId) return false;
    // Under the new model every row in `products` is a committed product —
    // existence of the row IS the commitment.
    return products.some(p => p.baseName === productId);
  }, [productId, products]);
};

export const usePersistedProductById = (productId: Nullable<string>) => {
  const { data: products, pending, error } = usePersistedProducts();

  const product = useMemo(() => {
    if (!productId) return null;
    return products.find(p => p.baseName === productId) ?? null;
  }, [productId, products]);

  return { data: product, pending, error };
};

// The public "give me the product for this identifier, committed or not" hook —
// committed row from the live DB if present, else resolved from chain into
// memory. THE entry point for any screen that shows a product by id (tabs,
// address bar, dialogs, settings, webview). Returns the standard resource-hook
// shape; `data` is null until at least one source resolves.
export function useDisplayedProduct(productId: Nullable<string>): { data: Product | null; pending: boolean; error: unknown } {
  const { data: persisted, pending: persistedPending, error: persistedError } = usePersistedProductById(productId);
  // Resolve from chain only for identifiers with no committed row — committed
  // products are served from the live DB above and never enter the chain cache,
  // so the two reads can't duplicate or diverge.
  const chainIdentifier = persistedPending || nonNullable(persisted) ? null : productId;
  const chain = useChainResolvedProduct(chainIdentifier);
  // Committing a product drops its chain entry a render before the live DB row arrives.
  // The product stays in `data` meanwhile, but no read stands behind it, so the gap
  // reports pending rather than settled.
  const chainAwaited = nonNullable(chainIdentifier) && !chain.fulfilled && !nonNullable(chain.error);

  return {
    data: persisted ?? chain.data,
    pending: persistedPending || chain.pending || chainAwaited,
    error: persistedError ?? chain.error,
  };
}

export function useIsPinned(productId: Nullable<string>): boolean {
  const { data: record } = usePersistedProductById(productId);
  return record?.pinned ?? false;
}
