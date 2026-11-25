import { type Page, expect } from '@playwright/test';

import { TEST_IDS } from '@/shared/test-ids';
import { DEFAULT_TIMEOUT } from '../helpers/timeouts';

/**
 * The browser New Tab page (rendered at `/new-tab/<id>`): wordmark, a centered
 * address bar, and the pinned/recent app grids. The pinned/recent grids stay
 * empty unless products + recents are seeded first (see `helpers/seed-products`).
 */
export class NewTabPage {
  constructor(private readonly page: Page) {}

  get page_() {
    return this.page.getByTestId(TEST_IDS.newTabPage);
  }

  get wordmark() {
    return this.page_.getByTestId(TEST_IDS.newTabWordmark);
  }

  get addressBar() {
    return this.page_.getByTestId(TEST_IDS.addressBarInput);
  }

  async expectVisible() {
    await expect(this.page_).toBeVisible({ timeout: DEFAULT_TIMEOUT });
  }

  async expectWordmarkVisible() {
    await expect(this.wordmark).toBeVisible({ timeout: DEFAULT_TIMEOUT });
  }

  async expectAddressBarVisible() {
    await expect(this.addressBar).toBeVisible({ timeout: DEFAULT_TIMEOUT });
  }

  // --- Pinned + recent app grids (need seeded products / recents) ---

  get pinnedCards() {
    return this.page_.getByTestId(TEST_IDS.newTabPinnedCard);
  }

  get recentCards() {
    return this.page_.getByTestId(TEST_IDS.newTabRecentCard);
  }

  get clearRecentsButton() {
    return this.page_.getByTestId(TEST_IDS.newTabClearRecents);
  }

  get recentToast() {
    return this.page.getByTestId(TEST_IDS.newTabRecentToast);
  }

  get undoButton() {
    return this.page.getByTestId(TEST_IDS.newTabRecentUndo);
  }

  /** At least `count` pinned app cards rendered (the page hardcodes 3 pinned ids). */
  async expectPinnedCardCount(count: number) {
    await expect(this.pinnedCards).toHaveCount(count, { timeout: DEFAULT_TIMEOUT });
  }

  async expectRecentCardCount(count: number) {
    await expect(this.recentCards).toHaveCount(count, { timeout: DEFAULT_TIMEOUT });
  }

  async expectRecentCardsVisible() {
    await expect(this.recentCards.first()).toBeVisible({ timeout: DEFAULT_TIMEOUT });
  }

  async clearRecents() {
    await expect(this.clearRecentsButton).toBeVisible({ timeout: DEFAULT_TIMEOUT });
    await this.clearRecentsButton.click();
  }

  async expectUndoToastVisible() {
    await expect(this.recentToast).toBeVisible({ timeout: DEFAULT_TIMEOUT });
  }

  async undo() {
    await expect(this.undoButton).toBeVisible({ timeout: DEFAULT_TIMEOUT });
    await this.undoButton.click();
  }

  async expectNoRecentCards() {
    await expect(this.recentCards).toHaveCount(0, { timeout: DEFAULT_TIMEOUT });
  }
}
