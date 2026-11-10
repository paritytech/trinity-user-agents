import { type Page } from '@playwright/test';
import { createBdd } from 'playwright-bdd';

import { expect, test } from '../fixtures/base';
import { clearAppData } from '../helpers/cleanup';
import { coreStorageSlotKeys } from '../helpers/core-session';
import { type E2eEnvironmentId, envToNetwork } from '../helpers/environment';
import { signInWithReset, waitForDashboardOrStuck } from '../helpers/sign-in';
import { type SigningHost } from '../helpers/signing-host';
import { DEFAULT_TIMEOUT, VERY_LONG_TIMEOUT } from '../helpers/timeouts';
import { DashboardPage } from '../page-objects/DashboardPage';
import { OnboardingPage } from '../page-objects/OnboardingPage';
import { UserPopover } from '../page-objects/UserPopover';

const { Given, When, Then } = createBdd(test);

function parseEnvironmentId(value: string): E2eEnvironmentId {
  if (value === 'nightly' || value === 'unstable') {
    return value;
  }
  throw new Error(`Unknown environment id: "${value}". Expected nightly | unstable.`);
}

/**
 * Full sign-in against this worker's `truapi-host` signer.
 *
 * Identity comes from the worker's slot directory, not from the `.feature` file: the
 * CLI owns the username and the harness owns the base path. The base name the step
 * still carries is vestigial — see the step definitions below.
 *
 * Ends at /dashboard — the retry path needs the outcome, so callers assert redirection
 * as a cheap re-check.
 */
async function signInAsPermanentUser(
  window: Page,
  signingHost: SigningHost,
  opts: { environmentId: E2eEnvironmentId },
): Promise<void> {
  const network = envToNetwork(opts.environmentId);
  const onboarding = new OnboardingPage(window);

  await signInWithReset({
    label: 'auth',
    signingHost,
    attempt: async () => {
      // Re-selecting an already-selected environment is an idempotent click;
      // after a beforeRetry clearAppData() reload it is required to get back
      // to the QR screen.
      await onboarding.selectEnvironment(opts.environmentId);
      await onboarding.waitForQrCode();
      const deeplink = await onboarding.pairingDeeplink();
      await signingHost.pair(deeplink, network);
      await waitForDashboardOrStuck(window);
    },
    beforeRetry: () => clearAppData(window),
  });
}

When('the user signs in on {string}', async ({ electronApp, signingHost }, environment: string) => {
  await signInAsPermanentUser(electronApp.window, signingHost, {
    environmentId: parseEnvironmentId(environment),
  });
});

Given('the user is signed in on {string}', async ({ electronApp, signingHost }, environment: string) => {
  await signInAsPermanentUser(electronApp.window, signingHost, {
    environmentId: parseEnvironmentId(environment),
  });
});

Given('the user selects the {string} environment', async ({ electronApp }, environment: string) => {
  const onboarding = new OnboardingPage(electronApp.window);
  await onboarding.selectEnvironment(parseEnvironmentId(environment));
});

Then('the selected environment is {string}', async ({ electronApp }, environment: string) => {
  const onboarding = new OnboardingPage(electronApp.window);
  await onboarding.expectSelectedEnvironment(parseEnvironmentId(environment));
});

// Env-agnostic switch: picks whatever other channel the build offers (CI ships only
// `nightly`, local `.env.local` adds `unstable`), and skips when there is nothing to
// switch to — so the scenario never hard-codes a channel that a given build lacks.
When('the user switches to a different environment', async ({ electronApp }) => {
  const onboarding = new OnboardingPage(electronApp.window);
  const ids = await onboarding.availableEnvironmentIds();
  test.skip(ids.length < 2, `only ${ids.length} environment(s) configured — nothing to switch to`);
  await onboarding.switchToDifferentEnvironment();
});

Then('the selected environment changed from {string}', async ({ electronApp }, environment: string) => {
  const onboarding = new OnboardingPage(electronApp.window);
  await onboarding.expectSelectedEnvironmentChangedFrom(parseEnvironmentId(environment));
});

Then('the user is redirected to dashboard', async ({ electronApp }) => {
  await electronApp.window.waitForURL(/dashboard/, { timeout: VERY_LONG_TIMEOUT });
});

Then('session data exists in localStorage', async ({ electronApp }) => {
  // The core persists its session as opaque `coreStorage` slots, not localStorage.
  const slots = await coreStorageSlotKeys(electronApp.window);
  expect(slots.length, 'expected persisted core session slots after sign-in').toBeGreaterThan(0);
});

Then('user info is visible in the top bar', async ({ electronApp }) => {
  const dashboard = new DashboardPage(electronApp.window);
  await expect(dashboard.userButton).toBeVisible({ timeout: DEFAULT_TIMEOUT });
});

When('the user clicks logout', async ({ electronApp }) => {
  const dashboard = new DashboardPage(electronApp.window);
  const popover = new UserPopover(electronApp.window);

  await dashboard.clickUserButton();
  await popover.logout();
});

Then('session data is removed from localStorage', async ({ electronApp }) => {
  // Logout disconnects asynchronously and redirects to onboarding; the navigation can
  // destroy the evaluate execution context mid-call. Retry until the page settles and
  // the core has cleared its session slot.
  await expect(async () => {
    const slots = await coreStorageSlotKeys(electronApp.window);
    expect(slots.length, `expected the core to clear its session slots on logout; slots=${slots.join(',')}`).toBe(0);
  }).toPass({ timeout: DEFAULT_TIMEOUT });
});

Then('user secrets are removed from localStorage', async ({ electronApp }) => {
  // The core keeps no per-session secrets in localStorage; the equivalent check is
  // that nothing survives under the old host-papp prefix either.
  await expect(async () => {
    const leftovers = await electronApp.window.evaluate(() =>
      Object.keys(localStorage).filter(k => k.includes('userSecret') || k.includes('UserSecrets')),
    );
    expect(leftovers.length, `expected no identity secrets in localStorage; keys=${leftovers.join(',')}`).toBe(0);
  }).toPass({ timeout: DEFAULT_TIMEOUT });
});

Then('the user is redirected to onboarding screen', async ({ electronApp }) => {
  await electronApp.window.waitForURL(/onboarding/, { timeout: DEFAULT_TIMEOUT });
});
