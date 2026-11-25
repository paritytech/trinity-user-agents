import { createBdd } from 'playwright-bdd';

import { authenticatedTest } from '../fixtures/authenticated';
import { ProductSettingsPage } from '../page-objects/ProductSettingsPage';

const { When, Then } = createBdd(authenticatedTest);

// Reuses "the user opens product settings from the actions menu" (product-actions.steps).

When('the user clicks the {string} product setting', async ({ authenticatedApp }, name: string) => {
  await new ProductSettingsPage(authenticatedApp.window).clickButton(name);
});

When('the user cancels the product settings dialog', async ({ authenticatedApp }) => {
  await new ProductSettingsPage(authenticatedApp.window).cancelDialog();
});

Then('the product settings dialog is dismissed', async ({ authenticatedApp }) => {
  await new ProductSettingsPage(authenticatedApp.window).expectDialogHidden();
});

Then('the {string} product setting is still available', async ({ authenticatedApp }, name: string) => {
  await new ProductSettingsPage(authenticatedApp.window).expectButtonVisible(name);
});

// --- TC-5.3.1 Clear product cache --------------------------------------------
// "the user clicks the {string} product setting" (above) opens the Clear Cache
// confirm dialog when passed "Clear Cache".

When('the user confirms clearing the cache', async ({ authenticatedApp }) => {
  await new ProductSettingsPage(authenticatedApp.window).confirmClearCache();
});

Then('the product cache is cleared', async ({ authenticatedApp }) => {
  await new ProductSettingsPage(authenticatedApp.window).expectCacheCleared();
});
