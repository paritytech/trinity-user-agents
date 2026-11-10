import { environmentUseCase } from '@/domains/application';
import { dotNsService } from '../dotns/service';
import { EXECUTABLE_KINDS } from '../product/manifest/constants';
import { readFreshExecutable } from '../product/manifest/resource';
import { type Executable } from '../product/manifest/types';
import { type PersistedProduct, productDb } from '../product/repository';
import { readProductFromChain } from '../product/resource';
import { type Product } from '../product/types';

import { dotNsUseCase } from './dotns';

// How many unpinned products `reconcileUnpinnedProducts` re-resolves concurrently.
// Each row is ~10 RPC reads, so this caps the launch-time burst on the dotNS endpoint.
const RECONCILE_BATCH_SIZE = 4;

// Read the `Product` fields off a `PersistedProduct`, dropping persistence metadata
// (`pinned`, timestamps) that consumers of the canonical struct shouldn't see.
function recordToProduct(record: PersistedProduct): Product {
  return {
    baseName: record.baseName,
    displayName: record.displayName,
    description: record.description,
    icon: record.icon,
    executables: record.executables,
    ...(record.owner ? { owner: record.owner } : {}),
  };
}

// The canonical chain-resolve primitive: resolve the active environment, then read.
// Exposed on `resolveProductUseCase`, not as a bare export, so other use cases reach
// it through the group surface.
//
// Deliberately UNCACHED — it does not go through `chainResolveResource`. That cache
// holds only *uncommitted* resolutions; `reconcileUnpinnedProducts` below re-resolves
// *committed* rows, so routing this through the resource would put committed products
// into that cache and break the invariant that the two stores cannot diverge.
async function fetchProductFromChain(baseName: string): Promise<Product | null> {
  const env = await environmentUseCase.getActive();

  return readProductFromChain(env, baseName);
}

// Re-resolve one already-frozen executable's current on-chain state. Shared by
// update-detection (`liveExecutableResource`) and the per-modality re-pin
// (`commitment.ts`) so a shown update row is ALWAYS applicable (null ⇒ no row AND
// no apply). Resolves the active environment; the chain read itself is the resource
// module's, so the re-pin path shares one implementation with the cached read.
async function resolveFreshExecutable(baseName: string, executable: Executable): Promise<Executable | null> {
  const env = await environmentUseCase.getActive();

  return readFreshExecutable(env, baseName, executable);
}

// Imperative blend read: the Product for an identifier, preferring the committed
// row and falling back to chain resolution for uncommitted ids. Pure — never
// writes the DB. (Refreshing stale unpinned rows is `reconcileUnpinnedProducts`,
// run on its own trigger; the React equivalent of this read is
// `useDisplayedProduct`.) For non-React callers that need the value once, on demand.
async function resolveProduct(identifier: string): Promise<Product | null> {
  const baseName = dotNsService.baseNameOf(identifier, await dotNsUseCase.getActiveTld());

  const stored = await productDb.getByBaseName(baseName);
  if (stored.isOk() && stored.value) return recordToProduct(stored.value);

  return fetchProductFromChain(baseName);
}

// Re-resolve every committed *unpinned* product against the chain and persist
// any drift. Explicit, owned refresh — run on a defined trigger (app launch),
// NOT as a side effect of viewing a product. Pinned rows are frozen and skipped;
// the row write goes through `productDb.update` (liveQuery re-emits, so the UI
// refreshes), and committed products are never in the chain-resolve cache, so no
// cache invalidation is needed here. Best-effort per row: one failure never
// blocks the others. The chain fan-out is batched (see RECONCILE_BATCH_SIZE) so
// launch doesn't hit the dotNS endpoint with every product at once.
async function reconcileUnpinnedProducts(): Promise<void> {
  const stored = await productDb.getAll();
  if (stored.isErr()) {
    console.warn('[reconcileUnpinnedProducts] could not read products:', stored.error);
    return;
  }

  const unpinned = stored.value.filter(row => !row.pinned);
  for (let i = 0; i < unpinned.length; i += RECONCILE_BATCH_SIZE) {
    await Promise.all(unpinned.slice(i, i + RECONCILE_BATCH_SIZE).map(reconcileRow));
  }
}

async function reconcileRow(row: PersistedProduct): Promise<void> {
  try {
    const fresh = await fetchProductFromChain(row.baseName);
    if (!fresh || !productDiffers(row, fresh)) return;

    const updateResult = await productDb.update(row.baseName, {
      displayName: fresh.displayName,
      description: fresh.description,
      icon: fresh.icon,
      executables: fresh.executables,
      owner: fresh.owner,
      updatedAt: Date.now(),
    });
    if (updateResult.isErr()) {
      console.warn(`[reconcileUnpinnedProducts] update failed for ${row.baseName}:`, updateResult.error);
    }
  } catch (err) {
    console.warn(`[reconcileUnpinnedProducts] re-resolve failed for ${row.baseName}:`, err);
  }
}

function productDiffers(stored: PersistedProduct, fresh: Product): boolean {
  if (stored.displayName !== fresh.displayName) return true;
  if (stored.description !== fresh.description) return true;
  if (stored.icon.cid !== fresh.icon.cid || stored.icon.format !== fresh.icon.format) return true;
  if ((stored.owner ?? null) !== (fresh.owner ?? null)) return true;
  for (const kind of EXECUTABLE_KINDS) {
    const a = stored.executables[kind];
    const b = fresh.executables[kind];
    if (!!a !== !!b) return true;
    if (a && b && a.contenthash !== b.contenthash) return true;
  }
  return false;
}

export const resolveProductUseCase = {
  resolveProduct,
  fetchProductFromChain,
  reconcileUnpinnedProducts,
  resolveFreshExecutable,
};
