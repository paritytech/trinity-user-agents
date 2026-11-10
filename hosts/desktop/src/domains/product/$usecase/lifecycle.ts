import { createSideEffect } from '@/shared/di';
import { isElectron } from '@/shared/env';
import { EXECUTABLE_KINDS, invalidateExecutableArchive, productDb } from '../product';
import { declinedUpdatesRepository } from '../product/declined-updates/repository';

import { coreStorageUseCase } from './coreStorage';
import { dotNsUseCase } from './dotns';
import { offlineCacheUseCase } from './offlineCache';
import { permissionsUseCase } from './permissions';

const onProductForgottenSideEffect = createSideEffect<{ productId: string }>({
  name: 'onProductForgotten',
});

// Product-internal teardown: drop the row, its permissions and cached archives,
// then clear the sandbox. Dashboard detachment is a cross-domain
// concern owned by the product-management aggregate, which calls this after it
// has removed the product from the dashboard.
async function purgeProduct(productId: string): Promise<boolean> {
  // Read the row's executables BEFORE deleting so we can pass each executable's
  // contenthash to invalidateExecutableArchive for precise cache eviction.
  const existingResult = await productDb.getByBaseName(productId);
  const existing = existingResult.isOk() ? existingResult.value : null;

  // Drop per-executable archive caches before the Dexie delete so the
  // productsResource liveQuery and the archive cache emit the post-forget
  // state in the same tick.
  for (const kind of EXECUTABLE_KINDS) {
    const executable = existing?.executables[kind];
    if (executable?.contenthash) {
      invalidateExecutableArchive(productId, kind, executable.contenthash);
    } else {
      invalidateExecutableArchive(productId, kind);
    }
  }

  const tld = await dotNsUseCase.getActiveTld();

  const [, , deleteResult] = await Promise.all([
    permissionsUseCase.clearProductPermissions({ productId, tld }),
    // Permissions go through the core so its in-memory copy drops with the slot. The
    // sweep then catches what no core API clears: AutoSigningKey and ProductSubtree
    // slots, plus any permission slot for a product the core never opened this run.
    coreStorageUseCase.clearProductSlots({ productId, tld }),
    productDb.delete(productId),
    offlineCacheUseCase.evictArchives(productId),
    // Best-effort: a failing declined-updates wipe must not abort the rest of
    // the reset, otherwise declined rows would leak past a forget.
    declinedUpdatesRepository.deleteByBaseName(productId).catch(error => {
      console.warn('deleteByBaseName (declinedUpdates) failed for', productId, error);
    }),
  ]);

  if (isElectron()) {
    void window.App.clearProductSandboxData(productId).catch(error => {
      console.warn('clearProductSandboxData failed for', productId, error);
    });
  }

  const success = deleteResult.isOk();
  if (success) {
    onProductForgottenSideEffect.apply({ productId });
  }

  return success;
}

function refreshProduct(domain: string) {
  for (const kind of EXECUTABLE_KINDS) {
    invalidateExecutableArchive(domain, kind);
  }
}

async function clearProductCache(productId: string): Promise<void> {
  if (isElectron()) {
    await window.App.clearProductData(productId);
  }
}

export const lifecycleUseCase = {
  onProductForgottenSideEffect,
  purgeProduct,
  refreshProduct,
  clearProductCache,
};
