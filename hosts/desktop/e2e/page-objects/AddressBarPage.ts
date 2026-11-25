import { type Page, expect } from '@playwright/test';

import { TEST_IDS } from '@/shared/test-ids';
import { DEFAULT_TIMEOUT, LONG_TIMEOUT, SHORT_TIMEOUT } from '../helpers/timeouts';

/**
 * The browser-chrome address bar (header `topBarCenterSlot`) and the input
 * surface it opens.
 *
 * The bar is no longer editable: it shows where you are, and pressing it raises
 * `focusAddressBarSideEffect`, which opens the input spotlight over the screen.
 * Typing, suggestions, keyboard navigation and submit all live in that surface,
 * so this page object owns both halves — every test that "types into the address
 * bar" is really typing into the spotlight.
 *
 * The surface is portalled to `document.body`, so its locators are NOT scoped to
 * the header, and while it is open a full-screen backdrop covers everything
 * behind it: the bar cannot be pressed again until the surface closes. `open()`
 * is idempotent for that reason.
 *
 * Suggestions render only what is persisted. In the no-auth `browser` project
 * nothing is committed unless a scenario seeds it (`helpers/seed-products`), and
 * with nothing to show the panel collapses — assert on `surface` rather than on
 * `suggestions` when the point is that the surface opened.
 */
export class AddressBarPage {
  constructor(private readonly page: Page) {}

  /** The bar itself. Scoped to the header — the new-tab page renders its own. */
  get bar() {
    return this.page.getByRole('banner').getByTestId(TEST_IDS.addressBarInput);
  }

  /** The spotlight's text field. */
  get input() {
    return this.page.getByTestId(TEST_IDS.inputModalityInput);
  }

  get surface() {
    return this.page.getByTestId(TEST_IDS.inputModalitySurface);
  }

  get suggestions() {
    return this.page.getByTestId(TEST_IDS.inputModalitySuggestions);
  }

  /** "Recently Opened" section — renders only when recents are seeded. */
  get recentsSection() {
    return this.page.getByTestId(TEST_IDS.inputModalityRecentsSection);
  }

  /** "Saved" section — every committed (persisted) product not already a recent. */
  get savedSection() {
    return this.page.getByTestId(TEST_IDS.inputModalitySavedSection);
  }

  /**
   * The suffix the field offers to complete to on Tab. Present only once the app
   * knows the network's TLD — it will not complete a name under a guess.
   */
  get ghostSuffix() {
    return this.page.getByTestId(TEST_IDS.inputModalityGhostSuffix);
  }

  get loadingBar() {
    return this.page.getByTestId(TEST_IDS.addressBarLoadingBar);
  }

  /**
   * Press the bar and wait for the surface. A no-op when it is already open —
   * the backdrop would swallow the click, and pressing it would close the very
   * surface the caller is asking for.
   *
   * Retried as a whole because the surface can open on its own, from a beat
   * earlier: the "+" button raises `focusAddressBarSideEffect` too, and that
   * `apply()` is not awaited, so the check below can run in the window before
   * the backdrop is up and the click then lands on it — closing the surface
   * instead of opening one. The next attempt finds the bar clickable again.
   */
  async open() {
    await expect(async () => {
      if (await this.input.isVisible().catch(() => false)) return;

      await expect(this.bar).toBeVisible({ timeout: SHORT_TIMEOUT });
      // force: Electron's appRegion drag layer on the header swallows pointer events.
      await this.bar.click({ force: true });
      await expect(this.input).toBeVisible({ timeout: SHORT_TIMEOUT });
    }).toPass({ timeout: DEFAULT_TIMEOUT });
  }

