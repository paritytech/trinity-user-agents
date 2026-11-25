import { createBdd } from 'playwright-bdd';

import { authenticatedTest } from '../fixtures/authenticated';
import { DEFAULT_TIMEOUT } from '../helpers/timeouts';
import { DashboardPage } from '../page-objects/DashboardPage';
import { SettingsPage } from '../page-objects/SettingsPage';

const { When, Then } = createBdd(authenticatedTest);

When('the user opens settings', async ({ authenticatedApp }) => {
  const dashboard = new DashboardPage(authenticatedApp.window);
  await dashboard.openSettings();
  await authenticatedApp.window.waitForURL(/settings/, { timeout: DEFAULT_TIMEOUT });
});

When('the user opens the settings page {string}', async ({ authenticatedApp }, name: string) => {
  await new SettingsPage(authenticatedApp.window).openNav(name);
});

When('the user navigates settings back', async ({ authenticatedApp }) => {
  await new SettingsPage(authenticatedApp.window).back();
});

When('the user navigates settings forward', async ({ authenticatedApp }) => {
  await new SettingsPage(authenticatedApp.window).forward();
});

Then(
  'the settings sidebar shows the {string}, {string} and {string} groups',
  async ({ authenticatedApp }, a: string, b: string, c: string) => {
    await new SettingsPage(authenticatedApp.window).expectGroupsVisible([a, b, c]);
  },
);

Then('the active settings page is {string}', async ({ authenticatedApp }, path: string) => {
  await new SettingsPage(authenticatedApp.window).expectActivePage(path);
});
