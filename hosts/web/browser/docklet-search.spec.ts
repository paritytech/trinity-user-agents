/**
 * Headless-browser test of the Docklet's search field, which finds a person
 * or an app. A synthetic wallet signs in with a saved chat record that holds
 * one contact; every socket is closed, the username index is a synthetic
 * answer on the dev server's own route, and every other request outside this
 * host and a loopback product is refused, so no chain, backend or peer is
 * reached and nothing is sent. It needs a Chromium: playwright-core finds the
 * one it installs, or set CHROMIUM_PATH.
 */
import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { join } from "node:path";
import type { Browser, BrowserContext, Page } from "playwright-core";
import {
  TEST_MNEMONIC,
  type RunningHost,
  closeTools,
  openTools,
  showAddress,
  startHost,
  startUrlProduct,
  testBrowser,
} from "./support/fixtures.js";

const STEP_MS = 60_000;
const TEST_MS = 180_000;
/** A BIP-39 test vector phrase. It holds nothing. */
const PHRASE =
  "letter advice cage absurd amount doctor acoustic avoid letter advice cage above";
const WALLET_ID = "docklet-search-wallet";
const KIM = `0x${"4b".repeat(32)}`;
/** The public development account Alice, listed in the synthetic index as zed.01. */
const ZED_SS58 = "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY";

const RECORD = {
  contacts: [
    {
      account: KIM,
      chatKey: `0x${"cc".repeat(32)}`,
      name: "kim.01",
      state: "active",
      messages: [
        {
          id: "kim-1",
          direction: "in",
          kind: "text",
          text: "hi",
          at: 1,
          status: "received",
        },
      ],
      outbound: null,
      expiry: { request: null, response: null },
      acknowledged: null,
    },
  ],
  requests: [],
  declined: [],
};

let browser: Browser;
let host: RunningHost;

beforeAll(async () => {
  host = await startHost();
  browser = await testBrowser();
}, STEP_MS);

afterAll(async () => {
  await host?.close();
});

async function open(
  viewport = { width: 390, height: 844 },
  allowed: string[] = [],
  phrase = PHRASE,
): Promise<{ context: BrowserContext; page: Page; searched: string[] }> {
  const context = await browser.newContext({ viewport });
  context.setDefaultTimeout(STEP_MS);
  await context.routeWebSocket(/.*/, (socket) => socket.close());
  const searched: string[] = [];
  await context.route(/.*/, (route) => {
    const url = new URL(route.request().url());
    if (url.pathname.endsWith("/__username-search")) {
      const prefix = url.searchParams.get("prefix") ?? "";
      searched.push(prefix);
      const usernames = [{ accountId: ZED_SS58, username: "zed.01" }].filter(
        (user) => user.username.startsWith(prefix),
      );
      return route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify({ usernames, nextCursor: null }),
      });
    }
    if (
      url.origin === host.origin ||
      allowed.some((origin) => url.href.startsWith(origin))
    )
      return route.fallback();
    return route.abort();
  });
  const page = await context.newPage();
  await page.goto(host.origin);
  await page.evaluate(
    ({ id, mnemonic, record }) => {
      localStorage.setItem(
        "truapi-web-signing-host:wallets:v1",
        JSON.stringify([{ id, name: "Search fixture", mnemonic }]),
      );
      localStorage.setItem(
        `truapi-web-signing-host:network:paseo:chat:${id}:v1`,
        JSON.stringify(record),
      );
    },
    { id: WALLET_ID, mnemonic: phrase, record: RECORD },
  );
  await page.reload();
  await page
    .locator("#core-status")
    .filter({ hasText: "Core ready" })
    .waitFor({ state: "attached" });
  // Importing the saved phrase signs in as the saved wallet, with its record.
  await openTools(page);
  await page
    .locator("#import")
    .evaluate((details) => ((details as HTMLDetailsElement).open = true));
  await page.fill("#mnemonic", phrase);
  await page.click("#add-wallet");
  await page.locator("#session").filter({ hasNotText: "Signed out" }).waitFor();
  await closeTools(page);
  return { context, page, searched };
}

/** The suggestion rows, by group, as the list shows them. */
const suggestions = (page: Page) =>
  page
    .locator("#address-recents li")
    .evaluateAll((items) =>
      items.map((item) =>
        item.getAttribute("role") === "option"
          ? `  ${item.querySelector(".recent-name")?.textContent ?? ""}${item.querySelector(".recent-id") ? ` (${item.querySelector(".recent-id")!.textContent})` : ""}`
          : (item.textContent ?? ""),
      ),
    );

async function type(page: Page, text: string): Promise<void> {
  await showAddress(page);
  await page.fill("#address", text);
}

async function screenshot(page: Page, name: string): Promise<void> {
  const directory = process.env.SEARCH_SCREENSHOTS;
  if (directory)
    await page.screenshot({ path: join(directory, `${name}.png`) });
}

