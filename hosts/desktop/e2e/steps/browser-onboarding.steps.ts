import { type Page, expect } from '@playwright/test';
import { createBdd } from 'playwright-bdd';

import { TEST_IDS } from '@/shared/test-ids';
import { test } from '../fixtures/link-tests';
import { DEFAULT_TIMEOUT } from '../helpers/timeouts';
import { DashboardPage } from '../page-objects/DashboardPage';

const { When, Then } = createBdd(test);

const hostPathname = (page: Page) =>
  page.evaluate(() => {
    const raw = window.location.hash.replace(/^#/, '') || window.location.pathname;
    try {
      return decodeURIComponent(raw);
    } catch {
      return raw;
    }
  });

When('the user opens settings from the user menu', async ({ electronApp }) => {
  await new DashboardPage(electronApp.window).openSettings();
});

Then('the host is on the settings page', async ({ electronApp }) => {
  await expect.poll(() => hostPathname(electronApp.window), { timeout: DEFAULT_TIMEOUT }).toMatch(/\/settings/);
});

When('the user opens the login flow from the user menu', async ({ electronApp }) => {
  const dashboard = new DashboardPage(electronApp.window);
  await dashboard.clickUserButton();
  // Signed out, the user-popover action labelled "Log In" reuses the logout testid.
  const loginAction = electronApp.window.getByTestId(TEST_IDS.userLogoutButton);
  await expect(loginAction).toBeVisible({ timeout: DEFAULT_TIMEOUT });
  await loginAction.click();
});

When('the user clicks the dashboard home button', async ({ electronApp }) => {
  await electronApp.window.getByTestId(TEST_IDS.homeButton).click({ timeout: DEFAULT_TIMEOUT });
});

Then('the dashboard is ready and not stuck loading', async ({ electronApp }) => {
  await expect.poll(() => hostPathname(electronApp.window), { timeout: DEFAULT_TIMEOUT }).toMatch(/\/dashboard/);
  // The add-widget toolbar button only renders once the dashboard has settled —
  // it never appears if the page is stuck on the loading screen.
  await expect(electronApp.window.getByTestId(TEST_IDS.dashboardAddWidgetButton)).toBeVisible({
    timeout: DEFAULT_TIMEOUT,
  });
});
