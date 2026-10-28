/**
 * Headless-browser test of the host in a phone's browser, emulated: the page
 * gestures, the zoom lock and full screen. An emulated touch screen shows what
 * the page asks of the browser, not what a real phone's browser does with it.
 * No wallet is used and every request outside this host is refused.
 */
import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import type { Browser, BrowserContext, Page } from "playwright-core";
import {
  type RunningHost,
  openTools,
  startHost,
  testBrowser,
} from "./support/fixtures.js";

const STEP_MS = 60_000;
const TEST_MS = 120_000;
const UNLOCKED = "width=device-width, initial-scale=1, viewport-fit=cover";

let browser: Browser;
let host: RunningHost;

beforeAll(async () => {
  host = await startHost();
  browser = await testBrowser();
}, STEP_MS);

afterAll(async () => {
  await host?.close();
});

/** A phone-sized touch page; `fullscreen` stubs the browser's full screen API. */
async function open(
  fullscreen: "offered" | "refused" | "absent",
): Promise<{ context: BrowserContext; page: Page; outside: string[] }> {
  const context = await browser.newContext({
    viewport: { width: 390, height: 844 },
    isMobile: true,
    hasTouch: true,
  });
  context.setDefaultTimeout(STEP_MS);
  await context.addInitScript((mode) => {
    let element: Element | null = null;
    Object.defineProperty(Document.prototype, "fullscreenEnabled", {
      configurable: true,
      get: () => mode !== "absent",
    });
    Object.defineProperty(Document.prototype, "fullscreenElement", {
      configurable: true,
      get: () => element,
    });
    const change = (next: Element | null) => {
      element = next;
      document.dispatchEvent(new Event("fullscreenchange"));
    };
    Element.prototype.requestFullscreen = function () {
      if (mode === "refused")
        return Promise.reject(new TypeError("Permissions check failed"));
      change(this);
      return Promise.resolve();
    };
    Document.prototype.exitFullscreen = () => {
      change(null);
      return Promise.resolve();
    };
  }, fullscreen);
  await context.routeWebSocket(/.*/, (socket) => socket.close());
  const outside: string[] = [];
  await context.route(/.*/, (route) => {
    const url = route.request().url();
    if (url.startsWith(host.origin)) return route.fallback();
    outside.push(url);
    return route.abort();
  });
  const page = await context.newPage();
  await page.goto(host.origin);
  await page
    .locator("#core-status")
    .filter({ hasText: "Core ready" })
    .waitFor({ state: "attached" });
  return { context, page, outside };
}

const viewportMeta = (page: Page) =>
  page.getAttribute("#viewport-meta", "content");

describe("the host on a phone", () => {
  test(
    "keeps the page from pull to refresh and double-tap zoom, and fields at 16 px",
    async () => {
      const { context, page, outside } = await open("offered");
      try {
        expect(
          await page.evaluate(() =>
            [document.documentElement, document.body].map((node) => {
              const style = getComputedStyle(node);
              return [style.overscrollBehaviorY, style.touchAction];
            }),
          ),
        ).toEqual([
          ["none", "manipulation"],
          ["none", "manipulation"],
        ]);
        // Pinch zoom stays until the lock is chosen.
        expect(await viewportMeta(page)).toBe(UNLOCKED);

        // Every field a user types in is at least 16 px, so focus does not zoom.
        await page.click("#pill");
        await openTools(page, "account");
        const fields = await page.evaluate(() =>
          [
            ...document.querySelectorAll<HTMLElement>(
              "input:not([type=checkbox]), textarea, select",
            ),
          ]
            .filter((field) => field.getClientRects().length > 0)
            .map((field) => ({
              id: field.id,
              size: parseFloat(getComputedStyle(field).fontSize),
            })),
        );
        expect(fields.map((field) => field.id)).toEqual(
          expect.arrayContaining(["address", "network"]),
        );
        expect(fields.filter((field) => field.size < 16)).toEqual([]);
        expect(outside).toEqual([]);
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );

  test(
    "Preview → This device locks page zoom, keeps it after a reload, and unlocks it",
    async () => {
      const { context, page } = await open("offered");
      try {
        await openTools(page, "preview");
        await page.locator("#zoom-lock").check();
        expect(await viewportMeta(page)).toBe(
          `${UNLOCKED}, maximum-scale=1, user-scalable=no`,
        );
        await page.reload();
        await page.waitForFunction(
          () =>
            (document.getElementById("zoom-lock") as HTMLInputElement).checked,
        );
        expect(await viewportMeta(page)).toContain("user-scalable=no");
        await openTools(page, "preview");
        await page.locator("#zoom-lock").uncheck();
        expect(await viewportMeta(page)).toBe(UNLOCKED);
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );

  test(
    "full screen starts and ends from its button, says why when refused, and is not offered where absent",
    async () => {
      const offered = await open("offered");
      try {
        const { page } = offered;
        await openTools(page, "preview");
        const button = page.locator("#fullscreen");
        expect(await button.innerText()).toBe("Full screen");
        await button.click();
        await page.waitForFunction(
          () => document.fullscreenElement === document.documentElement,
        );
        expect(await button.innerText()).toBe("Exit full screen");
        expect(await button.getAttribute("aria-pressed")).toBe("true");
        await button.click();
        await page.waitForFunction(() => document.fullscreenElement === null);
        expect(await button.innerText()).toBe("Full screen");
      } finally {
        await offered.context.close();
      }

      const refused = await open("refused");
      try {
        await openTools(refused.page, "preview");
        await refused.page.click("#fullscreen");
        await refused.page
          .locator("#device-status", { hasText: "Full screen did not change" })
          .waitFor();
      } finally {
        await refused.context.close();
      }

      const absent = await open("absent");
      try {
        await openTools(absent.page, "preview");
        expect(await absent.page.locator("#fullscreen").isHidden()).toBe(true);
        expect(
          await absent.page.locator("#device-status").innerText(),
        ).toContain("Add the page to the home screen");
      } finally {
        await absent.context.close();
      }
    },
    TEST_MS,
  );
});
