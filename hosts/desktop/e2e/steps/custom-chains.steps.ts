import { createBdd } from 'playwright-bdd';

import { authenticatedTest, expect } from '../fixtures/authenticated';
import { DEFAULT_TIMEOUT } from '../helpers/timeouts';
import { CustomChainsPage } from '../page-objects/CustomChainsPage';
import { DashboardPage } from '../page-objects/DashboardPage';

const { When, Then } = createBdd(authenticatedTest);

When('the user opens the custom chains settings', async ({ authenticatedApp }) => {
  const dashboard = new DashboardPage(authenticatedApp.window);
  await dashboard.openSettings();
  await authenticatedApp.window.waitForURL(/settings/, { timeout: DEFAULT_TIMEOUT });
  const link = authenticatedApp.window.getByRole('link', { name: 'Custom chains', exact: true });
  await expect(link).toBeVisible({ timeout: DEFAULT_TIMEOUT });
  await link.click();
  await authenticatedApp.window.waitForURL(/settings\/development\/custom-chains/, { timeout: DEFAULT_TIMEOUT });
});

When('the user enters the custom chain endpoint {string}', async ({ authenticatedApp }, url: string) => {
  await new CustomChainsPage(authenticatedApp.window).enterEndpoint(url);
});

When('the user submits the custom chain', async ({ authenticatedApp }) => {
  await new CustomChainsPage(authenticatedApp.window).submit();
});

Then('a custom chain error {string} is shown', async ({ authenticatedApp }, title: string) => {
  await new CustomChainsPage(authenticatedApp.window).expectError(title);
});
