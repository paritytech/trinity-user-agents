import { type Page, expect } from '@playwright/test';

import { TEST_IDS } from '@/shared/test-ids';
import { DEFAULT_TIMEOUT, VERY_LONG_TIMEOUT } from '../helpers/timeouts';

export class ContactSearchPage {
  constructor(private readonly page: Page) {}

  get searchInput() {
    return this.page.getByTestId(TEST_IDS.contactSearchInput);
  }

  get resultItems() {
    return this.page.getByTestId(TEST_IDS.contactResultItem);
  }

  resultByName(username: string) {
    return this.resultItems.filter({ hasText: username });
  }

  get messageInput() {
    return this.page.getByTestId(TEST_IDS.chatMessageInput);
  }

  get sendButton() {
    return this.page.getByTestId(TEST_IDS.chatSendButton);
  }

  /** Fullscreen must be opened beforehand via ChatPage.openFullscreen(). */
  async openFromFullscreen() {
    await expect(this.searchInput).toBeVisible({ timeout: DEFAULT_TIMEOUT });
    await this.searchInput.click();
  }

  async typeQuery(query: string) {
    await expect(this.searchInput).toBeVisible({ timeout: DEFAULT_TIMEOUT });
    await this.searchInput.fill(query);
  }

  async waitForResult(username: string) {
    await expect(this.resultByName(username)).toBeVisible({ timeout: DEFAULT_TIMEOUT });
  }

  /**
   * Wait for a username to appear in the results, re-issuing the search on each
   * miss instead of waiting on a single, stale query.
   *
   * The contact search (`useContactSearch`) only queries the indexer on input
   * change — it never refetches on its own. So a single `fill(query)` followed
   * by a long wait is futile: if that one query runs before the peer's on-chain
   * username attestation has propagated to the indexer (or before the search
   * manager became `ready`), the empty result set sticks forever. A real user
   * would retype; this re-fills the input each round so the debounced query
   * fires again, tolerating propagation lag without a giant fixed wait.
   */
  async findResultByRetyping(query: string, options: { totalTimeout?: number; pollTimeout?: number } = {}) {
    const { totalTimeout = VERY_LONG_TIMEOUT, pollTimeout = DEFAULT_TIMEOUT / 3 } = options;
    const result = this.resultByName(query);
    const deadline = Date.now() + totalTimeout;

    await expect(this.searchInput).toBeVisible({ timeout: DEFAULT_TIMEOUT });

    for (;;) {
      try {
        await expect(result).toBeVisible({ timeout: pollTimeout });
        return;
      } catch (error) {
        if (Date.now() >= deadline) throw error;
        // Re-issue the search: clearing collapses the result list, so refilling
        // forces a fresh debounced query against the indexer.
        await this.searchInput.fill('');
        await this.searchInput.fill(query);
      }
    }
  }

  /**
   * Select a contact result, landing directly in the draft invitation room
   * where the single invitation message is composed.
   */
  async selectResult(username: string) {
    await this.resultByName(username).click();
    await expect(this.messageInput).toBeVisible({ timeout: DEFAULT_TIMEOUT });
  }

  /**
   * Type the invitation message. (The redesigned flow composes the single
   * invitation message in the draft room — there is no separate welcome field.)
   */
  async fillWelcome(text: string) {
    await expect(this.messageInput).toBeVisible({ timeout: DEFAULT_TIMEOUT });
    await this.messageInput.fill(text);
  }

  /** The error line the draft room renders when a send failed and can be retried. */
  get sendError() {
    return this.page.getByText('Could not send the invitation', { exact: false });
  }

  /**
   * Send the invitation message (which creates the outgoing request) and wait
   * for the draft compose input to dismiss.
   *
   * Retries only on a **surfaced** failure — the first attempt can fail with
   * "Could not find encryption key for {address}" when the peer's P256 key
   * hasn't propagated through the Alice-side chain client yet (common right
   * after both users finished sign-in), and the draft room says so and comes
   * back. A timeout is not a retryable signal here: `MessageInput` unmounts the
   * send button for the whole `useTransition` that the send runs in, so while a
   * send is still in flight there is nothing left to click and a retry can only
   * fail on a locator that will not resolve until the send settles. So each
   * attempt waits out the on-chain round trip, and the next one starts only
   * once the app has said the last one failed.
   */
  async submitRequest(options: { retries?: number } = {}) {
    const { retries = 3 } = options;
    for (let attempt = 1; attempt <= retries; attempt++) {
      await expect(this.sendButton).toBeVisible({ timeout: DEFAULT_TIMEOUT });
      await this.sendButton.click();

      // Whichever lands first decides: the draft closing is success, the error
      // line is a retryable failure. Racing them means a fast failure is not
      // charged the full on-chain budget.
      const settled = await Promise.race([
        this.messageInput.waitFor({ state: 'hidden', timeout: VERY_LONG_TIMEOUT }).then(() => 'sent' as const),
        this.sendError.waitFor({ state: 'visible', timeout: VERY_LONG_TIMEOUT }).then(() => 'failed' as const),
      ]).catch(() => 'stuck' as const);

      if (settled === 'sent') return;
      if (settled === 'stuck') {
        throw new Error(
          '[ContactSearchPage] Send invitation neither dismissed the draft nor reported an error ' +
            `within ${VERY_LONG_TIMEOUT}ms — the send is still in flight, so retrying cannot help.`,
        );
      }
      if (attempt === retries) {
        throw new Error(`[ContactSearchPage] Send invitation reported an error on all ${retries} attempts`);
      }
      console.warn(`[contact-search] invite ${attempt}/${retries} reported an error, retrying`);
    }
  }
}
