import { expect } from '@playwright/test';
import { createBdd } from 'playwright-bdd';

import { test } from '../fixtures/link-tests';
import { clickMenuItem } from '../helpers/electron';
import { LONG_TIMEOUT } from '../helpers/timeouts';
import { evaluateInWebview } from '../helpers/webview';
import { TabsPage } from '../page-objects/TabsPage';

const { When, Then } = createBdd(test);

When('the user opens an additional new tab', async ({ electronApp }) => {
  await new TabsPage(electronApp.window).openNewTab();
});

When('the user jumps to tab {int}', async ({ electronApp }, n: number) => {
  await clickMenuItem(electronApp.app, 'Tab', `Tab ${n}`);
});

When('the user opens a new tab with the keyboard', async ({ electronApp }) => {
  await clickMenuItem(electronApp.app, 'Tab', 'New Tab');
});

When('the user closes the current tab with the keyboard', async ({ electronApp }) => {
  await clickMenuItem(electronApp.app, 'Tab', 'Close Tab');
});

When('a reload probe is set in the product', async ({ electronApp }) => {
  await evaluateInWebview(electronApp.window, 'window.__e2eReloadProbe = "set"; true');
});

When('the user reloads the current tab with the keyboard', async ({ electronApp }) => {
  await clickMenuItem(electronApp.app, 'View', 'Reload');
});

Then('the tab bar is hidden', async ({ electronApp }) => {
  await new TabsPage(electronApp.window).expectTabStripHidden();
});

Then('the tab bar shows {int} tabs', async ({ electronApp }, count: number) => {
  const tabs = new TabsPage(electronApp.window);
  await tabs.expectTabStripVisible();
  await tabs.expectTabCount(count);
});

When('the user switches to the link-tests product tab', async ({ electronApp, linkTestsTarget }) => {
  await new TabsPage(electronApp.window).selectTab(linkTestsTarget.identifier);
});

When('the user switches to the new tab', async ({ electronApp, linkTestsTarget }) => {
  const tabs = new TabsPage(electronApp.window);
  const ids = await tabs.tabIds();
  const newTabId = ids.find(id => id !== linkTestsTarget.identifier);
  expect(newTabId, 'no new-tab chip found').toBeTruthy();
  await tabs.selectTab(newTabId!);
});

When('the user drags the first tab past the second tab', async ({ electronApp }) => {
  await new TabsPage(electronApp.window).dragTab(0, 1);
});

Then('the link-tests product tab moves to the second position', async ({ electronApp, linkTestsTarget }) => {
  await expect
    .poll(() => new TabsPage(electronApp.window).tabIds(), { timeout: LONG_TIMEOUT })
    .toEqual([expect.not.stringMatching(`^${linkTestsTarget.identifier}$`), linkTestsTarget.identifier]);
});

When('the user hovers over the link-tests product tab', async ({ electronApp, linkTestsTarget }) => {
  await new TabsPage(electronApp.window).hoverTab(linkTestsTarget.identifier);
});

Then('the tab hover card shows the RAM usage', async ({ electronApp }) => {
  const tabs = new TabsPage(electronApp.window);
  await tabs.expectHoverCardVisible();
  await tabs.expectHoverRamVisible();
});

Then('the active tab is the link-tests product', async ({ electronApp, linkTestsTarget }) => {
  await new TabsPage(electronApp.window).expectActiveProduct(linkTestsTarget.identifier);
});

Then('the active tab is a new tab', async ({ electronApp }) => {
  await new TabsPage(electronApp.window).expectActiveNewTab();
});

Then('the address bar is focused', async ({ electronApp }) => {
  await new TabsPage(electronApp.window).expectAddressBarFocused();
});

Then('the reload probe is cleared', async ({ electronApp }) => {
  // Reloading the guest discards window state, so the probe set before the reload
  // is gone once the document reloads. Poll because executeJavaScript can reject
  // mid-reload.
  await expect
    .poll(
      async () => {
        try {
          return await evaluateInWebview(electronApp.window, 'window.__e2eReloadProbe ?? null');
        } catch {
          return 'pending';
        }
      },
      { timeout: LONG_TIMEOUT },
    )
    .toBeNull();
});
