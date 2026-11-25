import { type Page, expect } from '@playwright/test';

import { DEFAULT_TIMEOUT } from '../helpers/timeouts';

/**
 * The apps settings list (/settings/privacy/apps): every product the user has
 * interacted with (committed and permission-only). Each row shows the product's
 * display name and its dotNS address. Used to assert a product was forgotten —
 * a purged product loses both its persisted row and its offline cache, so it
 * disappears from this list entirely.
 */
export class ProductListSettingsPage {
  constructor(private readonly page: Page) {}

  // The list page renders a search box; its presence gates "list has loaded".
  get searchInput() {
    return this.page.getByRole('searchbox');
  }

  async expectLoaded() {
    await expect(this.searchInput).toBeVisible({ timeout: DEFAULT_TIMEOUT });
  }

  /**
   * Assert no row references the given product address. Matches on the address
   * stem (without the network TLD) since the row description carries the
   * normalized base name.
   *
   * Scoped to `main`: forgetting a product does not close the tab it is open in,
   * so the header keeps showing its address in the bar. A page-wide match counts
   * that as a surviving row and the assertion can never reach zero.
   */
  async expectProductAbsent(domain: string) {
    await this.expectLoaded();
    const stem = domain.replace(/\.[a-z0-9-]+$/i, '');
    await expect(this.page.getByRole('main').getByText(stem, { exact: false })).toHaveCount(0, {
      timeout: DEFAULT_TIMEOUT,
    });
  }
}
