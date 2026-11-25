import { expect } from '@playwright/test';
import { createBdd } from 'playwright-bdd';

import { test } from '../fixtures/link-tests';
import { seedRemoteUrlPermission } from '../helpers/seed-products';
import { LONG_TIMEOUT } from '../helpers/timeouts';

const { Given, When, Then } = createBdd(test);

// The grant the host would otherwise prompt for. E2E builds never prompt (see
// `seedRemoteUrlPermission`), so a scenario about a *permitted* external link
// has to establish the permission rather than rely on a dialog.
Given('the link-tests product may open {string} externally', async ({ electronApp, linkTestsTarget }, pattern: string) => {
  await seedRemoteUrlPermission(electronApp.window, linkTestsTarget.identifier, pattern);
});

// Replace shell.openExternal in the main process with a recorder so the test can
// assert the system browser was asked to open a URL — without actually launching one.
When('the system browser opener is stubbed', async ({ electronApp }) => {
  await electronApp.app.evaluate(({ shell }) => {
    const calls: string[] = [];
    // eslint-disable-next-line @typescript-eslint/consistent-type-assertions -- test stub global
    (globalThis as { __openExternalCalls?: string[] }).__openExternalCalls = calls;
    // eslint-disable-next-line @typescript-eslint/consistent-type-assertions -- override to record, not launch
    (shell as { openExternal: (url: string) => Promise<void> }).openExternal = (url: string) => {
      calls.push(url);
      return Promise.resolve();
    };
  });
});

Then('the system browser is asked to open {string}', async ({ electronApp }, url: string) => {
  await expect
    .poll(
      () =>
        electronApp.app.evaluate(
          // eslint-disable-next-line @typescript-eslint/consistent-type-assertions -- test stub global
          () => (globalThis as { __openExternalCalls?: string[] }).__openExternalCalls ?? [],
        ),
      { timeout: LONG_TIMEOUT },
    )
    .toContain(url);
});
