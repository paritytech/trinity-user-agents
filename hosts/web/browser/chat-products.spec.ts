/**
 * The developer tools' Chat tab on the dev server: which chat products a tab
 * runs and where New chat starts. How host requests travel stays in More. No wallet signs
 * in, every socket is closed and every request outside this host is refused,
 * so no worker starts and nothing reaches a chain. Kettle Chat's WebAssembly
 * is not needed: a product that is on but not signed in does not load it.
 */
import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import type { Browser, BrowserContext, Page } from "playwright-core";
import {
  type RunningHost,
  openTools,
  startHost,
  testBrowser,
  useChatProducts,
} from "./support/fixtures.js";

const STEP_MS = 60_000;

let browser: Browser;
let host: RunningHost;

beforeAll(async () => {
  host = await startHost();
  browser = await testBrowser();
}, STEP_MS);

afterAll(async () => {
  await host?.close();
}, STEP_MS);

const ready = (page: Page) =>
  page
    .locator("#core-status")
    .filter({ hasText: "Core ready" })
    .waitFor({ state: "attached" });

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

/** What the Chat tab shows: each switch, each state line, and New chat's choices. */
async function chatTab(page: Page) {
  await openTools(page, "chat");
  const select = page.locator("#new-chat-product");
  return {
    contact: await page.isChecked("#chat-product-contact-chat"),
    kettle: await page.isChecked("#chat-product-kettle-chat"),
    states: await page.locator("[data-chat-product-state]").allInnerTexts(),
    newChats: {
      options: await select.locator("option").allInnerTexts(),
      value: await select.inputValue(),
      disabled: await select.isDisabled(),
    },
  };
}

const NOT_RUNNING =
  "On · not running yet: it runs for a signed-in wallet or demo user";

describe("the developer tools' Chat tab", () => {
  test("runs Contact Chat alone by default, and each choice of products and of New chat's product holds across reloads", async () => {
    const { context, outside, errors, open } = await offline();
    try {
      const page = await open();
      expect(await chatTab(page)).toEqual({
        contact: true,
        kettle: false,
        states: [NOT_RUNNING, "Off · nothing runs"],
        newChats: {
          options: ["Contact Chat"],
          value: "contact-chat",
          disabled: true,
        },
      });
      // How host requests travel is the request service's, which every
      // product uses: it stays in More, not in Chat.
      expect([
        await page.locator("#dev-chat #wallet-delivery").count(),
        await page.locator("#dev-chat #reply-key-suite").count(),
        await page.locator("#dev-more #wallet-delivery").count(),
        await page.locator("#dev-more #reply-key-suite").count(),
        await page
          .locator("#wallet-delivery option", { hasText: "Off" })
          .count(),
      ]).toEqual([0, 0, 1, 1, 0]);

      await useChatProducts(page, { contact: false, kettle: true });
      await ready(page);
      expect(await chatTab(page)).toEqual({
        contact: false,
        kettle: true,
        states: ["Off · nothing runs", NOT_RUNNING],
        newChats: {
          options: ["Kettle Chat"],
          value: "kettle-chat",
          disabled: true,
        },
      });

      await useChatProducts(page, { contact: true, kettle: true });
      await ready(page);
      expect((await chatTab(page)).newChats).toEqual({
        options: ["Contact Chat", "Kettle Chat"],
        value: "contact-chat",
        disabled: false,
      });
      await useChatProducts(
        page,
        { contact: true, kettle: true },
        "kettle-chat",
      );
      await ready(page);
      await page.reload();
      await ready(page);
      expect((await chatTab(page)).newChats).toEqual({
        options: ["Kettle Chat", "Contact Chat"],
        value: "kettle-chat",
        disabled: false,
      });

      // Neither on: New chat has no product, and no worker frame starts.
      await useChatProducts(page, { contact: false, kettle: false });
      await ready(page);
      expect((await chatTab(page)).newChats).toEqual({
        options: ["No chat product is on"],
        value: "",
        disabled: true,
      });
      expect(
        page
          .frames()
          .filter((frame) => frame.url().includes("__truapi-worker.html")),
      ).toEqual([]);
      expect(outside).toEqual([]);
      expect(errors).toEqual([]);
    } finally {
      await context.close();
    }
  });

  test("a tab whose delivery was Off runs no chat product, and its delivery is the default again", async () => {
    const { context, errors, open } = await offline();
    try {
      await context.addInitScript(() => {
        if (sessionStorage.getItem("moved") !== null) return;
        sessionStorage.setItem("moved", "1");
        sessionStorage.setItem(
          "truapi-web-signing-host:dev:wallet-delivery",
          "off",
        );
      });
      const page = await open();
      const tab = await chatTab(page);
      await openTools(page, "more");
      expect({
        contact: tab.contact,
        kettle: tab.kettle,
        delivery: await page.inputValue("#wallet-delivery"),
        stored: await page.evaluate(() =>
          sessionStorage.getItem("truapi-web-signing-host:dev:wallet-delivery"),
        ),
      }).toEqual({
        contact: false,
        kettle: false,
        delivery: "network",
        stored: null,
      });
      expect(errors).toEqual([]);
    } finally {
      await context.close();
    }
  });
});
