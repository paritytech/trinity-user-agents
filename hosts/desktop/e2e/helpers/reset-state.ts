import { type Page, expect } from '@playwright/test';

import { TEST_IDS } from '@/shared/test-ids';

import { APP_DB_NAME, CHAT_DB_NAMES } from './cleanup';
import { setDialogAutoApprove } from './dialogs';
import { type ElectronAppContext } from './electron';
import { errorMessage } from './errors';
import { DEFAULT_TIMEOUT, VERY_LONG_TIMEOUT } from './timeouts';

/**
 * Soft-reset the shared, worker-scoped authenticated session back to a clean
 * `/dashboard` baseline between tests — WITHOUT re-running the QR/bot pairing.
 *
 * localStorage is restored to a BASELINE SNAPSHOT captured right after the
 * worker signed in — `captureLocalStorageBaseline` — rather than cleared or
 * preserved key by key. The reset is `clear()` + write the snapshot back.
 *
 * This deliberately names no storage keys at all. Both list-shaped alternatives
 * rot: a list of keys to CLEAR has to grow every time the app persists
 * something new, and when it doesn't the forgotten key silently bleeds state
 * into the next test (recent chats, the chat request preference and the
 * update-check versions had all been leaking that way). A list of keys to
 * PRESERVE has to track host-papp's session keys, which the SDK renames between
 * releases — this file used to document them as `SsoSessionsV3` while the SDK
 * had moved to `SsoSessionsV4`. The snapshot needs neither: whatever a freshly
 * signed-in worker had is by definition the clean authenticated baseline, and
 * the SSO/session layout stays entirely host-papp's business.
 *
 * It is also stricter than a preserve-list — a test that MUTATES a baseline key
 * (theme, settings) is rolled back too, not just one that adds new keys.
 *
 * Health-check + fallback: if after the reload the app is not authenticated on
 * `/dashboard` (session lost, renderer/process dead, or a prior test logged
 * out), we fall back to a full relaunch + re-sign-in so a poisoned previous
 * test can't cascade into the rest of the worker.
 */

// IndexedDB object stores in the unified app DB that hold PER-TEST state.
// Cleared by row-wipe (not DB delete) so the app's open Dexie connection isn't
// disturbed.
//
// `coreStorage` is deliberately ABSENT and MUST STAY ABSENT: the TrUAPI core
// persists the worker's auth session there (`domains/product/core-storage/`).
// Adding it here would log every worker out on the first soft-reset and force a
// full re-sign-in per test. The session is no longer localStorage-only.
const APP_DB_STORES_TO_CLEAR = ['products', 'dashboardLayouts', 'productStorage', 'productExecutableCache'] as const;

// The chat domain keeps its own standalone Dexie DBs. Every object store in
// these holds per-test chat state — wipe all of them.

/** Every localStorage entry, as written by whatever owns it. */
type LocalStorageSnapshot = Record<string, string>;

type WorkerAuthApp = {
  /** The current worker Electron app (may change after a fallback relaunch). */
  current(): ElectronAppContext;
  /**
   * localStorage as it looked right after this worker signed in — the state the
   * per-test reset restores. Re-captured by `relaunchAndSignIn`.
   */
  baseline(): LocalStorageSnapshot;
  /** Tear down the current worker app and launch + re-sign-in a fresh one. */
  relaunchAndSignIn(): Promise<ElectronAppContext>;
};

/** Clear the per-test object stores in the unified app DB by row-wipe. */
async function clearAppDbStores(page: Page): Promise<void> {
  await page.evaluate(
    async ({ dbName, stores }) => {
      const db = await new Promise<IDBDatabase | null>(resolve => {
        const request = indexedDB.open(dbName);
        request.onsuccess = () => resolve(request.result);
        request.onerror = () => resolve(null);
        request.onblocked = () => resolve(null);
      });
      if (!db) return;
      try {
        const present = stores.filter(name => db.objectStoreNames.contains(name));
        if (present.length === 0) return;
        await new Promise<void>(resolve => {
          const tx = db.transaction(present, 'readwrite');
          tx.oncomplete = () => resolve();
          tx.onerror = () => resolve();
          tx.onabort = () => resolve();
          for (const name of present) tx.objectStore(name).clear();
        });
      } finally {
        db.close();
      }
    },
    { dbName: APP_DB_NAME, stores: [...APP_DB_STORES_TO_CLEAR] },
  );
}

