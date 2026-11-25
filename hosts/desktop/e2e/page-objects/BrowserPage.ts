import { type ElectronApplication, type Page, expect } from '@playwright/test';

import { TEST_IDS } from '@/shared/test-ids';
import { CHAT, DASHBOARD_TAB_ID } from '@/aggregates/browser-tabs/constants';
import { escapeRegex } from '../helpers/regex';
import { DEFAULT_TIMEOUT, SHORT_TIMEOUT } from '../helpers/timeouts';

import { AddressBarPage } from './AddressBarPage';

/**
 * Page Object for browser tab interactions.
 * Handles opening products in new tabs, switching between tabs,
 * and verifying that tab content is loaded (not a gray empty page).
 */
export class BrowserPage {
  constructor(
    private readonly page: Page,
    _app: ElectronApplication,
  ) {}

  /**
   * The header address bar — a button showing where you are, not an editable
   * field. Typing goes through the input surface it opens; `AddressBarPage` owns
   * both halves.
   */
  get addressBar() {
    return new AddressBarPage(this.page);
  }

  /** All visible tab elements */
  get tabs() {
    return this.page.locator('[data-tab-id]');
  }

  /** New tab "+" button */
  private get newTabButton() {
    return this.page.getByTestId(TEST_IDS.newTabButton);
  }

  /** Get a specific tab by its identifier */
  tab(identifier: string) {
    return this.page.locator(`[data-tab-id="${identifier}"]`);
  }

  /** Open a product by typing its domain into the address bar.
   *  If tabs already exist, clicks "+" first; otherwise uses the address bar directly. */
  async openProductInNewTab(domain: string) {
    const hasNewTab = await this.newTabButton.isVisible().catch(() => false);

    if (hasNewTab) {
      // force: true needed because Electron's appRegion: 'drag' on the header
      // intercepts pointer events even though the button has appRegion: 'no-drag'
      await this.newTabButton.click({ force: true });
    }

    await this.addressBar.submit(domain);

    // <Tabs> hides the [data-tab-id] chip when there's exactly one selected
    // tab, so wait on the host route instead.
    await this.page.waitForURL(new RegExp(`#/product/${escapeRegex(domain)}(?:[/?]|$)`), {
      timeout: DEFAULT_TIMEOUT,
    });
  }

  /** Click on a specific tab to switch to it */
  async switchToTab(identifier: string) {
    await this.tab(identifier).click();
    // Allow webview visibility toggle and content to settle
    await this.page.waitForTimeout(2_000);
  }

  /**
   * Close open tabs until at most `baseline` chips remain (default: none).
   *
   * Every close re-renders the strip — and `<Tabs>` unmounts it entirely once a
   * single selected tab is left, which also cascades into
   * `cleanupOrphanDashboardTab` — so the button we resolved can detach before
   * the click dispatches. A plain `click()` then waits out its full timeout on a
   * locator that will never resolve again. Poll the tab count instead and treat
   * a lost click target as progress.
   */
  async closeAllTabs(baseline = 0) {
    const closeButtons = this.page.getByRole('button', { name: 'Close tab' });

    await expect
      .poll(
        async () => {
          if ((await this.tabs.count()) <= baseline) return true;

          // force: Electron's appRegion drag layer on the header swallows pointer events.
          await closeButtons
            .last()
            .click({ force: true, timeout: SHORT_TIMEOUT })
            .catch(() => {});

          return (await this.tabs.count()) <= baseline;
        },
        { timeout: DEFAULT_TIMEOUT, message: `tabs did not close down to ${baseline}` },
      )
      .toBe(true);
  }

  /** Get all tab identifiers in order */
  async getTabIdentifiers(): Promise<string[]> {
    const tabElements = await this.tabs.all();
    const identifiers: string[] = [];

    for (const tabEl of tabElements) {
      const id = await tabEl.getAttribute('data-tab-id');
      if (id) identifiers.push(id);
    }

    return identifiers;
  }

  /** Cycle through every open tab sequentially */
  async cycleThroughAllTabs() {
    const identifiers = await this.getTabIdentifiers();

    for (const id of identifiers) {
      await this.switchToTab(id);
    }
  }

