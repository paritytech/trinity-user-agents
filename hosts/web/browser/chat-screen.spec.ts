/**
 * Headless-browser test of the Chat screen over its chat providers, mounted
 * alone on a stand-in page of the host's Vite server by
 * `browser/chat-screen-harness.ts`. The draft provider talks to the synthetic
 * source in a real Web Worker; the Nova and product providers have stand-ins
 * behind them. Nothing reaches a network: every request outside this host is
 * refused and counted. Set CHAT_SCREENSHOTS to a directory to keep
 * screenshots of the synthetic screens. It needs a Chromium: playwright-core
 * finds the one it installs, or set CHROMIUM_PATH.
 */
import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { join } from "node:path";
import { type Browser, type BrowserContext, type Page } from "playwright-core";
import { type RunningHost, startHost, testBrowser } from "./fixtures.js";
import type { ChatHarness } from "./chat-screen-harness.js";

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
      const { mountChatScreen } = await import("/browser/chat-screen-harness.ts");
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
    (id) => (window as unknown as ChatWindow).__chat.unread("draft", id),
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
    "shows every provider's conversations in one list, with each provider's own steps",
    async () => {
      const { context, page, outside } = await open();
      try {
        await row(page, "draft/ada").waitFor();
        const order = await page
          .locator("[data-conversation]")
          .evaluateAll((rows) =>
            rows.map((entry) => (entry as HTMLElement).dataset.conversation),
          );
        // One list, newest activity first, whatever provider holds the conversation.
        expect(new Set(order.map((id) => id!.split("/")[0]))).toEqual(
          new Set(["nova", "product:quiz.test", "draft"]),
        );
        const times = await page
          .locator("[data-conversation]")
          .evaluateAll((rows) =>
            rows.map((entry) =>
              Number((entry as HTMLElement).dataset.updatedAt),
            ),
          );
        expect(times).toEqual([...times].sort((left, right) => right - left));
        // Only Nova has requests, starting a chat, and an account of its own.
        expect(await page.locator(".chat-request").innerText()).toContain(
          "sam.01",
        );
        expect(await page.locator('[aria-label="New chat"]').count()).toBe(1);
        expect(await page.locator(".chat-identity").innerText()).toContain(
          "Polkadot chat · Listening",
        );
        expect(await page.getAttribute("#chat-screen", "data-pending")).toBe(
          "1",
        );
        // The draft worker counts unread messages; the others do not.
        expect(
          await row(page, "draft/lin").locator(".chat-unread").innerText(),
        ).toBe("3");
        expect(await page.locator(".chat-unread").count()).toBe(1);
        await screenshot(page, "inbox");

        // A Nova conversation opens in the same view.
        await row(page, `nova/0x${"4b".repeat(32)}`).click();
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
    "pages a worker-owned history on scroll, and sends through the worker",
    async () => {
      const { context, page, outside } = await open();
      try {
        await row(page, "draft/ada").click();
        await message(page, "#120 ").waitFor();
        expect(await page.locator("[data-message-id]").count()).toBe(30);
        expect(await message(page, "#90 ").count()).toBe(0);
        // One sender per side in a direct chat, so no sender names.
        expect(await page.locator(".chat-bubble-sender").count()).toBe(0);

        // Scrolling to the top asks the worker for the page before, and the
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

        // Sending: the worker answers with the message, then moves it on to read.
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
            (window as unknown as ChatWindow).__chat.unread("draft", "ada") ===
            0,
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
    "names group senders, draws custom cards, buttons and files, and retries a failed message once",
    async () => {
      const { context, page } = await open();
      try {
        await row(page, "draft/design").click();
        await message(page, "#9 ").waitFor();
        expect(
          new Set(await page.locator(".chat-bubble-sender").allInnerTexts()),
        ).toEqual(new Set(["Ada", "Grace"]));

        // The worker draws its poll as a tree, and a press goes back to it.
        const poll = page.locator(
          '.product-card[data-message-type="draft.poll"]',
        );
        await poll.locator(".pr-text", { hasText: "0 votes" }).waitFor();
        await poll.locator("button", { hasText: "Vote" }).click();
        await poll.locator(".pr-text", { hasText: "1 vote" }).waitFor();

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

        // A restarted worker starts from its own state: the host kept no history for it.
        await harness(page, (chat) => chat.restartDraft());
        await poll.locator(".pr-text", { hasText: "0 votes" }).waitFor();
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
          await question.locator(".chat-reaction").allInnerTexts(),
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
    "keeps the worker running behind another room, and reads only what was on screen",
    async () => {
      const { context, page } = await open();
      try {
        // Opening Lin does not read it by itself: its messages are reported once on screen.
        await row(page, "draft/lin").waitFor();
        expect(await unread(page, "lin")).toBe(3);
        await row(page, "draft/lin").click();
        await page.waitForFunction(
          () =>
            (window as unknown as ChatWindow).__chat.unread("draft", "lin") ===
            0,
        );
        await back(page);
        expect(
          await row(page, "draft/lin").locator(".chat-unread").count(),
        ).toBe(0);

        // Send to Ada, then go to another room: the worker answers while Ada is not shown.
        await row(page, "draft/ada").click();
        await page.fill(".chat-composer-input", "are you there?");
        await page.press(".chat-composer-input", "Enter");
        await page
          .locator(".chat-message-out", { hasText: "are you there?" })
          .waitFor();
        await back(page);
        await row(page, "product:quiz.test/quiz").click();
        await page.waitForFunction(
          () =>
            (window as unknown as ChatWindow).__chat.unread("draft", "ada") ===
            1,
        );
        await back(page);
        expect(
          await row(page, "draft/ada").locator(".chat-unread").innerText(),
        ).toBe("1");
        await row(page, "draft/ada").click();
        await message(page, 'Got it: "are you there?"').waitFor();
        await page.waitForFunction(
          () =>
            (window as unknown as ChatWindow).__chat.unread("draft", "ada") ===
            0,
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
            (window as unknown as ChatWindow).__chat.unread("draft", "ada") ===
            1,
        );
        await harness(page, (chat) => chat.show(true));
        await page.waitForFunction(
          () =>
            (window as unknown as ChatWindow).__chat.unread("draft", "ada") ===
            0,
        );

        // A new session closes the conversation and starts every provider again.
        await harness(page, (chat) => chat.newSession());
        await row(page, "draft/ada").waitFor();
        expect(await page.locator(".chat-thread").count()).toBe(0);
        await page.waitForFunction(
          () =>
            (window as unknown as ChatWindow).__chat.unread("draft", "lin") ===
            3,
        );
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );
});
