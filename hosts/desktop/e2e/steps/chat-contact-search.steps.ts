import { expect } from '@playwright/test';
import { createBdd } from 'playwright-bdd';

import { TEST_IDS } from '@/shared/test-ids';
import { authenticatedTest } from '../fixtures/authenticated';
import { DEFAULT_TIMEOUT } from '../helpers/timeouts';
import { ChatPage } from '../page-objects/ChatPage';
import { ContactSearchPage } from '../page-objects/ContactSearchPage';

const { When, Then } = createBdd(authenticatedTest);

Then('the chat room list is empty', async ({ authenticatedApp }) => {
  await expect(authenticatedApp.window.getByTestId(TEST_IDS.chatRoomItem)).toHaveCount(0, { timeout: DEFAULT_TIMEOUT });
});

Then('the contact search is open', async ({ authenticatedApp }) => {
  await expect(new ContactSearchPage(authenticatedApp.window).searchInput).toBeVisible({ timeout: DEFAULT_TIMEOUT });
});

// Single-client chat navigation, shared with chat-list.feature. The two-client
// pair suite has its own Alice/Bob-prefixed wording in chat-p2p-pair.steps.ts.

When('the user opens the chat as a tab', async ({ authenticatedApp }) => {
  await new ChatPage(authenticatedApp.window).openFullscreen();
});

When('the user opens the contact search', async ({ authenticatedApp }) => {
  await new ContactSearchPage(authenticatedApp.window).openFromFullscreen();
});

When('the user searches contacts for {string}', async ({ authenticatedApp }, query: string) => {
  await new ContactSearchPage(authenticatedApp.window).typeQuery(query);
});

Then('the contact search shows no results', async ({ authenticatedApp }) => {
  const search = new ContactSearchPage(authenticatedApp.window);
  // Let the (single, non-refetching) search settle, then assert the empty set.
  await authenticatedApp.window.waitForTimeout(1_500);
  await expect(search.resultItems).toHaveCount(0, { timeout: DEFAULT_TIMEOUT });
});

When('the user clears the contact search', async ({ authenticatedApp }) => {
  await new ContactSearchPage(authenticatedApp.window).searchInput.fill('');
});

Then('the contact search query is empty', async ({ authenticatedApp }) => {
  await expect(new ContactSearchPage(authenticatedApp.window).searchInput).toHaveValue('', { timeout: DEFAULT_TIMEOUT });
});
