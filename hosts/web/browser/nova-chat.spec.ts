/**
 * Nova Chat between test users of the invitation demo, each in their own tab
 * of the normal host: Alice asks Bob, Bob accepts in his Inbox, they write
 * both ways, Bob's tab reloads and the chat goes on; then Carol asks Alice,
 * so Alice is the one who accepts. No wallet signs in, every socket is closed
 * and every request outside this host is refused, so no chain, backend or
 * peer is reached. It needs a Chromium (playwright-core finds the one it
 * installs, or set CHROMIUM_PATH).
 *
 * What runs for real: the dev server's archive of
 * `src/chat/nova-chat/worker.ts`, the archive check, the ordinary worker
 * loader and hidden frame, the host's Nova key boundary and its port
 * (`nova-chat/1`), the official apps' request, acceptance and device-route
 * bytes, the core's product accounts, `createProof`, storage and Statement
 * Store calls, the core's contact handles, the v2 draft bridge, and the
 * host's Chat screen, Inbox and prompts. Simulated: the People node's
 * statement methods, by a store the tabs share
 * (`fixtures/statements/local-statement-node.ts`), each notification claim,
 * and each conversation account's allowance.
 *
 * Both ends are this host. It shows Nova Chat working end to end in the
 * browser; it does not show that a Nova app on a phone reads it.
 */
import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import type { Browser, BrowserContext, Frame, Page } from "playwright-core";
import {
  type RunningHost,
  closeTools,
  startHost,
  testBrowser,
  useChatProducts,
  useInvitationDemo,
  probeChatWorkers,
} from "./support/fixtures.js";

const STEP_MS = 60_000;
const TEST_MS = 300_000;
const PRODUCT = "nova-chat.paseo";
const STATEMENTS = "truapi-web-signing-host:local-statements:";

let browser: Browser;
let host: RunningHost;

beforeAll(async () => {
  host = await startHost();
  browser = await testBrowser();
}, STEP_MS);

afterAll(async () => {
  await host?.close();
});

async function offline(): Promise<{
  context: BrowserContext;
  outside: string[];
  errors: string[];
  open(): Promise<Page>;
}> {
  const context = await browser.newContext({
    viewport: { width: 1280, height: 860 },
  });
  context.setDefaultTimeout(STEP_MS);
  await probeChatWorkers(context);
  await context.routeWebSocket(/.*/, (socket) => socket.close());
  const outside: string[] = [];
  await context.route(/.*/, (route) => {
    const url = route.request().url();
    if (url.startsWith(host.origin)) return route.fallback();
    outside.push(url);
    return route.abort();
  });
  const errors: string[] = [];
  return {
    context,
    outside,
    errors,
    open: async () => {
      const page = await context.newPage();
      page.on("pageerror", (error) => errors.push(error.message));
      await page.goto(host.origin);
      await ready(page);
      return page;
    },
  };
}

const ready = (page: Page) =>
  page
    .locator("#core-status")
    .filter({ hasText: "Core ready" })
    .waitFor({ state: "attached" });

/** Run Nova Chat alone in this tab, then act as `user`. Each reloads the tab. */
async function actAs(page: Page, user: string): Promise<void> {
  await useChatProducts(page, { contact: false, kettle: false, nova: true });
  await ready(page);
  await useInvitationDemo(page, user);
  await ready(page);
  await page
    .locator("#session")
    .filter({ hasNotText: "Signed out" })
    .waitFor({ state: "attached" });
  await closeTools(page);
}

interface WorkerWindow {
  __novaChatWorker?: {
    stage: string;
    reason?: string;
    errors: string[];
    record(): string | null;
  };
}

async function workerFrame(page: Page): Promise<Frame> {
  const until = Date.now() + STEP_MS;
  let last = "";
  while (Date.now() < until) {
    for (const frame of page.frames())
      if (frame.url().includes("__truapi-worker.html")) {
        const stage = await frame
          .evaluate(() => {
            const worker = (window as unknown as WorkerWindow).__novaChatWorker;
            return worker === undefined
              ? ""
              : `${worker.stage} ${worker.reason ?? ""}`;
          })
          .catch(() => "");
        if (stage.startsWith("running")) return frame;
        if (stage !== "") last = stage;
      }
    await page.waitForTimeout(100);
  }
  throw new Error(`Nova Chat's worker did not start: ${last}`);
}

/** Answer the core's question before the product's first statement, whenever it shows: always allow. */
function allowStatements(page: Page): () => void {
  let stopped = false;
  void (async () => {
    while (!stopped) {
      const prompt = page.locator("dialog.prompt", {
        hasText: "StatementSubmit",
      });
      if (await prompt.isVisible().catch(() => false)) {
        expect(await prompt.innerText()).toContain(PRODUCT);
        await prompt
          .getByRole("button", { name: "Always allow" })
          .click()
          .catch(() => {});
      }
      await page.waitForTimeout(200).catch(() => {});
    }
  })();
  return () => {
    stopped = true;
  };
}

const chat = (page: Page) => page.click('#dock [data-destination="chat"]');
const row = (page: Page, title: string) =>
  page.locator(`[data-conversation^="chat-v2:${PRODUCT}/"]`, {
    hasText: title,
  });
