import fs from 'fs/promises';
import os from 'os';
import path from 'path';

import { attachFailureScreenshot, shutdownElectronApp } from '../helpers/artifacts';
import { clearAppData, resetUserDataDir } from '../helpers/cleanup';
import { registerProductDialogHandlers } from '../helpers/dialogs';
import { networkTld } from '../helpers/dotns';
import { type ElectronAppContext, launchElectronApp } from '../helpers/electron';
import { type E2eEnvironmentId, envToNetwork } from '../helpers/environment';
import { errorMessage } from '../helpers/errors';
import {
  type LocalStorageSnapshot,
  type WorkerAuthApp,
  captureLocalStorageBaseline,
  resetToAuthenticatedBaseline,
} from '../helpers/reset-state';
import { signInWithReset, waitForDashboardOrStuck } from '../helpers/sign-in';
import { type SigningHost } from '../helpers/signing-host';
import { waitForIdle } from '../helpers/wait';
import { OnboardingPage } from '../page-objects/OnboardingPage';

import { setupPlatformParameter } from './allure-metadata';
import { test as baseTest } from './base';

export const AUTH_ENVIRONMENT_ID: E2eEnvironmentId = 'nightly';
const AUTH_NETWORK = envToNetwork(AUTH_ENVIRONMENT_ID);

/**
 * The dotNS suffix every signed-in project resolves against. Steps complete a
 * product label with this rather than spelling a suffix, so a scenario names
 * `host-playground` and the environment decides what that is.
 */
export const AUTH_TLD = networkTld(AUTH_ENVIRONMENT_ID);

export type AuthTestFixtures = {
  /**
   * Authenticated Electron context for the test. For ordinary tests this is the
   * shared, worker-scoped app (signed in once per worker) after a per-test
   * soft-reset back to a clean `/dashboard` baseline. For `@isolated` scenarios
   * it is a throwaway fresh Electron with its own sign-in (today's behavior),
   * for tests that mutate/end the session or need a cold start.
   */
  authenticatedApp: ElectronAppContext;
};

export type AuthWorkerFixtures = {
  /**
   * Worker-scoped temp `userDataDir` for the shared authenticated app. Persists
   * across all tests in the worker so the signed-in session survives reloads.
   */
  authenticatedWorkerDataDir: string;

  /**
   * Worker-scoped controller over the shared authenticated Electron app. Lazily
   * launches + signs in on first `ensure()`, so a worker running only
   * `@isolated` tests never pays the sign-in. `relaunchAndSignIn` is the
   * soft-reset's crash/logout fallback.
   */
  authenticatedWorkerApp: WorkerAuthApp & { ensure(): Promise<ElectronAppContext> };
};

async function runSignIn(app: ElectronAppContext, signingHost: SigningHost): Promise<void> {
  const onboarding = new OnboardingPage(app.window);
  await onboarding.selectEnvironment(AUTH_ENVIRONMENT_ID);
  await onboarding.waitForQrCode();
  const deeplink = await onboarding.pairingDeeplink();
  await signingHost.pair(deeplink, AUTH_NETWORK);
  await waitForDashboardOrStuck(app.window);
  await waitForIdle(app.window);
}

/**
 * Sign in with retries. Covers the nightly finality race, where the chain reports the
 * identity before it is usable and the core wedges on "Completing pairing…" — a storage
 * reset plus a fresh attempt clears it.
 */
async function signInWithRetry(app: ElectronAppContext, signingHost: SigningHost): Promise<void> {
  await signInWithReset({
    label: 'auth',
    signingHost,
    attempt: () => runSignIn(app, signingHost),
    beforeRetry: () => clearAppData(app.window),
  });
}

/** Launch a fresh Electron, install the dialog approver, clear state, sign in. */
async function launchAndSignIn(opts: { userDataDir: string; signingHost: SigningHost }): Promise<ElectronAppContext> {
  const app = await launchElectronApp({ userDataDir: opts.userDataDir, autotest: true });
  // Install the (flag-gated) dialog auto-approver once on this page. It rides
  // every navigation via addInitScript, so it survives soft-reset reloads; the
  // per-test reset only flips the enable flag.
  await registerProductDialogHandlers(app.window);
  await clearAppData(app.window);
  await signInWithRetry(app, opts.signingHost);
  return app;
}

