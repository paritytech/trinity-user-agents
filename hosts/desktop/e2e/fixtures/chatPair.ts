import fs from 'fs/promises';
import os from 'os';
import path from 'path';

import { type TestInfo } from '@playwright/test';
import { test as bddTest } from 'playwright-bdd';

import { attachFailureConsole, recordRendererConsole, shutdownElectronApp } from '../helpers/artifacts';
import { clearAppData, resetUserDataDir } from '../helpers/cleanup';
import { type ElectronAppContext, launchElectronApp } from '../helpers/electron';
import { type E2eEnvironmentId, envToNetwork } from '../helpers/environment';
import { errorMessage } from '../helpers/errors';
import { findRegistrations } from '../helpers/identity-backend';
import { waitForDashboardOrStuck, withSignInRetries } from '../helpers/sign-in';
import { SigningHost, slotPath } from '../helpers/signing-host';
import { waitForIdle } from '../helpers/wait';
import { OnboardingPage } from '../page-objects/OnboardingPage';

import { setupPlatformParameter } from './allure-metadata';

const CHAT_PAIR_ENVIRONMENT_ID: E2eEnvironmentId = 'nightly';
const CHAT_PAIR_NETWORK = envToNetwork(CHAT_PAIR_ENVIRONMENT_ID);

/**
 * A paired identity — one authenticated Electron instance plus the lite username
 * its signer registered.
 *
 * `liteUsername` is the on-chain registration WITH the backend-assigned numeric
 * index ("truapitest.15") — the exact string the app's contact search renders. UI
 * lookups must use it whole: the bare base name is a substring of every
 * registration of that name, so it stops being a unique match the moment the
 * backend holds a duplicate registration.
 */
export type PairIdentity = {
  app: ElectronAppContext;
  liteUsername: string;
  /**
   * Whether the identity registry holds a registration for {@link liteUsername}.
   *
   * `null` means the registry could not answer (unset `E2E_IDENTITY_URL`, a failed
   * request) and MUST NOT be read as "not registered" — see `identity-backend.ts`. Only
   * `false` is that claim, and it is decisive for this suite: the app's contact search
   * reads the same registry, so an unregistered peer can never appear in a result list.
   */
  registrationConfirmed: boolean | null;
};

/** Distinct identities per worker, cycled so consecutive scenarios never share one. */
export const CHATPAIR_POOL_SIZE = 3;

export type ChatPairWorkerFixtures = {
  chatPairCounter: { value: number };
};

export type ChatPairTestFixtures = {
  pairHosts: { alice: SigningHost; bob: SigningHost };
  aliceUserDataDir: string;
  bobUserDataDir: string;
  aliceContext: PairIdentity;
  bobContext: PairIdentity;
  alice: PairIdentity;
  bob: PairIdentity;
};

/**
 * `workerIndex` is part of the name, not decoration: `Date.now()` has millisecond
 * resolution and parallel workers reach this line together at the start of a run, so two
 * of them can compute the same path. `mkdir({ recursive: true })` succeeds silently on an
 * existing directory, so the collision would not surface as an error — it would hand two
 * Electron apps one `userDataDir` and let them share a session.
 */
async function createTempDir(prefix: string, workerIndex: number): Promise<string> {
  const dir = path.join(os.tmpdir(), 'polkadot-desktop-e2e', `${prefix}-w${workerIndex}-${Date.now()}`);
  await fs.mkdir(dir, { recursive: true });
  return dir;
}

async function cleanupTempDir(dir: string): Promise<void> {
  try {
    await fs.rm(dir, { recursive: true, force: true });
  } catch (error) {
    console.warn(`[cleanup] temp dir ${dir}: ${errorMessage(error)}`);
  }
}

async function signInOnce(opts: { userDataDir: string; host: SigningHost }): Promise<ElectronAppContext> {
  // `withSignInRetries` calls this again on failure with the same directory, and a profile
  // carrying the previous attempt's session boots straight to /dashboard — no onboarding,
  // no QR. Wiping first makes every attempt start from the same state as the first.
  await resetUserDataDir(opts.userDataDir);
  const context = await launchElectronApp({
    userDataDir: opts.userDataDir,
    autotest: true,
  });
  try {
    await clearAppData(context.window);
    await waitForIdle(context.window);
    const onboarding = new OnboardingPage(context.window);
    await onboarding.selectEnvironment(CHAT_PAIR_ENVIRONMENT_ID);
    await onboarding.waitForQrCode();
    const deeplink = await onboarding.pairingDeeplink();
    await opts.host.pair(deeplink, CHAT_PAIR_NETWORK);
    await waitForDashboardOrStuck(context.window);
    await waitForIdle(context.window);
    return context;
  } catch (err) {
    // If anything in the flow throws, tear down this Electron so the caller
    // can relaunch fresh (state reset, new pairing session).
    await shutdownElectronApp(context).catch(() => {});
    throw err;
  }
}

/**
 * Sign in with retries. Covers the nightly finality race, where the chain reports the
 * identity before it is usable and the next dashboard wait wedges on "Completing
 * pairing…". Each attempt launches a fresh Electron (`signInOnce`), so the retry is a
 * full relaunch after a short settle delay — a single retry isn't always enough when
 * nightly is lagging. See `helpers/sign-in.ts` for the shared policy (also used by the
 * `authenticated` fixture).
 */
