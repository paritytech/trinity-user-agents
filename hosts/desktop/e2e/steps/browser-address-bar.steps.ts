import { type Page } from '@playwright/test';
import { createBdd } from 'playwright-bdd';

import { TEST_IDS } from '@/shared/test-ids';
import { test } from '../fixtures/link-tests';
import { AddressBarPage } from '../page-objects/AddressBarPage';
import { LinkTestsPage } from '../page-objects/LinkTestsPage';

const { When, Then } = createBdd(test);

// Open a product from a clean tab state: click "+" if it's already showing,
// then drive the header address bar. Mirrors the shared link-tests open step.
async function openViaAddressBar(page: Page, value: string) {
  const newTabButton = page.getByTestId(TEST_IDS.newTabButton);
  if (await newTabButton.isVisible().catch(() => false)) {
    await newTabButton.click({ force: true });
  }
  await new AddressBarPage(page).submit(value);
}

When('the user focuses the address bar', async ({ electronApp }) => {
  await new AddressBarPage(electronApp.window).open();
});

When('the user types {string} into the address bar', async ({ electronApp }, text: string) => {
  await new AddressBarPage(electronApp.window).type(text);
});

When('the user presses {string} in the address bar', async ({ electronApp }, key: string) => {
  await new AddressBarPage(electronApp.window).pressKey(key);
});

When('the user clicks outside the address bar', async ({ electronApp }) => {
  await new AddressBarPage(electronApp.window).clickOutside();
});

// Opens the link-tests product against a path whose response the static server
// holds (the `__slow` marker), so the loading-progress bar stays up long enough
// to observe deterministically.
When('the user opens the link-tests product with a slow response', async ({ electronApp, linkTestsTarget }) => {
  await openViaAddressBar(electronApp.window, `${linkTestsTarget.address}/__slow`);
});

// Submits the bare link-tests domain with a trailing slash. Slashes around an
// empty path are normalized away, so this resolves to the same product
// identifier as the bare domain.
When(
  'the user submits the link-tests domain with a trailing slash in the address bar',
  async ({ electronApp, linkTestsTarget }) => {
    const bar = new AddressBarPage(electronApp.window);
    await bar.submit(`${linkTestsTarget.identifier}/`);
  },
);

// Submits the link-tests path wrapped in extra slashes (`<id>/link-tests/`).
// The trailing/leading slashes don't change which product the address bar
// resolves — it still opens the same link-tests identifier.
When(
  'the user submits the link-tests path with surrounding slashes in the address bar',
  async ({ electronApp, linkTestsTarget }) => {
    const bar = new AddressBarPage(electronApp.window);
    await bar.submit(`${linkTestsTarget.identifier}/link-tests/`);
  },
);

Then('the address bar ghost-completes {string} on Tab', async ({ electronApp }, label: string) => {
  await new AddressBarPage(electronApp.window).expectTabCompletes(label);
});

Then('the address bar value is {string}', async ({ electronApp }, value: string) => {
  await new AddressBarPage(electronApp.window).expectValue(value);
});

Then('the address input surface is visible', async ({ electronApp }) => {
  await new AddressBarPage(electronApp.window).expectSurfaceVisible();
});

Then('the address input surface is not visible', async ({ electronApp }) => {
  await new AddressBarPage(electronApp.window).expectSurfaceHidden();
});

Then('the address bar suggestions are visible', async ({ electronApp }) => {
  await new AddressBarPage(electronApp.window).expectSuggestionsVisible();
});

Then('the address bar suggestions are not visible', async ({ electronApp }) => {
  await new AddressBarPage(electronApp.window).expectSuggestionsHidden();
});

Then('the address bar shows the loading progress bar', async ({ electronApp }) => {
  await new AddressBarPage(electronApp.window).expectLoadingBarVisible();
});

Then('the address bar loading progress bar disappears once the product loads', async ({ electronApp }) => {
  await new AddressBarPage(electronApp.window).expectLoadingBarHidden();
});

// Reads the guest's window.location.pathname from the visible webview — the
// observable proof that the typed (and slash-normalized) path actually loaded.
Then('the link-tests webview pathname is {string}', async ({ electronApp }, expected: string) => {
  await new LinkTestsPage(electronApp.window).expectWebviewPathname(expected);
});
