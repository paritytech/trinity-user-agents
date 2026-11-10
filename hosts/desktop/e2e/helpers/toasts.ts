import { type Page, expect } from '@playwright/test';

import { DEFAULT_TIMEOUT } from './timeouts';

/**
 * Wait until no toast is covering the app chrome.
 *
 * Toasts render top-center over the header, above everything, and `src/index.css`
 * forces `pointer-events: auto !important` on them — so a click on a control
 * underneath is intercepted. Worse, sonner pauses a toast's dismiss timer while the
 * toaster is hovered, and Playwright parks the mouse over the toast while retrying
 * the intercepted click: the toast never expires and the click never lands, until
 * the click times out. Moving the pointer away first releases that pause and lets
 * the toast dismiss on its own.
 *
 * Call this before clicking header/product chrome in a step that follows a
 * toast-producing action (favorites, add-to-dashboard, offline access).
 */
export async function waitForToastsToClear(page: Page) {
  const toasts = page.locator('[data-sonner-toast]');
  if ((await toasts.count()) === 0) return;

  await page.mouse.move(0, 0);
  await expect(toasts).toHaveCount(0, { timeout: DEFAULT_TIMEOUT });
}
