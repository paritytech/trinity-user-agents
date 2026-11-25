import fs from 'fs/promises';

import { type Page } from '@playwright/test';

import { DEFAULT_TIMEOUT } from './timeouts';

/**
 * The live unified app database.
 *
 * Mirrors `APP_DB_NAME` in `src/shared/database/schema.ts`; that module cannot be imported
 * here because it builds its Dexie instance at module scope.
 */
export const APP_DB_NAME = 'polkadot-desktop-app-v1';

/** The chat domain's standalone databases — not part of the unified DB. */
export const CHAT_DB_NAMES = ['p2p-chat', 'products-chat'] as const;

/**
 * Return the app to a signed-out, empty state, then reload onto it.
 *
 * This MUST clear `coreStorage`, which is where the TrUAPI core keeps the auth session.
 * It is the one difference from the per-test soft reset in `reset-state.ts`, which
 * deliberately preserves that store to keep the worker signed in.
 *
 * Rows are cleared rather than the databases dropped: the app holds an open Dexie
 * connection, and `deleteDatabase` against it fires `blocked` and waits for a close that
 * does not come until the reload below — which is too late for the caller.
 */
export async function clearAppData(page: Page): Promise<void> {
  // Wait for the app to navigate away from about:blank, where localStorage is inaccessible
  await page.waitForURL(/^(?!about:blank)/, { timeout: DEFAULT_TIMEOUT });

  // Custom Electron protocols may have opaque origin — storage access can be denied by
  // Chromium even when the URL passes the about:blank guard, so every access is guarded.
  await page.evaluate(
    async (dbNames: string[]) => {
      try {
        localStorage.clear();
      } catch {
        /* opaque origin */
      }
      try {
        sessionStorage.clear();
      } catch {
        /* opaque origin */
      }

      const clearDatabase = (name: string) =>
        new Promise<void>(resolve => {
          let request: IDBOpenDBRequest;
          try {
            request = indexedDB.open(name);
          } catch {
            resolve();

            return;
          }
          // No version is requested, so no upgrade can be pending; `blocked` here would mean
          // waiting on something that will never resolve.
          request.onblocked = () => resolve();
          request.onerror = () => resolve();
          request.onsuccess = () => {
            const db = request.result;
            const stores = Array.from(db.objectStoreNames);
            if (stores.length === 0) {
              db.close();
              resolve();

              return;
            }
            let tx: IDBTransaction;
            try {
              tx = db.transaction(stores, 'readwrite');
            } catch {
              db.close();
              resolve();

              return;
            }
            for (const store of stores) {
              try {
                tx.objectStore(store).clear();
              } catch {
                /* store vanished mid-transaction */
              }
            }
            const settle = () => {
              db.close();
              resolve();
            };
            tx.oncomplete = settle;
            tx.onerror = settle;
            tx.onabort = settle;
          };
        });

      // Only touch databases that exist — `indexedDB.open` would otherwise create each
      // missing one as an empty database.
      let targets = dbNames;
      try {
        const present = new Set((await indexedDB.databases()).map(info => info.name));
        targets = dbNames.filter(name => present.has(name));
      } catch {
        /* `databases()` unsupported — fall back to attempting every name */
      }

      await Promise.all(targets.map(clearDatabase));
    },
    [APP_DB_NAME, ...CHAT_DB_NAMES],
  );

  // Reload so the app starts fresh with empty state
  await page.reload({ waitUntil: 'domcontentloaded' });
}

/**
 * Delete and recreate an Electron `userDataDir` between launches.
 *
 * {@link clearAppData} cannot sign an app out, and no amount of storage clearing will:
 * the TrUAPI core holds the session in memory, so the reload just re-attaches to it.
 * Measured directly — after `clearAppData` the `coreStorage` table reads 0 rows while the
 * window is still on `#/dashboard`. A relaunch into the same profile is no better, because
 * the core restores the session from disk before anything gets a chance to clear it.
 *
 * So a caller that needs the next launch to show onboarding MUST wipe the profile here,
 * while no process holds it, between shutting the app down and starting it again.
 */
export async function resetUserDataDir(dir: string): Promise<void> {
  await fs.rm(dir, { recursive: true, force: true });
  await fs.mkdir(dir, { recursive: true });
}
