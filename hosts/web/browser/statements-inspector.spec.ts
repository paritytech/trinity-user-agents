/**
 * Headless-browser test of the inspector's Statements view on its own: the
 * view and a `StatementLedger` mounted on a stand-in page of the host's Vite
 * server, fed synthetic traffic through the real People connection observer,
 * a claim, the request service's facts and a product report. No wallet,
 * core, product or chain is involved, and nothing leaves the page. It needs
 * a Chromium: playwright-core finds the one it installs, or set
 * CHROMIUM_PATH. Set STATEMENTS_SCREENSHOT to a directory to keep
 * screenshots of each tab.
 */
import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { type Browser, type BrowserContext, type Page } from "playwright-core";
import {
  TEST_MNEMONIC,
  buildFixture,
  openTools,
  publishApp,
  serveNames,
  startHost,
  testBrowser,
  type RunningHost,
} from "./support/fixtures.js";

const STEP_MS = 60_000;
const TEST_MS = 120_000;
const PAGE = "/__statements-view.html";

const VIEW_PAGE = `<!doctype html>
<html>
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <link rel="stylesheet" href="/src/style.css" />
    <style>
      body { display: block; background: #18191d; }
      .devtools { position: static; display: flex; flex-direction: column; width: 760px; height: 900px; padding: 8px; }
    </style>
  </head>
  <body>
    <aside class="devtools">
      <p style="color:#fbbf24;font:12px monospace;margin:0 0 6px">Synthetic example: Alice and Bob chat in Contact Chat. Made-up accounts and data.</p>
      <div id="statements-inspector" class="statements-inspector" role="tabpanel" aria-label="Statements"></div>
      <div id="chat-inspector" class="statements-inspector chat-inspector" role="tabpanel" aria-label="Chat" hidden></div>
    </aside>
    <script type="module">
      import { StatementLedger } from "/tools/statements/ledger.ts";
      import { mountStatementsView } from "/tools/statements/view.ts";
      import { mountChatView } from "/tools/chat/view.ts";
      import { observedConnection } from "/src/platform/statement-observer.ts";
      import { encodeSigned } from "/src/platform/statement.ts";

      const ledger = new StatementLedger();
      ledger.startSession("demo", "demo wallet, signed in");
      const shown = [];
      const peopleConnections = [];
      window.__peopleConnections = peopleConnections;
      const view = mountStatementsView(document.getElementById("statements-inspector"), {
        ledger,
        // Every People connection the view opens, and what it sends there.
        people: () => ({
          genesis: "0x" + "aa".repeat(32),
          chain: {
            async connect() {
              peopleConnections.push([]);
              const sent = peopleConnections.at(-1);
              return {
                send: (text) => sent.push(JSON.parse(text).method),
                async *responses() {},
                close() {},
              };
            },
          },
        }),
        network: () => "testnet",
        showTraces: (name) => shown.push("traces:" + name),
        inspectWorker: (name) => shown.push("worker:" + name),
      });
      const chat = mountChatView(document.getElementById("chat-inspector"), {
        ledger,
        showStatements(target) {
          shown.push("statements:" + JSON.stringify(target));
          window.__showView("statements");
          view.reveal(target);
        },
        showTraces: (name) => shown.push("traces:" + name),
        inspectWorker: (name) => shown.push("worker:" + name),
      });
      window.__showView = (name) => {
        view.show(name === "statements");
        chat.show(name === "chat");
      };
      window.__showView("chat");

      const fill = (byte) => new Uint8Array(32).fill(byte);
      const hex = (bytes) => "0x" + Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
      const own = fill(0xa1), peer = fill(0xb2);
      const sendTopic = fill(0x11), readTopic = fill(0x22), channel = fill(0x33);
      const now = Math.floor(Date.now() / 1000);
      const text = (messages) => new TextEncoder().encode(JSON.stringify({ version: "demo/1", kind: "messages", messages }));
      const statement = (signer, topic, ahead, data) => hex(encodeSigned(
        { expiry: BigInt(now + ahead) << 32n, channel, topics: [topic], data },
        signer,
        () => new Uint8Array(64),
      ));

      // A connection whose answers the page pushes, observed as the core's.
      const queue = []; let wake = () => {};
      const inner = {
        send() {},
        async *responses() { for (;;) { if (queue.length) { yield queue.shift(); continue; } await new Promise((r) => (wake = r)); } },
        close() {},
      };
      const connection = observedConnection(inner, "core", ledger.traffic);
      (async () => { for await (const _ of connection.responses()); })();
      const answer = (value) => { queue.push(JSON.stringify(value)); wake(); };
      const settle = () => new Promise((r) => setTimeout(r, 20));

      window.__feed = async () => {
        const index = "0x" + "01".repeat(32);
        const handle = "0x" + "44".repeat(16);
        ledger.invitations({ type: "request-sent", product: "contact-chat.test", handle });
        ledger.invitations({ type: "reply-authenticated", product: "contact-chat.test", handle });
        await ledger.claim(
          { kind: "product", product: "contact-chat.test", index, submit: true },
          async () => ({ account: hex(own), period: 20_390, outcome: "registered", seq: 4, collection: "People", blockHash: "0x" + "be".repeat(32) }),
        );
        ledger.invitations({ type: "chat-account", product: "contact-chat.test", handle, index, outcome: "allowed", account: hex(own), until: Date.now() + 3_600_000 });
        ledger.grants(ledger.epoch(), "0x" + "cd".repeat(32), [{ account: hex(own), value: { maxCount: 2, maxSize: 512000 } }]);
        const conversation = (sent) => ({
          id: "0x" + "c0".repeat(16),
          protocol: "contact-chat/simple-text/1",
          state: "connected",
          request: handle,
          account: hex(own),
          peers: [hex(peer)],
          listen: [hex(readTopic)],
          post: { topics: [hex(sendTopic)], channel: hex(channel) },
          logical: { sent, received: 1, duplicates: 0, failed: 0, posts: sent },
          window: { messages: sent, bytes: 90, maxMessages: 20, maxBytes: 16384, retentionSeconds: 172800 },
        });
        // The run's first report starts the cost window, before anything is sent.
        ledger.diagnostics({ name: "contact-chat.test", run: 1 }, { conversations: [conversation(0)] });

        connection.send(JSON.stringify({ jsonrpc: "2.0", id: 1, method: "statement_subscribeStatement", params: [{ matchAny: [hex(readTopic)] }] }));
        answer({ jsonrpc: "2.0", id: 1, result: "sub-1" });
        // As the light client answers: an empty first page, then what peers send, with no remaining.
        answer({ jsonrpc: "2.0", method: "statement_statement", params: { subscription: "sub-1", result: { event: "newStatements", data: { statements: [], remaining: 0 } } } });
        answer({ jsonrpc: "2.0", method: "statement_statement", params: { subscription: "sub-1", result: { event: "newStatements", data: { statements: [statement(peer, readTopic, 3600, text([{ id: "m1", text: "hello from the peer", at: 1 }]))] } } } });
        connection.send(JSON.stringify({ jsonrpc: "2.0", id: 2, method: "statement_submit", params: [statement(own, sendTopic, 7200, text([{ id: "m2", text: "first", at: 2 }]))] }));
        answer({ jsonrpc: "2.0", id: 2, result: { status: "new" } });
        connection.send(JSON.stringify({ jsonrpc: "2.0", id: 3, method: "statement_submit", params: [statement(own, sendTopic, 7300, text([{ id: "m2", text: "first", at: 2 }, { id: "m3", text: "second", at: 3 }]))] }));
        answer({ jsonrpc: "2.0", id: 3, result: { status: "new" } });
        connection.send(JSON.stringify({ jsonrpc: "2.0", id: 4, method: "statement_submit", params: [statement(own, sendTopic, 7000, text([]))] }));
        answer({ jsonrpc: "2.0", id: 4, error: { code: 7001, message: "Statement store error: none of the 3 connected peers accepted the statement" } });
        await settle();
        ledger.diagnostics({ name: "contact-chat.test", run: 1 }, { conversations: [conversation(2)] });
        await settle();
        return { own: hex(own) };
      };
      // A negative case, kept out of the example: another product naming
      // Alice's request handle and account.
      window.__borrow = async () => {
        ledger.diagnostics({ name: "other.test", run: 1 }, {
          conversations: [{ id: "0x" + "0b".repeat(16), protocol: "other/1", request: "0x" + "44".repeat(16), account: hex(own) }],
        });
        await settle();
      };
      window.__shown = shown;
      window.__ledger = ledger;
    </script>
  </body>
</html>`;

