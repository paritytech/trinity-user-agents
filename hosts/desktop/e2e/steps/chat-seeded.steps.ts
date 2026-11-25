import { createBdd } from 'playwright-bdd';

import { authenticatedTest, expect } from '../fixtures/authenticated';
import { type SeedChatMessageSpec, seedChatRooms } from '../helpers/seed-chat';
import { DEFAULT_TIMEOUT, VERY_LONG_TIMEOUT } from '../helpers/timeouts';
import { AddWidgetModalPage } from '../page-objects/AddWidgetModalPage';
import { ChatPage } from '../page-objects/ChatPage';
import { DashboardPage } from '../page-objects/DashboardPage';

const { Given, When, Then } = createBdd(authenticatedTest);

/** Deterministic, render-safe peer id derived from the display name. */
function peerIdFor(name: string): string {
  return `seed-peer-${name.replace(/\s+/g, '-').toLowerCase()}`;
}

/** A mixed incoming/outgoing sample conversation, all dated today. */
const SAMPLE_MESSAGES: SeedChatMessageSpec[] = [
  { text: 'Hello there', direction: 'incoming' },
  { text: 'How are you?', direction: 'incoming' },
  { text: 'My own message', direction: 'outgoing' },
  { text: 'Doing great thanks', direction: 'outgoing' },
];

Given('the chat conversation {string} is seeded with sample messages', async ({ authenticatedApp }, name: string) => {
  await seedChatRooms(authenticatedApp.window, [{ peerId: peerIdFor(name), peerUsername: name, messages: SAMPLE_MESSAGES }]);
});

Given('the chat conversation {string} is seeded across two days', async ({ authenticatedApp }, name: string) => {
  await seedChatRooms(authenticatedApp.window, [
    {
      peerId: peerIdFor(name),
      peerUsername: name,
      messages: [
        { text: 'Yesterday message', direction: 'incoming', ageDays: 1 },
        { text: 'Another yesterday', direction: 'outgoing', ageDays: 1 },
        { text: 'Today message', direction: 'incoming', ageDays: 0 },
      ],
    },
  ]);
});

Given('an empty chat conversation {string} is seeded', async ({ authenticatedApp }, name: string) => {
  await seedChatRooms(authenticatedApp.window, [{ peerId: peerIdFor(name), peerUsername: name, messages: [] }]);
});

Given(
  'two chat conversations {string} and {string} are seeded with different activity',
  async ({ authenticatedApp }, older: string, newer: string) => {
    await seedChatRooms(authenticatedApp.window, [
      { peerId: peerIdFor(older), peerUsername: older, messages: [{ text: 'old activity', direction: 'incoming', ageDays: 1 }] },
      { peerId: peerIdFor(newer), peerUsername: newer, messages: [{ text: 'new activity', direction: 'incoming', ageDays: 0 }] },
    ]);
  },
);

// "the user opens the chat fullscreen view" is defined once in
// chat-add-to-dashboard.steps.ts (both files load in the chat project, so a
// single shared definition avoids a duplicate-step error).

When('the user opens the seeded conversation {string}', async ({ authenticatedApp }, name: string) => {
  const chat = new ChatPage(authenticatedApp.window);
  await expect(chat.roomItemByName(name)).toBeVisible({ timeout: VERY_LONG_TIMEOUT });
  await chat.selectSession(name);
});

Then('at least {int} date separators are shown', async ({ authenticatedApp }, count: number) => {
  const chat = new ChatPage(authenticatedApp.window);
  await expect(chat.dateSeparators.first()).toBeVisible({ timeout: DEFAULT_TIMEOUT });
  expect(await chat.dateSeparators.count()).toBeGreaterThanOrEqual(count);
});

Then('the {string} date separator is shown', async ({ authenticatedApp }, label: string) => {
  const chat = new ChatPage(authenticatedApp.window);
  await expect(chat.dateSeparators.filter({ hasText: label })).toBeVisible({ timeout: DEFAULT_TIMEOUT });
});

Then('the no-messages placeholder is shown', async ({ authenticatedApp }) => {
  const chat = new ChatPage(authenticatedApp.window);
  await expect(chat.noMessagesPlaceholder).toBeVisible({ timeout: DEFAULT_TIMEOUT });
});

When('the user opens the message context menu on {string}', async ({ authenticatedApp }, text: string) => {
  const chat = new ChatPage(authenticatedApp.window);
  await chat.openContextMenu(text);
});

Then('the message context menu is shown', async ({ authenticatedApp }) => {
  const chat = new ChatPage(authenticatedApp.window);
  await expect(chat.contextMenu).toBeVisible({ timeout: DEFAULT_TIMEOUT });
});

When('the user opens the full emoji picker on {string}', async ({ authenticatedApp }, text: string) => {
  const chat = new ChatPage(authenticatedApp.window);
  await chat.openEmojiPicker(text);
});

Then('the emoji picker is shown', async ({ authenticatedApp }) => {
  const chat = new ChatPage(authenticatedApp.window);
  await expect(chat.emojiPicker).toBeVisible({ timeout: DEFAULT_TIMEOUT });
});

When('the user replies to {string}', async ({ authenticatedApp }, text: string) => {
  const chat = new ChatPage(authenticatedApp.window);
  await chat.replyToMessage(text);
});

Then('the reply composer is shown', async ({ authenticatedApp }) => {
  const chat = new ChatPage(authenticatedApp.window);
  await expect(chat.replyComposer).toBeVisible({ timeout: DEFAULT_TIMEOUT });
});

When('the user edits the own message {string}', async ({ authenticatedApp }, text: string) => {
  const chat = new ChatPage(authenticatedApp.window);
  await chat.editMessage(text);
});

