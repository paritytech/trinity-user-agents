import { type Page, expect } from '@playwright/test';

import { TEST_IDS } from '@/shared/test-ids';
import { escapeRegex } from '../helpers/regex';
import { DEFAULT_TIMEOUT } from '../helpers/timeouts';

import { AddressBarPage } from './AddressBarPage';

/**
 * Tab-bar interactions and selected-tab observation. The selected tab is read
 * from the host route — a product tab is `/product/<id>`, a new tab is
 * `/new-tab/<id>` — which is the stable signal for "which tab is active".
 */
export class TabsPage {
  constructor(private readonly page: Page) {}

  get newTabButton() {
    return this.page.getByTestId(TEST_IDS.newTabButton);
  }

  get tabStrip() {
    return this.page.getByTestId(TEST_IDS.browserTabStrip);
  }

  get hoverCard() {
    return this.page.getByTestId(TEST_IDS.tabHoverCard);
  }

  get hoverRam() {
    return this.page.getByTestId(TEST_IDS.tabHoverRam);
  }

  private tabChips() {
    return this.page.locator('[data-tab-id]');
  }

  /**
   * Create a real new-tab tab via the "+" button (navigates to /new-tab/<id>).
   *
   * "+" also raises `focusAddressBarSideEffect`, so the input surface opens over
   * the new tab with a backdrop across the whole window. Dismiss it: a caller
   * that goes on to click a tab chip would otherwise spend its click closing the
   * surface, exactly as a user's first click would.
   */
  async openNewTab() {
    // force: true — the draggable header intercepts pointer events.
    await this.newTabButton.click({ force: true });

    const bar = new AddressBarPage(this.page);
    // The surface is coming, so wait for it rather than race it: `apply()` is
    // not awaited, and a `close()` that checks a beat too early returns before
    // the backdrop is even up.
    await bar.expectSurfaceVisible();
    await bar.close();
  }

  /** Tab ids in DOM (left-to-right) order. */
  tabIds(): Promise<string[]> {
    return this.tabChips().evaluateAll(nodes =>
      nodes.map(node => node.getAttribute('data-tab-id') ?? '').filter(id => id.length > 0),
    );
  }

  async expectTabCount(count: number) {
    await expect.poll(() => this.tabChips().count(), { timeout: DEFAULT_TIMEOUT }).toBe(count);
  }

  async expectTabStripHidden() {
    await expect(this.tabStrip).toBeHidden({ timeout: DEFAULT_TIMEOUT });
  }

  async expectTabStripVisible() {
    await expect(this.tabStrip).toBeVisible({ timeout: DEFAULT_TIMEOUT });
  }

  async selectTab(id: string) {
    // force: true — the draggable chip intercepts pointer events.
    await this.page.locator(`[data-tab-id="${id}"]`).click({ force: true });
  }

  async hoverTab(id: string) {
    await this.page.locator(`[data-tab-id="${id}"]`).hover();
  }

  async expectHoverCardVisible() {
    await expect(this.hoverCard).toBeVisible({ timeout: DEFAULT_TIMEOUT });
  }

  async expectHoverRamVisible() {
    await expect(this.hoverRam).toBeVisible({ timeout: DEFAULT_TIMEOUT });
  }

  /**
   * Drag the tab at index `from` past the tab at index `to` to reorder them.
   * dnd-kit's PointerSensor needs a >5px move to activate plus intermediate
   * moves to register the drag, so this drives the mouse in manual steps
   * rather than a single dragTo.
   */
  async dragTab(from: number, to: number) {
    const chips = this.tabChips();
    const source = await chips.nth(from).boundingBox();
    const target = await chips.nth(to).boundingBox();
    if (!source || !target) throw new Error('tab chip not found for drag');

    const startX = source.x + source.width / 2;
    const startY = source.y + source.height / 2;
    const endX = target.x + target.width / 2;
    const endY = target.y + target.height / 2;

    await this.page.mouse.move(startX, startY);
    await this.page.mouse.down();
    // Cross the activation threshold, then walk to the target in steps.
    const steps = 10;
    for (let i = 1; i <= steps; i++) {
      await this.page.mouse.move(startX + ((endX - startX) * i) / steps, startY + ((endY - startY) * i) / steps);
    }
    // Overshoot slightly so the sortable settles past the target's midpoint.
    await this.page.mouse.move(endX + (endX > startX ? 12 : -12), endY);
    await this.page.mouse.up();
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

  async expectActiveProduct(identifier: string) {
    await expect
      .poll(() => this.hostPathname(), { timeout: DEFAULT_TIMEOUT })
      .toMatch(new RegExp(`^/product/${escapeRegex(identifier)}(?:/|$)`));
  }

  async expectActiveNewTab() {
    await expect.poll(() => this.hostPathname(), { timeout: DEFAULT_TIMEOUT }).toMatch(/^\/new-tab\//);
  }

  /**
   * "Focused" now means the input surface opened with its field ready — the bar
   * itself is a button and never takes a caret.
   */
  async expectAddressBarFocused() {
    const bar = new AddressBarPage(this.page);
    await bar.expectSurfaceVisible();
    await bar.expectFieldFocused();
  }
}
