/**
 * Headless-browser test of the Chat screen over its chat providers, mounted
 * alone on a stand-in page of the host's Vite server by
 * `browser/support/chat-screen-harness.ts`. The chat product provider talks to the
 * synthetic chat product over the Chat Extension v2 draft bridge in the page,
 * and a second one, the people product, to a stand-in conversation engine; the
 * product room provider has a stand-in behind it. Nothing reaches a network: every request outside this host is
 * refused and counted. Set CHAT_SCREENSHOTS to a directory to keep
 * screenshots of the synthetic screens. It needs a Chromium: playwright-core
 * finds the one it installs, or set CHROMIUM_PATH.
 */
import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { join } from "node:path";
import { type Browser, type BrowserContext, type Page } from "playwright-core";
import {
  type RunningHost,
  startHost,
  testBrowser,
} from "./support/fixtures.js";
import type { ChatHarness } from "./support/chat-screen-harness.js";

const STEP_MS = 60_000;
const TEST_MS = 150_000;
const PAGE = "/__chat-screen.html";

const SCREEN_PAGE = `<!doctype html>
<html>
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <link rel="stylesheet" href="/src/style.css" />
    <style>body { display: block; height: 100dvh; position: relative; }</style>
  </head>
  <body data-surface="chat">
    <section id="chat-screen" class="chat-screen" hidden></section>
    <script type="module">
      const { mountChatScreen } = await import("/browser/support/chat-screen-harness.ts");
      window.__chat = await mountChatScreen(document.getElementById("chat-screen"));
    </script>
  </body>
</html>`;

interface ChatWindow {
  __chat: ChatHarness;
}

let browser: Browser;
let host: RunningHost;

beforeAll(async () => {
  host = await startHost();
  browser = await testBrowser();
}, STEP_MS);

afterAll(async () => {
  await host?.close();
});

async function open(): Promise<{
  context: BrowserContext;
  page: Page;
  outside: string[];
}> {
  const context = await browser.newContext({
    viewport: { width: 420, height: 760 },
    timezoneId: "UTC",
  });
  context.setDefaultTimeout(STEP_MS);
  const outside: string[] = [];
  await context.route(/.*/, (route) => {
    const url = route.request().url();
    if (url.startsWith(host.origin)) return route.fallback();
    outside.push(url);
    return route.abort();
  });
  await context.route(`${host.origin}${PAGE}`, (route) =>
    route.fulfill({ contentType: "text/html", body: SCREEN_PAGE }),
  );
  const errors: string[] = [];
  const page = await context.newPage();
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto(`${host.origin}${PAGE}`);
  await page.waitForFunction(() => "__chat" in window);
  expect(errors).toEqual([]);
  return { context, page, outside };
}

const harness = <T>(page: Page, run: (chat: ChatHarness) => T) =>
  page.evaluate(
    (source) =>
      new Function("chat", `return (${source})(chat)`)(
        (window as unknown as ChatWindow).__chat,
      ),
    run.toString(),
  ) as Promise<Awaited<T>>;

const unread = (page: Page, conversationId: string) =>
  page.evaluate(
    (id) =>
      (window as unknown as ChatWindow).__chat.unread("chat-v2:chat.test", id),
    conversationId,
  );

const row = (page: Page, conversation: string) =>
  page.locator(`[data-conversation="${conversation}"]`);

const message = (page: Page, text: string) =>
  page.locator("[data-message-id]", { hasText: text });

async function back(page: Page): Promise<void> {
  await page.click('[aria-label="Back to chats"]');
  await page.locator(".chat-list").waitFor();
}

async function screenshot(page: Page, name: string): Promise<void> {
  const directory = process.env.CHAT_SCREENSHOTS;
  if (directory)
    await page.screenshot({ path: join(directory, `${name}.png`) });
}

