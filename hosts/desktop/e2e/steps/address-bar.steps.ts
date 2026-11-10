import { type DataTable, createBdd } from 'playwright-bdd';

import { expect, test } from '../fixtures/base';
import { DEFAULT_TIMEOUT } from '../helpers/timeouts';
import { BrowserPage } from '../page-objects/BrowserPage';
import { DashboardPage } from '../page-objects/DashboardPage';

const { Then } = createBdd(test);

Then('the address bar survives the following inputs:', async ({ electronApp }, inputs: DataTable) => {
  const browser = new BrowserPage(electronApp.window, electronApp.app);
  const dashboard = new DashboardPage(electronApp.window);

  const baselineTabs = await browser.tabs.count();

  // `rows()` drops the header; `flat()` unwraps the single column into plain strings.
  for (const input of inputs.rows().flat()) {
    await browser.addressBar.submit(input);
    // Only an input that names a product closes the surface on its own, and a
    // surface left open covers the header with its backdrop — so the next
    // iteration could neither press the bar nor close a tab.
    await browser.addressBar.close();

    // Responsiveness check: if the renderer crashed, these will fail.
    // DEFAULT_TIMEOUT covers any processing delay — no fixed sleep needed.
    await expect(browser.addressBar.bar, `crashed after input: ${JSON.stringify(input)}`).toBeVisible({
      timeout: DEFAULT_TIMEOUT,
    });
    await expect(dashboard.userButton, `top bar gone after input: ${JSON.stringify(input)}`).toBeVisible({
      timeout: DEFAULT_TIMEOUT,
    });

    // Close any tabs the input happened to open so the next iteration
    // starts from the same baseline.
    await browser.closeAllTabs(baselineTabs);
  }
});