async function signIn(opts: { userDataDir: string; host: SigningHost; label: string }): Promise<ElectronAppContext> {
  return withSignInRetries(() => signInOnce(opts), { label: opts.label });
}

async function buildPairContext(opts: { label: string; userDataDir: string; host: SigningHost }): Promise<PairIdentity> {
  const start = Date.now();
  // The username read must follow the sign-in: `liteUsername()` reads the host's own
  // startup output, and `signIn` → `signInOnce` → `host.pair()` is what starts it.
  const app = await signIn({ userDataDir: opts.userDataDir, host: opts.host, label: opts.label });
  const liteUsername = await opts.host.liteUsername();
  const registrations = await findRegistrations(liteUsername);
  const registrationConfirmed = registrations === null ? null : registrations.length > 0;
  console.info(`[${opts.label}] signed in as "${liteUsername}" in ${Math.round((Date.now() - start) / 1000)}s`);
  return { app, liteUsername, registrationConfirmed };
}

async function attachPairScreenshot(identity: PairIdentity, testInfo: TestInfo, label: string): Promise<void> {
  if (testInfo.status === testInfo.expectedStatus) return;
  try {
    const shot = await identity.app.window.screenshot();
    if (shot) await testInfo.attach(`${label}-screenshot`, { body: shot, contentType: 'image/png' });
  } catch {
    // Window may be closed — best-effort.
  }
}

export const chatPairTest = bddTest.extend<ChatPairTestFixtures, ChatPairWorkerFixtures>({
  // Worker-scoped counter: advances on every test (including retries, which
  // must get a fresh slot to avoid inheriting polluted on-chain state from the
  // prior attempt).
  chatPairCounter: [
    // eslint-disable-next-line no-empty-pattern -- Playwright fixture signature requires destructuring
    async ({}, use) => {
      await use({ value: 0 });
    },
    { scope: 'worker' },
  ],

  // Test-scoped: the signer pair this test drives. Its own `chatpair` namespace —
  // `e2e/fixtures/authenticated.ts` also runs under the `chat` project, so its worker
  // fixture already claims `chat-w<index>`.
  pairHosts: async ({ chatPairCounter }, use, testInfo) => {
    // The counter is worker-scoped and restarts at 0 in every worker, so it cannot
    // index the pool on its own — offset by the worker slot or two workers collide.
    const slot = testInfo.parallelIndex * CHATPAIR_POOL_SIZE + (chatPairCounter.value++ % CHATPAIR_POOL_SIZE);
    await use({
      alice: new SigningHost(slotPath('chatpair', slot, 'a')),
      bob: new SigningHost(slotPath('chatpair', slot, 'b')),
    });
  },

  // eslint-disable-next-line no-empty-pattern -- Playwright fixture signature requires destructuring
  aliceUserDataDir: async ({}, use, testInfo) => {
    const dir = await createTempDir('alice', testInfo.workerIndex);
    await use(dir);
    await cleanupTempDir(dir);
  },

  // eslint-disable-next-line no-empty-pattern -- Playwright fixture signature requires destructuring
  bobUserDataDir: async ({}, use, testInfo) => {
    const dir = await createTempDir('bob', testInfo.workerIndex);
    await use(dir);
    await cleanupTempDir(dir);
  },

  aliceContext: async ({ aliceUserDataDir, pairHosts }, use) => {
    const identity = await buildPairContext({
      label: 'Alice',
      userDataDir: aliceUserDataDir,
      host: pairHosts.alice,
    });
    await use(identity);
    await shutdownElectronApp(identity.app);
    // Order matters: the process must be down before its pairings are cleared, and a
    // leaked pairing blocks this identity's rotation once its daily allowance runs out.
    await pairHosts.alice.stop();
    await pairHosts.alice.removeDevices();
  },

  bobContext: async ({ bobUserDataDir, pairHosts }, use) => {
    const identity = await buildPairContext({
      label: 'Bob',
      userDataDir: bobUserDataDir,
      host: pairHosts.bob,
    });
    await use(identity);
    await shutdownElectronApp(identity.app);
    await pairHosts.bob.stop();
    await pairHosts.bob.removeDevices();
  },

  // Console recording starts here rather than in `buildPairContext` so the
  // buffer covers exactly the test body — sign-in chatter would bury it.
  alice: async ({ aliceContext }, use, testInfo) => {
    await setupPlatformParameter();
    const readConsole = recordRendererConsole(aliceContext.app);
    await use(aliceContext);
    await attachPairScreenshot(aliceContext, testInfo, 'alice');
    await attachFailureConsole(readConsole, testInfo, 'alice');
  },

  bob: async ({ bobContext }, use, testInfo) => {
    const readConsole = recordRendererConsole(bobContext.app);
    await use(bobContext);
    await attachPairScreenshot(bobContext, testInfo, 'bob');
    await attachFailureConsole(readConsole, testInfo, 'bob');
  },
});

export { expect } from '@playwright/test';