export const authenticatedTest = baseTest.extend<AuthTestFixtures, AuthWorkerFixtures>({
  authenticatedWorkerDataDir: [
    // eslint-disable-next-line no-empty-pattern -- Playwright fixture signature requires destructuring
    async ({}, use, workerInfo) => {
      const dir = path.join(os.tmpdir(), 'polkadot-desktop-e2e', `worker-auth-${workerInfo.workerIndex}-${Date.now()}`);
      await fs.mkdir(dir, { recursive: true });
      await use(dir);
      await fs
        .rm(dir, { recursive: true, force: true })
        .catch(err => console.warn(`[cleanup] worker dir ${dir}: ${errorMessage(err)}`));
    },
    { scope: 'worker' },
  ],

  authenticatedWorkerApp: [
    async ({ signingHost, authenticatedWorkerDataDir }, use) => {
      let ctx: ElectronAppContext | null = null;
      // localStorage as it stands immediately after sign-in, before any test has
      // run. The per-test soft-reset restores exactly this, so it never has to
      // name a session key (see helpers/reset-state.ts).
      let storageBaseline: LocalStorageSnapshot = {};

      const launch = async (): Promise<ElectronAppContext> => {
        // No explicit warm-up: `setup-signers` has already provisioned this worker's
        // slot, and a warm slot restores in seconds inside `pair()`.
        const app = await launchAndSignIn({ userDataDir: authenticatedWorkerDataDir, signingHost });
        storageBaseline = await captureLocalStorageBaseline(app.window);

        return app;
      };

      const controller: WorkerAuthApp & { ensure(): Promise<ElectronAppContext> } = {
        ensure: async () => {
          if (!ctx) ctx = await launch();
          return ctx;
        },
        current: () => {
          if (!ctx) throw new Error('authenticatedWorkerApp not initialised — call ensure() first');
          return ctx;
        },
        baseline: () => storageBaseline,
        relaunchAndSignIn: async () => {
          if (ctx) await shutdownElectronApp(ctx).catch(() => {});
          // The profile still holds the dead session, and the core restores it on boot —
          // the relaunched app would come up on /dashboard and never render the onboarding
          // QR this re-sign-in is about to wait for. Wipe it while nothing holds it.
          await resetUserDataDir(authenticatedWorkerDataDir);
          ctx = await launch();
          return ctx;
        },
      };

      await use(controller);

      if (ctx) await shutdownElectronApp(ctx);
    },
    { scope: 'worker' },
  ],

  authenticatedApp: async ({ authenticatedWorkerApp, signingHost, userDataDir, autotest }, use, testInfo) => {
    await setupPlatformParameter();
    const autoApproveDialogs = !testInfo.tags.includes('@manual-permissions');

    // `@isolated`: tests that mutate/end the session (logout) or need a cold
    // start get a throwaway fresh Electron with their own sign-in, leaving the
    // shared worker app untouched.
    if (testInfo.tags.includes('@isolated')) {
      const app = await launchElectronApp({ userDataDir, autotest });
      try {
        if (autoApproveDialogs) await registerProductDialogHandlers(app.window);
        await clearAppData(app.window);
        await signInWithRetry(app, signingHost);
        await use(app);
      } finally {
        await attachFailureScreenshot(app, testInfo);
        await shutdownElectronApp(app);
      }
      return;
    }

    // Ordinary test: reuse the shared worker app after a soft-reset to a clean
    // authenticated baseline. Shutdown happens at worker teardown, not here.
    await authenticatedWorkerApp.ensure();
    const app = await resetToAuthenticatedBaseline(authenticatedWorkerApp, { autoApproveDialogs });
    await use(app);
    await attachFailureScreenshot(app, testInfo);
  },
});

export { expect } from '@playwright/test';
