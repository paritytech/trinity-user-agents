import { createBdd } from 'playwright-bdd';

import { authenticatedTest, expect } from '../fixtures/authenticated';
import { DEFAULT_TIMEOUT } from '../helpers/timeouts';
import { DashboardPage } from '../page-objects/DashboardPage';

const { When, Then } = createBdd(authenticatedTest);

When('the user toggles the theme to dark', async ({ authenticatedApp }) => {
  const dashboard = new DashboardPage(authenticatedApp.window);
  await dashboard.setTheme('Night');
  await authenticatedApp.window.waitForTimeout(500);
});

When('the user opens the appearance settings', async ({ authenticatedApp }) => {
  const dashboard = new DashboardPage(authenticatedApp.window);
  await dashboard.openSettings();
  await authenticatedApp.window.waitForURL(/settings/, { timeout: DEFAULT_TIMEOUT });
  const appearanceLink = authenticatedApp.window.getByRole('link', { name: 'Appearance', exact: true });
  await expect(appearanceLink).toBeVisible({ timeout: DEFAULT_TIMEOUT });
  await appearanceLink.click();
});

Then('the color mode options Device, Day and Night are available', async ({ authenticatedApp }) => {
  for (const name of ['Device', 'Day', 'Night']) {
    await expect(authenticatedApp.window.getByRole('radio', { name, exact: true })).toBeVisible({
      timeout: DEFAULT_TIMEOUT,
    });
  }
});

When('the user selects the {string} color mode', async ({ authenticatedApp }, name: string) => {
  const radio = authenticatedApp.window.getByRole('radio', { name, exact: true });
  await expect(radio).toBeVisible({ timeout: DEFAULT_TIMEOUT });
  await radio.click();
});

Then('the {string} color mode is selected', async ({ authenticatedApp }, name: string) => {
  await expect(authenticatedApp.window.getByRole('radio', { name, exact: true })).toBeChecked({
    timeout: DEFAULT_TIMEOUT,
  });
});

Then('the authenticated dashboard screenshot is taken as {string}', async ({ authenticatedApp, $testInfo }, name: string) => {
  const dashboard = new DashboardPage(authenticatedApp.window);
  await dashboard.takeScreenshot($testInfo, `${name}-${process.platform}`);
});
