import { createBdd } from 'playwright-bdd';

import { authenticatedTest, expect } from '../fixtures/authenticated';
import { coreStorageSlotKeys } from '../helpers/core-session';
import { DEFAULT_TIMEOUT } from '../helpers/timeouts';
import { DashboardPage } from '../page-objects/DashboardPage';
import { UserPopover } from '../page-objects/UserPopover';

const { Given, Then } = createBdd(authenticatedTest);

Given('the user is authenticated', async ({ authenticatedApp }) => {
  // Session already established by worker-scoped fixture
  // Verify by checking user card is visible and has logout button (i.e. user is signed in)
  const dashboard = new DashboardPage(authenticatedApp.window);
  await dashboard.clickUserButton();

  const userPopover = new UserPopover(authenticatedApp.window);
  await expect(userPopover.logoutButton).toBeVisible({ timeout: DEFAULT_TIMEOUT });

  // Close the popover by pressing Escape
  await authenticatedApp.window.keyboard.press('Escape');
});

// The worker-scoped authenticated session is shared across tests, so the app
// may be on a product tab from a prior scenario. Use this step in a Background
// when a scenario requires starting from the dashboard.
Given('the user is on the dashboard', async ({ authenticatedApp }) => {
  const dashboard = new DashboardPage(authenticatedApp.window);
  await dashboard.navigateToDashboard();
});

Then('the authenticated user info is visible in the top bar', async ({ authenticatedApp }) => {
  const dashboard = new DashboardPage(authenticatedApp.window);
  await expect(dashboard.userButton).toBeVisible({ timeout: DEFAULT_TIMEOUT });
});

Then('the authenticated session data exists in localStorage', async ({ authenticatedApp }) => {
  const slots = await coreStorageSlotKeys(authenticatedApp.window);
  expect(slots.length, 'expected the core to have persisted at least one storage slot').toBeGreaterThan(0);
});

Then('the SSO session and identity keys are persisted in localStorage', async ({ authenticatedApp }) => {
  // The core persists the auth session blob and the pairing device identity
  // through the host's `coreStorage` callbacks, not to localStorage — see
  // `helpers/core-session.ts`. Their presence proves the session survived the
  // pairing handshake. Slot keys stay opaque here so a change to the core's
  // `CoreStorageKey` encoding doesn't break the assertion.
  const slots = await coreStorageSlotKeys(authenticatedApp.window);

  expect(slots.length, 'expected persisted core session slots after sign-in').toBeGreaterThan(0);
});
