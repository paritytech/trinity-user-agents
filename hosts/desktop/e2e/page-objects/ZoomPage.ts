import { type Page, expect } from '@playwright/test';

import { TEST_IDS } from '@/shared/test-ids';
import { DEFAULT_TIMEOUT } from '../helpers/timeouts';

const ZOOM_BUTTON_TEST_IDS: Record<string, string> = {
  in: TEST_IDS.zoomIn,
  out: TEST_IDS.zoomOut,
  reset: TEST_IDS.zoomReset,
};

/**
 * The transient zoom indicator shown over the top-center of a product tab.
 *
 * It surfaces ONLY on an actual zoom-level change (a user zoom action) and
 * auto-fades after ~1.5s (`ZoomIndicator.VISIBLE_MS`). Visibility assertions are
 * therefore time-sensitive: assert the percentage promptly after triggering a
 * zoom, before the fade window elapses.
 */
export class ZoomPage {
  constructor(private readonly page: Page) {}

  get indicator() {
    return this.page.getByTestId(TEST_IDS.zoomIndicator);
  }

  get percentLabel() {
    return this.page.getByTestId(TEST_IDS.zoomPercent);
  }

  /** Assert the indicator is on screen and shows the given label (e.g. "120%"). */
  async expectPercent(label: string) {
    await expect(this.indicator).toBeVisible({ timeout: DEFAULT_TIMEOUT });
    await expect(this.percentLabel).toHaveText(label, { timeout: DEFAULT_TIMEOUT });
  }

  /** Click one of the indicator's own buttons ("in" | "out" | "reset"). Requires the indicator visible. */
  async clickButton(direction: string) {
    const testId = ZOOM_BUTTON_TEST_IDS[direction];
    if (!testId) throw new Error(`Unknown zoom button direction: ${direction}`);
    await this.page.getByTestId(testId).click({ timeout: DEFAULT_TIMEOUT });
  }

  /** Assert the indicator is not (or no longer) on screen. */
  async expectHidden() {
    await expect(this.indicator).toBeHidden({ timeout: DEFAULT_TIMEOUT });
  }
}
