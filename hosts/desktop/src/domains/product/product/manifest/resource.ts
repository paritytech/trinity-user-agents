import { namehash } from 'viem';

import { createQueryResource } from '@/shared/resource';
import { type HexString } from '@/shared/types';
import { type Environment } from '@/domains/application';
import { dotNsGateway } from '../../dotns/gateway';
import { dotNsService } from '../../dotns/service';
import { archiveStoreGateway } from '../archive-store/gateway';
import { type Product } from '../types';

import { type ExecutableKind, EXECUTABLE_TEXT_RECORD_KEY } from './constants';
import { archiveGateway } from './gateway';
import { type ExecutableManifest } from './schemas';
import { manifestService } from './service';
import { type Executable, type ExecutableContent, type LiveExecutable } from './types';

// Cache identity per (product, kind, contenthash) — all three are needed so
// a new deployment (new contenthash) bypasses the old cached bytes.
export function archiveCacheKey(baseName: string, kind: ExecutableKind, contenthash: HexString): string {
  return `${baseName}#${kind}#${contenthash}`;
}

// Cache key for a (product, kind) with no executable present. Must stay
// byte-identical across the resource `key`, the cache `map`, and the read hook,
// or the "no executable" entry silently misses.
export function missingArchiveCacheKey(baseName: string, kind: ExecutableKind): string {
  return `${baseName}#${kind}#missing`;
}

// Offline-first load of one executable's archive: disk store, else IPFS, then
// warm main's in-memory cache. A fallback chain is data access, so it lives here
// rather than in a use case — the `warm` call populates main's `polkadot://`
// serving cache so the bytes just read can actually be served, which is part of
// delivering the read, not a domain write.
//
// Disk hit (Electron): workers pull their bytes back into the renderer;
// app/widget are served by the main process over `polkadot://`, so the webview
// only needs origin/domain (empty `files`). Miss: fetch from IPFS, warm main's
// in-memory cache, return the bytes. On web every load is an IPFS fetch.
async function loadExecutableArchive(
  product: Product,
  kind: ExecutableKind,
  ipfsGatewayUrl: string,
): Promise<ExecutableContent | null> {
  const executable = product.executables[kind];
  if (!executable) return null;
  const { identifier, contenthash } = executable;

  if (await archiveStoreGateway.has(identifier, contenthash)) {
    if (kind === 'worker') {
      const stored = await archiveStoreGateway.get(identifier, contenthash);
      if (stored) return { contenthash, archive: { domain: identifier, origin: stored.origin, files: stored.files } };
    } else {
      const origin = dotNsService.generateProductBase(identifier);
      return { contenthash, archive: { domain: identifier, origin, files: {} } };
    }
  }

  // Miss → fetch from IPFS. Unpinned products warm main's in-memory cache only;
  // durable disk persistence happens on pin via `offlineCacheUseCase`.
  const fetched = await archiveGateway.fetchExecutable(product, kind, ipfsGatewayUrl);
  if (!fetched) return null;
  // Only app/widget are served by main's polkadot:// handler out of its in-memory
  // cache, so only they need warming. Workers run in the renderer and consume the
  // returned files directly — warming main with worker bytes it never serves is
  // pure waste, so skip it.
  if (kind !== 'worker') {
    const result = await archiveStoreGateway.warm(fetched.archive);
    if (!result.success) {
      throw new Error(`Failed to register product: ${identifier}. Error: ${result.error}`);
    }
  }

  return fetched;
}

// Per-executable archive read, cached by content. The contenthash is already on
// the Executable (populated at resolve time in `$usecase/resolve.ts`).
//
// `ipfsGatewayUrl` is deliberately absent from the key: the key is
// content-addressed, so the contenthash already names the bytes exactly and which
// gateway served them is not part of the entry's identity. Keying on it would
// refetch byte-identical archives on every environment change (the "parameter the
// key already subsumes" exception in project-structure.md).
export const executableArchiveResource = createQueryResource<{
  product: Product;
  kind: ExecutableKind;
  ipfsGatewayUrl: string;
}>({
  key: ({ product, kind }) => {
    const executable = product.executables[kind];
    return executable
      ? archiveCacheKey(product.baseName, kind, executable.contenthash)
      : missingArchiveCacheKey(product.baseName, kind);
  },
})
  .request<ExecutableContent | null>(({ product, kind, ipfsGatewayUrl }) => loadExecutableArchive(product, kind, ipfsGatewayUrl))
  .mock(() => null)
  .timeout(60_000)
  .cache<Record<string, ExecutableContent | null>>({
    staleAfter: Number.POSITIVE_INFINITY,
    initial: {},
    map(cache, value, { product, kind }) {
      const executable = product.executables[kind];
      const cacheKey = executable
        ? archiveCacheKey(product.baseName, kind, executable.contenthash)
        : missingArchiveCacheKey(product.baseName, kind);
      // Store null too so subsequent mounts for the same (product, kind, contenthash)
      // read "no executable" from cache instead of re-firing the fetch.
      return { ...cache, [cacheKey]: value };
    },
  })
  .build();

// Read the already-cached archive for (product, kind) WITHOUT triggering a fetch.
// Returns null when nothing is cached, or when the cached entry carries no bytes
// (an app/widget entry served from disk is cached as `files: {}`). Lets the pin
// prefetch reuse bytes the user already downloaded by opening the product instead
// of re-fetching the same archive from IPFS.
export function peekExecutableArchive(product: Product, kind: ExecutableKind): ExecutableContent | null {
  const executable = product.executables[kind];
  if (!executable) return null;
  const cached = executableArchiveResource.snapshot()[archiveCacheKey(product.baseName, kind, executable.contenthash)];
  if (!cached || Object.keys(cached.archive.files).length === 0) return null;
  return cached;
}

