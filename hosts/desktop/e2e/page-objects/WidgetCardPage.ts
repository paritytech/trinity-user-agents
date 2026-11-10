import { type Page, expect } from '@playwright/test';

import { TEST_IDS } from '@/shared/test-ids';
import { DEFAULT_TIMEOUT, LONG_TIMEOUT } from '../helpers/timeouts';

/**
 * Page Object for a product widget card on the dashboard
 * (`DashboardCardChrome` rendering a `dashboardProductWidget`): its topbar
 * fullscreen action and the ••• card menu (resize / reload / cleanup / remove),
 * plus the card body's webview.
 *
 * The card's menu trigger and fullscreen button are `opacity-0` until hover but
 * stay hit-testable, so clicks are forced (same approach as
 * `DashboardPage.removeAllWidgets`). The body webview is scoped to the card so a
 * backgrounded product tab's webview can't shadow it.
 */
export class WidgetCardPage {
  constructor(private readonly page: Page) {}

  get card() {
    return this.page.getByTestId(TEST_IDS.dashboardProductWidget).first();
  }

  get menuTrigger() {
    return this.card.getByTestId(TEST_IDS.dashboardWidgetMenuTrigger);
  }

  get fullscreenButton() {
    return this.card.getByTestId(TEST_IDS.dashboardWidgetFullscreenButton);
  }

  async expectVisible() {
    await expect(this.card).toBeVisible({ timeout: LONG_TIMEOUT });
  }

  private async openMenu() {
    await expect(this.menuTrigger).toBeAttached({ timeout: LONG_TIMEOUT });
    await this.card.scrollIntoViewIfNeeded();
    // The trigger is opacity-0 until the card is hovered — reveal it first.
    await this.card.hover();
    // Click WITHOUT force so Playwright hit-tests the target and waits for it to
    // be stable: a force-click on a not-yet-settled dashboard (a lingering
    // product tab chip or the "added to Dashboard" success toast still shifting
    // layout) can land on the wrong element and navigate away, leaving the menu
    // closed — the TC-3.3.2 flake. The menu is a controlled Radix dropdown, so
    // confirm the trigger reports expanded; retry the click if it didn't open.
    await expect(async () => {
      if ((await this.menuTrigger.getAttribute('aria-expanded')) !== 'true') {
        await this.menuTrigger.click({ timeout: DEFAULT_TIMEOUT });
      }
      await expect(this.menuTrigger).toHaveAttribute('aria-expanded', 'true');
    }).toPass({ timeout: LONG_TIMEOUT });
  }

  async clickFullscreen() {
    await expect(this.fullscreenButton).toBeAttached({ timeout: LONG_TIMEOUT });
    await this.card.scrollIntoViewIfNeeded();
    // The fullscreen button is opacity-0 until the card is hovered — reveal it
    // first, then click WITHOUT force so Playwright hit-tests the target and
    // waits for the dashboard to settle. A force-click skips the hit-test and,
    // on a not-yet-settled dashboard, lands on the wrong element so the
    // fullscreen navigation never fires — the TC-3.3.3 flake, the same shape as
    // the TC-3.3.2 menu-trigger fix in openMenu().
    await this.card.hover();
    await this.fullscreenButton.click({ timeout: DEFAULT_TIMEOUT });
  }

  // Order by ascending height footprint (small h2 < medium h4 ≈ horizontal h4 <
  // large h8). A widget added as "Large" can shrink to whichever of these its
  // manifest offers.
  private static readonly SMALLER_SIZE_PREFERENCE = ['Small', 'Medium', 'Horizontal'];

  /**
   * Resize the card to the first offered size smaller (shorter) than its current
   * Large footprint. The exact set of sizes is manifest-driven, so we pick the
   * first preference that the menu actually renders.
   */
  async resizeToSmaller() {
    await this.openMenu();

    for (const label of WidgetCardPage.SMALLER_SIZE_PREFERENCE) {
      const sizeItem = this.page.getByRole('menuitem', { name: label, exact: true });
      if ((await sizeItem.count()) > 0 && (await sizeItem.first().isVisible())) {
        await sizeItem.first().click();
        return;
      }
    }

    const offered = await this.page.getByRole('menuitem').allInnerTexts();
    throw new Error(`No smaller widget size offered in the card menu. Items: ${JSON.stringify(offered)}`);
  }