interface ViewWindow {
  __feed(): Promise<{ own: string }>;
  __borrow(): Promise<void>;
  __showView(name: "chat" | "statements"): void;
  __shown: string[];
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

async function openView(): Promise<{ context: BrowserContext; page: Page }> {
  const context = await browser.newContext({
    viewport: { width: 800, height: 940 },
  });
  context.setDefaultTimeout(STEP_MS);
  await context.grantPermissions(["clipboard-read", "clipboard-write"]);
  await context.route(
    (url) => !url.href.startsWith(host.origin),
    (route) => route.abort(),
  );
  await context.route(`${host.origin}${PAGE}`, (route) =>
    route.fulfill({ contentType: "text/html", body: VIEW_PAGE }),
  );
  const page = await context.newPage();
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto(`${host.origin}${PAGE}`);
  await page.waitForFunction(() => "__feed" in window);
  await page.evaluate(() => (window as unknown as ViewWindow).__feed());
  // The view draws a moment after the ledger changes.
  await page.waitForSelector(".si-card");
  expect(errors).toEqual([]);
  return { context, page };
}

const shot = async (page: Page, name: string) => {
  const dir = process.env.STATEMENTS_SCREENSHOT;
  if (dir !== undefined && dir !== "")
    await page.locator(".devtools").screenshot({ path: `${dir}/${name}.png` });
};

const factsOf = (page: Page, scope: string) =>
  page.locator(`${scope} .si-facts > dt`).evaluateAll((terms) =>
    terms.map((term) => {
      const value = term.nextElementSibling as HTMLElement;
      // What shows, then ‖ and what its closed "why" holds.
      const why = value.querySelector(".si-why .si-note")?.textContent ?? "";
      return `${term.textContent} | ${value.querySelector(".si-src")?.textContent} | ${value.innerText.replace(/\s+/g, " ")} ‖ ${why}`;
    }),
  );

describe("the Statements and Chat views", () => {
  test(
    "Chat shows what the product reported, tied to host facts only where the host ties them, and links to Statements",
    async () => {
      const { context, page } = await openView();
      try {
        const card = page.locator("#chat-inspector .si-card").first();
        expect(await card.locator(".si-card-title").innerText()).toContain(
          "Conversation",
        );
        const facts = await factsOf(
          page,
          "#chat-inspector .si-card:first-of-type",
        );
        const line = (label: string) =>
          facts.find((entry) => entry.startsWith(`${label} |`)) ?? "";
        expect(line("Protocol")).toContain(
          "| reported | reported contact-chat/simple-text/1",
        );
        expect(line("Request")).toContain("| host |");
        expect(line("Request")).toContain("sender · reply authenticated");
        expect(line("chatAccount")).toContain("allowed 0xa1a1a1");
        expect(line("Account")).toContain("| reported | reported 0xa1a1a1");
        expect(line("Account")).toContain(
          "‖ The account the product says it posts as",
        );
        // The tie is the host's fact, on its own line with its own tag.
        expect(line("Tie")).toContain("| host | host tied by host");
        expect(line("Account")).not.toContain("tied by host");
        expect(line("Delivery")).toContain(
          "‖ The host's request and delivery events for it",
        );
        expect(line("Read topics")).toContain("| reported |");
        expect(line("Messages")).toContain(
          "2 sent · 1 received · 0 duplicates",
        );
        expect(line("Window")).toContain("2/20 msgs");
        expect(line("Per message")).toMatch(
          /\| derived \| derived 1\.00 stmts · \d+ B \(2 broadcast · 3 tried · 2 sent\)/,
        );
        // Resources are Statements': none show in the Chat card.
        for (const label of ["Claim", "Grant", "Observed", "Posts", "Channel"])
          expect(line(label)).toBe("");
        expect(await card.locator(".si-why[open]").count()).toBe(0);
        await shot(page, "chat");

        // The account link opens Statements on that account.
        await card
          .locator(".si-facts dd", { hasText: "tied by host" })
          .locator("button")
          .click();
        await page.locator("#statements-inspector .si-detail").waitFor();
        const account = await factsOf(page, "#statements-inspector .si-detail");
        const at = (label: string) =>
          account.find((entry) => entry.startsWith(`${label} |`)) ?? "";
        expect(at("Claim")).toContain("| core |");
        expect(at("Claim")).toContain(
          "allowance slot 4 (People) · day 20390 · claimed now",
        );
        expect(at("Claim")).toContain("not the person's registration");
        expect(at("Grant")).toContain("2 stmts · 500.0 KiB · not remaining");
        expect(at("Observed")).toMatch(
          /1 stmt · \d+ B · seen \d\d:\d\d:\d\d · not current/,
        );
        expect(at("Posts")).toContain("3 submitted: 2 broadcast");
        expect(at("Peer refusals")).toMatch(/\| inspector not visible\s*why ‖/);
        expect(account.filter((entry) => entry.startsWith("Role |"))).toEqual([
          expect.stringContaining(
            "contact-chat.test's account at Raw(0x0101…0101)",
          ),
          expect.stringContaining("allowed for request 0x4444…4444"),
          expect.stringContaining(
            "| reported | reported contact-chat.test says it posts as this account",
          ),
        ]);
        const shown = await page.locator("#statements-inspector").innerText();
        expect(shown).not.toMatch(/at least \d+ of|lower bound|\bof 2\b/i);
        // Statements names no protocol.
        expect(shown).not.toContain("simple-text");
        const caveats = page.locator("#statements-inspector .si-caveats");
        expect(await caveats.locator("summary").innerText()).toBe(
          "light client · new = broadcast, not stored · peer refusals unseen · observed ≠ current · grant ≠ remaining · data is public",
        );
        await shot(page, "accounts");

        // An open "why" stays open when the ledger changes and the view draws again.
        const grantWhy = page
          .locator("#statements-inspector .si-detail dd", {
            hasText: "not remaining",
          })
          .locator(".si-why");
        await grantWhy.locator("summary").click();
        await page.evaluate(() =>
          (
            window as unknown as {
              __ledger: { invitations(event: object): void };
            }
          ).__ledger.invitations({
            type: "request-paused",
            product: "contact-chat.test",
            handle: "0x" + "44".repeat(16),
            problem: null,
          }),
        );
        await page.waitForTimeout(600);
        expect(await grantWhy.getAttribute("open")).not.toBeNull();

        // Filters set by hand do not hide what a link opens.
        await page.locator(".si-tab", { hasText: "Timeline" }).click();
        await page.locator(".si-chip.si-errors").click();
        await page.fill(
          "#statements-inspector .si-search",
          "nothing matches this",
        );
        expect(
          await page.locator("#statements-inspector .si-event-text").count(),
        ).toBe(0);

        // A topic link narrows the Statements timeline to it.
        await page.evaluate(() =>
          (window as unknown as ViewWindow).__showView("chat"),
        );
        const chatWhy = card
          .locator(".si-facts dd", { hasText: "contact-chat/simple-text/1" })
          .locator(".si-why");
        await chatWhy.locator("summary").click();
        await page.evaluate(() =>
          (
            window as unknown as {
              __ledger: { invitations(event: object): void };
            }
          ).__ledger.invitations({
            type: "request-paused",
            product: "contact-chat.test",
            handle: "0x" + "44".repeat(16),
            problem: null,
          }),
        );
        await page.waitForTimeout(600);
        expect(await chatWhy.getAttribute("open")).not.toBeNull();
        await page
          .locator("#chat-inspector .si-facts dd", { hasText: "0x2222" })
          .first()
          .locator("button")
          .first()
          .click();
        await page.locator("#statements-inspector .si-focus").waitFor();
        expect(
          await page.locator("#statements-inspector .si-focus").innerText(),
        ).toContain("topic");
        const texts = await page
          .locator("#statements-inspector .si-event-text")
          .allInnerTexts();
        expect(
          texts.some((text) => text.includes("after the initial batch")),
        ).toBe(true);
        expect(
          await page
            .locator("#statements-inspector .si-errors")
            .getAttribute("aria-pressed"),
        ).toBe("false");
        expect(
          await page.locator("#statements-inspector .si-search").inputValue(),
        ).toBe("");
        expect(
          texts.some((text) => text.includes("reports conversation")),
        ).toBe(false);
        await shot(page, "timeline");

        await page.evaluate(() =>
          (window as unknown as ViewWindow).__showView("chat"),
        );
        await card.locator("button", { hasText: "Worker" }).click();
        await card.locator("button", { hasText: "TrUAPI" }).click();
        const calls = await page.evaluate(
          () => (window as unknown as ViewWindow).__shown,
        );
        expect(calls.slice(-2)).toEqual([
          "worker:contact-chat.test",
          "traces:contact-chat.test",
        ]);
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );

  test(
    "a product naming another's request and account gets no host facts in Chat",
    async () => {
      const { context, page } = await openView();
      try {
        await page.evaluate(() => (window as unknown as ViewWindow).__borrow());
        const borrower = page.locator("#chat-inspector .si-card", {
          hasText: "other.test",
        });
        await borrower.waitFor();
        const text = await borrower.innerText();
        expect(text).toContain("not held by host");
        expect(text).toContain("reported only");
        expect(text).not.toContain("reply authenticated");
        expect(
          await borrower.locator("button", { hasText: "Statements" }).count(),
        ).toBe(0);
        expect(text).toContain(
          "not known: the host holds no claim or allowance tying its account to other.test",
        );
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );

  test(
    "Statements lists subscriptions and topics with no probe, captures only when asked, and copies metadata only",
    async () => {
      const { context, page } = await openView();
      try {
        await page.evaluate(() =>
          (window as unknown as ViewWindow).__showView("statements"),
        );
        expect(
          await page.locator("#statements-inspector .si-tab").allInnerTexts(),
        ).toEqual(["Accounts", "Topics", "Timeline"]);
        await page.locator(".si-tab", { hasText: "Topics" }).click();
        await page.waitForSelector(".si-sub-row");
        expect(await page.locator(".si-sub-row").first().innerText()).toContain(
          "core · open · 0 initial · 1 later",
        );
        expect(
          await page
            .locator("#statements-inspector button", { hasText: /probe/i })
            .count(),
        ).toBe(0);
        expect(await page.locator(".si-no-probe").innerText()).toContain(
          "the inspector opens no statement subscription of its own",
        );
        expect(
          await page.evaluate(
            () =>
              (window as unknown as { __peopleConnections: string[][] })
                .__peopleConnections,
          ),
        ).toEqual([]);
        await shot(page, "topics");

        await page.locator(".si-tab", { hasText: "Timeline" }).click();
        await page.locator(".si-chip.si-errors").click();
        expect(await page.locator(".si-event-text").allInnerTexts()).toEqual([
          expect.stringContaining(
            "not broadcast: none of the 3 connected peers took it",
          ),
        ]);

        await page.locator(".si-chip", { hasText: "Capture data" }).click();
        expect(
          await page.locator("#statements-inspector .si-warning").innerText(),
        ).toContain("never exported");
        await page.locator(".si-chip", { hasText: /Capturing/ }).click();
        expect(
          await page.locator("#statements-inspector .si-warning").isHidden(),
        ).toBe(true);

        await page.locator(".si-chip", { hasText: "Copy evidence" }).click();
        const copied = await page.evaluate(() =>
          navigator.clipboard.readText(),
        );
        expect((JSON.parse(copied) as { format: string }).format).toBe(
          "truapi-web-host/statements-evidence/1",
        );
        expect(copied).not.toContain("hello from the peer");
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );

  test(
    "Statements and Chat are inspector views of the host page, and start empty",
    async () => {
      const context = await browser.newContext({
        viewport: { width: 1440, height: 900 },
      });
      context.setDefaultTimeout(STEP_MS);
      // Nothing leaves loopback: no chain is reached.
      await context.route("**/*", (route) =>
        new URL(route.request().url()).hostname === "localhost"
          ? route.continue()
          : route.abort(),
      );
      const page = await context.newPage();
      try {
        await page.goto(host.origin);
        await openTools(page, "inspector");
        await page.click('[data-inspector-view="statements"]');
        await page.locator("#statements-inspector .si-title").waitFor();
        expect(await page.locator("#inspector-note").innerText()).toContain(
          "in memory for this session",
        );
        expect(
          await page.locator("#statements-inspector .si-empty").innerText(),
        ).toContain("No account seen yet");
        await page.click('[data-inspector-view="chat"]');
        expect(await page.locator("#inspector-note").innerText()).toContain(
          "report about themselves",
        );
        expect(
          await page.locator("#chat-inspector .si-empty").innerText(),
        ).toContain("No conversation reported.");
        await shot(page, "host-empty");
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );

  test(
    "shows what a running worker reports, as reported, and marks it stale once the worker is removed",
    async () => {
      const worker = "d.paseo";
      const context = await browser.newContext({
        viewport: { width: 1280, height: 900 },
      });
      context.setDefaultTimeout(STEP_MS);
      await context.routeWebSocket(/.*/, (socket) => socket.close());
      await context.route(
        (url) => !url.href.startsWith(host.origin),
        (route) => route.abort(),
      );
      await serveNames(context, [
        {
          record: `worker.${worker}`,
          app: publishApp({
            "index.js": await buildFixture("diagnostics-worker.ts"),
          }),
          text: {
            executable: JSON.stringify({
              $v: 1,
              kind: "worker",
              appVersion: [0, 1, 0],
              entrypoint: "index.js",
              includes: { chat: true },
            }),
          },
        },
      ]);
      const page = await context.newPage();
      try {
        await page.goto(host.origin);
        await page
          .locator("#core-status")
          .filter({ hasText: "Core ready" })
          .waitFor({ state: "attached" });
        await openTools(page, "account");
        await page
          .locator("#import")
          .evaluate((details) => ((details as HTMLDetailsElement).open = true));
        await page.fill("#mnemonic", TEST_MNEMONIC);
        await page.click("#add-wallet");
        await page
          .locator("#session")
          .filter({ hasNotText: "Signed out" })
          .waitFor();
        await openTools(page, "product");
        await page.fill("#worker-name", worker);
        await page.press("#worker-name", "Enter");
        await page
          .locator(
            `#worker-list [data-worker="${worker}"][data-state="running"]`,
          )
          .waitFor();

        await openTools(page, "inspector");
        await page.click('[data-inspector-view="chat"]');
        const card = page.locator("#chat-inspector .si-card");
        await card.filter({ hasText: "demo/plain/1" }).waitFor();
        expect(await card.innerText()).toContain("waiting");
        expect(await card.locator(".si-src").first().innerText()).toBe(
          "reported",
        );

        // A new report replaces the last one of that run.
        const frame = page
          .frames()
          .find((entry) => entry.url().includes("__truapi-worker.html"))!;
        await frame.evaluate(() =>
          (
            window as unknown as {
              __diagnosticsWorker: { report(state: string): void };
            }
          ).__diagnosticsWorker.report("connected"),
        );
        await card.locator(".si-badge", { hasText: "connected" }).waitFor();
        expect(await card.count()).toBe(1);

        // A removed worker's last report no longer holds.
        await openTools(page, "product");
        await page.click(
          `#worker-list [data-worker="${worker}"] >> text=Remove`,
        );
        await openTools(page, "inspector");
        await card
          .locator(".si-badge", { hasText: "stale: worker stopped" })
          .waitFor();
        await shot(page, "host-worker-report");
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );
});
