import { type Page, expect } from '@playwright/test';

import { TEST_IDS } from '@/shared/test-ids';
import { escapeRegex } from '../helpers/regex';
import { DEFAULT_TIMEOUT } from '../helpers/timeouts';

/**
 * The Settings area: sidebar groups, nav items (rendered as links), and the
 * settings-scoped back/forward buttons (which reuse the navigation test-ids —
 * the browser ones return null on /settings routes, so they don't collide).
 */
export class SettingsPage {
  constructor(private readonly page: Page) {}

  group(title: string) {
    return this.page.getByText(title, { exact: true });
  }

  navLink(name: string) {
    return this.page.getByRole('link', { name, exact: true });
  }

  get backButton() {
    return this.page.getByTestId(TEST_IDS.navigationBackButton);
  }

  get forwardButton() {
    return this.page.getByTestId(TEST_IDS.navigationForwardButton);
  }

  async expectGroupsVisible(titles: string[]) {
    for (const title of titles) {
      await expect(this.group(title)).toBeVisible({ timeout: DEFAULT_TIMEOUT });
    }
  }

  async openNav(name: string) {
    const link = this.navLink(name);
    await expect(link).toBeVisible({ timeout: DEFAULT_TIMEOUT });
    await link.click();
  }

  async back() {
    await this.backButton.click({ timeout: DEFAULT_TIMEOUT });
  }

  async forward() {
    await this.forwardButton.click({ timeout: DEFAULT_TIMEOUT });
  }

  private hostPathname(): Promise<string> {
    return this.page.evaluate(() => {
      const raw = window.location.hash.replace(/^#/, '') || window.location.pathname;
      try {
        return decodeURIComponent(raw);
      } catch {
        return raw;
      }
    });
  }

  async expectActivePage(path: string) {
    await expect.poll(() => this.hostPathname(), { timeout: DEFAULT_TIMEOUT }).toMatch(new RegExp(`${escapeRegex(path)}$`));
  }
}