  /**
   * Assert that the currently visible tab has a loaded webview.
   * A "gray page" means the webview exists but shows no content.
   * We verify the webview has a non-empty URL (content has loaded).
   * Also asserts no permission/alias dialog is left covering the product —
   * the autotest auto-approver feature should have dismissed it immediately.
   */
  async expectActiveTabHasContent(tabId?: string) {
    await expect(
      this.page.getByTestId(TEST_IDS.aliasPermissionAllow),
      'alias permission dialog should be auto-approved, not overlapping product content',
    ).toBeHidden({ timeout: DEFAULT_TIMEOUT });
    await expect(
      this.page.getByTestId(TEST_IDS.permissionDialogAllowAlways),
      'device/remote permission dialog should be auto-approved, not overlapping product content',
    ).toBeHidden({ timeout: DEFAULT_TIMEOUT });
    await expect(
      this.page.getByTestId(TEST_IDS.productSubtreeAllow),
      'product account-subtree request should be auto-approved, not overlapping product content',
    ).toBeHidden({ timeout: DEFAULT_TIMEOUT });

    // The native system tabs (chat, dashboard) render React views, not a product
    // `<webview>`, so the `div[aria-hidden="false"] webview` assertion is
    // structurally invalid for them — it only ever "passed" when a product pane
    // was transiently still un-hidden mid tab-switch (the TC-4.3.1 flake). Assert
    // their own content instead.
    if (tabId === CHAT) {
      await expect(this.page.getByTestId(TEST_IDS.chatRoomList)).toBeVisible({ timeout: DEFAULT_TIMEOUT });
      return;
    }
    if (tabId === DASHBOARD_TAB_ID) {
      await expect(
        this.page.getByTestId(TEST_IDS.dashboardGrid).or(this.page.getByTestId(TEST_IDS.dashboardEmptyState)),
      ).toBeVisible({ timeout: DEFAULT_TIMEOUT });
      return;
    }

    const visibleWebview = this.page.locator('div[aria-hidden="false"] webview');

    await expect(visibleWebview).toBeAttached({ timeout: DEFAULT_TIMEOUT });

    // Verify the webview has loaded a real URL (not empty string). Poll: the
    // guest's getURL can be transiently empty during an internal SPA navigation.
    await expect.poll(() => this.activeWebviewHasUrl(), { timeout: DEFAULT_TIMEOUT }).toBe(true);
  }

  /** True when the active (visible) tab's webview reports a non-empty URL. */
  private activeWebviewHasUrl(): Promise<boolean> {
    return this.page.evaluate(() => {
      const wv = document.querySelector('div[aria-hidden="false"] webview');
      if (!wv || !('getURL' in wv)) return false;
      const getURL = wv.getURL;
      if (typeof getURL !== 'function') return false;
      return Boolean(getURL.call(wv));
    });
  }

  /**
   * Reload the active product via the address-bar refresh control
   * (`AddressBarRefreshButton`). The input surface is dismissed first — while it
   * is open its backdrop covers the whole header. This is the address-bar
   * "control" reload path — distinct from the View → Reload accelerator covered
   * by TC-4.3.4.
   */
  async reloadActiveProductViaControl() {
    await this.addressBar.close();
    const refresh = this.page.getByTestId(TEST_IDS.browserRefreshButton);
    await expect(refresh).toBeVisible({ timeout: DEFAULT_TIMEOUT });
    await refresh.click({ timeout: DEFAULT_TIMEOUT });
  }

  /**
   * Assert the active product SPA keeps its content rendered across a short
   * settle window — catches a product that mounts then blanks to a gray page.
   * Re-asserts `expectActiveTabHasContent` before and after the settle.
   */
  async expectActiveTabKeepsContent() {
    await this.expectActiveTabHasContent();
    // Brief settle so a delayed blank-out would surface; not a `timeout:` option.
    await this.page.waitForTimeout(3_000);
    await this.expectActiveTabHasContent();
  }

  /** Switch to each tab and verify it renders content */
  async expectAllTabsHaveContent() {
    const identifiers = await this.getTabIdentifiers();

    for (const id of identifiers) {
      await this.switchToTab(id);
      await this.expectActiveTabHasContent(id);
    }
  }
}
