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
 * `apps/nova-chat/worker.ts`, the archive check, the ordinary worker
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
import type {
  Browser,
  BrowserContext,
  Frame,
  Locator,
  Page,
} from "playwright-core";
import {
  type RunningHost,
  closeTools,
  openTools,
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

async function offline(probe = true): Promise<{
  context: BrowserContext;
  outside: string[];
  errors: string[];
  open(): Promise<Page>;
}> {
  const context = await browser.newContext({
    viewport: { width: 1280, height: 860 },
  });
  context.setDefaultTimeout(STEP_MS);
  if (probe) await probeChatWorkers(context);
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

/** Reload `page` and wait for Nova Chat's worker to say, on its console, that it runs. */
async function reloadRunning(page: Page): Promise<void> {
  const running = page.waitForEvent("console", {
    predicate: (message) => message.text() === "Nova Chat is running.",
    timeout: STEP_MS,
  });
  await page.reload();
  await ready(page);
  await running;
}

/** Keep every diagnostics report a worker frame posts to the host page, for the test to read. */
async function keepReports(context: BrowserContext): Promise<void> {
  await context.addInitScript(() => {
    if (window !== window.top) return;
    const kept: unknown[] = [];
    (window as unknown as { __reportsSeen: unknown[] }).__reportsSeen = kept;
    addEventListener("message", (event: MessageEvent) => {
      const data = event.data as { type?: unknown } | null;
      if (data?.type === "truapi-product-diagnostics") kept.push(data);
    });
  });
}

const reportsSeen = (page: Page) =>
  page.evaluate(() =>
    JSON.stringify(
      (window as unknown as { __reportsSeen?: unknown[] }).__reportsSeen ?? [],
    ),
  );

/** Whether the worker frames of `page` were asked for, or hold, a test probe. */
async function testHooks(page: Page): Promise<boolean[]> {
  const frames = page
    .frames()
    .filter((frame) => frame.url().includes("__truapi-worker.html"));
  expect(frames.length).toBeGreaterThan(0);
  return Promise.all(
    frames.map(
      async (frame) =>
        new URL(frame.url()).searchParams.get("probe") === "1" ||
        (await frame.evaluate(
          () =>
            "__novaChatWorker" in window ||
            "__contactChatWorker" in window ||
            "__kettleChatWorker" in window,
        )),
    ),
  );
}

/** Open the inspector's Chat view; answer Nova Chat's conversation card. */
async function novaCard(page: Page) {
  await openTools(page, "inspector");
  await page.click('[data-inspector-view="chat"]');
  return page.locator(
    `#chat-inspector .si-card[data-key^="conversation:${PRODUCT}:0x"]`,
  );
}

/** A card's facts, label to text, the first of each label. */
async function factsOf(card: Locator): Promise<Record<string, string>> {
  return card.evaluate((node) => {
    const out: Record<string, string> = {};
    for (const term of node.querySelectorAll("dt")) {
      const label = term.textContent ?? "";
      const value = (term.nextElementSibling as HTMLElement | null)?.innerText;
      if (!(label in out) && value !== undefined)
        out[label] = value.replace(/\s+/g, " ").trim();
    }
    return out;
  });
}

/** Wait until `check` holds of the card's facts; answer them. */
async function factsUntil(
  card: Locator,
  check: (facts: Record<string, string>) => boolean,
): Promise<Record<string, string>> {
  const until = Date.now() + STEP_MS;
  let facts = await factsOf(card);
  while (!check(facts)) {
    if (Date.now() > until)
      throw new Error(`The card never matched: ${JSON.stringify(facts)}`);
    await card.page().waitForTimeout(200);
    facts = await factsOf(card);
  }
  return facts;
}

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

  // The start's 30 s limit is for the product. The tab's clock is Playwright's,
  // so the prompt can stay open past it without a real 30 s wait.
  test(
    "a Send prompt left open past the start's 30 s: Cancel keeps nothing, and Send starts the chat, which Sent shows waiting",
    async () => {
      const { context, outside, errors, open } = await offline();
      await context.clock.install();
      const stops: (() => void)[] = [];
      try {
        const alice = await open();
        await actAs(alice, "alice");
        const bob = await open();
        await actAs(bob, "bob");
        await workerFrame(alice);
        await workerFrame(bob);
        stops.push(allowStatements(alice), allowStatements(bob));

        const welcome = "Hi Bob, after a long look";
        const prompt = alice.locator("dialog.prompt", {
          hasText: "Send invitation",
        });
        const formError = alice.locator(".chat-new .chat-error");
        await chat(alice);
        await alice.click('[aria-label="New chat"]');
        await alice
          .locator("dialog.prompt", { hasText: "New chat" })
          .locator("button", { hasText: "Bob" })
          .click();
        await alice.fill(".chat-new-request textarea", welcome);

        await alice.click('[data-action="start-chat"]');
        await prompt.waitFor();
        await alice.clock.fastForward(31_000);
        await alice.waitForTimeout(500);
        expect(await formError.count()).toBe(0);
        await prompt
          .getByRole("button", { name: "Cancel", exact: true })
          .click();
        await formError
          .filter({ hasText: "You did not send the request." })
          .waitFor();

        await alice.click('[data-action="start-chat"]');
        await prompt.waitFor();
        await alice.clock.fastForward(31_000);
        await alice.waitForTimeout(500);
        await prompt.getByRole("button", { name: "Send", exact: true }).click();
        await alice
          .locator("#chat-screen", {
            hasText: "Waiting for them to accept your request",
          })
          .first()
          .waitFor();
        expect(
          await alice.locator(".chat-error", { hasText: "in time" }).count(),
        ).toBe(0);

        await toList(alice);
        await alice.click('#chat-tabs [data-chat-tab="inbox"]');
        const sent = alice.locator('[data-group="sent"] .notification-row', {
          hasText: "Bob",
        });
        await sent.and(alice.locator('[data-sent-status="waiting"]')).waitFor();
        expect(await sent.count()).toBe(1);

        await accept(bob, "Alice", welcome);
        await toList(alice);
        await alice.click('#chat-tabs [data-chat-tab="inbox"]');
        await sent.waitFor({ state: "detached" });

        expect(outside).toEqual([]);
        expect(errors).toEqual([]);
      } finally {
        for (const stop of stops) stop();
        await context.close();
      }
    },
    TEST_MS,
  );

  test(
    "the Chat inspector shows each Nova conversation as Nova Chat reports it: updated, linked to Statements, back after a reload, with no message text and no test hook",
    async () => {
      const { context, outside, errors, open } = await offline(false);
      await keepReports(context);
      const stops: (() => void)[] = [];
      try {
        const alice = await open();
        await actAs(alice, "alice");
        await reloadRunning(alice);
        const bob = await open();
        await actAs(bob, "bob");
        await reloadRunning(bob);
        for (const page of [alice, bob])
          expect(await testHooks(page)).not.toContain(true);
        stops.push(allowStatements(alice), allowStatements(bob));

        await request(alice, "Bob", "Hi Bob, from the web");
        const card = await novaCard(alice);
        await card.locator(".si-badge", { hasText: "outgoing" }).waitFor();
        expect(await card.count()).toBe(1);
        const asked = await factsOf(card);
        expect(asked.Protocol).toContain("reported nova-apps/chat-session");
        // Not followed yet: no account, topics or peers, rather than empty ones.
        for (const label of ["Account", "Peers", "Listens on", "Publishes on"])
          expect(asked[label]).toBeUndefined();
        expect(asked["Delivery (this run)"]).toBe("reported not following");
        await closeTools(alice);

        await accept(bob, "Alice", "Hi Bob, from the web");
        await say(alice, "Bob", "Hello Bob");
        await reads(bob, "Alice", "Hello Bob");
        await say(bob, "Alice", "Hello Alice");
        await reads(alice, "Bob", "Hello Alice");

        await novaCard(alice);
        await card.locator(".si-badge", { hasText: "established" }).waitFor();
        const facts = await factsUntil(
          card,
          (each) =>
            /\b1 sent\b/.test(each.Messages ?? "") &&
            /\b1 received\b/.test(each.Messages ?? "") &&
            (each["Stored history"] ?? "").includes("0 awaiting ack") &&
            each.Peers !== undefined,
        );
        const { Account: account, Tie: tie } = facts;
        expect(facts).toMatchObject({
          Account: expect.stringMatching(/^reported 0x[0-9a-f]{6}…[0-9a-f]{4}/),
          Peers: expect.stringMatching(/^reported 0x[0-9a-f]{6}…/),
          "Listens on": expect.stringMatching(/^reported 0x[0-9a-f]{6}…/),
          "Publishes on": expect.stringMatching(/^reported 0x[0-9a-f]{6}…/),
          Messages: expect.stringContaining("0 failed"),
          "Stored history": "reported 1 yours · 1 theirs · 0 awaiting ack",
          "Delivery (this run)": expect.stringMatching(/^reported following/),
          "Allowance (stored)": expect.stringMatching(
            /^reported until \d{4}-\d\d-\d\d \d\d:\d\d UTC$/,
          ),
        });
        // Here each conversation account's allowance is simulated, so the
        // host holds nothing tying it to the product; with a real one it does.
        expect(tie).toMatch(
          /^(host tied to nova-chat\.paseo|inspector none · reported only)/,
        );

        // Each link opens Statements on exactly what it names.
        const own = (await card
          .locator('button[title^="Show account "]')
          .first()
          .getAttribute("title"))!.split(" ")[2]!;
        await card.locator('button[title^="Show account "]').first().click();
        await alice
          .locator(
            `#statements-inspector .si-detail[data-key="account:${own}"]`,
          )
          .waitFor();
        await alice.click('[data-inspector-view="chat"]');
        const listened = card.locator('button[title^="Show topic "]').first();
        const topic = (await listened.getAttribute("title"))!.split(" ")[2]!;
        await listened.click();
        await alice
          .locator(
            `#statements-inspector .si-detail[data-key="topic:${topic}"]`,
          )
          .waitFor();
        await alice.click('[data-inspector-view="chat"]');
        // The account the product names posts on the channel it names.
        await card.locator("details.si-columns-about > summary").click();
        expect(await card.locator(".si-columns-about").innerText()).toContain(
          "posts here",
        );

        // No message text, plain or hex, in what the tools show or were sent.
        const shown = [
          await alice.locator("#chat-inspector").innerText(),
          await alice.locator("#statements-inspector").innerText(),
          await reportsSeen(alice),
        ].join("\n");
        for (const text of [
          "Hello Bob",
          "Hello Alice",
          "Hi Bob, from the web",
        ]) {
          expect(shown.includes(text)).toBe(false);
          expect(shown.includes(utf8Hex(text))).toBe(false);
        }
        // Reports are debounced: far fewer than the steps that changed them.
        const posted = JSON.parse(await reportsSeen(alice)) as unknown[];
        expect(posted.length).toBeGreaterThan(0);
        expect(posted.length).toBeLessThan(40);

        // A new worker run reports the same conversation again: what it
        // counts starts over, what it stored stays.
        await closeTools(alice);
        await reloadRunning(alice);
        expect(await testHooks(alice)).not.toContain(true);
        await novaCard(alice);
        await card.locator(".si-badge", { hasText: "established" }).waitFor();
        const again = await factsUntil(
          card,
          (each) => each.Account !== undefined && each.Messages !== undefined,
        );
        expect(again).toMatchObject({
          Messages: expect.stringMatching(/^reported 0 sent · 0 received/),
          "Stored history": "reported 1 yours · 1 theirs · 0 awaiting ack",
        });
        expect(again.Account).toBe(account);
        expect(await card.count()).toBe(1);

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
