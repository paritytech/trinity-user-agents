import { type Locator, type Page, expect } from '@playwright/test';

import { TEST_IDS } from '@/shared/test-ids';
import { DEFAULT_TIMEOUT, SHORT_TIMEOUT } from '../helpers/timeouts';
import { waitForToastsToClear } from '../helpers/toasts';

/**
 * Page Object for the product actions menu (•••) and its items,
 * including "Enable offline access" and "Proceed in Chat".
 */
export class ProductActionsPage {
  constructor(private readonly page: Page) {}

  get menuTrigger() {
    return this.page.getByTestId(TEST_IDS.productActionsMenuTrigger);
  }

  /**
   * The "Proceed in Chat" menu item. It only renders for products whose worker
   * declares chat support, so it's matched by its accessible name (the shared
   * `productActionsMenuItem` testid can't disambiguate it from sibling items).
   */
  get proceedInChatMenuItem() {
    return this.page.getByRole('menuitem', { name: 'Proceed in Chat', exact: true });
  }

  get offlineAccessMenuItem() {
    return this.page.getByTestId(TEST_IDS.offlineAccessMenuItem);
  }

  get offlineAccessEnableConfirm() {
    return this.page.getByTestId(TEST_IDS.offlineAccessEnableConfirm);
  }

  get offlineAccessPinIndicator() {
    return this.page.getByTestId(TEST_IDS.offlineAccessPinIndicator);
  }

  get offlineAccessRemoveConfirm() {
    return this.page.getByTestId(TEST_IDS.offlineAccessRemoveConfirm);
  }

  get offlineAccessDialogCancel() {
    return this.page.getByTestId(TEST_IDS.offlineAccessDialogCancel);
  }

  get openSettingsMenuItem() {
    return this.page.getByTestId(TEST_IDS.productActionsMenuOpenSettings);
  }

  // The dashboard-shortcut item is a toggle whose label flips with state: a
  // widget-less product offers "Add to Favorites", which becomes "Remove from
  // Favorites" once pinned. Matched by accessible name (the shared
  // `productActionsMenuItem` testid can't disambiguate sibling items).
  get addToFavoritesMenuItem() {
    return this.page.getByRole('menuitem', { name: 'Add to Favorites', exact: true });
  }

  get removeFromFavoritesMenuItem() {
    return this.page.getByRole('menuitem', { name: 'Remove from Favorites', exact: true });
  }

  /**
   * Click a menu item, re-resolving it on each attempt.
   *
   * The dashboard item re-renders once `useDisplayedProduct` settles — it swaps
   * "Add to Dashboard" for "Add to Favorites" by manifest shape — so an item
   * resolved by an assertion can detach before the click dispatches, and Playwright
   * then waits out its whole timeout on a node that never comes back. Asserting and
   * clicking inside one retried block re-resolves both. A retry cannot double-fire:
   * a landed click closes the menu, so the next attempt finds nothing to click and
   * the caller's own assertion decides the outcome.
   */
  private async clickMenuItem(item: Locator) {
    await expect(async () => {
      await expect(item).toBeVisible({ timeout: SHORT_TIMEOUT });
      await item.click({ timeout: SHORT_TIMEOUT });
    }).toPass({ timeout: DEFAULT_TIMEOUT });
  }

  async clickAddToFavorites() {
    await this.clickMenuItem(this.addToFavoritesMenuItem);
  }

  async clickRemoveFromFavorites() {
    await this.clickMenuItem(this.removeFromFavoritesMenuItem);
  }

  async openProductSettings() {
    await this.clickMenuItem(this.openSettingsMenuItem);
  }

  async openMenu() {
    await expect(this.menuTrigger).toBeVisible({ timeout: DEFAULT_TIMEOUT });
    // The trigger sits under the toast area, and every favorites / dashboard /
    // offline-access action this menu performs raises one — so re-opening the
    // menu right after would click the toast instead.
    await waitForToastsToClear(this.page);

    // Clicking the trigger does not guarantee the menu is open: the same
    // re-render that detaches items under `clickMenuItem` also drops the
    // dropdown's open state, and a click that opened nothing would still let
    // this method return — leaving the caller's own assertion to time out
    // against a closed menu and blame the item instead of the open. Re-click
    // until an item is really on screen. Open Settings is the sentinel because
    // every product's menu renders it, while the slot items above it are
    // conditional on the product.
    await expect(async () => {
      if (!(await this.openSettingsMenuItem.isVisible())) {
        await this.menuTrigger.click({ timeout: SHORT_TIMEOUT });
      }
      await expect(this.openSettingsMenuItem).toBeVisible({ timeout: SHORT_TIMEOUT });
    }).toPass({ timeout: DEFAULT_TIMEOUT });
  }

  async clickOfflineAccessMenuItem() {
    await this.clickMenuItem(this.offlineAccessMenuItem);
  }

  async confirmEnableOfflineAccess() {
    await expect(this.offlineAccessEnableConfirm).toBeVisible({ timeout: DEFAULT_TIMEOUT });
    await this.offlineAccessEnableConfirm.click();
  }

  // When a product is already pinned, the same menu item is labelled "Remove
  // offline use" and opens the remove dialog instead.
  async confirmRemoveOfflineAccess() {
    await expect(this.offlineAccessRemoveConfirm).toBeVisible({ timeout: DEFAULT_TIMEOUT });
    await this.offlineAccessRemoveConfirm.click();
  }

  // Cancel the Enable-offline dialog via its secondary button. The dialog closes
  // without pinning the product.
  async cancelOfflineAccessDialog() {
    await expect(this.offlineAccessDialogCancel).toBeVisible({ timeout: DEFAULT_TIMEOUT });
    await this.offlineAccessDialogCancel.click();
  }

  // The Enable-offline dialog is dismissed once its confirm button is gone.
  async expectOfflineAccessDialogDismissed() {
    await expect(this.offlineAccessEnableConfirm).toBeHidden({ timeout: DEFAULT_TIMEOUT });
  }

  async expectPinIndicatorVisible() {
    await expect(this.offlineAccessPinIndicator).toBeVisible({ timeout: DEFAULT_TIMEOUT });
  }

  async expectPinIndicatorHidden() {
    await expect(this.offlineAccessPinIndicator).toBeHidden({ timeout: DEFAULT_TIMEOUT });
  }

  /**
   * Open the ••• menu and pick "Proceed in Chat". The product's worker declares
   * its room straight into storage, so there is no confirmation step — the room
   * surfaces in the Quick Chat popover on its own once the worker has run.
   */
  async proceedInChat() {
    await this.openMenu();
    await this.clickMenuItem(this.proceedInChatMenuItem);
  }
}