  /** Dismiss the surface if it is open, leaving the screen underneath usable. */
  async close() {
    await expect(async () => {
      if (!(await this.surface.isVisible().catch(() => false))) return;

      // Escape is handled on `document`, so it is sent to the page rather than
      // to the field: pressing a locator waits for that element to be
      // actionable, and the field is neither stable mid-morph nor visible at
      // all while the scanner is up.
      await this.page.keyboard.press('Escape');
      await expect(this.surface).toBeHidden({ timeout: SHORT_TIMEOUT });
    }).toPass({ timeout: DEFAULT_TIMEOUT });
  }

  /** Replace the field contents, firing the change handler (ghost suffix etc.). */
  async type(text: string) {
    await this.open();
    await this.input.fill(text);
  }

  async pressKey(key: string) {
    await this.input.press(key);
  }

  /** Type a value and submit it (Enter), the way a user opens a product. */
  /**
   * Filling is retried together with the open, not after it: `open()` already
   * handles a stray click landing on the backdrop, but when that lands in the
   * window *after* it returned, the field detaches mid-fill and never comes back.
   */
  async submit(value: string) {
    await expect(async () => {
      await this.open();
      await this.input.fill(value, { timeout: SHORT_TIMEOUT });
      // Commit inside the retried block, not after it. The surface re-renders
      // between the fill and the commit — suggestions settle, the ghost suffix
      // resolves once the network's TLD does — and that detaches the field. A
      // press left outside then waits out its whole timeout on a node that will
      // never be actionable again. Re-opening and re-submitting the same value is
      // idempotent: it routes to the same product either way.
      await this.input.press('Enter', { timeout: SHORT_TIMEOUT });
    }).toPass({ timeout: DEFAULT_TIMEOUT });
  }

  /**
   * Tab commits whatever suffix the field is offering, so the expectation is read
   * off the app rather than hardcoded — the suffix is per-network and this project
   * asserts that completion happens, not which network it is on.
   */
  async expectTabCompletes(label: string) {
    await expect(this.ghostSuffix).toBeVisible({ timeout: DEFAULT_TIMEOUT });
    const suffix = (await this.ghostSuffix.innerText()).trim();
    expect(suffix).toMatch(/^\.[a-z0-9][a-z0-9-]*$/);

    await this.pressKey('Tab');
    await this.expectValue(`${label}${suffix}`);
  }

  async expectValue(value: string) {
    await expect(this.input).toHaveValue(value, { timeout: DEFAULT_TIMEOUT });
  }

  async expectSurfaceVisible() {
    await expect(this.surface).toBeVisible({ timeout: DEFAULT_TIMEOUT });
  }

  async expectSurfaceHidden() {
    await expect(this.surface).toBeHidden({ timeout: DEFAULT_TIMEOUT });
  }

  async expectFieldFocused() {
    await expect(this.input).toBeFocused({ timeout: DEFAULT_TIMEOUT });
  }

  async expectSuggestionsVisible() {
    await expect(this.suggestions).toBeVisible({ timeout: DEFAULT_TIMEOUT });
  }

  async expectSuggestionsHidden() {
    await expect(this.suggestions).toBeHidden({ timeout: DEFAULT_TIMEOUT });
  }

  async expectRecentsSectionVisible() {
    await expect(this.recentsSection).toBeVisible({ timeout: DEFAULT_TIMEOUT });
  }

  async expectSavedSectionVisible() {
    await expect(this.savedSection).toBeVisible({ timeout: DEFAULT_TIMEOUT });
  }

  /** Assert the suggestions surface an item whose domain text matches. */
  async expectSuggestionItem(baseName: string) {
    await expect(this.suggestions.getByText(baseName, { exact: false }).first()).toBeVisible({ timeout: DEFAULT_TIMEOUT });
  }

  /** Click well outside the card — hits the surface's own backdrop. */
  async clickOutside() {
    await this.page.mouse.click(10, 400);
  }

  async expectLoadingBarVisible() {
    await expect(this.loadingBar).toBeVisible({ timeout: DEFAULT_TIMEOUT });
  }

  async expectLoadingBarHidden() {
    await expect(this.loadingBar).toBeHidden({ timeout: LONG_TIMEOUT });
  }
}
