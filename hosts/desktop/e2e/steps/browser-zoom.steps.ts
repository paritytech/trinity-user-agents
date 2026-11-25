import { createBdd } from 'playwright-bdd';

import { test } from '../fixtures/link-tests';
import { clickMenuItem } from '../helpers/electron';
import { ZoomPage } from '../page-objects/ZoomPage';

const { When, Then } = createBdd(test);

// Zoom is driven through the View menu (the same path the accelerators take):
// the menu item's click handler sends the `view:zoom-*` IPC to the renderer,
// which applies it only when a product webview is on screen.
When('the user zooms in', async ({ electronApp }) => {
  await clickMenuItem(electronApp.app, 'View', 'Zoom In');
});

When('the user zooms out', async ({ electronApp }) => {
  await clickMenuItem(electronApp.app, 'View', 'Zoom Out');
});

When('the user resets zoom', async ({ electronApp }) => {
  await clickMenuItem(electronApp.app, 'View', 'Actual Size');
});

When('the user reloads the product from the menu', async ({ electronApp }) => {
  await clickMenuItem(electronApp.app, 'View', 'Reload');
});

When('the user clicks the zoom indicator {string} button', async ({ electronApp }, direction: string) => {
  await new ZoomPage(electronApp.window).clickButton(direction);
});

When('the zoom indicator fades', async ({ electronApp }) => {
  // The indicator auto-hides ~1.5s after the last zoom change (VISIBLE_MS).
  // Wait past that window, then confirm it is gone before the next assertion.
  await electronApp.window.waitForTimeout(1_800);
  await new ZoomPage(electronApp.window).expectHidden();
});

Then('the zoom indicator shows {string}', async ({ electronApp }, label: string) => {
  await new ZoomPage(electronApp.window).expectPercent(label);
});

Then('the zoom indicator does not appear', async ({ electronApp }) => {
  // Give any spurious indicator a chance to surface, then assert it stayed hidden.
  await electronApp.window.waitForTimeout(500);
  await new ZoomPage(electronApp.window).expectHidden();
});
