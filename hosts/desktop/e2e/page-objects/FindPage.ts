import { type Page, expect } from '@playwright/test';

import { TEST_IDS } from '@/shared/test-ids';
import { DEFAULT_TIMEOUT, LONG_TIMEOUT } from '../helpers/timeouts';

/**
 * The find-in-page bar shown over a product tab. The bar itself is host-renderer
 * DOM (Playwright drives it directly), but the match count is produced by the
 * native Chromium search over the guest webview and reported back asynchronously —
 * so counter assertions poll with a longer timeout.
 */
export class FindPage {
  constructor(private readonly page: Page) {}

  get bar() {
    return this.page.getByTestId(TEST_IDS.findBar);
  }

  get input() {
    return this.page.getByTestId(TEST_IDS.findInput);
  }

  get count() {
    return this.page.getByTestId(TEST_IDS.findCount);
  }

  get nextButton() {
    return this.page.getByTestId(TEST_IDS.findNext);
  }

  get previousButton() {
    return this.page.getByTestId(TEST_IDS.findPrevious);
  }

  get closeButton() {
    return this.page.getByTestId(TEST_IDS.findClose);
  }

  async expectOpen() {
    await expect(this.bar).toBeVisible({ timeout: DEFAULT_TIMEOUT });
  }

  async expectClosed() {
    await expect(this.bar).toBeHidden({ timeout: DEFAULT_TIMEOUT });
  }

  async expectInputFocused() {
    await expect(this.input).toBeFocused({ timeout: DEFAULT_TIMEOUT });
  }

  async search(term: string) {
    await this.input.fill(term);
  }

  /** Counter format is "{current}/{total}" (e.g. "1/3"); reported async by the guest. */
  async expectCount(text: string) {
    await expect(this.count).toHaveText(text, { timeout: LONG_TIMEOUT });
  }

  async next() {
    await this.nextButton.click({ timeout: DEFAULT_TIMEOUT });
  }

  async previous() {
    await this.previousButton.click({ timeout: DEFAULT_TIMEOUT });
  }

  async close() {
    await this.closeButton.click({ timeout: DEFAULT_TIMEOUT });
  }

  async expectNavigationDisabled() {
    await expect(this.nextButton).toBeDisabled({ timeout: DEFAULT_TIMEOUT });
    await expect(this.previousButton).toBeDisabled({ timeout: DEFAULT_TIMEOUT });
  }
}
