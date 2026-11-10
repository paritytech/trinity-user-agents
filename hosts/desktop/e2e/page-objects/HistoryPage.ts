import { type Page, expect } from '@playwright/test';

import { TEST_IDS } from '@/shared/test-ids';
import { DEFAULT_TIMEOUT } from '../helpers/timeouts';

/**
 * The toolbar back / forward history buttons. They reflect the selected tab's
 * per-tab history (`canGoBack` / `canGoForward`): disabled at the ends of the
 * stack, enabled otherwise.
 */
export class HistoryPage {
  constructor(private readonly page: Page) {}

  get backButton() {
    return this.page.getByTestId(TEST_IDS.navigationBackButton);
  }

  get forwardButton() {
    return this.page.getByTestId(TEST_IDS.navigationForwardButton);
  }

  async clickBack() {
    await this.backButton.click({ timeout: DEFAULT_TIMEOUT });
  }

  async clickForward() {
    await this.forwardButton.click({ timeout: DEFAULT_TIMEOUT });
  }

  async expectBackEnabled(enabled: boolean) {
    if (enabled) await expect(this.backButton).toBeEnabled({ timeout: DEFAULT_TIMEOUT });
    else await expect(this.backButton).toBeDisabled({ timeout: DEFAULT_TIMEOUT });
  }

  async expectForwardEnabled(enabled: boolean) {
    if (enabled) await expect(this.forwardButton).toBeEnabled({ timeout: DEFAULT_TIMEOUT });
    else await expect(this.forwardButton).toBeDisabled({ timeout: DEFAULT_TIMEOUT });
  }

  /** Assert both buttons are present and back sits to the left of forward (toolbar order). */
  async expectBackLeftOfForward() {
    await expect(this.backButton).toBeVisible({ timeout: DEFAULT_TIMEOUT });
    await expect(this.forwardButton).toBeVisible({ timeout: DEFAULT_TIMEOUT });
    const back = await this.backButton.boundingBox();
    const forward = await this.forwardButton.boundingBox();
    expect(back, 'back button bounding box').not.toBeNull();
    expect(forward, 'forward button bounding box').not.toBeNull();
    expect(back!.x).toBeLessThan(forward!.x);
  }
}
