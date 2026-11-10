import { createBdd } from 'playwright-bdd';

import { test } from '../fixtures/link-tests';
import { clickMenuItem } from '../helpers/electron';
import { FindPage } from '../page-objects/FindPage';

const { When, Then } = createBdd(test);

// Find is opened through the Edit menu (the same path Cmd/Ctrl+F takes): the menu
// item sends `edit:find`, which the renderer honours only when a product webview
// is on screen.
When('the user opens find in page', async ({ electronApp }) => {
  await clickMenuItem(electronApp.app, 'Edit', 'Find');
});

When('the user searches for {string}', async ({ electronApp }, term: string) => {
  await new FindPage(electronApp.window).search(term);
});

When('the user steps to the next find match', async ({ electronApp }) => {
  await new FindPage(electronApp.window).next();
});

When('the user steps to the previous find match', async ({ electronApp }) => {
  await new FindPage(electronApp.window).previous();
});

When('the user closes the find bar', async ({ electronApp }) => {
  await new FindPage(electronApp.window).close();
});

Then('the find bar is visible', async ({ electronApp }) => {
  await new FindPage(electronApp.window).expectOpen();
});

Then('the find bar is not visible', async ({ electronApp }) => {
  // Give the menu IPC a beat to (not) open the bar, then assert it stayed closed.
  await electronApp.window.waitForTimeout(500);
  await new FindPage(electronApp.window).expectClosed();
});

Then('the find input is focused', async ({ electronApp }) => {
  await new FindPage(electronApp.window).expectInputFocused();
});

Then('the find counter shows {string}', async ({ electronApp }, text: string) => {
  await new FindPage(electronApp.window).expectCount(text);
});

Then('the find navigation buttons are disabled', async ({ electronApp }) => {
  await new FindPage(electronApp.window).expectNavigationDisabled();
});