  async cleanup() {
    await this.openMenu();
    const cleanupItem = this.page.getByRole('menuitem', { name: 'Cleanup Dashboard', exact: true });
    await expect(cleanupItem).toBeVisible({ timeout: DEFAULT_TIMEOUT });
    await cleanupItem.click();
  }

  async remove() {
    await this.openMenu();
    const remove = this.page.getByTestId(TEST_IDS.dashboardWidgetMenuRemove);
    await expect(remove).toBeVisible({ timeout: DEFAULT_TIMEOUT });
    await remove.click();
  }

  async reload() {
    await this.openMenu();
    const reload = this.page.getByTestId(TEST_IDS.productWidgetReloadButton);
    await expect(reload).toBeVisible({ timeout: DEFAULT_TIMEOUT });
    await reload.click();
  }

  async expectGone() {
    await expect(this.page.getByTestId(TEST_IDS.dashboardProductWidget)).toHaveCount(0, { timeout: DEFAULT_TIMEOUT });
  }

  /** Pixel height of the card frame — used to prove a resize changed the footprint. */
  async cardHeight(): Promise<number> {
    const box = await this.card.boundingBox();
    return box?.height ?? 0;
  }

  // --- Webview body -----------------------------------------------------------

  /** The card body's webview has loaded a real URL (not a blank "gray page"). */
  async expectWebviewLoaded() {
    const webview = this.card.locator('webview');
    await expect(webview).toBeAttached({ timeout: LONG_TIMEOUT });

    await expect.poll(() => this.cardWebviewUrl(), { timeout: LONG_TIMEOUT }).toMatch(/\S/);
  }

  private cardWebviewUrl(): Promise<string> {
    return this.page.evaluate(testId => {
      const card = document.querySelector(`[data-testid="${testId}"]`);
      const wv = card?.querySelector('webview');
      if (!wv || !('getURL' in wv)) return '';
      const getURL = wv.getURL;
      if (typeof getURL !== 'function') return '';
      return String(getURL.call(wv) ?? '');
    }, TEST_IDS.dashboardProductWidget);
  }

  async setReloadProbe() {
    await this.expectWebviewLoaded();
    await this.page.evaluate(async testId => {
      const card = document.querySelector(`[data-testid="${testId}"]`);
      // eslint-disable-next-line @typescript-eslint/consistent-type-assertions
      const wv = card?.querySelector('webview') as
        | (HTMLElement & {
            executeJavaScript: (code: string) => Promise<unknown>;
          })
        | null;
      if (!wv) throw new Error('No widget webview found to seed the reload probe');
      await wv.executeJavaScript('window.__e2eWidgetReloadProbe = "set"; true');
    }, TEST_IDS.dashboardProductWidget);
  }

  /**
   * Reloading the widget remounts its webview (a fresh `key`), so the probe set
   * before the reload is gone once the new document loads. Poll because the
   * webview is briefly detached mid-remount and `executeJavaScript` can reject.
   */
  async expectReloadProbeCleared() {
    await expect
      .poll(
        () =>
          this.page.evaluate(async testId => {
            const card = document.querySelector(`[data-testid="${testId}"]`);
            // eslint-disable-next-line @typescript-eslint/consistent-type-assertions
            const wv = card?.querySelector('webview') as
              | (HTMLElement & {
                  executeJavaScript: (code: string) => Promise<unknown>;
                })
              | null;
            if (!wv) return 'pending';
            try {
              return await wv.executeJavaScript('window.__e2eWidgetReloadProbe ?? null');
            } catch {
              return 'pending';
            }
          }, TEST_IDS.dashboardProductWidget),
        { timeout: LONG_TIMEOUT },
      )
      .toBeNull();
  }
}
