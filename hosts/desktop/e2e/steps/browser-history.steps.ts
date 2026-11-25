import { createBdd } from 'playwright-bdd';

import { test } from '../fixtures/link-tests';
import { clickMenuItem } from '../helpers/electron';
import { HistoryPage } from '../page-objects/HistoryPage';

const { When, Then } = createBdd(test);

function isEnabled(state: string): boolean {
  if (state === 'enabled') return true;
  if (state === 'disabled') return false;
  throw new Error(`Expected "enabled" or "disabled", got "${state}"`);
}

When('the user clicks the browser back button', async ({ electronApp }) => {
  await new HistoryPage(electronApp.window).clickBack();
});

When('the user clicks the browser forward button', async ({ electronApp }) => {
  await new HistoryPage(electronApp.window).clickForward();
});

// Tab → Back / Forward (CmdOrCtrl+[ / ]) — the menu accelerator path.
When('the user navigates back via the menu', async ({ electronApp }) => {
  await clickMenuItem(electronApp.app, 'Tab', 'Back');
});

When('the user navigates forward via the menu', async ({ electronApp }) => {
  await clickMenuItem(electronApp.app, 'Tab', 'Forward');
});

Then('the browser back button is {word}', async ({ electronApp }, state: string) => {
  await new HistoryPage(electronApp.window).expectBackEnabled(isEnabled(state));
});

Then('the browser forward button is {word}', async ({ electronApp }, state: string) => {
  await new HistoryPage(electronApp.window).expectForwardEnabled(isEnabled(state));
});

Then('the back button is positioned left of the forward button', async ({ electronApp }) => {
  await new HistoryPage(electronApp.window).expectBackLeftOfForward();
});
