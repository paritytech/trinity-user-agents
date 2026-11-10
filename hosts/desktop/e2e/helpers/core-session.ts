import { type Page } from '@playwright/test';

/**
 * Reading the TrUAPI core's persisted session.
 *
 * The core owns the session now, and it does NOT persist to localStorage the way
 * host-papp did (`polkadot_Polkadot Desktop_*`). It writes opaque slots — the auth
 * session blob, the pairing device identity, permission authorizations — through
 * the host's `coreStorage` callbacks, which land in the app Dexie DB's
 * `coreStorage` store keyed by the hex of the SCALE-encoded `CoreStorageKey`
 * (`src/domains/product/core-storage/repository.ts`).
 *
 * Slot keys are opaque here on purpose: asserting on a decoded `CoreStorageKey`
 * variant would pin the test to the core's wire encoding, which is upstream's to
 * change. Presence/absence of slots is what the session assertions actually need.
 */

const APP_DB_NAME = 'polkadot-desktop-app-v1';
const CORE_STORAGE_STORE = 'coreStorage';

/** Hex keys of every slot the core currently has persisted. Empty when signed out. */
export async function coreStorageSlotKeys(page: Page): Promise<string[]> {
  return page.evaluate(
    async ({ dbName, storeName }) => {
      const db = await new Promise<IDBDatabase | null>(resolve => {
        const request = indexedDB.open(dbName);
        request.onsuccess = () => resolve(request.result);
        request.onerror = () => resolve(null);
        request.onblocked = () => resolve(null);
      });
      if (!db) return [];

      try {
        // The store only exists from schema v5 on; a DB opened before the upgrade
        // ran has no such store and must read as "no slots", not as a crash.
        if (!db.objectStoreNames.contains(storeName)) return [];

        return await new Promise<string[]>(resolve => {
          const request = db.transaction(storeName, 'readonly').objectStore(storeName).getAllKeys();
          request.onsuccess = () => resolve(request.result.map(String));
          request.onerror = () => resolve([]);
        });
      } finally {
        db.close();
      }
    },
    { dbName: APP_DB_NAME, storeName: CORE_STORAGE_STORE },
  );
}
