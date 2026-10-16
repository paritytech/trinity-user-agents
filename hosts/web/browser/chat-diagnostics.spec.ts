/**
 * Headless-browser test of the inspector's Chat view on its own: the view and
 * a `ChatDiagnostics` mounted on a stand-in page of the host's Vite server,
 * fed synthetic metadata events. No wallet, chat engine, statement store or
 * chain is involved. It needs a Chromium: playwright-core finds the one it
 * installs, or set CHROMIUM_PATH.
 */
import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import {
  type Browser,
  type BrowserContext,
  type Page,
  chromium,
} from "playwright-core";
import { type RunningHost, startHost } from "./fixtures.js";
import type { ChatEventBody } from "../src/chat/diagnostics.js";

const STEP_MS = 60_000;
const PAGE = "/__chat-diagnostics-view.html";
const KEY = `0x${"ab".repeat(32)}`;
const INBOX = `0x${"5e".repeat(16)}${"a1".repeat(16)}`;
const SESSION = `0x${"c0".repeat(16)}${"de".repeat(16)}`;

/** A session's worth of events, in the shapes the engine records. */
const SAMPLE_EVENTS: ChatEventBody[] = [
  { type: "lifecycle", state: "starting" },
  {
    type: "subscription",
    stream: "requests",
    action: "subscribe",
    topics: 1,
    topicIds: [INBOX],
  },
  {
    type: "subscription",
    stream: "sessions",
    action: "subscribe",
    topics: 1,
    topicIds: [SESSION],
  },
  { type: "lifecycle", state: "listening" },
  {
    type: "session",
    peer: "0xee17…ad3b",
    data: "response",
    requestId: "0fc45950…",
    code: 0,
    matched: false,
  },
  {
    type: "session",
    peer: "0xee17…ad3b",
    data: "request",
    requestId: "4D4446B9…",
    kinds: [17, 17, 17, 1, 1],
    duplicates: 0,
  },
  {
    type: "request",
    requestId: "0DEE65E0…",
    peer: "0xee17…ad3b",
    version: 2,
    outcome: "contact active",
  },
  {
    type: "lookup",
    account: "0x3c42…5b45",
    outcome: "found",
    from: ["people"],
  },
  {
    type: "submit",
    purpose: "messages",
    peer: "0xee17…ad3b",
    requestId: "14a2e480…",
    messages: 1,
    result: "stored",
    status: "new",
  },
  {
    type: "session",
    peer: "0xee17…ad3b",
    data: "response",
    requestId: "14a2e480…",
    code: 0,
    matched: true,
  },
  {
    type: "lookup",
    account: "0x9f01…77aa",
    outcome: "error",
    reason: `the read failed for ${KEY}`,
  },
  {
    type: "set-aside",
    stream: "requests",
    reason: "its proof does not verify",
  },
  {
    type: "submit",
    purpose: "ack",
    peer: "0x9f01…77aa",
    result: "refused",
    status: "noAllowance",
  },
];

/** The stand-in page: the view in an inspector-sized box, events fed from the test. */
const VIEW_PAGE = `<!doctype html>
<html>
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <link rel="stylesheet" href="/src/style.css" />
    <style>
      body { display: block; height: 100dvh; }
      .inspector { height: 100dvh; max-height: none; }
    </style>
  </head>
  <body data-inspector="open">
    <section class="inspector">
      <div id="chat-diagnostics" class="chat-diagnostics" hidden></div>
    </section>
    <script type="module">
      import { mountChatDiagnostics } from "/src/chat-diagnostics-view.ts";
      import { ChatDiagnostics } from "/src/chat/diagnostics.ts";
      let now = Date.UTC(2026, 9, 16, 23, 56, 23);
      const diagnostics = new ChatDiagnostics(300, () => now);
      const view = mountChatDiagnostics(
        document.getElementById("chat-diagnostics"),
        diagnostics,
      );
      view.show(true);
      window.__diag = {
        record(events, stepMs) {
          for (const event of events) {
            now += stepMs;
            diagnostics.record(event);
          }
        },
        clear: () => diagnostics.clear(),
        count: () => diagnostics.list().length,
      };
    </script>
  </body>
</html>`;

interface DiagWindow {
  __diag: {
    record(events: ChatEventBody[], stepMs: number): void;
    clear(): void;
    count(): number;
  };
}

/** Open the stand-in page on a running host and wait for the view. */
async function openView(
  context: BrowserContext,
  origin: string,
): Promise<Page> {
  await context.route(`${origin}${PAGE}`, (route) =>
    route.fulfill({ contentType: "text/html", body: VIEW_PAGE }),
  );
  const page = await context.newPage();
  await page.goto(`${origin}${PAGE}`);
  await page.waitForFunction(() => "__diag" in window);
  return page;
}

