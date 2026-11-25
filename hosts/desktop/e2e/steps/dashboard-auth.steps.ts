import { type Page } from '@playwright/test';
import { createBdd } from 'playwright-bdd';

import { TEST_IDS } from '@/shared/test-ids';
import { AUTH_TLD, authenticatedTest, expect } from '../fixtures/authenticated';
import { productName } from '../helpers/dotns';
import { setWindowSize } from '../helpers/electron';
import { seedDashboardPages, seedDashboardWidget, seedProducts } from '../helpers/seed-products';
import { DEFAULT_TIMEOUT } from '../helpers/timeouts';
import { AddWidgetModalPage } from '../page-objects/AddWidgetModalPage';
import { BrowserPage } from '../page-objects/BrowserPage';
import { DashboardPage } from '../page-objects/DashboardPage';
import { ProductWidgetModalPage } from '../page-objects/ProductWidgetModalPage';
import { WidgetCardPage } from '../page-objects/WidgetCardPage';

const { Given, When, Then } = createBdd(authenticatedTest);

// A narrow window: width clamps to the 800px window minimum, well under the
// dashboard grid's 1366px min-width, so the grid is forced to overflow.
const NARROW_WINDOW = { width: 800, height: 800 };

// Seeds write via raw IndexedDB, which Dexie's liveQuery does NOT observe, so a
// reload is required for the dashboard to re-read the seeded layout. The
// authenticated session persists in the userDataDir, so a reload returns to
// /dashboard without re-signing-in.
async function reloadAuthenticatedDashboard(page: Page) {
  await page.reload({ waitUntil: 'domcontentloaded' });
  await page.waitForURL(/dashboard/, { timeout: DEFAULT_TIMEOUT });
}

When('the user opens the quick chat popover', async ({ authenticatedApp }) => {
  await new DashboardPage(authenticatedApp.window).openQuickChat();
});

When('the user expands quick chat to the chat tab', async ({ authenticatedApp }) => {
  const expand = authenticatedApp.window.getByTestId(TEST_IDS.quickChatExpandButton);
  await expect(expand).toBeVisible({ timeout: DEFAULT_TIMEOUT });
  await expand.click();
});

Then('the host is on the chat route', async ({ authenticatedApp }) => {
  await authenticatedApp.window.waitForURL(/\/chat/, { timeout: DEFAULT_TIMEOUT });
});

// "the user opens {string} in a new tab" is shared from offline-access.steps.

When('the user opens the Add Widget modal', async ({ authenticatedApp }) => {
  await new DashboardPage(authenticatedApp.window).openAddWidgetModal();
});

Then('the Add Widget modal is open', async ({ authenticatedApp }) => {
  await expect(authenticatedApp.window.getByRole('dialog')).toBeVisible({ timeout: DEFAULT_TIMEOUT });
  await expect(authenticatedApp.window.locator('input[type="search"]').first()).toBeVisible({ timeout: DEFAULT_TIMEOUT });
});

When('the user returns to the dashboard via the home button', async ({ authenticatedApp }) => {
  await new DashboardPage(authenticatedApp.window).navigateToDashboard();
});

Then('the dashboard is shown', async ({ authenticatedApp }) => {
  await new DashboardPage(authenticatedApp.window).waitForDashboard();
});

// --- 3.1 Layout & Navigation -------------------------------------------------

Then('the dashboard grid is shown', async ({ authenticatedApp }) => {
  const dashboard = new DashboardPage(authenticatedApp.window);
  await expect(dashboard.addWidgetButton).toBeVisible({ timeout: DEFAULT_TIMEOUT });
  await expect(dashboard.grid).toBeVisible({ timeout: DEFAULT_TIMEOUT });
});

When('the user removes all dashboard widgets', async ({ authenticatedApp }) => {
  await new DashboardPage(authenticatedApp.window).removeAllWidgets();
});

Then('the empty dashboard state is shown', async ({ authenticatedApp }) => {
  await new DashboardPage(authenticatedApp.window).expectEmptyState();
});

When('the user shrinks the window', async ({ authenticatedApp }) => {
  await setWindowSize(authenticatedApp.app, NARROW_WINDOW);
});

