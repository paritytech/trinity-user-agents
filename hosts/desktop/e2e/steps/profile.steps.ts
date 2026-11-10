import { createBdd } from 'playwright-bdd';

import { authenticatedTest, expect } from '../fixtures/authenticated';
import { DEFAULT_TIMEOUT } from '../helpers/timeouts';
import { DashboardPage } from '../page-objects/DashboardPage';
import { UserPopover } from '../page-objects/UserPopover';

const { When, Then } = createBdd(authenticatedTest);

When('the user opens the user popover', async ({ authenticatedApp }) => {
  await new DashboardPage(authenticatedApp.window).clickUserButton();
});

When('the user logs out from the user popover', async ({ authenticatedApp }) => {
  await new UserPopover(authenticatedApp.window).logout();
});

Then('the profile popover shows a full username', async ({ authenticatedApp }) => {
  const popover = new UserPopover(authenticatedApp.window);
  await expect(popover.displayName).toBeVisible({ timeout: DEFAULT_TIMEOUT });
  const name = (await popover.getUsername())?.trim() ?? '';
  expect(name.length).toBeGreaterThan(0);
});

Then('the user is returned to the onboarding screen', async ({ authenticatedApp }) => {
  // Logout disconnects asynchronously and redirects to onboarding.
  await authenticatedApp.window.waitForURL(/onboarding/, { timeout: DEFAULT_TIMEOUT });
});
