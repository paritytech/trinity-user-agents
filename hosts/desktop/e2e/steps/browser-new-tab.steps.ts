import { createBdd } from 'playwright-bdd';

import { test } from '../fixtures/link-tests';
import { NewTabPage } from '../page-objects/NewTabPage';
import { TabsPage } from '../page-objects/TabsPage';

const { When, Then } = createBdd(test);

When('the user opens a new tab page', async ({ electronApp }) => {
  await new TabsPage(electronApp.window).openNewTab();
  await new NewTabPage(electronApp.window).expectVisible();
});

Then('the new tab page shows the wordmark', async ({ electronApp }) => {
  await new NewTabPage(electronApp.window).expectWordmarkVisible();
});

Then('the new tab page shows the address bar', async ({ electronApp }) => {
  await new NewTabPage(electronApp.window).expectAddressBarVisible();
});
