import { type Page, expect } from '@playwright/test';

import { TEST_IDS } from '@/shared/test-ids';
import { DEFAULT_TIMEOUT } from '../helpers/timeouts';

/**
 * The "Custom chains" settings sub-page: add a chain by WebSocket endpoint.
 * Invalid (non-ws/wss) URLs are rejected client-side with an error toast before
 * any connection is attempted; a syntactically valid but unreachable endpoint
 * surfaces a connection-failed toast once discovery gives up.
 */
export class CustomChainsPage {
  constructor(private readonly page: Page) {}

  get endpointInput() {
    return this.page.getByTestId(TEST_IDS.customChainsEndpointInput);
  }

  get addButton() {
    return this.page.getByTestId(TEST_IDS.customChainsAddButton);
  }

  async enterEndpoint(url: string) {
    await expect(this.endpointInput).toBeVisible({ timeout: DEFAULT_TIMEOUT });
    await this.endpointInput.fill(url);
  }

  async submit() {
    await expect(this.addButton).toBeEnabled({ timeout: DEFAULT_TIMEOUT });
    await this.addButton.click();
  }

  async expectError(title: string) {
    await expect(this.page.getByText(title).first()).toBeVisible({ timeout: DEFAULT_TIMEOUT });
  }
}
