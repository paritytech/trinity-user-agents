import { type DataTable, createBdd } from 'playwright-bdd';

import { AUTH_TLD, authenticatedTest } from '../fixtures/authenticated';
import { productName } from '../helpers/dotns';
import { BrowserPage } from '../page-objects/BrowserPage';

const { Given, When, Then } = createBdd(authenticatedTest);

Given('no product tabs are open', async ({ authenticatedApp }) => {
  const browser = new BrowserPage(authenticatedApp.window, authenticatedApp.app);
  await browser.closeAllTabs();
});

When('the user opens products in new tabs:', async ({ authenticatedApp }, products: DataTable) => {
  const browser = new BrowserPage(authenticatedApp.window, authenticatedApp.app);

  // `rows()` drops the header; `flat()` unwraps the single column into plain
  // strings. The table lists labels; the environment decides the suffix.
  for (const label of products.rows().flat()) {
    await browser.openProductInNewTab(productName(label, AUTH_TLD));

    // Let the product start loading before opening the next tab
    await authenticatedApp.window.waitForTimeout(3_000);
  }
});

When('the user cycles through all tabs', async ({ authenticatedApp }) => {
  const browser = new BrowserPage(authenticatedApp.window, authenticatedApp.app);
  await browser.cycleThroughAllTabs();
});

Then('every tab has loaded content', async ({ authenticatedApp }) => {
  const browser = new BrowserPage(authenticatedApp.window, authenticatedApp.app);
  await browser.expectAllTabsHaveContent();
});