Then('the edit composer is shown', async ({ authenticatedApp }) => {
  const chat = new ChatPage(authenticatedApp.window);
  await expect(chat.editComposer).toBeVisible({ timeout: DEFAULT_TIMEOUT });
});

When('the user opens the context menu on the incoming message {string}', async ({ authenticatedApp }, text: string) => {
  const chat = new ChatPage(authenticatedApp.window);
  await chat.openContextMenu(text, 'incoming');
});

When('the user opens the context menu on the own message {string}', async ({ authenticatedApp }, text: string) => {
  const chat = new ChatPage(authenticatedApp.window);
  await chat.openContextMenu(text, 'outgoing');
});

Then('the edit action is not offered', async ({ authenticatedApp }) => {
  const chat = new ChatPage(authenticatedApp.window);
  await expect(chat.editMenuItem()).toBeHidden({ timeout: DEFAULT_TIMEOUT });
});

Then('the edit action is offered', async ({ authenticatedApp }) => {
  const chat = new ChatPage(authenticatedApp.window);
  await expect(chat.editMenuItem()).toBeVisible({ timeout: DEFAULT_TIMEOUT });
});

When('the user copies the message {string}', async ({ authenticatedApp }, text: string) => {
  const chat = new ChatPage(authenticatedApp.window);
  await chat.copyMessageText(text);
});

Then('the clipboard contains {string}', async ({ authenticatedApp }, text: string) => {
  await expect
    .poll(
      async () => {
        try {
          return await authenticatedApp.window.evaluate(() => navigator.clipboard.readText());
        } catch {
          return '';
        }
      },
      { timeout: DEFAULT_TIMEOUT },
    )
    .toBe(text);
});

When('the user deletes the conversation from the header menu', async ({ authenticatedApp }) => {
  const chat = new ChatPage(authenticatedApp.window);
  await chat.deleteConversation();
});

Then('the conversation {string} is removed from the chat list', async ({ authenticatedApp }, name: string) => {
  const chat = new ChatPage(authenticatedApp.window);
  await expect(chat.roomItemByName(name)).toBeHidden({ timeout: DEFAULT_TIMEOUT });
});

Then('the conversation {string} is listed before {string}', async ({ authenticatedApp }, first: string, second: string) => {
  const chat = new ChatPage(authenticatedApp.window);
  const names = await chat.roomNamesInOrder();
  const firstIndex = names.findIndex(n => n.includes(first));
  const secondIndex = names.findIndex(n => n.includes(second));
  expect(firstIndex, `"${first}" present in [${names.join(' | ')}]`).toBeGreaterThanOrEqual(0);
  expect(secondIndex, `"${second}" present in [${names.join(' | ')}]`).toBeGreaterThanOrEqual(0);
  expect(firstIndex).toBeLessThan(secondIndex);
});

// ── Composer: send-gating + Enter/Shift+Enter (TC-7.2.4 / 7.2.5) ─────────────

When('the user types {string} into the chat composer', async ({ authenticatedApp }, text: string) => {
  await new ChatPage(authenticatedApp.window).typeInComposer(text);
});

When('the user replaces the chat composer with {string}', async ({ authenticatedApp }, text: string) => {
  await new ChatPage(authenticatedApp.window).fillComposer(text);
});

When('the user presses {string} in the chat composer', async ({ authenticatedApp }, key: string) => {
  await new ChatPage(authenticatedApp.window).pressInComposer(key);
});

Then('the chat send control is unavailable', async ({ authenticatedApp }) => {
  // The send button is conditionally rendered (only mounts once the composer
  // has content), so "disabled for empty/whitespace" means "not present".
  await expect(new ChatPage(authenticatedApp.window).sendButton).toBeHidden({ timeout: DEFAULT_TIMEOUT });
});

Then('the chat send control is available', async ({ authenticatedApp }) => {
  await expect(new ChatPage(authenticatedApp.window).sendButton).toBeVisible({ timeout: DEFAULT_TIMEOUT });
});

Then('the chat composer contains a newline', async ({ authenticatedApp }) => {
  await expect.poll(() => new ChatPage(authenticatedApp.window).composerValue(), { timeout: DEFAULT_TIMEOUT }).toMatch(/\n/);
});

Then('the chat composer has no newline', async ({ authenticatedApp }) => {
  // Enter triggers submit (newline suppressed). On a seeded room the send throws
  // for lack of session keys, so the draft is retained unchanged — still without
  // a newline, proving Enter did not insert one.
  await expect.poll(() => new ChatPage(authenticatedApp.window).composerValue(), { timeout: DEFAULT_TIMEOUT }).not.toMatch(/\n/);
});

// ── Dashboard chat widget (TC-7.6.3) ─────────────────────────────────────────

When('the user adds the chat widget to the dashboard', async ({ authenticatedApp }) => {
  // seedChatRooms already returned the renderer to /dashboard.
  await new DashboardPage(authenticatedApp.window).openAddWidgetModal();
  await new AddWidgetModalPage(authenticatedApp.window).addNativeWidget('Chat');
});

Then('the dashboard chat widget lists the conversation {string}', async ({ authenticatedApp }, name: string) => {
  const chat = new ChatPage(authenticatedApp.window);
  await expect(chat.chatWidgetCard).toBeVisible({ timeout: DEFAULT_TIMEOUT });
  await expect(chat.chatWidgetRoomByName(name).first()).toBeVisible({ timeout: VERY_LONG_TIMEOUT });
});

When('the user opens the dashboard chat widget fullscreen', async ({ authenticatedApp }) => {
  await new ChatPage(authenticatedApp.window).openWidgetFullscreen();
});

Then('the chat fullscreen route is shown', async ({ authenticatedApp }) => {
  await authenticatedApp.window.waitForURL(/\/chat/, { timeout: DEFAULT_TIMEOUT });
});
