import { of } from 'rxjs';
import { namehash } from 'viem';

import { createQueryResource, createStreamResource } from '@/shared/resource';
import { type HexString } from '@/shared/types';
import { type Environment } from '@/domains/application';
import { dotNsGateway } from '../dotns/gateway';
import { dotNsService } from '../dotns/service';

import { EXECUTABLE_KINDS, MANIFEST_TEXT_RECORD_KEY } from './manifest/constants';
import { readExecutableAt } from './manifest/resource';
import { manifestService } from './manifest/service';
import { type RootManifest } from './manifest/types';
import { type PersistedProduct, productDb } from './repository';
import { type Product } from './types';

export const productsResource = createStreamResource({
  key: () => 'products',
})
  .subscribe<PersistedProduct[]>(() => productDb.subscribeToAll())
  .mock(() => of([]))
  .cache<PersistedProduct[]>({
    initial: [],
    map(_, products) {
      return products;
    },
  })
  .build();

// Synthetic root for legacy products — no `manifest` record means no metadata,
// so surface the bare base name and let downstream defaults apply (the icon
// hook returns null for an empty cid).
function legacyRoot(baseName: string): RootManifest {
  return {
    $v: 1,
    displayName: baseName,
    description: '',
    icon: { cid: '', format: 'png' },
  };
}

// Legacy branch (pre-manifest): contenthash on the global content-resolver
// contract, with no registry resolver indirection — exactly how the app
// resolved products before manifests. The app archive lives at the bare base,
// so the synthesized app executable's identifier IS the base name (not
// `app.<base>` like the manifest branch).
async function resolveLegacy(env: Environment, baseName: string): Promise<Product | null> {
  const contenthash = await dotNsGateway.readLegacyContentHash(env, namehash(baseName));
  if (!contenthash) return null;

  return manifestService.assembleProduct({
    baseName,
    root: legacyRoot(baseName),
    executables: { app: manifestService.legacyApp(baseName, contenthash) },
  });
}

// Manifest branch: metadata in the base `manifest` record; each kind
// lives at its own `<kind>.<base>` subname with its own resolver + contenthash.
async function resolveFromManifest(
  env: Environment,
  baseName: string,
  rootText: string,
  owner: HexString | null,
): Promise<Product | null> {
  const root = manifestService.parseRootManifest(rootText);
  if (!root) return null;

  const entries = await Promise.all(
    EXECUTABLE_KINDS.map(async kind => {
      const { contenthash, manifest } = await readExecutableAt(env, namehash(dotNsService.subnameOf(baseName, kind)), kind);
      // A manifest product's executable is only well-formed with both present —
      // drop the kind otherwise, leaving the product's other kinds intact.
      if (!contenthash || !manifest) return null;
      return { manifest, contenthash };
    }),
  );

  const executables = manifestService.executablesFromManifests(baseName, entries);
  return manifestService.assembleProduct({ baseName, root, executables, owner: owner ?? undefined });
}

// Manifest resolution if the base node has a registry resolver carrying a
// `manifest` record; otherwise legacy (contenthash on the global content resolver, no
// registry entry required). The registry lookup gates only the manifest path —
// legacy products predate it, so a missing resolver must NOT block resolution.
// Null only when neither path finds anything.
//
// The active `env` is a parameter: this reads gateways only, never a use case.
// `resolveProductUseCase.fetchProductFromChain` is the env-resolving wrapper over it,
// and `chainResolveResource` below is the cache over it.
export async function readProductFromChain(env: Environment, baseName: string): Promise<Product | null> {
  const node = namehash(baseName);
  const resolver = await dotNsGateway.readResolver(env, node);

  if (resolver) {
    const [rootText, owner] = await Promise.all([
      dotNsGateway.readText(env, resolver, node, MANIFEST_TEXT_RECORD_KEY),
      dotNsGateway.readOwner(env, node).catch(() => null),
    ]);
    if (rootText) return resolveFromManifest(env, baseName, rootText, owner);
  }

  return resolveLegacy(env, baseName);
}

// Cache identity for a chain resolution. The environment is part of it because the
// same base name resolves to different products on different dotNS chains, and
// `baseNameOf` collapses case/suffix variants (`Foo.dot`, `foo`) onto one entry.
//
// The resource key and the cache-map key are the SAME string on purpose:
// `invalidate()` deletes by the resource key, so any divergence leaves the cached
// value behind. (Previously the key was `chain:<baseName>` while the cache stored
// `<baseName>`, so `invalidateChainResolve` dropped only the in-flight request and
// never the value the README says it drops.)
export function chainResolveCacheKey(environment: Environment, identifier: string, tld: string): string {
  return `chain:${environment.id}:${dotNsService.baseNameOf(identifier, tld)}`;
}

// Pure chain-resolution cache: resolves an identifier from the chain into memory
// with NO DB read. It is only ever invoked for products that are NOT committed —
// committed products are served from `productsResource` (the live DB) via
// `usePersistedProduct*`, so a committed product never lands in this cache and
// the two can't duplicate or diverge. 1-minute staleness bounds repeat chain
// hits while a long-lived UI keeps an uncommitted identifier mounted.
export const chainResolveResource = createQueryResource<{ identifier: string; environment: Environment; tld: string }>({
  key: ({ identifier, environment, tld }) => chainResolveCacheKey(environment, identifier, tld),
})
  .request<Product | null>(({ identifier, environment, tld }) =>
    readProductFromChain(environment, dotNsService.baseNameOf(identifier, tld)),
  )
  .mock(() => null)
  .timeout(60_000)
  .cache<Record<string, Product | null>>({
    staleAfter: 60_000,
    initial: {},
    map(cache, value, { identifier, environment, tld }) {
      return { ...cache, [chainResolveCacheKey(environment, identifier, tld)]: value };
    },
  })
  .build();

// Evicts the chain-resolve entry for an identifier. Called by commitment use
// cases once an identifier becomes committed, so the now-redundant in-memory
// chain copy is dropped and reads fall through to the live DB row.
export function invalidateChainResolve(environment: Environment, identifier: string, tld: string): void {
  chainResolveResource.invalidate({ identifier, environment, tld });
}
