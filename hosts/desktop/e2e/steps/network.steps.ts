import { createBdd } from 'playwright-bdd';

import { authenticatedTest, expect } from '../fixtures/authenticated';
import { DEFAULT_TIMEOUT } from '../helpers/timeouts';
import { DashboardPage } from '../page-objects/DashboardPage';
import { NetworkSettingsPage } from '../page-objects/NetworkSettingsPage';

const { When, Then } = createBdd(authenticatedTest);

When('the user opens the testnet endpoint settings', async ({ authenticatedApp }) => {
  const dashboard = new DashboardPage(authenticatedApp.window);
  await dashboard.openSettings();
  await authenticatedApp.window.waitForURL(/settings/, { timeout: DEFAULT_TIMEOUT });
  const link = authenticatedApp.window.getByRole('link', { name: 'Testnet endpoint', exact: true });
  await expect(link).toBeVisible({ timeout: DEFAULT_TIMEOUT });
  await link.click();
  await authenticatedApp.window.waitForURL(/settings\/development\/network/, { timeout: DEFAULT_TIMEOUT });
});

When('the user opens the environment selector', async ({ authenticatedApp }) => {
  await new NetworkSettingsPage(authenticatedApp.window).openSelect();
});

When('the user selects a different environment', async ({ authenticatedApp }) => {
  await new NetworkSettingsPage(authenticatedApp.window).selectDifferentEnvironment();
});

When('the user selects the current environment', async ({ authenticatedApp }) => {
  await new NetworkSettingsPage(authenticatedApp.window).selectCurrentEnvironment();
});

When('the user cancels the network change dialog', async ({ authenticatedApp }) => {
  await new NetworkSettingsPage(authenticatedApp.window).cancelDialog();
});

Then('the environment selector lists at least {int} environments', async ({ authenticatedApp }, n: number) => {
  await new NetworkSettingsPage(authenticatedApp.window).expectOptionCountAtLeast(n);
});

Then('the network change logout dialog is shown', async ({ authenticatedApp }) => {
  await new NetworkSettingsPage(authenticatedApp.window).expectDialogVisible();
});

Then('the network change logout dialog is not shown', async ({ authenticatedApp }) => {
  // Give the dialog a beat to (not) appear before asserting it stayed hidden.
  await authenticatedApp.window.waitForTimeout(500);
  await new NetworkSettingsPage(authenticatedApp.window).expectDialogHidden();
});

Then('the testnet endpoint settings are still shown', async ({ authenticatedApp }) => {
  await new NetworkSettingsPage(authenticatedApp.window).expectStillOnSettings();
});