const record = (page: Page, events: ChatEventBody[], stepMs = 1300) =>
  page.evaluate(
    ([events, stepMs]) =>
      (window as unknown as DiagWindow).__diag.record(events, stepMs),
    [events, stepMs] as const,
  );

let browser: Browser;
let host: RunningHost;

beforeAll(async () => {
  host = await startHost();
  browser = await chromium.launch({
    executablePath: process.env.CHROMIUM_PATH || undefined,
  });
}, STEP_MS);

afterAll(async () => {
  await browser?.close();
  await host?.close();
});

async function newPage(): Promise<{ context: BrowserContext; page: Page }> {
  const context = await browser.newContext({ timezoneId: "UTC" });
  context.setDefaultTimeout(STEP_MS);
  await context.grantPermissions(["clipboard-read", "clipboard-write"]);
  const page = await openView(context, host.origin);
  await record(page, SAMPLE_EVENTS);
  return { context, page };
}

const rows = (page: Page) => page.locator(".cdv-op");
const row = (page: Page, text: string) => rows(page).filter({ hasText: text });
const detail = (page: Page) => page.locator(".ins-detail .cdv-detail");
const statusText = (page: Page) => page.locator(".ins-status").innerText();

describe("chat diagnostics view", () => {
  test(
    "filters by search, direction, type and problems, and names message kinds",
    async () => {
      const { context, page } = await newPage();
      try {
        expect(await rows(page).count()).toBe(SAMPLE_EVENTS.length);
        expect(await row(page, "4D4446B9").innerText()).toContain(
          "device added (17) ×3, push token (1) ×2",
        );

        await page.click(".cdv-problems");
        expect(await rows(page).count()).toBe(3);
        expect(
          await page.locator(".ins-stat", { hasText: "events" }).innerText(),
        ).toContain("3 shown");
        await page.click(".cdv-problems");

        await page.locator(".ins-chan", { hasText: "sent" }).click();
        expect(await rows(page).count()).toBe(2);
        await page.locator(".ins-chan", { hasText: "all" }).click();

        await page.fill(".ins-filter", "14a2e480");
        expect(await rows(page).count()).toBe(2);
        await page.fill(".ins-filter", "device added");
        expect(await rows(page).count()).toBe(1);
        await page.fill(".ins-filter", "nothing like this");
        expect(await page.locator(".td-op-empty").innerText()).toBe(
          "no events match the filter",
        );
        await page.fill(".ins-filter", "");

        await page.selectOption(".ins-sort", "lookup");
        expect(await rows(page).count()).toBe(2);
      } finally {
        await context.close();
      }
    },
    STEP_MS,
  );

  test(
    "shows the selected event's detail and the events related to it",
    async () => {
      const { context, page } = await newPage();
      try {
        await row(page, "4D4446B9").click();
        expect(await detail(page).locator(".td-trace-id").innerText()).toBe(
          "#6 · session",
        );
        expect(
          await detail(page).locator(".td-frame-method").allTextContents(),
        ).toEqual([
          "request 4D4446B9… from 0xee17…ad3b · 5 messages: device added (17) ×3, push token (1) ×2",
          "device added (17)",
          "device added (17)",
          "device added (17)",
          "push token (1)",
          "push token (1)",
        ]);

        await detail(page).getByText("only account 0xee17…ad3b").click();
        expect(await rows(page).count()).toBe(5);
        expect(await page.locator(".cdv-related").innerText()).toBe(
          "account 0xee17…ad3b ✕",
        );
        // The first Escape closes the detail, the second drops the filter.
        await page.focus(".ins-list");
        await page.keyboard.press("Escape");
        expect(await detail(page).count()).toBe(0);
        await page.keyboard.press("Escape");
        expect(await rows(page).count()).toBe(SAMPLE_EVENTS.length);

        // The summary's account pills filter the same way.
        await page
          .locator(".ins-summary .ins-method", { hasText: "0x9f01" })
          .click();
        expect(await rows(page).count()).toBe(2);
      } finally {
        await context.close();
      }
    },
    STEP_MS,
  );

  test(
    "moves the selection with the keyboard",
    async () => {
      const { context, page } = await newPage();
      try {
        await page.focus(".ins-list");
        await page.keyboard.press("Home");
        await page.keyboard.press("ArrowDown");
        await page.keyboard.press("ArrowDown");
        expect(await rows(page).nth(2).getAttribute("aria-selected")).toBe(
          "true",
        );
        expect(await detail(page).locator(".td-trace-id").innerText()).toBe(
          "#3 · subscribe",
        );
        await page.keyboard.press("End");
        expect(await detail(page).innerText()).toContain("noAllowance");
        await page.keyboard.press("/");
        expect(
          await page.evaluate(() => document.activeElement?.className),
        ).toBe("ins-filter");
      } finally {
        await context.close();
      }
    },
    STEP_MS,
  );

  test(
    "shows and copies a subscription's topics in full",
    async () => {
      const { context, page } = await newPage();
      try {
        await row(page, "subscribe requests").click();
        expect(await detail(page).locator(".cdv-fields").innerText()).toContain(
          `request inbox topic\n${INBOX}`,
        );
        await row(page, "subscribe sessions").click();
        expect(await detail(page).locator(".cdv-fields").innerText()).toContain(
          `session topic 1\n${SESSION}`,
        );
        await page.fill(".ins-filter", SESSION.slice(10, 30));
        expect(await rows(page).count()).toBe(1);

        await detail(page).locator("[data-copy-event]").click();
        await detail(page)
          .locator("[data-copy-event]", { hasText: "copied" })
          .waitFor();
        const copied = JSON.parse(
          await page.evaluate(() => navigator.clipboard.readText()),
        );
        expect(copied.event).toEqual({
          type: "subscription",
          stream: "sessions",
          action: "subscribe",
          topics: 1,
          topicIds: [SESSION],
        });
      } finally {
        await context.close();
      }
    },
    STEP_MS,
  );

  test(
    "pausing freezes the list while recording goes on",
    async () => {
      const { context, page } = await newPage();
      try {
        await page.click(".cdv-follow");
        expect(await page.locator(".cdv-follow").innerText()).toBe("paused");
        await record(page, [
          { type: "lifecycle", state: "stopped" },
          { type: "lifecycle", state: "starting" },
        ]);
        expect(await rows(page).count()).toBe(SAMPLE_EVENTS.length);
        expect(
          await page.evaluate(() =>
            (window as unknown as DiagWindow).__diag.count(),
          ),
        ).toBe(SAMPLE_EVENTS.length + 2);
        expect(await statusText(page)).toContain("paused at #13");
        await page.click(".cdv-newer");
        expect(await rows(page).count()).toBe(SAMPLE_EVENTS.length + 2);
        expect(await page.locator(".cdv-follow").innerText()).toBe("following");
      } finally {
        await context.close();
      }
    },
    STEP_MS,
  );

  test(
    "a cleared session takes the selection, filter and pause with it",
    async () => {
      const { context, page } = await newPage();
      try {
        await row(page, "4D4446B9").click();
        await detail(page).getByText("only account 0xee17…ad3b").click();
        await page.click(".cdv-follow");
        await page.evaluate(() =>
          (window as unknown as DiagWindow).__diag.clear(),
        );
        expect(await rows(page).count()).toBe(0);
        expect(await page.locator(".cdv-follow").innerText()).toBe("following");
        expect(await page.locator(".cdv-related").isVisible()).toBe(false);
        expect(await detail(page).count()).toBe(0);
        expect(await page.locator(".ins-summary").innerText()).toContain(
          "No chat events yet",
        );
        await record(page, [{ type: "lifecycle", state: "starting" }]);
        expect(await rows(page).count()).toBe(1);
        expect(await detail(page).count()).toBe(0);
      } finally {
        await context.close();
      }
    },
    STEP_MS,
  );

  test(
    "keeps the recorded bound, counts what it evicts and drops a forgotten selection",
    async () => {
      const { context, page } = await newPage();
      try {
        await rows(page).first().click();
        const filler: ChatEventBody[] = Array.from({ length: 300 }, () => ({
          type: "subscription",
          stream: "sessions",
          action: "subscribe",
          topics: 1,
          topicIds: [SESSION],
        }));
        await record(page, filler, 10);
        expect(await rows(page).count()).toBe(300);
        expect(await detail(page).count()).toBe(0);
        expect(await statusText(page)).toContain("13 evicted");
      } finally {
        await context.close();
      }
    },
    STEP_MS,
  );

  test(
    "copies an event as metadata, with long hex shortened",
    async () => {
      const { context, page } = await newPage();
      try {
        await row(page, "0x9f01…77aa: error").click();
        expect(
          await page.locator(".chat-diagnostics").innerText(),
        ).not.toContain(KEY);
        await detail(page).locator("[data-copy-event]").click();
        await detail(page)
          .locator("[data-copy-event]", { hasText: "copied" })
          .waitFor();
        const copied = await page.evaluate(() =>
          navigator.clipboard.readText(),
        );
        expect(copied).not.toContain(KEY);
        expect(JSON.parse(copied)).toEqual({
          seq: 11,
          time: "2026-10-16T23:56:37.300Z",
          direction: "read",
          outcome: "error",
          event: {
            type: "lookup",
            account: "0x9f01…77aa",
            outcome: "error",
            reason: "the read failed for 0xabab…abab",
          },
        });
      } finally {
        await context.close();
      }
    },
    STEP_MS,
  );
});
