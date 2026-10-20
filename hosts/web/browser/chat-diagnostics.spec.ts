/**
 * Headless-browser test of the inspector's Chat view on its own: the view and
 * a `ChatDiagnostics` mounted on a stand-in page of the host's Vite server,
 * fed synthetic metadata events. No wallet, chat engine, statement store or
 * chain is involved. It needs a Chromium: playwright-core finds the one it
 * installs, or set CHROMIUM_PATH.
 */
import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { type Browser, type BrowserContext, type Page } from "playwright-core";
import { type RunningHost, startHost, testBrowser } from "./fixtures.js";
import type { ChatEventBody } from "../src/chat/diagnostics.js";
import type { ScenarioValues } from "./chat-transport-scenario.js";

const STEP_MS = 60_000;
const TEST_MS = 120_000;
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
        async run() {
          const { runScenario } = await import(
            "/browser/chat-transport-scenario.ts"
          );
          return runScenario(diagnostics);
        },
      };
    </script>
  </body>
</html>`;

interface DiagWindow {
  __diag: {
    record(events: ChatEventBody[], stepMs: number): void;
    clear(): void;
    count(): number;
    run(): Promise<ScenarioValues>;
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
  browser = await testBrowser();
}, STEP_MS);

afterAll(async () => {
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

describe("chat diagnostics transport layers", () => {
  async function scenarioPage(level: "off" | "wire" | "decrypted") {
    const context = await browser.newContext({ timezoneId: "UTC" });
    context.setDefaultTimeout(STEP_MS);
    await context.grantPermissions(["clipboard-read", "clipboard-write"]);
    const page = await openView(context, host.origin);
    await page.selectOption(".cdv-capture", level);
    const values = await page.evaluate(() =>
      (window as unknown as DiagWindow).__diag.run(),
    );
    return { context, page, values };
  }
  const layer = (page: Page, name: "Raw" | "Decrypted" | "Decoded") =>
    page.locator(".cdv-layer-tabs [data-layer]", { hasText: name }).click();
  const panel = (page: Page) => page.locator(".cdv-layer");
  const clipboard = (page: Page) =>
    page.evaluate(() => navigator.clipboard.readText());
  const copyIn = (page: Page, title: string) =>
    panel(page)
      .locator(".cdv-block", { hasText: title })
      .locator("[data-copy-layer]")
      .click();

  test(
    "shows a received session request raw, decrypted and decoded, with its values",
    async () => {
      const { context, page, values } = await scenarioPage("decrypted");
      try {
        await row(page, "push token (1), device added (17)").first().click();

        await layer(page, "Raw");
        const raw = await panel(page).innerText();
        expect(raw).toContain("Public:");
        expect(raw).toContain("JSON-RPC #");
        expect(raw).toContain("◀ received · statement_statement");
        expect(raw).toContain(`"statements":["${values.control}"`);
        expect(raw).toContain(
          `exact bytes the engine received, from the frame's hex`,
        );
        const statementCopy = panel(page)
          .locator(".cdv-block", { hasText: "Encoded signed statement" })
          .locator("[data-copy-layer]");
        await statementCopy.click();
        expect(await clipboard(page)).toBe(values.control);

        await layer(page, "Decrypted");
        expect(await panel(page).innerText()).toContain("Sensitive:");
        await panel(page).locator("[data-copy-layer]").click();
        expect(await clipboard(page)).toBe(values.controlPlaintext);

        await layer(page, "Decoded");
        const decodedText = await panel(page).innerText();
        expect(decodedText).toContain("Protocol fields");
        expect(decodedText).toContain("Inspector annotations");
        await copyIn(page, "Protocol fields");
        const protocol = JSON.parse(await clipboard(page));
        await copyIn(page, "Inspector annotations");
        const annotations = JSON.parse(await clipboard(page));
        // Each part copies alone: no annotation is mixed into the protocol fields.
        expect(JSON.stringify(protocol)).not.toContain('"name"');
        expect(JSON.stringify(protocol)).not.toContain("iso");
        expect(protocol.plaintext.messages.items).toMatchObject([
          {
            message: {
              versioned: {
                content: {
                  index: 1,
                  token: { bytes: values.token },
                  platform: 1,
                },
              },
            },
          },
          {
            message: {
              versioned: {
                content: {
                  index: 17,
                  statementAccountId: {
                    length: 32,
                    bytes: values.deviceAccount,
                  },
                  encryptionPublicKey: { length: 32, bytes: values.deviceKey },
                },
              },
            },
          },
        ]);
        expect(annotations.plaintext).toMatchObject({
          "$.messages.items[0].message.versioned.content.index": {
            name: "Token",
          },
          "$.messages.items[0].message.versioned.content.platform": {
            name: "IOS",
          },
          "$.messages.items[1].message.versioned.content.index": {
            name: "DeviceAdded",
          },
        });
        expect(annotations.statement.$.hash).toMatch(/^0x[0-9a-f]{64}$/);
        await panel(page).getByText("copy both").click();
        expect(JSON.parse(await clipboard(page))).toEqual({
          protocol,
          annotations,
        });

        // The chosen layer stays chosen for the next event.
        await row(page, "subscribe requests").first().click();
        await row(page, "push token (1), device added (17)").first().click();
        expect(
          await page
            .locator('.cdv-layer-tabs [aria-selected="true"]')
            .innerText(),
        ).toBe("Decoded");
      } finally {
        await context.close();
      }
    },
    STEP_MS,
  );

  test(
    "an acknowledgement shows the frame that sent it, the store's answer and the response it sealed",
    async () => {
      const { context, page } = await scenarioPage("decrypted");
      try {
        await row(page, "ack to").last().click();
        await layer(page, "Raw");
        const raw = await panel(page).innerText();
        expect(raw).toContain("▶ sent · statement_submit");
        expect(raw).toContain("◀ received · answer");
        expect(raw).toContain('"result":{"status":"new"}');
        expect(raw).toContain("exact bytes the engine submitted");
        await layer(page, "Decoded");
        await panel(page).getByText("copy both").click();
        const decoded = JSON.parse(await clipboard(page));
        expect(decoded.protocol.plaintext).toEqual({
          index: 1,
          requestId: expect.any(Object),
          responseCode: 0,
        });
        expect(decoded.annotations.plaintext["$.responseCode"]).toEqual({
          name: "acknowledged",
        });
      } finally {
        await context.close();
      }
    },
    STEP_MS,
  );

  test(
    "a statement that does not decrypt keeps its bytes and says why; a replay is a repeat",
    async () => {
      const { context, page, values } = await scenarioPage("decrypted");
      try {
        await page.fill(".ins-filter", values.garbledHash);
        const garbled = row(page, "set aside");
        expect(await garbled.count()).toBe(2);
        expect(
          await garbled.last().locator(".cdv-badge-repeat").textContent(),
        ).toBe("repeat ×2");
        await garbled.first().click();
        await layer(page, "Decrypted");
        expect(await panel(page).innerText()).toContain(
          "No decrypted bytes: could not decrypt",
        );
        await layer(page, "Raw");
        expect(await panel(page).innerText()).toContain(values.garbled);
      } finally {
        await context.close();
      }
    },
    STEP_MS,
  );

  test(
    "the wire level shows what travelled and keeps no plaintext",
    async () => {
      const { context, page, values } = await scenarioPage("wire");
      try {
        await row(page, "push token (1), device added (17)").first().click();
        await layer(page, "Raw");
        expect(await panel(page).innerText()).toContain(values.control);
        await layer(page, "Decrypted");
        expect(await panel(page).innerText()).toContain(
          "No decrypted bytes: decrypted capture is off",
        );
        await layer(page, "Decoded");
        await panel(page).getByText("copy both").click();
        expect(JSON.parse(await clipboard(page))).toMatchObject({
          protocol: { plaintext: null },
          annotations: { plaintext: null },
        });
        expect(
          await page.locator(".chat-diagnostics").innerText(),
        ).not.toContain(values.token);
      } finally {
        await context.close();
      }
    },
    STEP_MS,
  );

  test(
    "with capture off an event names its statement and holds no bytes",
    async () => {
      const { context, page, values } = await scenarioPage("off");
      try {
        await row(page, "push token (1), device added (17)").first().click();
        expect(await detail(page).innerText()).toContain(
          "Payload capture is off.",
        );
        expect(await detail(page).locator(".cdv-layer-tabs").count()).toBe(0);
        expect(
          await page.locator(".chat-diagnostics").innerText(),
        ).not.toContain(values.control.slice(2, 60));
      } finally {
        await context.close();
      }
    },
    STEP_MS,
  );

  test(
    "turning capture on later cannot show what travelled before",
    async () => {
      const { context, page } = await scenarioPage("off");
      try {
        await page.evaluate(() => new Promise((done) => setTimeout(done, 5)));
        await page.selectOption(".cdv-capture", "decrypted");
        await row(page, "push token (1), device added (17)").first().click();
        expect(await detail(page).innerText()).toContain(
          "travelled before capture was turned on",
        );
      } finally {
        await context.close();
      }
    },
    STEP_MS,
  );

  test(
    "no key or secret appears in any layer, and a session change drops everything",
    async () => {
      const { context, page, values } = await scenarioPage("decrypted");
      try {
        const seen: string[] = [];
        const count = await rows(page).count();
        for (let index = 0; index < count; index++) {
          await rows(page).nth(index).click();
          for (const name of ["Raw", "Decrypted", "Decoded"] as const)
            if ((await page.locator(".cdv-layer-tabs").count()) > 0) {
              await layer(page, name);
              seen.push(await panel(page).innerText());
            }
          seen.push(await detail(page).innerText());
        }
        const all = seen.join("\n").toLowerCase();
        expect(all).toContain(values.token.slice(2));
        for (const secret of values.secrets) expect(all).not.toContain(secret);

        expect(await page.locator(".ins-status").innerText()).toContain(
          "capturing wire + decrypted",
        );
        await page.evaluate(() =>
          (window as unknown as DiagWindow).__diag.clear(),
        );
        expect(await page.locator(".cdv-capture").inputValue()).toBe("off");
        expect(await rows(page).count()).toBe(0);
        expect(await detail(page).count()).toBe(0);
        expect(await page.locator(".ins-status").innerText()).toContain(
          "metadata only",
        );
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );
});