Then('the dashboard grid scrolls horizontally', async ({ authenticatedApp }) => {
  const dashboard = new DashboardPage(authenticatedApp.window);
  await expect.poll(() => dashboard.isGridHorizontallyScrollable(), { timeout: DEFAULT_TIMEOUT }).toBe(true);
});

// --- 3.2 Add Widgets & Favorites --------------------------------------------

Then('the Add Widget catalog lists the published widget product {string}', async ({ authenticatedApp }, query: string) => {
  const modal = new AddWidgetModalPage(authenticatedApp.window);
  await modal.expectCatalogPopulated();
  await modal.search(query);
  await modal.expectCatalogPopulated();
});

When('the user searches the Add Widget catalog for {string}', async ({ authenticatedApp }, query: string) => {
  await new AddWidgetModalPage(authenticatedApp.window).search(query);
});

Then('the Add Widget catalog lists a matching entry {string}', async ({ authenticatedApp }, label: string) => {
  await new AddWidgetModalPage(authenticatedApp.window).expectMatchVisible(label);
});

Then('the Add Widget catalog shows no results', async ({ authenticatedApp }) => {
  await new AddWidgetModalPage(authenticatedApp.window).expectNoResults();
});

When('the user adds the open product to the dashboard as a {string} widget', async ({ authenticatedApp }, sizeLabel: string) => {
  const modal = new ProductWidgetModalPage(authenticatedApp.window);
  await modal.open();
  await modal.addAsWidget(sizeLabel);
});

When('the user adds the open product to favorites', async ({ authenticatedApp }) => {
  const modal = new ProductWidgetModalPage(authenticatedApp.window);
  await modal.open();
  await modal.addToFavorites();
});

When('the user navigates back to the dashboard', async ({ authenticatedApp }) => {
  await new DashboardPage(authenticatedApp.window).navigateToDashboard();
});

Then('a product widget appears on the dashboard', async ({ authenticatedApp }) => {
  await new DashboardPage(authenticatedApp.window).expectProductWidgetVisible();
});

Then('a favorites folder with an icon appears on the dashboard', async ({ authenticatedApp }) => {
  await new DashboardPage(authenticatedApp.window).expectFavoritesFolderWithIcon();
});

Then('re-opening the add-to-dashboard modal shows Open for the widget', async ({ authenticatedApp }) => {
  const modal = new ProductWidgetModalPage(authenticatedApp.window);
  await modal.open();
  await modal.expectActionShowsOpen();
  await modal.close();
});

Then('re-opening the add-to-dashboard modal shows the product already in favorites', async ({ authenticatedApp }) => {
  const modal = new ProductWidgetModalPage(authenticatedApp.window);
  await modal.open();
  await modal.expectFavoritesAlreadyAdded();
  await modal.close();
});

// --- 3.3 Product Widget Card -------------------------------------------------

// Reach a deterministic single-card grid: clear the seeded default widget, then
// add CoinFlip as a Large product widget so the sole card is CoinFlip's.
Given('the user starts with only a CoinFlip widget on the dashboard', async ({ authenticatedApp }) => {
  const dashboard = new DashboardPage(authenticatedApp.window);
  await dashboard.removeAllWidgets();

  const browser = new BrowserPage(authenticatedApp.window, authenticatedApp.app);
  await browser.openProductInNewTab(productName('coinflipgame03', AUTH_TLD));

  const modal = new ProductWidgetModalPage(authenticatedApp.window);
  await modal.open();
  await modal.addAsWidget('Large');

  await dashboard.navigateToDashboard();
  await dashboard.expectProductWidgetVisible();
});

Then('the product widget body loads its webview', async ({ authenticatedApp }) => {
  await new WidgetCardPage(authenticatedApp.window).expectWebviewLoaded();
});

Given('a reload probe is set in the product widget', async ({ authenticatedApp }) => {
  await new WidgetCardPage(authenticatedApp.window).setReloadProbe();
});

When('the user reloads the product widget from its card menu', async ({ authenticatedApp }) => {
  await new WidgetCardPage(authenticatedApp.window).reload();
});

Then('the product widget reload probe is cleared', async ({ authenticatedApp }) => {
  await new WidgetCardPage(authenticatedApp.window).expectReloadProbeCleared();
});

When('the user opens the product widget fullscreen from its card', async ({ authenticatedApp }) => {
  await new WidgetCardPage(authenticatedApp.window).clickFullscreen();
});