const bubble = (page: Page, text: string) =>
  page.locator(".chat-bubble-text", { hasText: text });

async function request(from: Page, to: string, welcome: string): Promise<void> {
  await chat(from);
  await from.click('[aria-label="New chat"]');
  await from
    .locator("dialog.prompt", { hasText: "New chat" })
    .locator("button", { hasText: to })
    .click();
  await from.fill(".chat-new-request textarea", welcome);
  await from.click('[data-action="start-chat"]');
  const prompt = from.locator("dialog.prompt", { hasText: "Send invitation" });
  expect(await prompt.innerText()).toContain("Uses daily slots");
  await prompt.getByRole("button", { name: "Send", exact: true }).click();
  await from
    .locator("#chat-screen", {
      hasText: "Waiting for them to accept your request",
    })
    .first()
    .waitFor();
}

/** Chat's list, with its Chats and Inbox tabs, also from an open conversation. */
async function toList(page: Page): Promise<void> {
  await chat(page);
  const back = page.locator('[aria-label="Back to chats"]');
  if (await back.isVisible()) await back.click();
}

async function accept(
  page: Page,
  from: string,
  welcome: string,
): Promise<void> {
  await toList(page);
  await page.click('#chat-tabs [data-chat-tab="inbox"]');
  const item = page.locator(".notification-row", { hasText: from });
  await item.waitFor();
  expect(await item.innerText()).toContain(welcome);
  await item.locator('[data-action="notification-accept"]').click();
  const prompt = page.locator("dialog.prompt", {
    hasText: "Accept chat request",
  });
  await prompt.getByRole("button", { name: "Accept", exact: true }).click();
  await page.click('#chat-tabs [data-chat-tab="chats"]');
}

/** Open the conversation with `name` from the list of chats. */
async function openRow(page: Page, name: string): Promise<void> {
  await toList(page);
  await page.click('#chat-tabs [data-chat-tab="chats"]');
  await row(page, name).click();
}

async function say(page: Page, to: string, text: string): Promise<void> {
  await openRow(page, to);
  const input = page.locator(".chat-composer-input");
  await input.waitFor();
  await input.fill(text);
  await page.press(".chat-composer-input", "Enter");
  await bubble(page, text).waitFor();
}

async function reads(page: Page, from: string, text: string): Promise<void> {
  await openRow(page, from);
  await bubble(page, text).waitFor();
  expect(await bubble(page, text).count()).toBe(1);
}

/** Every statement's data the shared store holds, as text. */
async function storedText(page: Page): Promise<string> {
  return page.evaluate((prefix) => {
    const texts: string[] = [];
    for (const key of Object.keys(localStorage))
      if (key.startsWith(prefix)) texts.push(localStorage.getItem(key) ?? "");
    return texts.join("\n");
  }, STATEMENTS);
}

const utf8Hex = (text: string) => Buffer.from(text, "utf8").toString("hex");

describe("Nova Chat between test users", () => {
  test(
    "Alice asks Bob, Bob accepts, both write, Bob reloads and the chat goes on; then Carol asks Alice and Alice accepts",
    async () => {
      const { context, outside, errors, open } = await offline();
      const stops: (() => void)[] = [];
      try {
        const alice = await open();
        await actAs(alice, "alice");
        const bob = await open();
        await actAs(bob, "bob");
        await workerFrame(alice);
        await workerFrame(bob);
        stops.push(allowStatements(alice), allowStatements(bob));

        await request(alice, "Bob", "Hi Bob, from the web");
        await accept(bob, "Alice", "Hi Bob, from the web");
        await say(alice, "Bob", "Hello Bob");
        await reads(bob, "Alice", "Hello Bob");
        await say(bob, "Alice", "Hello Alice");
        await reads(alice, "Bob", "Hello Alice");

        // The store holds sealed frames, never the text.
        const stored = await storedText(alice);
        expect(stored.length).toBeGreaterThan(0);
        for (const text of ["Hello Bob", "Hello Alice"]) {
          expect(stored.includes(text)).toBe(false);
          expect(stored.includes(utf8Hex(text))).toBe(false);
        }

        // Bob's tab reloads: his worker starts again from storage, the
        // history is there once, and the next message arrives once.
        await bob.reload();
        await ready(bob);
        await workerFrame(bob);
        await reads(bob, "Alice", "Hello Bob");
        await say(alice, "Bob", "After your reload");
        await reads(bob, "Alice", "After your reload");
        await say(bob, "Alice", "Still here");
        await reads(alice, "Bob", "Still here");

        // The other direction: Carol asks Alice, who accepts.
        const carol = await open();
        await actAs(carol, "carol");
        await workerFrame(carol);
        stops.push(allowStatements(carol));
        await request(carol, "Alice", "Hi Alice, it is Carol");
        await accept(alice, "Carol", "Hi Alice, it is Carol");
        await say(alice, "Carol", "Hello Carol");
        await reads(carol, "Alice", "Hello Carol");
        await say(carol, "Alice", "Hello from Carol");
        await reads(alice, "Carol", "Hello from Carol");

        expect(outside).toEqual([]);
        expect(errors).toEqual([]);
      } finally {
        for (const stop of stops) stop();
        await context.close();
      }
    },
    TEST_MS,
  );
});
