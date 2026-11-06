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
import type { StatementLedger } from "../tools/statements/ledger.js";

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
      import { peopleTransport } from "/tools/statements/transport.ts";
      import { mountStatementsView } from "/tools/statements/view.ts";
      import { mountChatView } from "/tools/chat/view.ts";
      import { observedConnection } from "/src/platform/statement-observer.ts";
      import { encodeSigned } from "/src/platform/statement.ts";
      import { isNoSubmitRefusal } from "/tools/demos/no-submit.ts";
      import { inboxTopic, periodOf } from "/src/invitations/statement-delivery.ts";

      // As the host page builds it with People on the light client: the dev
      // tools' guard says which errors it made.
      const ledger = new StatementLedger(
        peopleTransport("light-client"),
        {},
        Date.now,
        isNoSubmitRefusal,
      );
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
        showChat(key) {
          shown.push("chat:" + key);
          window.__showView("chat");
          chat.reveal(key);
        },
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

      // A connection whose answers the page pushes, observed as \`source\`'s.
      const pushed = (source) => {
        const queue = []; let wake = () => {};
        const inner = {
          send() {},
          async *responses() { for (;;) { if (queue.length) { yield queue.shift(); continue; } await new Promise((r) => (wake = r)); } },
          close() {},
        };
        const connection = observedConnection(inner, source, ledger.traffic);
        (async () => { for await (const _ of connection.responses()); })();
        return { connection, answer: (value) => { queue.push(JSON.stringify(value)); wake(); } };
      };
      const { connection, answer } = pushed("core");
      const host = pushed("host");
      const settle = () => new Promise((r) => setTimeout(r, 20));
      // The host's day inbox on this network, as its delivery reads it: today's and yesterday's.
      const today = inboxTopic("testnet", periodOf(Date.now()));
      const yesterday = inboxTopic("testnet", periodOf(Date.now()) - 1);
      const notification = fill(0xd4);
      // Another host's request on the shared inbox: sealed to this wallet or not, its signer is not this wallet's.
      const stranger = fill(0xe7);

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
          facts: [
            { label: "Published message batch", value: \`\${sent} of 20 messages · 90 of 16384 B\` },
            { label: "Requested expiry", value: "48 h per post; peers may evict it sooner" },
          ],
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
        // The host reads its day inbox, and posts the request there as a notification account.
        host.connection.send(JSON.stringify({ jsonrpc: "2.0", id: 1, method: "statement_subscribeStatement", params: [{ matchAny: [hex(yesterday), hex(today)] }] }));
        host.answer({ jsonrpc: "2.0", id: 1, result: "inbox" });
        host.answer({ jsonrpc: "2.0", method: "statement_statement", params: { subscription: "inbox", result: { event: "newStatements", data: { statements: [], remaining: 0 } } } });
        ledger.invitations({ type: "delivery-account", post: handle, account: hex(notification), period: periodOf(Date.now()) });
        host.connection.send(JSON.stringify({ jsonrpc: "2.0", id: 2, method: "statement_submit", params: [hex(encodeSigned({ expiry: BigInt(now + 86400) << 32n, channel: null, topics: [today], data: new Uint8Array(200) }, notification, () => new Uint8Array(64)))] }));
        host.answer({ jsonrpc: "2.0", id: 2, result: { status: "new" } });
        host.answer({ jsonrpc: "2.0", method: "statement_statement", params: { subscription: "inbox", result: { event: "newStatements", data: { statements: [hex(encodeSigned({ expiry: BigInt(now + 86400) << 32n, channel: null, topics: [today], data: new Uint8Array(300) }, stranger, () => new Uint8Array(64)))] } } } });
        ledger.invitations({ type: "request-received", product: "contact-chat.test", handle: "0x" + "55".repeat(16) });
        await settle();
        ledger.diagnostics({ name: "contact-chat.test", run: 1 }, { conversations: [conversation(2)] });
        await settle();
        return { own: hex(own) };
      };
      // A submission this tab's no-submit guard answers, before any light client.
      window.__refuseHere = async () => {
        connection.send(JSON.stringify({ jsonrpc: "2.0", id: 5, method: "statement_submit", params: [statement(own, sendTopic, 7400, text([]))] }));
        answer({ jsonrpc: "2.0", id: 5, error: { code: -32000, message: "statement_submit refused: this tab submits nothing" } });
        await settle();
      };
      // A negative case, kept out of the example: another product naming
      // Alice's request handle and account.
      window.__borrow = async () => {
        ledger.diagnostics({ name: "other.test", run: 1 }, {
          conversations: [{ id: "0x" + "0b".repeat(16), protocol: "other/1", request: "0x" + "44".repeat(16), account: hex(own) }],
        });
        await settle();
      };
      window.__reveal = (target) => {
        window.__showView("statements");
        view.reveal(target);
      };
      window.__shown = shown;
      window.__ledger = ledger;
    </script>
  </body>
</html>`;

interface ViewWindow {
  __feed(): Promise<{ own: string }>;
  __reveal(
    target: { account: string; channel?: string } | { topic: string },
  ): void;
  __ledger: StatementLedger;
  __borrow(): Promise<void>;
  __refuseHere(): Promise<void>;
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
        // The title comes from the host's request, not the product's local id.
        expect(await card.locator(".si-card-title").innerText()).toBe(
          "Chat you started",
        );
        expect(await card.locator(".si-card-head").innerText()).not.toContain(
          "0xc0c0",
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
        // The tie is to the product, never to one conversation.
        expect(line("Tie")).toContain(
          "| host | host tied to contact-chat.test",
        );
        expect(line("Tie")).toContain(
          "‖ A claim or an allowance of contact-chat.test names this account. The host ties accounts to a product, not to a conversation.",
        );
        expect(line("Account")).not.toContain("tied to");
        expect(line("Delivery")).toContain(
          "‖ The Statements timeline narrowed to this request",
        );
        expect(line("Listens on")).toContain("| reported | reported 0x2222");
        expect(line("Publishes on")).toContain("| reported | reported 0x1111");
        expect(line("Peers")).toContain("| reported | reported 0xb2b2");
        expect(line("Messages")).toContain(
          "2 sent · 1 received · 0 duplicates",
        );
        // What one post carries is the protocol's: under Protocol details, as reported.
        expect(line("Window")).toBe("");
        expect(
          await card
            .locator(".si-sub", { hasText: "Protocol details" })
            .count(),
        ).toBe(1);
        expect(line("Published message batch")).toContain(
          "| reported | reported 2 of 20 messages · 90 of 16384 B",
        );
        expect(line("Requested expiry")).toContain(
          "| reported | reported 48 h per post; peers may evict it sooner",
        );
        // The conversation's local id is a detail, said for what it is.
        const details = card.locator("details.si-columns-about");
        expect(await details.getAttribute("open")).toBeNull();
        expect(line("Conversation ID")).toContain(
          "‖ The product's own local id for this conversation. Not an account, a key or a topic.",
        );
        expect(line("Per message")).toMatch(
          /\| derived \| derived 1\.00 stmts · \d+ B \(2 broadcast · 3 tried · 2 sent\)/,
        );
        // Resources are Statements': the card shows none, only what this
        // tab observed of its accounts, as links there.
        for (const label of ["Claim", "Grant", "Observed", "Posts", "Answers"])
          expect(line(label)).toBe("");
        expect(await card.locator(".si-why[open]").count()).toBe(0);
        await shot(page, "chat");

        // The account itself is the link: it opens Statements on that account, focused.
        await card
          .locator("dd", {
            has: page.locator(".si-src", { hasText: "reported" }),
          })
          .locator(".si-link", { hasText: "0xa1a1" })
          .first()
          .click();
        await page.locator("#statements-inspector .si-detail").waitFor();
        expect(
          await page.evaluate(
            () =>
              (document.activeElement as HTMLElement | null)?.getAttribute(
                "aria-pressed",
              ) ?? "",
          ),
        ).toBe("true");
        const account = await factsOf(page, "#statements-inspector .si-detail");
        const at = (label: string) =>
          account.find((entry) => entry.startsWith(`${label} |`)) ?? "";
        expect(at("Owner")).toContain(
          "| core | core This walletwhy ‖ Derived from the signed-in wallet in this session: contact-chat.test's account at Raw(0x0101…0101); allowed for request 0x4444…4444. A record of this tab, not a signature check.",
        );
        expect(at("Claim")).toContain("| core |");
        expect(at("Claim")).toContain(
          "allowance slot 4 (People) · day 20390 · claimed now",
        );
        expect(at("Claim")).toContain("not the person's registration");
        expect(at("Grant")).toContain("2 stmts · 500.0 KiB · not remaining");
        expect(at("Observed")).toMatch(
          /1 stmt · \d+ B · seen \d\d:\d\d:\d\d · not current/,
        );
        expect(at("Posts")).toContain("| inspector | inspector 3 tried");
        expect(at("Answers")).toContain(
          "| light client | light client 2 broadcast · 1 RPC error",
        );
        expect(at("Refused here")).toBe("");
        expect(at("Peer refusals")).toMatch(/\| inspector not visible\s*why ‖/);
        // Role says what it is used for; how it was derived and what it may
        // spend are details of their own.
        expect(
          account.filter((entry) =>
            /^(Role|Derived as|Allowance) \|/.test(entry),
          ),
        ).toEqual([
          expect.stringContaining(
            "Role | reported | reported contact-chat.test says it posts as this account",
          ),
          expect.stringContaining(
            "Derived as | core | core contact-chat.test's account at Raw(0x0101…0101)",
          ),
          expect.stringContaining(
            "Allowance | host | host contact-chat.test · allowed for request 0x4444…4444",
          ),
        ]);
        // The host's allowance ties it to that request's chat: one link,
        // the host's, to the conversation the product reports for it.
        expect(account.filter((entry) => entry.startsWith("Chat |"))).toEqual([
          expect.stringContaining(
            "Chat | host | host contact-chat.test · allowed for request 0x4444…4444",
          ),
        ]);
        // The list says each account in a few words under named columns;
        // the caveats are said once, under About these columns.
        const rows = page.locator("#statements-inspector .si-list-row");
        const selected = page.locator(
          '#statements-inspector .si-list-row[aria-pressed="true"]',
        );
        expect(await selected.locator(".si-cell").allTextContents()).toEqual([
          "Chat sender reported",
          "This wallet",
          "Slot 4 · claimed",
          "2 · 500.0 KiB",
          expect.stringMatching(/^1 · \d+ B · \d\d:\d\d$/),
        ]);
        expect(
          await page
            .locator("#statements-inspector .si-list-head span")
            .allTextContents(),
        ).toEqual([
          "Account",
          "Role",
          "Owner",
          "Allocation",
          "Grant",
          "Observed",
        ]);
        // On a wide dock the headings sit over their columns.
        await page.setViewportSize({ width: 1240, height: 940 });
        const devtools = page.locator(".devtools");
        await devtools.evaluate((node) => {
          (node as HTMLElement).style.width = "1200px";
        });
        const lefts = await page.evaluate(() => {
          const left = (nodes: Iterable<Element>) =>
            [...nodes].map((node) =>
              Math.round(node.getBoundingClientRect().left),
            );
          return {
            head: left(
              document.querySelectorAll(
                "#statements-inspector .si-list-head span",
              ),
            ),
            row: left(
              document.querySelector("#statements-inspector .si-list-row")!
                .children,
            ),
          };
        });
        expect(lefts.head).toEqual(lefts.row);
        expect(new Set(lefts.head).size).toBe(6);
        await devtools.evaluate((node) => {
          (node as HTMLElement).style.width = "";
        });
        await page.setViewportSize({ width: 800, height: 940 });
        const listText = (
          await rows.evaluateAll((nodes) =>
            nodes.map((node) => (node as HTMLElement).innerText),
          )
        ).join("\n");
        for (const repeated of [
          "not remaining",
          "not current",
          "says it",
          "claimed now",
          "day ",
        ])
          expect({ repeated, found: listText.includes(repeated) }).toEqual({
            repeated,
            found: false,
          });
        const columnsAbout = page.locator(
          "#statements-inspector .si-columns-about",
        );
        expect(await columnsAbout.getAttribute("open")).toBeNull();
        await columnsAbout.locator("summary").click();
        expect(await columnsAbout.locator("li").allInnerTexts()).toEqual([
          "Role: what it is used for. Chat sender and Chat peer are what a product reported, unchecked. Product account: derived for a product, use unknown.",
          "Owner: This wallet = the core or this host derived it from the signed-in wallet, this session. Unknown here may still be this wallet's, from another origin, browser or session. Both start again with each session and on Clear.",
          "Allocation: the latest daily slot claim this tab saw.",
          "Grant: statements · size the chain grants, read at a block. Not what is left.",
          "Observed: statements · size · last seen in this tab. Not what the network holds now.",
        ]);
        await columnsAbout.locator("summary").click();
        // A role a product reported, unchecked, is marked so.
        expect(await rows.locator(".si-role").allInnerTexts()).toEqual([
          "Chat sender reported",
          "Chat peer reported",
          "Setup notification",
          "Observed signer",
        ]);
        // Whose each account is, only from what this tab derived: the inbox's
        // stranger stays unknown though the host received a request this session.
        expect(
          await rows.evaluateAll((nodes) =>
            nodes.map(
              (node) =>
                (node.querySelector("[data-owner]") as HTMLElement).innerText,
            ),
          ),
        ).toEqual([
          "This wallet",
          "Reported only",
          "This wallet",
          "Unknown here",
        ]);

        const shown = await page.locator("#statements-inspector").innerText();
        expect(shown).not.toMatch(/at least \d+ of|lower bound|\bof 2\b/i);
        // Statements names no protocol.
        expect(shown).not.toContain("simple-text");
        // One short takeaway, closed; the distinctions open under it.
        const caveats = page.locator("#statements-inspector .si-caveats");
        expect({
          summary: await caveats.locator("summary").innerText(),
          open: await caveats.getAttribute("open"),
        }).toEqual({
          summary: "Observed traffic, not network state · About this data",
          open: null,
        });
        await caveats.locator("summary").click();
        expect(await caveats.locator("li").allInnerTexts()).toEqual([
          "Source: this tab's light client. It holds no statements.",
          "new = broadcast to a peer, not stored. Peers' refusals never reach this tab.",
          "Observed ≠ current: a first page is always empty; later pages carry what peers send.",
          "Grant ≠ remaining: a grant is the allowance, not what is left.",
          "Statement bytes are public; encrypted contents stay sealed. Contact Chat text is plaintext.",
          "Source tags: light client, core, host, chain, reported (unchecked), derived, inspector.",
        ]);
        await caveats.locator("summary").click();
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

        // A topic link opens the topic's detail, not the timeline.
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
        await card.locator(".si-link", { hasText: "0x2222" }).click();
        const topic = page.locator("#statements-inspector .si-detail");
        await topic.waitFor();
        expect(
          await page
            .locator('#statements-inspector .si-tab[aria-selected="true"]')
            .innerText(),
        ).toBe("Topics");
        const topicFacts = await factsOf(
          page,
          "#statements-inspector .si-detail",
        );
        const about = (label: string) =>
          topicFacts.filter((entry) => entry.startsWith(`${label} |`));
        expect(about("Name")).toEqual([
          expect.stringContaining(
            "| reported | reported contact-chat.test · incoming",
          ),
        ]);
        expect(about("Listening")).toEqual([
          expect.stringContaining(
            "| light client | light client S1 · core · after its initial batch",
          ),
          expect.stringContaining(
            "| reported | reported contact-chat.test says it reads it",
          ),
        ]);
        expect(about("Publishing")).toEqual([
          expect.stringContaining(
            "| inspector | inspector no submission asked here",
          ),
        ]);
        expect(about("Observed")).toEqual([
          expect.stringMatching(
            /\| derived \| derived 1 stmt · last \d\d:\d\d:\d\d · not current/,
          ),
        ]);
        // Its statements name their signer, which links to the account.
        await topic.locator(".si-statement summary").first().click();
        expect(
          await topic.locator(".si-statement .si-link").first().innerText(),
        ).toContain("0xb2b2");
        // Subscription counters are nested in the detail, closed.
        expect(
          await topic
            .locator("details.si-columns-about > summary")
            .allInnerTexts(),
        ).toEqual(["Subscriptions (1)"]);
        await shot(page, "topic-detail");

        // The detail's Timeline action narrows the timeline to the topic, with no stale filter.
        await topic.locator(".si-chip", { hasText: "Timeline" }).click();
        await page.locator("#statements-inspector .si-focus").waitFor();
        expect(
          await page.locator("#statements-inspector .si-focus").innerText(),
        ).toContain("topic contact-chat.test · incoming");
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

        // Request timeline narrows it to the host's request events.
        await page.evaluate(() =>
          (window as unknown as ViewWindow).__showView("chat"),
        );
        await card.locator(".si-chip", { hasText: "Request timeline" }).click();
        await page.locator("#statements-inspector .si-focus").waitFor();
        expect(
          await page.locator("#statements-inspector .si-focus").innerText(),
        ).toContain("request 0x4444…4444");
        const requestEvents = await page
          .locator("#statements-inspector .si-event-text")
          .allInnerTexts();
        expect(requestEvents.length).toBeGreaterThan(0);
        for (const text of requestEvents) expect(text).toMatch(/0x4444…4444/);

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

        // From the account, its Chat link opens the Chat view on that
        // conversation's card, focused.
        await card
          .locator("dd", {
            has: page.locator(".si-src", { hasText: "reported" }),
          })
          .locator(".si-link", { hasText: "0xa1a1" })
          .first()
          .click();
        await page
          .locator("#statements-inspector .si-detail [data-chat-link]")
          .click();
        await page
          .locator('#chat-inspector .si-card[data-revealed="true"]')
          .waitFor();
        expect(
          await page.evaluate(() => ({
            chatShown: !document.getElementById("chat-inspector")!.hidden,
            focused: (document.activeElement as HTMLElement | null)?.dataset
              .key,
            asked: (window as unknown as ViewWindow).__shown.at(-1),
          })),
        ).toEqual({
          chatShown: true,
          focused: "conversation:contact-chat.test:0x" + "c0".repeat(16),
          asked: "chat:conversation:contact-chat.test:0x" + "c0".repeat(16),
        });
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );

  test(
    "Chat links each account it names, and each of its channels, to those statements in Statements, a channel scoped by its account",
    async () => {
      const { context, page } = await openView();
      try {
        const own = "0x" + "a1".repeat(32);
        const peer = "0x" + "b2".repeat(32);
        const channel = "0x" + "33".repeat(32);
        const card = page.locator(
          `#chat-inspector .si-card[data-key="conversation:contact-chat.test:0x${"c0".repeat(16)}"]`,
        );
        await card.locator("details.si-columns-about > summary").click();
        const facts = await factsOf(
          page,
          "#chat-inspector .si-card:first-of-type",
        );
        // The host's allowance is the reported account: one link for it,
        // one for the peer, each its latest observed per channel.
        expect(
          facts.filter((entry) => entry.startsWith("Statements |")),
        ).toEqual([
          expect.stringMatching(
            /^Statements \| derived \| derived own 1 stmt · 1 ch · \d\d:\d\d peer 1 stmt · 1 ch · \d\d:\d\d\s*why ‖ .*does not show whose they are/,
          ),
        ]);
        // Both post on the same channel id: under Details, one channel per
        // account, and only the product's own says it posts there.
        expect(facts.filter((entry) => entry.startsWith("Channel |"))).toEqual([
          expect.stringMatching(
            /^Channel \| derived \| derived own · 0x333333…3333 · latest \d\d:\d\d:\d\d · 1 replaced · posts here reported/,
          ),
          expect.stringMatching(
            /^Channel \| derived \| derived peer · 0x333333…3333 · latest \d\d:\d\d:\d\d · 0 replaced ‖ $/,
          ),
        ]);

        await shot(page, "chat-statements");
        const detail = page.locator("#statements-inspector .si-detail");
        const versions = () =>
          detail
            .locator(".si-statement")
            .evaluateAll((boxes) =>
              boxes.map((box) => (box as HTMLElement).dataset.version ?? ""),
            );
        const chip = detail.locator("[data-channel-filter]");
        // The own account's channel opens Statements on that account, its
        // statements narrowed to its channel, the last observed first: the
        // latest and the one it replaced, none of the peer's on the same
        // channel id.
        await card
          .locator(`[data-observed-channel="${own}:${channel}"]`)
          .click();
        await chip.waitFor();
        expect({
          asked: await page.evaluate(() =>
            (window as unknown as ViewWindow).__shown.at(-1),
          ),
          account: await detail.getAttribute("data-key"),
          picked: await detail
            .locator(`[data-channel="${channel}"]`)
            .getAttribute("aria-pressed"),
          row: await detail
            .locator(`[data-key="channel:${channel}"]`)
            .innerText(),
          heading: await detail.locator(".si-sub").last().innerText(),
          versions: await versions(),
        }).toEqual({
          asked: "statements:" + JSON.stringify({ account: own, channel }),
          account: `account:${own}`,
          picked: "true",
          row: expect.stringContaining("contact-chat.test posts here reported"),
          heading: "Statements on this channel (last 20)",
          versions: ["latest", "replaced"],
        });
        expect(
          await detail.locator(".si-statement > summary").allInnerTexts(),
        ).toEqual([
          expect.stringMatching(
            /^latest · 0x[0-9a-f]{6}…[0-9a-f]{4} · \d+ B · last \d\d:\d\d:\d\d · broadcast · observed once$/,
          ),
          expect.stringMatching(/^replaced · .* · observed once$/),
        ]);
        await shot(page, "account-channel");

        // The chip shows every channel again.
        await chip.click();
        await detail
          .locator(".si-sub", { hasText: "Statements observed (last 20)" })
          .waitFor();
        expect([await chip.count(), await versions()]).toEqual([0, ["", ""]]);

        // Picking the channel in the list narrows it again; choosing the
        // account row anew, or opening the account from Chat, does not keep it.
        await detail.locator(`[data-channel="${channel}"]`).click();
        await chip.waitFor();
        const row = page.locator(
          '#statements-inspector .si-list-row[aria-pressed="true"]',
        );
        await row.click();
        await row.waitFor({ state: "detached" });
        await page
          .locator("#statements-inspector .si-list-row", { hasText: "0xa1a1" })
          .click();
        await detail.waitFor();
        expect(await chip.count()).toBe(0);
        await detail.locator(`[data-channel="${channel}"]`).click();
        await chip.waitFor();
        await page.evaluate(() =>
          (window as unknown as ViewWindow).__showView("chat"),
        );
        await card.locator(`[data-observed-account="${own}"]`).click();
        await detail
          .locator(".si-sub", { hasText: "Statements observed (last 20)" })
          .waitFor();
        expect(await chip.count()).toBe(0);

        // The peer's channel is the peer's: its one statement, its latest.
        await page.evaluate(() =>
          (window as unknown as ViewWindow).__showView("chat"),
        );
        await card
          .locator(`[data-observed-channel="${peer}:${channel}"]`)
          .click();
        await page
          .locator(
            `#statements-inspector .si-detail[data-key="account:${peer}"]`,
          )
          .locator("[data-channel-filter]")
          .waitFor();
        expect(await versions()).toEqual(["latest"]);

        // A channel this account never posted on, whoever else did, or an
        // account this tab never saw, shows nothing, and says so.
        const other = "0x" + "99".repeat(32);
        await page.evaluate(
          ([account, channel]) =>
            (window as unknown as ViewWindow).__reveal({ account, channel }),
          [other, channel],
        );
        await page
          .locator(
            `#statements-inspector .si-detail[data-key="account:${other}"]`,
          )
          .waitFor();
        expect(
          await factsOf(page, "#statements-inspector .si-detail"),
        ).toContainEqual(
          expect.stringContaining(
            `Channel | inspector | inspector ${channel} · nothing observed`,
          ),
        );
        await page.evaluate(
          ([account, channel]) =>
            (window as unknown as ViewWindow).__reveal({ account, channel }),
          [peer, "0x" + "44".repeat(32)],
        );
        await detail.locator(".si-channel-missing").waitFor();
        expect(await detail.locator(".si-statement").count()).toBe(0);

        // A chat whose accounts were never observed says so, and its own
        // reported channel shows as reported only: another account's
        // statements on the same channel id are not its.
        await page.evaluate(
          ([account, peer, channel]) => {
            const view = window as unknown as ViewWindow;
            view.__ledger.diagnostics(
              { name: "unseen.test", run: 1 },
              {
                conversations: [
                  {
                    id: "u1",
                    protocol: "unseen/1",
                    account,
                    peers: [peer],
                    post: { topics: [], channel },
                  },
                ],
              },
            );
            view.__showView("chat");
          },
          ["0x" + "f1".repeat(32), "0x" + "f2".repeat(32), channel],
        );
        const unseen = page.locator(
          '#chat-inspector .si-card[data-key="conversation:unseen.test:u1"]',
        );
        await unseen.waitFor();
        await unseen.locator("details.si-columns-about > summary").click();
        expect(
          (
            await factsOf(
              page,
              '#chat-inspector .si-card[data-key="conversation:unseen.test:u1"]',
            )
          ).filter((entry) => /^(Statements|Channel) \|/.test(entry)),
        ).toEqual([
          expect.stringContaining(
            "Statements | derived | derived own not observed peer not observed",
          ),
          expect.stringContaining(
            "Channel | reported | reported own · 0x333333…3333 · not observed",
          ),
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
        // Its account still links to what this tab saw of that account: navigation, not a tie.
        await borrower.locator(".si-link", { hasText: "0xa1a1" }).click();
        await page.locator("#statements-inspector .si-detail").waitFor();
        expect(
          await page
            .locator("#statements-inspector .si-detail dd", {
              hasText: "other.test says it posts as this account",
            })
            .locator(".si-src")
            .innerText(),
        ).toBe("reported");
        // An account the host allowed, with no use reported, is a product
        // account: its allowance shows under Allocation, and no use is invented.
        const allowed = "0x" + "f6".repeat(32);
        await page.evaluate(
          (account) =>
            (
              window as unknown as {
                __ledger: { invitations(event: object): void };
              }
            ).__ledger.invitations({
              type: "chat-account",
              product: "other.test",
              handle: "0x" + "66".repeat(16),
              index: "0x" + "02".repeat(32),
              outcome: "reused",
              account,
            }),
          allowed,
        );
        await page.evaluate(
          (account) => (window as unknown as ViewWindow).__reveal({ account }),
          allowed,
        );
        const allowedRow = page.locator(
          '#statements-inspector .si-list-row[aria-pressed="true"]',
        );
        await allowedRow.waitFor();
        expect(await allowedRow.locator(".si-cell").allTextContents()).toEqual([
          "Product account",
          "This wallet",
          "Chat allowance",
          "Not read",
          "Not observed",
        ]);
        // An account no one recorded opens empty, and nothing is read for it.
        const unknown = "0x" + "e5".repeat(32);
        await page.evaluate(
          (account) => (window as unknown as ViewWindow).__reveal({ account }),
          unknown,
        );
        const missing = page.locator("#statements-inspector .si-detail", {
          hasText: unknown,
        });
        await missing.waitFor();
        expect(await missing.innerText()).toContain("nothing in this session");
        expect(
          await page.evaluate(
            () =>
              (window as unknown as { __peopleConnections: string[][] })
                .__peopleConnections,
          ),
        ).toEqual([]);
        await shot(page, "unknown-account");
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );

  test(
    "Statements lists topics by role with no probe, nests subscriptions, captures only when asked, and copies metadata only",
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
        const rows = page.locator(
          "#statements-inspector .si-topics .si-list-row",
        );
        await rows.first().waitFor();
        // One row per topic: a name only where it is known, the role from its
        // own sources, who holds a subscription open, and what was observed.
        const table = await rows.evaluateAll((nodes) =>
          nodes.map((node) =>
            [...node.children].map((cell) =>
              (cell as HTMLElement).innerText.replace(/\s+/g, " ").trim(),
            ),
          ),
        );
        expect(table).toEqual([
          [
            expect.stringMatching(
              /^Shared setup inbox · today UTC 0x\w{4}…\w{4}$/,
            ),
            "Both",
            "host · open",
            expect.stringMatching(/^2 · \d\d:\d\d$/),
          ],
          [
            expect.stringMatching(
              /^contact-chat\.test · incoming 0x2222…2222 reported$/,
            ),
            "Listening",
            "core · open",
            expect.stringMatching(/^1 · \d\d:\d\d$/),
          ],
          [
            expect.stringMatching(
              /^Shared setup inbox · yesterday UTC 0x\w{4}…\w{4}$/,
            ),
            "Listening",
            "host · open",
            "None seen",
          ],
          [
            expect.stringMatching(
              /^contact-chat\.test · outgoing 0x1111…1111 reported$/,
            ),
            "Publishing",
            "None",
            expect.stringMatching(/^2 · \d\d:\d\d$/),
          ],
        ]);
        expect(
          await page
            .locator("#statements-inspector .si-topics .si-list-head span")
            .allTextContents(),
        ).toEqual(["Topic", "Role", "Subscription", "Observed"]);
        // On a wide dock the headings sit over their columns.
        await page.setViewportSize({ width: 1240, height: 940 });
        await page.locator(".devtools").evaluate((node) => {
          (node as HTMLElement).style.width = "1200px";
        });
        const topicLefts = await page.evaluate(() => {
          const left = (nodes: Iterable<Element>) =>
            [...nodes].map((node) =>
              Math.round(node.getBoundingClientRect().left),
            );
          return {
            head: left(
              document.querySelectorAll(
                "#statements-inspector .si-topics .si-list-head span",
              ),
            ),
            row: left(
              document.querySelector(
                "#statements-inspector .si-topics .si-list-row",
              )!.children,
            ),
          };
        });
        expect(topicLefts.head).toEqual(topicLefts.row);
        expect(new Set(topicLefts.head).size).toBe(4);
        await shot(page, "topics-wide");
        await page.locator(".devtools").evaluate((node) => {
          (node as HTMLElement).style.width = "";
        });
        await page.setViewportSize({ width: 800, height: 940 });
        expect(
          await page
            .locator("#statements-inspector button", { hasText: /probe/i })
            .count(),
        ).toBe(0);
        expect(await page.locator(".si-no-probe").innerText()).toContain(
          "a subscription listens on one or more topics. The inspector opens none of its own.",
        );
        // Subscriptions, with their counters, are one closed list under the topics.
        const all = page.locator("#statements-inspector .si-subscriptions");
        expect(await all.getAttribute("open")).toBeNull();
        expect(await all.locator(":scope > summary").innerText()).toBe(
          "All subscriptions (2)",
        );
        await all.locator(":scope > summary").click();
        expect(await all.locator(".si-sub-row").allInnerTexts()).toEqual([
          expect.stringContaining("host · open · 0 initial · 1 later"),
          expect.stringContaining("core · open · 0 initial · 1 later"),
        ]);
        await all.locator(":scope > summary").click();
        // A row opens its detail from the keyboard too.
        await rows.first().focus();
        await page.keyboard.press("Enter");
        const inbox = page.locator("#statements-inspector .si-detail");
        await inbox.waitFor();
        const inboxFacts = await factsOf(
          page,
          "#statements-inspector .si-detail",
        );
        expect(
          inboxFacts.filter((entry) =>
            /^(Name|Listening|Publishing) \|/.test(entry),
          ),
        ).toEqual([
          expect.stringMatching(
            /^Name \| derived \| derived Shared setup inbox · today UTC ?why ‖ The draft delivery's day inbox on testnet for UTC day \d+, worked out here from the network and the day\. Every host using this draft delivery /,
          ),
          expect.stringContaining("| light client | light client S2 · host ·"),
          expect.stringContaining(
            "| light client | light client 1 broadcast · 1 tried",
          ),
        ]);
        // Each statement says whose its signer is: this wallet's own post, and
        // another host's, which stays unknown here.
        expect(
          (await inbox.locator(".si-statement > summary").allInnerTexts()).map(
            (text) => /signer [a-z ]+? ·/.exec(text)?.[0],
          ),
        ).toEqual(["signer unknown here ·", "signer this wallet ·"]);
        await inbox.locator(".si-statement > summary").first().click();
        expect(
          await inbox
            .locator(".si-statement")
            .first()
            .locator("dd", { hasText: "Unknown here" })
            .innerText(),
        ).toContain("Unknown here");
        await shot(page, "topic-inbox-owners");
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
          "truapi-web-host/statements-evidence/3",
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
    "a submission this tab's no-submit guard refused counts as tried and refused here, never as the light client's",
    async () => {
      const { context, page } = await openView();
      try {
        await page.evaluate(() =>
          (window as unknown as ViewWindow).__refuseHere(),
        );
        await page.evaluate(() =>
          (window as unknown as ViewWindow).__showView("statements"),
        );
        await page
          .locator("#statements-inspector .si-list-row")
          .first()
          .click();
        await page.locator("#statements-inspector .si-detail").waitFor();
        const account = await factsOf(page, "#statements-inspector .si-detail");
        const at = (label: string) =>
          account.find((entry) => entry.startsWith(`${label} |`)) ?? "";
        expect(at("Posts")).toContain("| inspector | inspector 4 tried");
        expect(at("Answers")).toContain(
          "| light client | light client 2 broadcast · 1 RPC error",
        );
        expect(at("Refused here")).toContain(
          "| host | host 1 by the no-submit guard",
        );
        expect(at("Refused here")).toContain("no peer got them");
        expect(
          await page.locator("#statements-inspector .si-summary").innerText(),
        ).toContain("5 submits · 3 answered · 1 errors · 1 refused here");
        await shot(page, "refused-here-account");

        await page.locator(".si-tab", { hasText: "Timeline" }).click();
        await page.locator(".si-chip", { hasText: "Submits" }).click();
        const events = await page
          .locator("#statements-inspector .si-event")
          .evaluateAll((rows) =>
            rows.map(
              (row) =>
                `${row.querySelector(".si-src")?.textContent} | ${row.querySelector(".si-event-text")?.textContent}`,
            ),
          );
        expect(events.slice(-2)).toEqual([
          expect.stringMatching(/^core \| Submit #5 asked by the core: /),
          "host | Submit #5 refused in this tab: it submits nothing",
        ]);
        await shot(page, "refused-here-timeline");

        // At a phone's width an event's text wraps; its source tag stays one line.
        await page
          .locator(".devtools")
          .evaluate((node) => ((node as HTMLElement).style.width = "390px"));
        const heights = await page
          .locator("#statements-inspector .si-event")
          .evaluateAll((rows) =>
            rows.map((row) => [
              row.querySelector(".si-event-text")!.getBoundingClientRect()
                .height,
              row.querySelector(".si-src")!.getBoundingClientRect().height,
            ]),
          );
        const tallest = Math.max(...heights.map(([text]) => text!));
        const tagLine = Math.min(...heights.map(([, tag]) => tag!));
        expect(tallest).toBeGreaterThan(tagLine * 2);
        for (const [, tag] of heights) expect(tag).toBe(tagLine);
        await shot(page, "refused-here-timeline-narrow");
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );

  test(
    "on a phone-width dock the inspector's note sits on its own line under the view tabs",
    async () => {
      const context = await browser.newContext({
        viewport: { width: 390, height: 844 },
      });
      context.setDefaultTimeout(STEP_MS);
      await context.route("**/*", (route) =>
        new URL(route.request().url()).hostname === "localhost"
          ? route.continue()
          : route.abort(),
      );
      const page = await context.newPage();
      try {
        await page.goto(host.origin);
        await openTools(page, "inspector");
        for (const view of ["statements", "chat", "activity"] as const) {
          await page.click(`[data-inspector-view="${view}"]`);
          const [head, tabs, note] = await Promise.all(
            [
              ".inspector .panel-head",
              ".inspector .inspector-views",
              "#inspector-note",
            ].map(
              async (selector) => (await page.locator(selector).boundingBox())!,
            ),
          );
          expect(note.y).toBeGreaterThanOrEqual(tabs.y + tabs.height - 1);
          expect(note.width).toBeGreaterThan(head.width * 0.8);
          expect(note.x + note.width).toBeLessThanOrEqual(390);
          expect(tabs.x + tabs.width).toBeLessThanOrEqual(390);
        }
        const dir = process.env.STATEMENTS_SCREENSHOT;
        if (dir !== undefined && dir !== "") {
          await page.click('[data-inspector-view="statements"]');
          await page.screenshot({ path: `${dir}/host-narrow-statements.png` });
          await page.click('[data-inspector-view="chat"]');
          await page.screenshot({ path: `${dir}/host-narrow-chat.png` });
        }
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
        // A protocol that reports no details has no Protocol details, and no window.
        expect(await card.innerText()).not.toMatch(/Protocol details|Window/);
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
