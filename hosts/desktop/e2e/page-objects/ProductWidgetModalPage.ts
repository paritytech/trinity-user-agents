import { type Locator, type Page, expect } from '@playwright/test';

import { TEST_IDS } from '@/shared/test-ids';
import { DEFAULT_TIMEOUT } from '../helpers/timeouts';

/**
 * Page Object for the per-product widget/favorites modal
 * (`FavoriteSizeSelectorModal` → `AddWidgetModalProductPanel`), opened from the
 * product actions ••• menu's "Add to dashboard" item.
 *
 * The same modal powers four flows:
 *  - add as a sized widget (size chip `Small`/`Medium`/`Large`/`Horizontal` + the
 *    per-card `Add` button),
 *  - add as a 1×1 favorite (the `Add to Favorites` button),
 *  - re-open for a product already on the dashboard → the per-card button reads
 *    `Open` instead of `Add`,
 *  - re-open for a product already favourited → the favorites button is disabled
 *    and reads `Added to Favorites`.
 *
 * The dialog is scoped by its always-present `Add(ed) to Favorites` button so a
 * stray permission/error dialog can't shadow the locator.
 */
export class ProductWidgetModalPage {
  constructor(private readonly page: Page) {}

  get dialog(): Locator {
    return this.page.getByRole('dialog').filter({
      has: this.page.getByRole('button', { name: /^Add(ed)? to Favorites$/ }),
    });
  }

  /** Open the modal via the address-bar product actions ••• menu. */
  async open() {
    const trigger = this.page.getByTestId(TEST_IDS.productActionsMenuTrigger);
    await expect(trigger).toBeVisible({ timeout: DEFAULT_TIMEOUT });
    await trigger.click();

    const addToDashboard = this.page.getByRole('menuitem', { name: 'Add to dashboard', exact: true });
    await expect(addToDashboard).toBeVisible({ timeout: DEFAULT_TIMEOUT });
    await addToDashboard.click();

    await expect(this.dialog).toBeVisible({ timeout: DEFAULT_TIMEOUT });
  }

  async addAsWidget(sizeLabel: string) {
    const sizeChip = this.dialog.getByRole('button', { name: sizeLabel, exact: true });
    await expect(sizeChip).toBeVisible({ timeout: DEFAULT_TIMEOUT });
    await sizeChip.click();
    await expect(sizeChip).toHaveAttribute('aria-pressed', 'true');

    const addButton = this.dialog.getByRole('button', { name: 'Add', exact: true });
    await expect(addButton).toBeEnabled({ timeout: DEFAULT_TIMEOUT });
    await addButton.click();

    await this.close();
  }

  async addToFavorites() {
    const favoritesButton = this.dialog.getByRole('button', { name: 'Add to Favorites', exact: true });
    await expect(favoritesButton).toBeEnabled({ timeout: DEFAULT_TIMEOUT });
    await favoritesButton.click();

    await this.close();
  }

  /** Already-on-dashboard widgets surface `Open`, never `Add`. */
  async expectActionShowsOpen() {
    await expect(this.dialog.getByRole('button', { name: 'Open', exact: true })).toBeVisible({ timeout: DEFAULT_TIMEOUT });
    await expect(this.dialog.getByRole('button', { name: 'Add', exact: true })).toHaveCount(0);
  }

  /** Already-favourited products show a disabled `Added to Favorites` button. */
  async expectFavoritesAlreadyAdded() {
    const addedButton = this.dialog.getByRole('button', { name: 'Added to Favorites', exact: true });
    await expect(addedButton).toBeVisible({ timeout: DEFAULT_TIMEOUT });
    await expect(addedButton).toBeDisabled();
  }

  async close() {
    // Dismiss via the dialog's dedicated close button rather than a global
    // Escape keydown. Right after an add the dialog re-renders heavily (the
    // per-card `Add` swaps to `Open`, a success toast portal mounts) and the
    // shell prevents `onOpenAutoFocus`, so focus isn't anchored inside the
    // dialog — a single Escape can land on a transient/detached node and miss
    // Radix's escape→close path, leaving the dialog open (flaky). The close
    // button triggers `onOpenChange(false)` directly, independent of focus and
    // re-render timing.
    await this.dialog.getByRole('button', { name: 'Close' }).click({ timeout: DEFAULT_TIMEOUT });
    await expect(this.dialog).toBeHidden({ timeout: DEFAULT_TIMEOUT });
  }
}
