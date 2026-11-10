import { type Page } from '@playwright/test';
import { createBdd } from 'playwright-bdd';

import { TEST_IDS } from '@/shared/test-ids';
import { authenticatedTest, expect } from '../fixtures/authenticated';
import { DEFAULT_TIMEOUT } from '../helpers/timeouts';
import { ChatPage } from '../page-objects/ChatPage';

const { When, Then } = createBdd(authenticatedTest);

// The native Add-to-Dashboard modal, scoped by its always-present favourites
// button (label flips Add → Added once chat is favourited; the regex tolerates both).
const addToDashboardModal = (page: Page) =>
  page.getByRole('dialog').filter({ has: page.getByRole('button', { name: /^Add(ed)? to Favorites$/ }) });

When('the user opens the chat fullscreen view', async ({ authenticatedApp }) => {
  const chat = new ChatPage(authenticatedApp.window);
  await chat.openFullscreen();
});

When('the user opens the Add to Dashboard modal from the chat address bar', async ({ authenticatedApp }) => {
  const chat = new ChatPage(authenticatedApp.window);
  await chat.openAddToDashboard();
});

Then('the chat Add to Dashboard modal is shown without the product actions menu', async ({ authenticatedApp }) => {
  const page = authenticatedApp.window;
  await expect(addToDashboardModal(page)).toBeVisible({ timeout: DEFAULT_TIMEOUT });
  // The chat route owns its single leading affordance — the product ••• menu must not appear.
  await expect(page.getByTestId(TEST_IDS.productActionsMenuTrigger)).toBeHidden();
});

When('the user adds chat as a {string} widget', async ({ authenticatedApp }, sizeLabel: string) => {
  const page = authenticatedApp.window;
  const dialog = addToDashboardModal(page);

  const sizeChip = dialog.getByRole('button', { name: sizeLabel, exact: true });
  await expect(sizeChip).toBeVisible({ timeout: DEFAULT_TIMEOUT });
  await sizeChip.click();
  await expect(sizeChip).toHaveAttribute('aria-pressed', 'true');

  const addButton = dialog.getByRole('button', { name: 'Add', exact: true });
  await expect(addButton).toBeEnabled({ timeout: DEFAULT_TIMEOUT });
  await addButton.click();

  // Adding the widget is an async layout read-modify-write; the panel acknowledges
  // it by flipping the primary button Add → Open once the card is persisted (the
  // dashboard liveQuery has observed the save). Wait for that before the next step:
  // the favourites add is another read-modify-write over the same layout blob, so
  // firing it while the widget save is still in flight lets the two clobber each
  // other (last write wins) and drops the just-added widget card.
  await expect(dialog.getByRole('button', { name: 'Open', exact: true })).toBeVisible({ timeout: DEFAULT_TIMEOUT });
});

When('the user adds chat to favourites', async ({ authenticatedApp }) => {
  const dialog = addToDashboardModal(authenticatedApp.window);
  const favouritesButton = dialog.getByRole('button', { name: 'Add to Favorites', exact: true });
  await expect(favouritesButton).toBeEnabled({ timeout: DEFAULT_TIMEOUT });
  await favouritesButton.click();
});

Then('chat is shown as added to favourites', async ({ authenticatedApp }) => {
  const dialog = addToDashboardModal(authenticatedApp.window);
  await expect(dialog.getByRole('button', { name: 'Added to Favorites', exact: true })).toBeVisible({
    timeout: DEFAULT_TIMEOUT,
  });
});

When('the user closes the Add to Dashboard modal', async ({ authenticatedApp }) => {
  const page = authenticatedApp.window;
  await page.keyboard.press('Escape');
  await expect(addToDashboardModal(page)).toBeHidden({ timeout: DEFAULT_TIMEOUT });
});

Then('the chat widget is shown on the dashboard', async ({ authenticatedApp }) => {
  await expect(authenticatedApp.window.getByTestId(TEST_IDS.chatWidget)).toBeVisible({ timeout: DEFAULT_TIMEOUT });
});
