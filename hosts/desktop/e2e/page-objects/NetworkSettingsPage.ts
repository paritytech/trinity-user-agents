import { type Page, expect } from '@playwright/test';

import { DEFAULT_TIMEOUT } from '../helpers/timeouts';

/**
 * The "Testnet endpoint" settings sub-page: an environment `Select` and the
 * logout-confirmation dialog shown when switching to a different environment
 * while signed in. Options are matched by text relative to the current value,
 * so no environment names are hard-coded.
 */
export class NetworkSettingsPage {
  constructor(private readonly page: Page) {}

  get trigger() {
    return this.page.getByRole('combobox').first();
  }

  get dialog() {
    return this.page.getByRole('alertdialog');
  }

  options() {
    return this.page.getByRole('option');
  }

  async openSelect() {
    await expect(this.trigger).toBeVisible({ timeout: DEFAULT_TIMEOUT });
    await this.trigger.click();
    await expect(this.options().first()).toBeVisible({ timeout: DEFAULT_TIMEOUT });
  }

  async currentValue(): Promise<string> {
    return (await this.trigger.textContent())?.trim() ?? '';
  }

  // Radix aria-hides the trigger while the listbox is open, so the current value
  // is read from the (closed) trigger first, then the select is opened and an
  // option chosen by comparing its text to that value.
  async selectDifferentEnvironment() {
    const current = await this.currentValue();
    await this.openSelect();
    for (const option of await this.options().all()) {
      const text = (await option.textContent())?.trim() ?? '';
      if (text && text !== current) {
        await option.click();
        return;
      }
    }
    throw new Error('No environment option different from the current one was found');
  }

  async selectCurrentEnvironment() {
    const current = await this.currentValue();
    await this.openSelect();
    for (const option of await this.options().all()) {
      const text = (await option.textContent())?.trim() ?? '';
      if (text === current) {
        await option.click();
        return;
      }
    }
    throw new Error('Current environment option not found in the list');
  }

  async cancelDialog() {
    await this.dialog.getByRole('button', { name: 'Cancel', exact: true }).click();
  }

  async expectOptionCountAtLeast(n: number) {
    await expect.poll(() => this.options().count(), { timeout: DEFAULT_TIMEOUT }).toBeGreaterThanOrEqual(n);
  }

  async expectDialogVisible() {
    await expect(this.dialog).toBeVisible({ timeout: DEFAULT_TIMEOUT });
  }

  async expectDialogHidden() {
    await expect(this.dialog).toBeHidden({ timeout: DEFAULT_TIMEOUT });
  }

  async expectStillOnSettings() {
    await expect(this.trigger).toBeVisible({ timeout: DEFAULT_TIMEOUT });
  }
}