/** Clear every object store in each standalone chat DB. */
async function clearChatDbs(page: Page): Promise<void> {
  await page.evaluate(
    async (dbNames: string[]) => {
      await Promise.all(
        dbNames.map(
          dbName =>
            new Promise<void>(resolve => {
              const request = indexedDB.open(dbName);
              request.onerror = () => resolve();
              request.onblocked = () => resolve();
              request.onsuccess = () => {
                const db = request.result;
                const storeNames = Array.from(db.objectStoreNames);
                if (storeNames.length === 0) {
                  db.close();
                  resolve();
                  return;
                }
                const tx = db.transaction(storeNames, 'readwrite');
                tx.oncomplete = () => {
                  db.close();
                  resolve();
                };
                tx.onerror = () => {
                  db.close();
                  resolve();
                };
                tx.onabort = () => {
                  db.close();
                  resolve();
                };
                for (const name of storeNames) tx.objectStore(name).clear();
              };
            }),
        ),
      );
    },
    [...CHAT_DB_NAMES],
  );
}

/**
 * Snapshot localStorage as the clean authenticated baseline. Call once per
 * signed-in worker app, immediately after sign-in and before any test runs —
 * everything present at that moment is session/first-boot state the reset must
 * put back.
 */
export async function captureLocalStorageBaseline(page: Page): Promise<LocalStorageSnapshot> {
  return page.evaluate(() => {
    const snapshot: Record<string, string> = {};
    for (const key of Object.keys(localStorage)) {
      const value = localStorage.getItem(key);
      if (value !== null) snapshot[key] = value;
    }

    return snapshot;
  });
}

/** Roll localStorage back to the baseline snapshot and drop sessionStorage. */
async function restoreWebStorageBaseline(page: Page, baseline: LocalStorageSnapshot): Promise<void> {
  await page.evaluate((snapshot: Record<string, string>) => {
    try {
      localStorage.clear();
      for (const [key, value] of Object.entries(snapshot)) localStorage.setItem(key, value);
    } catch {
      // opaque origin / storage unavailable — best-effort
    }
    try {
      sessionStorage.clear();
    } catch {
      // best-effort
    }
  }, baseline);
}

/** Wait until the renderer is authenticated and back on `/dashboard`. */
async function waitForAuthenticatedDashboard(page: Page, timeout: number): Promise<void> {
  await page.waitForURL(/dashboard/, { timeout });
  await expect(page.getByTestId(TEST_IDS.userButton)).toBeVisible({ timeout });
}

/**
 * Attempt the cheap soft-reset on the current worker page. Throws if the app
 * does not come back authenticated on `/dashboard` (caller falls back).
 */
async function attemptSoftReset(page: Page, baseline: LocalStorageSnapshot, autoApproveDialogs: boolean): Promise<void> {
  await clearAppDbStores(page);
  await clearChatDbs(page);
  await restoreWebStorageBaseline(page, baseline);

  // Set the dialog-approver gate BEFORE reload — survives the reload and the
  // observer (re-installed by addInitScript on reload) reads it.
  await setDialogAutoApprove(page, autoApproveDialogs);

  // Reset the hash route to root so the `/` loader re-runs and routes an
  // authenticated user to /dashboard (a bare reload would keep a stale product
  // tab from the previous scenario).
  await page.evaluate(() => {
    window.location.hash = '#/';
  });

  await page.reload({ waitUntil: 'domcontentloaded' });
  await waitForAuthenticatedDashboard(page, DEFAULT_TIMEOUT);
}

export async function resetToAuthenticatedBaseline(
  workerApp: WorkerAuthApp,
  opts: { autoApproveDialogs: boolean },
): Promise<ElectronAppContext> {
  const app = workerApp.current();

  try {
    await attemptSoftReset(app.window, workerApp.baseline(), opts.autoApproveDialogs);
    return app;
  } catch (err) {
    console.warn(`[reset] soft reset failed (${errorMessage(err)}), relaunching and re-signing in`);
  }

  // Fallback: the previous test left the session dead/logged-out, or the
  // renderer crashed. Relaunch a fresh worker app and re-sign-in.
  const fresh = await workerApp.relaunchAndSignIn();
  // A fresh sign-in lands authenticated on /dashboard already; just set the
  // dialog gate for this test (no reload needed) and confirm readiness.
  await setDialogAutoApprove(fresh.window, opts.autoApproveDialogs);
  await waitForAuthenticatedDashboard(fresh.window, VERY_LONG_TIMEOUT);
  return fresh;
}

export type { LocalStorageSnapshot, WorkerAuthApp };
