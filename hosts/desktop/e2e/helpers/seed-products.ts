import { type Page } from '@playwright/test';

import { RECENT_PRODUCTS_STORAGE_KEY } from '@/domains/product/recents/constants';

import { DEFAULT_TIMEOUT } from './timeouts';

declare global {
  // eslint-disable-next-line @typescript-eslint/consistent-type-definitions -- augmenting the lib `Window` interface requires `interface`
  interface Window {
    /** Test-only grant hook wired in `src/bootstrap.ts` under AUTOTEST/e2e. */
    __grantRemoteUrlPermission?: (productId: string, url: string) => Promise<void>;
  }
}

/**
 * Pre-launch product seeding for the e2e suite.
 *
 * The no-auth `browser` project (and a fresh authenticated session) persists NO
 * products, so any case depending on recents / favorites / a pinned new-tab grid
 * / a pre-existing dashboard widget is otherwise unautomatable. These helpers
 * write directly into the app's Dexie database (`polkadot-desktop-app-v1`) and
 * the address-bar recents localStorage, so the renderer reads them on its next
 * boot exactly as if the user had committed the products themselves.
 *
 * Persistence facts (verified against `src/`):
 *  - `database.products` keyPath = `baseName`; rows survive boot because
 *    `reconcileUnpinnedProducts` only re-resolves metadata against the chain
 *    (a no-op offline) and NEVER deletes a row.
 *  - `dashboardLayouts` keyPath = `id` (the main layout is `'main'`).
 *  - The address-bar / new-tab recents state hydrates from localStorage ONCE at
 *    module init (`persistLocalStorage`, no storage-event listener).
 *
 * IMPORTANT — always reload after seeding. These helpers write through a raw
 * IndexedDB connection, which Dexie's `liveQuery` (behind `productsResource` /
 * `subscribeToMain`) does NOT observe, and the recents state only re-reads
 * localStorage on a fresh boot. So a seed is only picked up after a `page.reload()`
 * (the browser project then re-skips onboarding; an authenticated session reloads
 * straight back to /dashboard). The reload re-reads every table from disk fresh.
 *
 * All state lands in the per-test fresh `userDataDir`, so the suite's existing
 * `clearAppData()` (fresh dir per test) cleans it up — nothing leaks between tests.
 */

const APP_DB_NAME = 'polkadot-desktop-app-v1';

// Physical localStorage key the recents state reads on boot:
// `${LOCAL_STORAGE_PREFIX}${stateKey}_value`, where the prefix is `polkadot_`
// (@novasamatech/storage-adapter) and the state key is the domain's own — taken
// from the domain rather than restated, so a rename lands here as a type error
// instead of as three silently empty test assertions.
const RECENTS_LS_KEY = `polkadot_${RECENT_PRODUCTS_STORAGE_KEY}_value`;

const MAX_WIDGET_WIDTH = 4;
const MAX_WIDGET_HEIGHT = 8;

export type SeedProductSpec = {
  /** dotNS base name and primary key, e.g. `coinflipgame03.dot`. */
  baseName: string;
  /** Display name shown in lists; defaults to the base name. */
  displayName?: string;
  /**
   * Whether the product carries a `widget` executable. Default `true`.
   * Set `false` to drive `ProductWidgetBody`'s "Domain not found" state — with
   * no widget executable, `loadExecutableArchive` returns null synchronously.
   */
  withWidget?: boolean;
};

type ProductExecutableRecord = {
  kind: string;
  identifier: string;
  contenthash: string;
  appVersion: number[];
  dimensions?: { height: number[]; width?: number };
};

type ProductRecord = {
  baseName: string;
  displayName: string;
  description: string;
  icon: { cid: string; format: string };
  executables: { app?: ProductExecutableRecord; widget?: ProductExecutableRecord; worker?: ProductExecutableRecord };
  pinned: boolean;
  createdAt: number;
  updatedAt: number;
};

type DashboardCardRecord = {
  i: string;
  x: number;
  y: number;
  w: number;
  h: number;
  minW: number;
  maxW: number;
  minH: number;
  maxH: number;
  resizeHandles: string[];
  payload: { kind: string; productId: string };
};

// A deterministic 32-byte hex contenthash — never resolves to real IPFS content
// (these seeds are read-only fixtures), but is shaped like a real one so the
// storage-boundary reconstruction in `productDb.rowToPersisted` succeeds.
const DUMMY_CONTENTHASH = `0x${'ab'.repeat(32)}`;

