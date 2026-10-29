/**
 * Headless-browser test of the host with an on-screen keyboard open. A
 * browser cannot open a real keyboard, so each test gives the page a visual
 * viewport it controls, as iOS Safari reports one: the layout viewport keeps
 * its height and only the visible area shrinks. No wallet signs in, every
 * socket is closed and every request outside this host is refused. It needs a
 * Chromium: playwright-core finds the one it installs, or set CHROMIUM_PATH.
 */
import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { join } from "node:path";
import type { Browser, BrowserContext, Page } from "playwright-core";
import {
  type RunningHost,
  closeTools,
  openTools,
  showAddress,
  startHost,
  testBrowser,
} from "./support/fixtures.js";

const STEP_MS = 60_000;
const TEST_MS = 180_000;

let browser: Browser;
let host: RunningHost;

beforeAll(async () => {
  host = await startHost();
  browser = await testBrowser();
}, STEP_MS);

afterAll(async () => {
  await host?.close();
});

interface FakeVisual {
  height: number;
  offsetTop: number;
  scale: number;
}

async function open(viewport: { width: number; height: number }): Promise<{
  context: BrowserContext;
  page: Page;
  outside: string[];
}> {
  const context = await browser.newContext({ viewport });
  context.setDefaultTimeout(STEP_MS);
  await context.routeWebSocket(/.*/, (socket) => socket.close());
  const outside: string[] = [];
  await context.route(/.*/, (route) => {
    const url = route.request().url();
    if (url.startsWith(host.origin)) return route.fallback();
    outside.push(url);
    return route.abort();
  });
  // Products opened before, so the search card has suggestions to show.
  await context.addInitScript(() => {
    if (localStorage.getItem("truapi-web-signing-host:recents:v1:signed-out"))
      return;
    localStorage.setItem(
      "truapi-web-signing-host:recents:v1:signed-out",
      JSON.stringify(
        ["zed", "zebra", "zinc", "zoo", "zulu"].map((name, index) => ({
          address: `${name}.paseo`,
          productId: `${name}.paseo`,
          entered: false,
          at: Date.now() - index * 60_000,
        })),
      ),
    );
  });
  await context.addInitScript(() => {
    class Visual extends EventTarget {
      height = window.innerHeight;
      offsetTop = 0;
      scale = 1;
    }
    Object.defineProperty(window, "visualViewport", {
      value: new Visual(),
      configurable: true,
    });
  });
  const page = await context.newPage();
  await page.goto(host.origin);
  await page
    .locator("#core-status")
    .filter({ hasText: "Core ready" })
    .waitFor({ state: "attached" });
  return { context, page, outside };
}

/** What the browser reports after a keyboard opens, or closes with `null`. */
const setKeyboard = (
  page: Page,
  area: { height: number; offsetTop?: number; scale?: number } | null,
) =>
  page.evaluate((next) => {
    const visual = window.visualViewport as unknown as FakeVisual & EventTarget;
    visual.height = next?.height ?? window.innerHeight;
    visual.offsetTop = next?.offsetTop ?? 0;
    visual.scale = next?.scale ?? 1;
    visual.dispatchEvent(new Event("resize"));
  }, area);

const box = async (page: Page, selector: string) => {
  const found = await page.locator(selector).boundingBox();
  if (found === null) throw new Error(`${selector} is not drawn`);
  return found;
};

const bottom = async (page: Page, selector: string) => {
  const found = await box(page, selector);
  return found.y + found.height;
};

async function screenshot(page: Page, name: string): Promise<void> {
  const directory = process.env.KEYBOARD_SCREENSHOTS;
  if (directory)
    await page.screenshot({ path: join(directory, `${name}.png`) });
}

describe("the host with an on-screen keyboard", () => {
  test(
    "keeps the Docklet, the search field and its suggestions above a keyboard that shrank only the visible area",
    async () => {
      const { context, page, outside } = await open({
        width: 390,
        height: 844,
      });
      try {
        await showAddress(page);
        await page.fill("#address", "z");
        await page.locator("#address-recents li").first().waitFor();
        // Before the keyboard the card and the Docklet sit at the bottom of the layout.
        expect(await bottom(page, "#dock")).toBeGreaterThan(520);
        await screenshot(page, "before-keyboard");

        await setKeyboard(page, { height: 520 });
        const height = () =>
          page.evaluate(() => document.body.getBoundingClientRect().height);
        await page.waitForFunction(
          () => document.body.getBoundingClientRect().height === 520,
        );
        expect(await height()).toBe(520);
        // Everything the user types into or reads stays inside what shows.
        for (const selector of [
          "#dock",
          "#search-card",
          "#address",
          "#address-recents",
        ])
          expect(await bottom(page, selector)).toBeLessThanOrEqual(520);
        expect((await box(page, "#address-recents")).y).toBeGreaterThanOrEqual(
          0,
        );
        expect(
          await page.evaluate(
            () => document.documentElement.scrollWidth - window.innerWidth,
          ),
        ).toBeLessThanOrEqual(0);
        await screenshot(page, "keyboard-open");

        // Closing the card and the keyboard puts everything back.
        await page.keyboard.press("Escape");
        await setKeyboard(page, null);
        await page.waitForFunction(
          () => document.body.getBoundingClientRect().height === innerHeight,
        );
        expect(await bottom(page, "#dock")).toBeGreaterThan(520);
        expect(
          await page.evaluate(() =>
            document.documentElement.style.getPropertyValue("--app-height"),
          ),
        ).toBe("");
        expect(outside).toEqual([]);
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );

  test(
    "follows the area the browser panned to, and leaves a zoomed page alone",
    async () => {
      const { context, page } = await open({ width: 390, height: 844 });
      try {
        await setKeyboard(page, { height: 520, offsetTop: 60 });
        await page.waitForFunction(
          () => document.body.getBoundingClientRect().top === 60,
        );
        expect((await box(page, "body")).height).toBe(520);

        // A pinch zoom shrinks the visible area too; it is not a keyboard.
        await setKeyboard(page, { height: 422, scale: 2 });
        await page.waitForFunction(
          () => document.body.getBoundingClientRect().top === 0,
        );
        expect((await box(page, "body")).height).toBe(844);
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );

  // A browser that resizes the layout for the keyboard reports a visible area
  // as tall as the layout, so nothing is added.
  test(
    "adds nothing where the layout already shrank, and the desktop tools and Phone preview are as they were",
    async () => {
      const { context, page } = await open({ width: 1280, height: 860 });
      try {
        await setKeyboard(page, { height: 860 });
        expect((await box(page, "body")).height).toBe(860);
        await openTools(page, "preview");
        expect((await box(page, "body")).height).toBe(860);
        expect(await page.getAttribute("body", "data-devtools")).toBe("open");
        await closeTools(page);
        // The same window with a keyboard covering part of it.
        await setKeyboard(page, { height: 600 });
        await page.waitForFunction(
          () => document.body.getBoundingClientRect().height === 600,
        );
        await setKeyboard(page, null);
        await page.waitForFunction(
          () => document.body.getBoundingClientRect().height === 860,
        );
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );
});