describe("the Docklet's search", () => {
  test(
    "lists apps and people apart, picks no one while typing, and a contact opens their chat",
    async () => {
      const { context, page, searched } = await open();
      try {
        // From Pocket, so Chat coming to the front is seen.
        await page.click('#dock [data-destination="pocket"]');
        await showAddress(page);
        expect(await page.getAttribute("#address", "placeholder")).toBe(
          "Find a person or app",
        );
        expect(await page.locator(".search-note").count()).toBe(0);

        // A name could be either: what Enter opens is an app, people stand apart.
        await type(page, "zed");
        await page
          .locator("#address-recents li", { hasText: "zed.01" })
          .waitFor();
        expect(await suggestions(page)).toEqual([
          "Apps",
          "  Open zed.paseo",
          "People",
          "  zed.01 (Not in your contacts)",
        ]);
        expect(searched).toEqual(["zed"]);
        await screenshot(page, "search-zed");
        // Typing opened nothing and showed no one.
        expect(await page.getAttribute("body", "data-product")).toBe("closed");
        expect(await page.locator("#chat-screen").isHidden()).toBe(true);

        // The list stays inside the app, above the field.
        const list = await page.locator("#address-recents").boundingBox();
        const app = await page.locator("#product-frame").boundingBox();
        const field = await page.locator("#address").boundingBox();
        expect(list!.y).toBeGreaterThanOrEqual(app!.y);
        expect(list!.y + list!.height).toBeLessThanOrEqual(field!.y);

        // Found by username: checked on chain before anything shows. Offline
        // the check cannot pass, so nothing is chosen; typing lets it go.
        await page
          .locator("#address-recents li", { hasText: "zed.01" })
          .click();
        await page
          .locator("#address-recents .recent-status", {
            hasText: /zed\.01/,
          })
          .waitFor();
        await page.waitForTimeout(300);
        expect(await page.locator("#chat-screen").isHidden()).toBe(true);
        expect(await page.locator("#search-card").isVisible()).toBe(true);

        // A contact, by name: chosen with the keyboard, their chat opens.
        await type(page, "kim");
        await page
          .locator("#address-recents li", { hasText: "kim.01" })
          .waitFor();
        expect(await suggestions(page)).toContain("  kim.01");
        await page.keyboard.press("ArrowDown");
        await page.keyboard.press("ArrowDown");
        expect(
          await page
            .locator('#address-recents [aria-selected="true"] .recent-name')
            .textContent(),
        ).toBe("kim.01");
        await page.keyboard.press("Enter");
        await page.locator("#search-card").waitFor({ state: "hidden" });
        await page.locator('#chat-screen [data-focus="composer"]').waitFor();
        expect(await page.locator("#chat-screen").isVisible()).toBe(true);
        expect(
          await page.evaluate(
            () => (document.activeElement as HTMLElement | null)?.dataset.focus,
          ),
        ).toBe("composer");
        expect(await page.getAttribute("body", "data-product")).toBe("closed");
        await screenshot(page, "kim-chat");

        // Escape closes the card and gives focus back to the Pill.
        await type(page, "kim");
        await page.keyboard.press("Escape");
        await page.keyboard.press("Escape");
        await page.locator("#search-card").waitFor({ state: "hidden" });
        expect(await page.evaluate(() => document.activeElement?.id)).toBe(
          "pill",
        );
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );

  // Who exists does not depend on whether Chat runs: with the public
  // development phrase, whose chat stays off, people are still found, and
  // picking one says why Chat cannot show them instead of opening anything.
  test(
    "while Chat cannot start a conversation, people are found and picking one says why",
    async () => {
      const { context, page, searched } = await open(
        undefined,
        [],
        TEST_MNEMONIC,
      );
      try {
        // From Pocket, so Chat coming to the front is seen.
        await page.click('#dock [data-destination="pocket"]');
        await type(page, "zed");
        await page
          .locator("#address-recents li", { hasText: "zed.01" })
          .waitFor();
        expect(await suggestions(page)).toEqual([
          "Apps",
          "  Open zed.paseo",
          "People",
          "  zed.01 (Not in your contacts)",
        ]);
        expect(searched).toEqual(["zed"]);
        await page
          .locator("#address-recents li", { hasText: "zed.01" })
          .click();
        await page
          .locator("#address-recents .recent-status", {
            hasText:
              "Chat cannot start a conversation: Not available for the public development phrase.",
          })
          .waitFor();
        expect(await page.locator("#search-card").isVisible()).toBe(true);
        expect(await page.locator("#chat-screen").isHidden()).toBe(true);
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );

  test(
    "a URL is an app: no people are looked up, and Enter opens it",
    async () => {
      const product = await startUrlProduct(host.origin);
      const { context, page, searched } = await open(undefined, [product.url]);
      try {
        await type(page, product.url);
        await page.waitForTimeout(400);
        expect(
          await page.locator("#address-recents .recent-heading").count(),
        ).toBe(0);
        expect(searched).toEqual([]);
        await page.press("#address", "Enter");
        await page.locator("body[data-product='open']").waitFor();
      } finally {
        await context.close();
        product.close();
      }
    },
    TEST_MS,
  );

  test(
    "after sign-out no people are offered",
    async () => {
      const { context, page } = await open();
      try {
        await type(page, "zed");
        await page
          .locator("#address-recents li", { hasText: "zed.01" })
          .waitFor();
        await openTools(page);
        await page.click("#sign-out");
        await page.locator("#session", { hasText: "Signed out" }).waitFor();
        await closeTools(page);
        await type(page, "zed");
        await page.waitForTimeout(400);
        expect(
          await page.locator("#address-recents .recent-heading").count(),
        ).toBe(0);
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );
});
