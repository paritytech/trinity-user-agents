/**
 * Autotest mode — the switches that let an e2e run drive the app.
 *
 * Runtime configuration via the Electron preload bridge (`window.App`):
 *   AUTOTEST=true npm run start:electron
 *
 * For e2e tests these env vars are passed to the Electron process at launch time. No
 * separate build is needed — the same production build serves normal and e2e runs.
 */

const isElectron = typeof window !== 'undefined' && !!window.App;

export const AUTOTEST_ENABLED = isElectron ? !!window.App.autotest : false;
export const E2E_TEST_ENABLED = isElectron ? !!window.App.e2eTest : false;

if (AUTOTEST_ENABLED) {
  console.info('%c[AUTOTEST]%c Mode enabled', 'color: #58a6ff; font-weight: bold', 'color: inherit');
}

declare global {
  // eslint-disable-next-line @typescript-eslint/consistent-type-definitions -- declaration merging requires interface
  interface Window {
    // Test-only: grant a product the external-URL permission through the core, the same
    // path an "Allow always" takes. Wired in `src/bootstrap.ts` under AUTOTEST/e2e only.
    __grantRemoteUrlPermission?: (productId: string, url: string) => Promise<void>;
  }
}
