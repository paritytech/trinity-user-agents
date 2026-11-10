import { createBdd } from 'playwright-bdd';

import { authenticatedTest, expect } from '../fixtures/authenticated';
import { DEFAULT_TIMEOUT, LONG_TIMEOUT } from '../helpers/timeouts';
import { evaluateInWebview } from '../helpers/webview';
import { BrowserPage } from '../page-objects/BrowserPage';
import { ProductActionsPage } from '../page-objects/ProductActionsPage';

const { Given, When, Then } = createBdd(authenticatedTest);

// "the user opens {string} in a new tab" and "the user opens the product actions
// menu" are shared from offline-access.steps (same authenticated project).

Then('the product actions menu is open', async ({ authenticatedApp }) => {
  await expect(new ProductActionsPage(authenticatedApp.window).openSettingsMenuItem).toBeVisible({
    timeout: DEFAULT_TIMEOUT,
  });
});

When('the user opens product settings from the actions menu', async ({ authenticatedApp }) => {
  await new ProductActionsPage(authenticatedApp.window).openProductSettings();
});

Then('the app navigates to the product settings page', async ({ authenticatedApp }) => {
  await authenticatedApp.window.waitForURL(/settings\/privacy\/apps\//, { timeout: DEFAULT_TIMEOUT });
});

Then('the actions menu offers {string}', async ({ authenticatedApp }, label: string) => {
  await expect(authenticatedApp.window.getByText(label, { exact: true })).toBeVisible({ timeout: DEFAULT_TIMEOUT });
});

When('the user adds the product to favorites from the actions menu', async ({ authenticatedApp }) => {
  await new ProductActionsPage(authenticatedApp.window).clickAddToFavorites();
});

When('the user removes the product from favorites from the actions menu', async ({ authenticatedApp }) => {
  await new ProductActionsPage(authenticatedApp.window).clickRemoveFromFavorites();
});

// --- TC-5.1.3 Reload an open product / TC-5.1.4 SPA content stays visible -----

Given('a reload probe is set in the open product', async ({ authenticatedApp }) => {
  // Wait for the product webview to attach + load before seeding the probe — the
  // webview mounts a beat after the host route settles.
  await new BrowserPage(authenticatedApp.window, authenticatedApp.app).expectActiveTabHasContent();
  await evaluateInWebview(authenticatedApp.window, 'window.__e2eReloadProbe = "set"; true');
});

When('the user reloads the open product via the address bar control', async ({ authenticatedApp }) => {
  await new BrowserPage(authenticatedApp.window, authenticatedApp.app).reloadActiveProductViaControl();
});

Then('the open product reload probe is cleared', async ({ authenticatedApp }) => {
  // Reloading the guest discards window state, so the probe set before the
  // reload is gone once the document reloads. Poll because executeJavaScript can
  // reject mid-reload.
  await expect
    .poll(
      async () => {
        try {
          return await evaluateInWebview(authenticatedApp.window, 'window.__e2eReloadProbe ?? null');
        } catch {
          return 'pending';
        }
      },
      { timeout: LONG_TIMEOUT },
    )
    .toBeNull();
});

Then('the open product content remains visible after settling', async ({ authenticatedApp }) => {
  await new BrowserPage(authenticatedApp.window, authenticatedApp.app).expectActiveTabKeepsContent();
});