// Evict the cached archive for a (baseName, kind, contenthash?).
// With contenthash: evicts a single exact entry.
// Without contenthash: evicts ALL entries for (baseName, kind) — useful when
// a product is refreshed and we don't have the old contenthash at hand.
export function invalidateExecutableArchive(baseName: string, kind: ExecutableKind, contenthash?: HexString): void {
  if (contenthash !== undefined) {
    // Single-entry eviction: synthesize a minimal Product whose key function produces this key.
    executableArchiveResource.invalidate({
      // eslint-disable-next-line @typescript-eslint/consistent-type-assertions -- key fn reads only baseName + executables[kind].contenthash
      product: { baseName, executables: { [kind]: { contenthash } } } as Product,
      kind,
    });
    return;
  }

  // Prefix eviction: iterate current cache keys and evict all matching (baseName, kind).
  const prefix = `${baseName}#${kind}#`;
  const currentCache = executableArchiveResource.snapshot();
  for (const cacheKey of Object.keys(currentCache)) {
    if (cacheKey.startsWith(prefix)) {
      // Extract the contenthash from the key suffix and invalidate that entry.
      // eslint-disable-next-line @typescript-eslint/consistent-type-assertions -- slice result is a valid HexString key
      const entryContenthash = cacheKey.slice(prefix.length) as HexString;
      executableArchiveResource.invalidate({
        // eslint-disable-next-line @typescript-eslint/consistent-type-assertions -- key fn reads only baseName + executables[kind].contenthash
        product: { baseName, executables: { [kind]: { contenthash: entryContenthash } } } as Product,
        kind,
      });
    }
  }
}

// Read a node's registry contenthash + parsed executable manifest. Either may be
// null (no resolver, no contenthash, or an unparseable manifest); the manifest
// text read is caught to null so a decode failure doesn't lose the contenthash.
// Exported because the product-level resolution in `../resource.ts` composes it too.
// The active `env` is a parameter — this reads gateways only, never a use case.
export async function readExecutableAt(
  env: Environment,
  node: HexString,
  kind: ExecutableKind,
): Promise<{ contenthash: HexString | null; manifest: ExecutableManifest | null }> {
  const resolver = await dotNsGateway.readResolver(env, node);
  if (!resolver) return { contenthash: null, manifest: null };
  const [contenthash, text] = await Promise.all([
    dotNsGateway.readContentHashAt(env, resolver, node),
    dotNsGateway.readText(env, resolver, node, EXECUTABLE_TEXT_RECORD_KEY).catch(() => null),
  ]);
  return { contenthash, manifest: manifestService.parseExecutableManifest(text, kind) };
}

// Re-resolve one already-frozen executable's current on-chain state into a fresh
// `Executable` (current contenthash + declared version), or null when none can be
// built. Reads the executable's own `identifier` (so a legacy app at the bare base
// resolves), falling back to the legacy contenthash resolver.
// `resolveProductUseCase.resolveFreshExecutable` is the env-resolving wrapper over this.
export async function readFreshExecutable(
  env: Environment,
  baseName: string,
  executable: Executable,
): Promise<Executable | null> {
  const node = namehash(executable.identifier);
  const { contenthash: registry, manifest } = await readExecutableAt(env, node, executable.kind);
  const contenthash = registry ?? (await dotNsGateway.readLegacyContentHash(env, node));
  if (!contenthash) return null;

  if (manifest) return manifestService.executableFromManifest(baseName, manifest, contenthash);
  // No parseable manifest: only `app` is buildable (widget/worker need manifest-only fields).
  return executable.kind === 'app' ? manifestService.legacyApp(executable.identifier, contenthash) : null;
}

// Cache identity for an update-detection read. Environment-scoped because a base
// name resolves differently per dotNS chain. Resource key and cache-map key are the
// same string on purpose — `invalidate()` deletes by the resource key.
export function liveExecutableCacheKey(environment: Environment, baseName: string, kind: ExecutableKind): string {
  return `live:${environment.id}:${baseName}#${kind}`;
}

// Update-detection read for (product, kind): current contenthash + declared
// version, chain-only (no IPFS fetch). Caches `readFreshExecutable` and projects it
// to the minimal `LiveExecutable` the per-modality detection hook needs; null ⇒ no
// update. The active environment arrives as a parameter, so this reads gateways only.
export const liveExecutableResource = createQueryResource<{
  product: Product;
  kind: ExecutableKind;
  environment: Environment;
}>({
  key: ({ product, kind, environment }) => liveExecutableCacheKey(environment, product.baseName, kind),
})
  .request<LiveExecutable | null>(async ({ product, kind, environment }) => {
    const executable = product.executables[kind];
    if (!executable) return null;
    const fresh = await readFreshExecutable(environment, product.baseName, executable);
    return fresh ? { contenthash: fresh.contenthash, version: fresh.appVersion } : null;
  })
  .mock(() => null)
  .timeout(15_000)
  .cache<Record<string, LiveExecutable | null>>({
    staleAfter: 30_000,
    initial: {},
    map(cache, value, { product, kind, environment }) {
      return { ...cache, [liveExecutableCacheKey(environment, product.baseName, kind)]: value };
    },
  })
  .build();
