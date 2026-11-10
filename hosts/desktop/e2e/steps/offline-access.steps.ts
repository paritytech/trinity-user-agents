import { createBdd } from 'playwright-bdd';

import { AUTH_TLD, authenticatedTest } from '../fixtures/authenticated';
import { productName } from '../helpers/dotns';
import { DEFAULT_TIMEOUT } from '../helpers/timeouts';
import { BrowserPage } from '../page-objects/BrowserPage';
import { ProductActionsPage } from '../page-objects/ProductActionsPage';
import { ProductListSettingsPage } from '../page-objects/ProductListSettingsPage';
import { ProductSettingsPage } from '../page-objects/ProductSettingsPage';

const { Given, When, Then } = createBdd(authenticatedTest);

Given('the user opens {string} in a new tab', async ({ authenticatedApp }, label: string) => {
  const browser = new BrowserPage(authenticatedApp.window, authenticatedApp.app);
  await browser.openProductInNewTab(productName(label, AUTH_TLD));
});

When('the user opens the product actions menu', async ({ authenticatedApp }) => {
  const productActions = new ProductActionsPage(authenticatedApp.window);
  await productActions.openMenu();
});

When('the user selects {string}', async ({ authenticatedApp }, itemLabel: string) => {
  // Route to the correct menu item based on label
  const productActions = new ProductActionsPage(authenticatedApp.window);

  // Both "Enable offline access" and "Remove offline use" render under the same
  // menu-item testid (the label toggles on the pinned state), so both route here.
  if (itemLabel === 'Enable offline access' || itemLabel === 'Remove offline use') {
    await productActions.clickOfflineAccessMenuItem();
  } else {
    throw new Error(`Unknown menu item: "${itemLabel}"`);
  }
});

When('the user confirms the offline access dialog', async ({ authenticatedApp }) => {
  const productActions = new ProductActionsPage(authenticatedApp.window);
  await productActions.confirmEnableOfflineAccess();
});

When('the user confirms the remove offline access dialog', async ({ authenticatedApp }) => {
  const productActions = new ProductActionsPage(authenticatedApp.window);
  await productActions.confirmRemoveOfflineAccess();
});

Then('the pin indicator appears next to the product address', async ({ authenticatedApp }) => {
  const productActions = new ProductActionsPage(authenticatedApp.window);
  await productActions.expectPinIndicatorVisible();
});

Then('the pin indicator disappears from the product address', async ({ authenticatedApp }) => {
  const productActions = new ProductActionsPage(authenticatedApp.window);
  await productActions.expectPinIndicatorHidden();
});

When('the user cancels the offline access dialog', async ({ authenticatedApp }) => {
  const productActions = new ProductActionsPage(authenticatedApp.window);
  await productActions.cancelOfflineAccessDialog();
});

Then('the offline access dialog is dismissed', async ({ authenticatedApp }) => {
  const productActions = new ProductActionsPage(authenticatedApp.window);
  await productActions.expectOfflineAccessDialogDismissed();
});

Then('the pin indicator does not appear next to the product address', async ({ authenticatedApp }) => {
  const productActions = new ProductActionsPage(authenticatedApp.window);
  await productActions.expectPinIndicatorHidden();
});

// Forget flow (TC-13.5.1) — confirms the destructive dialog opened by the
// "Forget App" product setting (clicked via the shared product-settings step).
When('the user confirms forgetting the product', async ({ authenticatedApp }) => {
  await new ProductSettingsPage(authenticatedApp.window).confirmForget();
});

Then('the {string} product is removed from the apps settings list', async ({ authenticatedApp }, label: string) => {
  await authenticatedApp.window.waitForURL(/\/settings\/privacy\/apps$/, { timeout: DEFAULT_TIMEOUT });
  await new ProductListSettingsPage(authenticatedApp.window).expectProductAbsent(productName(label, AUTH_TLD));
});