Then('the app navigates to the product fullscreen route', async ({ authenticatedApp }) => {
  await authenticatedApp.window.waitForURL(/\/product\//, { timeout: DEFAULT_TIMEOUT });
});

When('the user opens the favorited product from its dashboard icon', async ({ authenticatedApp }) => {
  await new DashboardPage(authenticatedApp.window).openFirstFavoriteIcon();
});

Then('resizing the product widget to a smaller size shrinks its footprint', async ({ authenticatedApp }) => {
  const card = new WidgetCardPage(authenticatedApp.window);
  await card.expectVisible();
  const before = await card.cardHeight();
  expect(before).toBeGreaterThan(0);
  await card.resizeToSmaller();
  await expect.poll(() => card.cardHeight(), { timeout: DEFAULT_TIMEOUT }).toBeLessThan(before);
});

When('the user cleans up the dashboard from the widget card menu', async ({ authenticatedApp }) => {
  await new WidgetCardPage(authenticatedApp.window).cleanup();
});

Then('the product widget remains on the dashboard', async ({ authenticatedApp }) => {
  const dashboard = new DashboardPage(authenticatedApp.window);
  await dashboard.expectProductWidgetVisible();
  await expect(dashboard.grid).toBeVisible({ timeout: DEFAULT_TIMEOUT });
});

When('the user removes the product widget via its card menu', async ({ authenticatedApp }) => {
  await new WidgetCardPage(authenticatedApp.window).remove();
});

Then('no product widget remains on the dashboard', async ({ authenticatedApp }) => {
  await new WidgetCardPage(authenticatedApp.window).expectGone();
});

// --- 3.4 Favorites Folder ----------------------------------------------------

Given('the dashboard has no favorites folder', async ({ authenticatedApp }) => {
  await new DashboardPage(authenticatedApp.window).expectFavoritesFolderAbsent();
});

When('the user removes the favorites folder via its card menu', async ({ authenticatedApp }) => {
  await new DashboardPage(authenticatedApp.window).removeFavoritesFolder();
});

Then('the favorites folder is removed', async ({ authenticatedApp }) => {
  await new DashboardPage(authenticatedApp.window).expectFavoritesFolderHidden();
});

// --- 5.2.2 Widget not-found (seeded) -----------------------------------------

// A committed product with NO widget executable + a dashboard widget card
// pointing at it. The card's body resolves the (absent) widget archive to null
// synchronously, so `ProductWidgetBody` renders "Domain not found". The seed is
// a raw IndexedDB write (bypasses Dexie's liveQuery), so reload to re-read it.
Given('a dashboard widget for an unresolvable product is seeded', async ({ authenticatedApp }) => {
  const productId = 'unresolvable-widget-xyz.dot';
  await seedProducts(authenticatedApp.window, [{ baseName: productId, withWidget: false }]);
  await seedDashboardWidget(authenticatedApp.window, productId);
  await reloadAuthenticatedDashboard(authenticatedApp.window);
});

Then('the product widget shows the not-found state', async ({ authenticatedApp }) => {
  await new DashboardPage(authenticatedApp.window).expectWidgetNotFound();
});

// --- 3.1.4 Pagination (seeded multi-page layout) -----------------------------

// Two pages, each holding one widget card, so the toolbar renders two page dots.
Given('a two-page dashboard is seeded', async ({ authenticatedApp }) => {
  const pageOne = 'page-one-widget-xyz.dot';
  const pageTwo = 'page-two-widget-xyz.dot';
  await seedProducts(authenticatedApp.window, [
    { baseName: pageOne, withWidget: false },
    { baseName: pageTwo, withWidget: false },
  ]);
  await seedDashboardPages(authenticatedApp.window, [pageOne, pageTwo]);
  await reloadAuthenticatedDashboard(authenticatedApp.window);
});

Then('the dashboard shows {int} page indicator dots', async ({ authenticatedApp }, count: number) => {
  await new DashboardPage(authenticatedApp.window).expectPaginationTabCount(count);
});

When('the user selects dashboard page {int}', async ({ authenticatedApp }, page: number) => {
  await new DashboardPage(authenticatedApp.window).selectPage(page - 1);
});

Then('dashboard page {int} is active', async ({ authenticatedApp }, page: number) => {
  await new DashboardPage(authenticatedApp.window).expectPageActive(page - 1);
});