describe("chat screen over providers", () => {
  test(
    "New chat opens the host's contact picker: a contact opens their chat, anyone else a request sent only when asked",
    async () => {
      const { context, page, outside } = await open();
      const picker = page.locator("dialog.prompt", { hasText: "New chat" });
      const search = picker.getByRole("searchbox");
      const found = picker.locator(".prompt-lookup");
      const startedChats = () =>
        harness(page, (chat) =>
          chat.asked().filter((line) => line.startsWith("people start")),
        );
      try {
        await row(page, "chat-v2:people.test/kim").waitFor();

        // The picker, not a screen of its own: the contacts, here Kim, and no product named.
        await page.click('[aria-label="New chat"]');
        await picker.waitFor();
        expect(
          await picker.locator(".prompt-list button").allInnerTexts(),
        ).toEqual(["kim.01"]);
        expect(await picker.locator(".prompt-from").count()).toBe(0);
        expect(await page.locator(".chat-new").count()).toBe(0);

        // Cancel leaves the list as it was.
        await page.keyboard.press("Escape");
        await picker.waitFor({ state: "detached" });
        expect(await page.locator(".chat-list").count()).toBe(1);

        // Someone whose username names another account on chain is not chosen.
        await page.click('[aria-label="New chat"]');
        await search.fill("eve");
        await found.getByRole("button", { name: /^eve\.02\b/ }).click();
        await found
          .getByText(
            "On chain, eve.02 names another account than the index lists.",
          )
          .waitFor();

        // Kim is a contact: listed once, and choosing her opens her chat.
        await search.fill("kim");
        await found.getByText("No one else by that username.").waitFor();
        await picker.getByRole("button", { name: /^kim\.01\b/ }).click();
        await page
          .locator(".chat-composer, [data-message-id]")
          .first()
          .waitFor();
        await back(page);

        // Zed is not: choosing him opens a request to him and sends nothing.
        await page.click('[aria-label="New chat"]');
        await search.fill("zed");
        await found.getByRole("button", { name: /^zed\.01\b/ }).click();
        await picker.waitFor({ state: "detached" });
        const send = page.locator('[data-action="start-chat"]');
        expect(await send.innerText()).toBe("Send request to zed.01");
        await screenshot(page, "new-chat-request");
        expect(await startedChats()).toEqual([]);

        // Sending goes through the provider, with Zed's account, once.
        await page.fill('[data-focus="welcome"]', "hello");
        await send.click();
        await page
          .locator(".chat-error", { hasText: "Not in this test." })
          .waitFor();
        expect(await startedChats()).toEqual([
          `people start 0x${"7e".repeat(32)} hello`,
        ]);
        await back(page);

        // A choice made after the session changed is not an answer.
        await page.click('[aria-label="New chat"]');
        await search.fill("zed");
        const zed = found.getByRole("button", { name: /^zed\.01\b/ });
        await zed.waitFor();
        await harness(page, (chat) => chat.newSession());
        await zed.click();
        await picker.waitFor({ state: "detached" });
        expect(await page.locator(".chat-new").count()).toBe(0);
        expect(await page.locator(".chat-list").count()).toBe(1);
        expect(await startedChats()).toHaveLength(1);
        expect(outside).toEqual([]);
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );

  test(
    "blocks a host contact from their conversation, lists the blocked, and Unblock offers them in the picker again",
    async () => {
      const { context, page, outside } = await open();
      const picker = page.locator("dialog.prompt", { hasText: "New chat" });
      const offered = async () => {
        await page.click('[aria-label="New chat"]');
        await picker.waitFor();
        const names = await picker
          .locator(".prompt-list button")
          .allInnerTexts();
        await page.keyboard.press("Escape");
        await picker.waitFor({ state: "detached" });
        return names.sort();
      };
      const blockedNames = () =>
        page.locator(".chat-blocked .chat-row-title").allInnerTexts();
      const removals = () => harness(page, (chat) => chat.removals());
      try {
        await row(page, "chat-v2:people.test/kim").waitFor();
        expect(await offered()).toEqual(["kim.01"]);
        const before = await removals();

        // A product room's person is not a host contact: no options.
        await row(page, "chat-v2:chat.test/ada").click();
        await page.locator('[aria-label="Back to chats"]').waitFor();
        expect(await page.locator('[data-action="chat-options"]').count()).toBe(
          0,
        );
        await back(page);

        // Kim is: Block, from her conversation's options, after the user confirms.
        await row(page, "chat-v2:people.test/kim").click();
        await page.click('[data-action="chat-options"]');
        await page.click('dialog.prompt button:has-text("Block")');
        const confirm = page.locator("dialog.prompt", {
          hasText: "Block kim.01?",
        });
        await confirm.getByRole("button", { name: "Cancel" }).click();
        expect(await removals()).toBe(before);
        await page.click('[data-action="chat-options"]');
        await page.click('dialog.prompt button:has-text("Block")');
        await confirm
          .locator('button[data-tone="destructive"]', { hasText: "Block" })
          .click();
        await page
          .locator('[data-action="chat-options"]')
          .waitFor({ state: "detached" });
        // The core is told, so her handle stops resolving.
        expect(await removals()).toBe(before + 1);
        await back(page);
        expect(await offered()).toEqual([]);

        await page.click('[data-action="blocked"]');
        expect((await blockedNames()).sort()).toEqual(["kim.01", "lou.01"]);
        await screenshot(page, "blocked");
        await page.locator('[data-action="unblock"]').first().click();
        await page.locator('[data-action="unblock"]').first().click();
        await page
          .locator(".chat-empty-title", { hasText: "No one is blocked" })
          .waitFor();
        await back(page);
        expect(await offered()).toEqual(["kim.01", "lou.01"]);

        // With no one blocked, Blocked stays reachable, and Block a contact
        // opens the host's picker over its contacts only.
        await page.click('[data-action="blocked"]');
        await page.click('[data-action="block-contact"]');
        const choose = page.locator("dialog.prompt", {
          hasText: "Block a contact",
        });
        expect(
          (await choose.locator(".prompt-list button").allInnerTexts()).sort(),
        ).toEqual(["kim.01", "lou.01"]);
        await page.keyboard.press("Escape");
        await choose.waitFor({ state: "detached" });
        expect(await blockedNames()).toEqual([]);
        await page.click('[data-action="block-contact"]');
        await choose.getByRole("button", { name: /^lou\.01\b/ }).click();
        await page.locator(".chat-blocked .chat-row-title").waitFor();
        expect(await blockedNames()).toEqual(["lou.01"]);
        expect(await removals()).toBe(before + 2);
        await back(page);
        expect(await offered()).toEqual(["kim.01"]);
        expect(outside).toEqual([]);
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );

  test(
    "a person chosen outside Chat opens their chat, or a request sent only when asked, with focus in it",
    async () => {
      const { context, page, outside } = await open();
      const startedChats = () =>
        harness(page, (chat) =>
          chat.asked().filter((line) => line.startsWith("people start")),
        );
      try {
        await row(page, "chat-v2:people.test/kim").waitFor();
        expect(
          await harness(page, (chat) =>
            chat.startWith(`0x${"4b".repeat(32)}`, "kim.01", false),
          ),
        ).toBe(true);
        await page.locator('[data-focus="composer"]').waitFor();
        expect(
          await page.evaluate(
            () => (document.activeElement as HTMLElement | null)?.dataset.focus,
          ),
        ).toBe("composer");
        await back(page);

        expect(
          await harness(page, (chat) =>
            chat.startWith(`0x${"7e".repeat(32)}`, "zed.01", true),
          ),
        ).toBe(true);
        const send = page.locator('[data-action="start-chat"]');
        expect(await send.innerText()).toBe("Send request to zed.01");
        expect(
          await page.evaluate(
            () => (document.activeElement as HTMLElement | null)?.dataset.focus,
          ),
        ).toBe("welcome");
        expect(await startedChats()).toEqual([]);
        expect(outside).toEqual([]);
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );

  test(
    "shows every provider's conversations in one list, with each provider's own steps",
    async () => {
      const { context, page, outside } = await open();
      try {
        await row(page, "chat-v2:chat.test/ada").waitFor();
        const order = await page
          .locator("[data-conversation]")
          .evaluateAll((rows) =>
            rows.map((entry) => (entry as HTMLElement).dataset.conversation),
          );
        // One list, newest activity first, whatever provider holds the conversation.
        expect(new Set(order.map((id) => id!.split("/")[0]))).toEqual(
          new Set([
            "chat-v2:people.test",
            "product:quiz.test",
            "chat-v2:chat.test",
          ]),
        );
        const times = await page
          .locator("[data-conversation]")
          .evaluateAll((rows) =>
            rows.map((entry) =>
              Number((entry as HTMLElement).dataset.updatedAt),
            ),
          );
        expect(times).toEqual([...times].sort((left, right) => right - left));
        // One New chat, for the provider that starts chats.
        expect(await page.locator('[aria-label="New chat"]').count()).toBe(1);
        // The chat product counts unread messages; the others do not.
        expect(
          await row(page, "chat-v2:chat.test/lin")
            .locator(".chat-unread")
            .innerText(),
        ).toBe("3");
        expect(await page.locator(".chat-unread").count()).toBe(1);
        await screenshot(page, "inbox");

        // Kim's conversation opens in the same view, named by the host.
        expect(
          await row(page, "chat-v2:people.test/kim")
            .locator(".chat-row-title")
            .innerText(),
        ).toBe("kim.01");
        await row(page, "chat-v2:people.test/kim").click();
        await message(page, "Yes, see you")
          .locator(".chat-tick", { hasText: "delivered" })
          .waitFor();
        await back(page);
        expect(outside).toEqual([]);
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );

  test(
    "pages a product-owned history on scroll, and sends through the product",
    async () => {
      const { context, page, outside } = await open();
      try {
        await row(page, "chat-v2:chat.test/ada").click();
        await message(page, "#120 ").waitFor();
        expect(await page.locator("[data-message-id]").count()).toBe(30);
        expect(await message(page, "#90 ").count()).toBe(0);
        // One sender per side in a direct chat, so no sender names.
        expect(await page.locator(".chat-bubble-sender").count()).toBe(0);

        // Scrolling to the top asks the product for the page before, and the
        // reader stays where they were instead of jumping to the oldest message.
        await page
          .locator(".chat-thread")
          .evaluate((thread) => (thread.scrollTop = 0));
        await message(page, "#61 ").waitFor();
        expect(await page.locator("[data-message-id]").count()).toBe(60);
        const thread = (await page.locator(".chat-thread").boundingBox())!;
        const anchor = (await message(page, "#91 ").boundingBox())!;
        expect(anchor.y).toBeGreaterThanOrEqual(thread.y);
        expect(anchor.y).toBeLessThan(thread.y + thread.height);
        expect(
          await page
            .locator(".chat-thread")
            .evaluate((element) => element.scrollTop),
        ).toBeGreaterThan(0);

        // Sending: the product answers with the message, then moves it on to read.
        await page
          .locator(".chat-thread")
          .evaluate((thread) => (thread.scrollTop = thread.scrollHeight));
        await page.fill(".chat-composer-input", "hello worker");
        await page.press(".chat-composer-input", "Enter");
        const sent = page.locator(".chat-message-out", {
          hasText: "hello worker",
        });
        await sent.waitFor();
        expect(["pending", "sent", "delivered", "read"]).toContain(
          (await sent.getAttribute("data-status")) ?? "",
        );
        await page
          .locator('.chat-message-out[data-status="read"]', {
            hasText: "hello worker",
          })
          .waitFor();
        await message(page, 'Got it: "hello worker"').waitFor();
        expect(await sent.count()).toBe(1);
        // The answer was on screen, so it was reported seen and is not unread.
        await page.waitForFunction(
          () =>
            (window as unknown as ChatWindow).__chat.unread(
              "chat-v2:chat.test",
              "ada",
            ) === 0,
        );
        await screenshot(page, "conversation");
        expect(outside).toEqual([]);
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );

  test(
    "names group senders, retries a failed message once, restarts the product, and draws product room cards, buttons and files",
    async () => {
      const { context, page } = await open();
      try {
        await row(page, "chat-v2:chat.test/design").click();
        await message(page, "#9 ").waitFor();
        expect(
          new Set(await page.locator(".chat-bubble-sender").allInnerTexts()),
        ).toEqual(new Set(["Ada", "Grace"]));

        await page
          .locator(".chat-system", { hasText: "Grace joined the group" })
          .waitFor();

        // A failed message is retried as the same message, never a second one.
        await page.fill(".chat-composer-input", "[fail] lunch?");
        await page.press(".chat-composer-input", "Enter");
        const mine = page.locator(".chat-message-out", {
          hasText: "[fail] lunch?",
        });
        await page
          .locator('.chat-message-out[data-status="failed"]', {
            hasText: "[fail] lunch?",
          })
          .waitFor();
        expect(await mine.locator(".chat-failed").innerText()).toContain(
          "Not sent:",
        );
        await mine.locator(".chat-link", { hasText: "Retry" }).click();
        await page
          .locator('.chat-message-out[data-status="delivered"]', {
            hasText: "[fail] lunch?",
          })
          .waitFor();
        expect(await mine.count()).toBe(1);
        await screenshot(page, "group");

        // A restarted product starts from its own state: the host kept no history for it.
        await harness(page, (chat) => chat.restartProduct());
        await mine.waitFor({ state: "detached" });
        await message(page, "#9 ").waitFor();
        expect(await mine.count()).toBe(0);
        await back(page);

        // A product worker's room: its card through Renderer, and an echo of what is typed.
        await row(page, "product:quiz.test/quiz").click();
        const counter = page.locator(
          '.product-card[data-message-type="quiz.counter"]',
        );
        await counter.locator(".pr-text", { hasText: "count 0" }).waitFor();
        await counter.locator("button", { hasText: "bump" }).click();
        await counter.locator(".pr-text", { hasText: "count 1" }).waitFor();
        // Buttons the host draws, a press answered by the worker, and its reaction on them.
        const question = page.locator(".chat-message", {
          hasText: "Which is a prime?",
        });
        expect(
          await question
            .locator(".chat-reaction:not(.chat-reaction-add)")
            .allInnerTexts(),
        ).toEqual(["🤔"]);
        await question
          .locator('.chat-actions[data-layout="Column"] .chat-action', {
            hasText: "7",
          })
          .click();
        await message(page, "picked: seven").waitFor();
        // A file whose address is not https is shown, but not as a link.
        const file = page.locator(".chat-file", { hasText: "answers.png" });
        expect(await file.evaluate((node) => node.tagName)).toBe("DIV");
        expect(await file.innerText()).toContain("image/png · 2.3 MB");
        expect(await file.innerText()).toContain("opens only https links");
        await screenshot(page, "product-room");

        await page.fill(".chat-composer-input", "hi quiz");
        await page.press(".chat-composer-input", "Enter");
        await message(page, "echo: hi quiz").waitFor();
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );

  test(
    "keeps the product answering behind another room, and reads only what was on screen",
    async () => {
      const { context, page } = await open();
      try {
        // Opening Lin does not read it by itself: its messages are reported once on screen.
        await row(page, "chat-v2:chat.test/lin").waitFor();
        expect(await unread(page, "lin")).toBe(3);
        await row(page, "chat-v2:chat.test/lin").click();
        await page.waitForFunction(
          () =>
            (window as unknown as ChatWindow).__chat.unread(
              "chat-v2:chat.test",
              "lin",
            ) === 0,
        );
        await back(page);
        expect(
          await row(page, "chat-v2:chat.test/lin")
            .locator(".chat-unread")
            .count(),
        ).toBe(0);

        // Send to Ada, then go to another room: the product answers while Ada is not shown.
        await row(page, "chat-v2:chat.test/ada").click();
        await page.fill(".chat-composer-input", "are you there?");
        await page.press(".chat-composer-input", "Enter");
        await page
          .locator(".chat-message-out", { hasText: "are you there?" })
          .waitFor();
        await back(page);
        await row(page, "product:quiz.test/quiz").click();
        await page.waitForFunction(
          () =>
            (window as unknown as ChatWindow).__chat.unread(
              "chat-v2:chat.test",
              "ada",
            ) === 1,
        );
        await back(page);
        expect(
          await row(page, "chat-v2:chat.test/ada")
            .locator(".chat-unread")
            .innerText(),
        ).toBe("1");
        await row(page, "chat-v2:chat.test/ada").click();
        await message(page, 'Got it: "are you there?"').waitFor();
        await page.waitForFunction(
          () =>
            (window as unknown as ChatWindow).__chat.unread(
              "chat-v2:chat.test",
              "ada",
            ) === 0,
        );

        // With the screen hidden, an answer arriving in the open conversation stays unread.
        await page.fill(".chat-composer-input", "still there?");
        await page.press(".chat-composer-input", "Enter");
        await page
          .locator(".chat-message-out", { hasText: "still there?" })
          .waitFor();
        await harness(page, (chat) => chat.show(false));
        await page.waitForFunction(
          () =>
            (window as unknown as ChatWindow).__chat.unread(
              "chat-v2:chat.test",
              "ada",
            ) === 1,
        );
        await harness(page, (chat) => chat.show(true));
        await page.waitForFunction(
          () =>
            (window as unknown as ChatWindow).__chat.unread(
              "chat-v2:chat.test",
              "ada",
            ) === 0,
        );

        // A new session closes the conversation and starts every provider again.
        await harness(page, (chat) => chat.newSession());
        await row(page, "chat-v2:chat.test/ada").waitFor();
        expect(await page.locator(".chat-thread").count()).toBe(0);
        await page.waitForFunction(
          () =>
            (window as unknown as ChatWindow).__chat.unread(
              "chat-v2:chat.test",
              "lin",
            ) === 3,
        );
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );

  test(
    "reacts in a product room through its worker, and the user's own messages take none",
    async () => {
      const { context, page, outside } = await open();
      try {
        // A product room: the user's reaction goes to the worker as MessagePosted(Reaction).
        await row(page, "product:quiz.test/quiz").click();
        const welcome = message(page, "Welcome to the quiz");
        await welcome.locator('[data-action="react"]').click();
        await welcome
          .locator('.chat-reaction-picker [data-emoji="🙏"]')
          .click();
        await welcome
          .locator('.chat-reaction[data-self="true"][data-emoji="🙏"]')
          .waitFor();
        // The user's own messages take none: the product does not know their ids.
        await page.fill(".chat-composer-input", "hi quiz");
        await page.press(".chat-composer-input", "Enter");
        await page
          .locator(".chat-message-out", { hasText: "hi quiz" })
          .waitFor();
        expect(
          await page
            .locator(".chat-message-out", { hasText: "hi quiz" })
            .locator('[data-action="react"]')
            .count(),
        ).toBe(0);

        const asked = await harness(page, (chat) => chat.asked());
        expect(asked).toEqual([
          expect.stringMatching(/^quiz Reaction m-\S+ 🙏$/),
        ]);
        expect(outside).toEqual([]);
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );
});