function buildProductRecord(spec: SeedProductSpec): ProductRecord {
  const now = Date.now();
  const withWidget = spec.withWidget ?? true;
  const app: ProductExecutableRecord = {
    kind: 'app',
    identifier: spec.baseName,
    contenthash: DUMMY_CONTENTHASH,
    appVersion: [1, 0, 0],
  };
  const widget: ProductExecutableRecord = {
    kind: 'widget',
    identifier: `widget.${spec.baseName}`,
    contenthash: DUMMY_CONTENTHASH,
    appVersion: [1, 0, 0],
    dimensions: { height: [2, 4], width: 2 },
  };

  return {
    baseName: spec.baseName,
    displayName: spec.displayName ?? spec.baseName,
    description: '',
    icon: { cid: '', format: 'png' },
    executables: withWidget ? { app, widget } : { app },
    pinned: false,
    createdAt: now,
    updatedAt: now,
  };
}

function buildWidgetCard(productId: string): DashboardCardRecord {
  return {
    i: productId,
    x: 0,
    y: 0,
    w: 2,
    h: 4,
    minW: 1,
    maxW: MAX_WIDGET_WIDTH,
    minH: 4,
    maxH: MAX_WIDGET_HEIGHT,
    resizeHandles: ['s', 'w', 'e', 'n', 'sw', 'nw', 'se', 'ne'],
    payload: { kind: 'product:widget', productId },
  };
}

// Open the existing app DB at its CURRENT version (no version arg → no upgrade
// triggered, all object stores present) and `put` each record into `storeName`.
// The app may hold its own connection open concurrently; IndexedDB allows that.
async function idbPut(page: Page, storeName: string, records: unknown[]): Promise<void> {
  await page.evaluate(
    async ({ dbName, store, rows }) => {
      const db = await new Promise<IDBDatabase>((resolve, reject) => {
        const request = indexedDB.open(dbName);
        request.onsuccess = () => resolve(request.result);
        request.onerror = () => reject(request.error);
      });
      try {
        if (!db.objectStoreNames.contains(store)) {
          throw new Error(`Object store "${store}" not found in "${dbName}" — seed after the app has booted.`);
        }
        await new Promise<void>((resolve, reject) => {
          const tx = db.transaction(store, 'readwrite');
          tx.oncomplete = () => resolve();
          tx.onerror = () => reject(tx.error);
          tx.onabort = () => reject(tx.error);
          const objectStore = tx.objectStore(store);
          for (const row of rows) objectStore.put(row);
        });
      } finally {
        db.close();
      }
    },
    { dbName: APP_DB_NAME, store: storeName, rows: records },
  );
}

/** Wait until the renderer has navigated off `about:blank` so storage is reachable. */
async function waitForRenderer(page: Page): Promise<void> {
  await page.waitForURL(/^(?!about:blank)/, { timeout: DEFAULT_TIMEOUT });
}

/** Seed N committed products into `database.products`. */
export async function seedProducts(page: Page, specs: SeedProductSpec[]): Promise<void> {
  await waitForRenderer(page);
  await idbPut(page, 'products', specs.map(buildProductRecord));
}

/** Seed the address-bar / new-tab recents list (localStorage). Needs a reload to take effect. */
export async function seedAddressBarRecents(page: Page, baseNames: string[]): Promise<void> {
  await waitForRenderer(page);
  await page.evaluate(
    ({ key, ids }) => {
      localStorage.setItem(key, JSON.stringify(ids));
    },
    { key: RECENTS_LS_KEY, ids: baseNames },
  );
}

/**
 * Grant a product the external-URL permission for `pattern`.
 *
 * e2e builds pass `promptForUnmatchedRemoteAccess: false` (`src/bootstrap.ts`), so an
 * unmatched URL is denied with no dialog. A test about a permitted external link must
 * establish the grant itself. Permissions are core-owned, so this grants through the
 * core via `window.__grantRemoteUrlPermission` rather than seeding storage by hand. No
 * reload needed: the core holds the grant the moment it is set.
 */
export async function seedRemoteUrlPermission(page: Page, productId: string, pattern: string): Promise<void> {
  await waitForRenderer(page);
  // The hook is wired late in `bootstrap` (after async work), so wait for it.
  await page.waitForFunction(() => typeof window.__grantRemoteUrlPermission === 'function', { timeout: DEFAULT_TIMEOUT });
  await page.evaluate(({ id, url }) => window.__grantRemoteUrlPermission?.(id, url), { id: productId, url: pattern });
}

/** Seed the main dashboard layout with a single product-widget card. */
export async function seedDashboardWidget(page: Page, productId: string): Promise<void> {
  await waitForRenderer(page);
  await idbPut(page, 'dashboardLayouts', [
    { id: 'main', pages: [[buildWidgetCard(productId)]], activePageIndex: 0, updatedAt: Date.now() },
  ]);
}

/** Seed the main dashboard layout with one product-widget card per page (≥2 pages → pagination). */
export async function seedDashboardPages(page: Page, productIdsPerPage: string[]): Promise<void> {
  await waitForRenderer(page);
  const pages = productIdsPerPage.map(productId => [buildWidgetCard(productId)]);
  await idbPut(page, 'dashboardLayouts', [{ id: 'main', pages, activePageIndex: 0, updatedAt: Date.now() }]);
}
