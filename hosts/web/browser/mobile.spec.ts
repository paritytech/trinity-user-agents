/**
 * Headless-browser test of the host in a phone's browser, emulated: the page
 * gestures, the zoom lock and full screen. An emulated touch screen shows what
 * the page asks of the browser, not what a real phone's browser does with it.
 * No wallet is used and every request outside this host is refused.
 */
import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { join } from "node:path";
import type { Browser, BrowserContext, Page } from "playwright-core";
import {
  type RunningHost,
  closeTools,
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

type Point = { x: number; y: number };

/**
 * A one-finger touch from `from` to `to` in `steps` moves, `pause` ms apart.
 * `during` runs before the finger lifts.
 */
async function swipe(
  page: Page,
  from: Point,
  to: Point,
  { steps = 8, pause = 30, during = async () => {} } = {},
): Promise<void> {
  const cdp = await page.context().newCDPSession(page);
  const touch = (
    type: "touchStart" | "touchMove" | "touchEnd",
    points: Point[],
  ) => cdp.send("Input.dispatchTouchEvent", { type, touchPoints: points });
  await touch("touchStart", [from]);
  for (let step = 1; step <= steps; step += 1) {
    await touch("touchMove", [
      {
        x: from.x + ((to.x - from.x) * step) / steps,
        y: from.y + ((to.y - from.y) * step) / steps,
      },
    ]);
    if (pause > 0) await page.waitForTimeout(pause);
  }
  await during();
  await touch("touchEnd", []);
  await cdp.detach();
}

/** The middle of `selector`'s box, `down` pixels below its middle. */
async function middle(page: Page, selector: string, down = 0): Promise<Point> {
  const box = (await page.locator(selector).first().boundingBox())!;
  return { x: box.x + box.width / 2, y: box.y + box.height / 2 + down };
}

/** Wait until `selector` has no transition or animation running. */
const settled = (page: Page, selector: string) =>
  page.waitForFunction(
    (target) => document.querySelector(target)!.getAnimations().length === 0,
    selector,
  );

const toolsState = (page: Page) => page.getAttribute("body", "data-devtools");

/** Whether the Developer sheet is open and back where it rests, with no drag left on it. */
async function toolsAtRest(page: Page): Promise<boolean> {
  await settled(page, "#devtools");
  return page.evaluate(() => {
    const panel = document.getElementById("devtools")!;
    return (
      document.body.dataset.devtools === "open" &&
      panel.style.transform === "" &&
      getComputedStyle(panel).transform === "none"
    );
  });
}

async function screenshot(page: Page, name: string): Promise<void> {
  const directory = process.env.SHEET_SCREENSHOTS;
  if (directory)
    await page.screenshot({ path: join(directory, `${name}.png`) });
}

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

  test(
    "the Developer sheet's handle closes it on a tap or a long drag, and leaves the sheet's other gestures alone",
    async () => {
      const { context, page, outside } = await open("offered");
      try {
        const reopen = async () => {
          await openTools(page, "chat");
          expect(await toolsAtRest(page)).toBe(true);
        };
        await reopen();
        await screenshot(page, "developer-sheet-open");
        // The handle is a target a finger finds, around the bar it draws.
        const grab = (await page.locator(".devtools-grab").boundingBox())!;
        expect(grab.width).toBeGreaterThanOrEqual(120);
        expect(grab.height).toBeGreaterThanOrEqual(24);

        await page.tap(".devtools-grab");
        await page
          .locator('body[data-devtools="closed"]')
          .waitFor({ state: "attached" });

        // A short slow drag shows the sheet following, then springs back.
        await reopen();
        const handle = await middle(page, ".devtools-grab");
        let pulled = 0;
        await swipe(
          page,
          handle,
          { x: handle.x, y: handle.y + 60 },
          {
            during: async () => {
              // Touch moves are delivered once per frame; let the last one land.
              pulled = await page.evaluate(async () => {
                await new Promise((frame) =>
                  requestAnimationFrame(() => requestAnimationFrame(frame)),
                );
                return new DOMMatrix(
                  getComputedStyle(document.getElementById("devtools")!)
                    .transform,
                ).m42;
              });
              await screenshot(page, "developer-sheet-dragging");
            },
          },
        );
        expect(pulled).toBeCloseTo(60, 0);
        expect(await toolsAtRest(page)).toBe(true);

        // Sideways on the header, down inside the content and along the
        // tabs belong to what is there, not to the handle.
        const header = await middle(page, "#devtools-title");
        await swipe(page, header, { x: header.x + 120, y: header.y + 20 });
        expect(await toolsAtRest(page)).toBe(true);
        const panel = (await page.locator("#devtools").boundingBox())!;
        const content = { x: panel.x + panel.width / 2, y: panel.y + 220 };
        await swipe(page, content, { x: content.x, y: content.y + 200 });
        expect(await toolsAtRest(page)).toBe(true);
        const tabs = await middle(page, ".devtools-tabs");
        await swipe(page, tabs, { x: tabs.x, y: tabs.y + 200 });
        expect(await toolsAtRest(page)).toBe(true);
        // A drag up never resizes the sheet.
        await swipe(page, handle, { x: handle.x, y: handle.y - 150 });
        expect(await toolsAtRest(page)).toBe(true);
        expect(await page.evaluate(() => innerHeight)).toBe(844);
        expect(
          await page.evaluate(
            () => document.getElementById("devtools")!.offsetHeight,
          ),
        ).toBe(Math.round(844 * 0.82));

        // A long drag from the header closes it.
        await swipe(page, header, { x: header.x, y: header.y + 200 });
        expect(await toolsState(page)).toBe("closed");
        await settled(page, "#devtools");
        await screenshot(page, "developer-sheet-closed");

        // So does one from the handle. A flick's speed is covered by the unit
        // test: touches sent from here arrive too far apart to time one.
        await reopen();
        await swipe(page, handle, { x: handle.x, y: handle.y + 150 });
        expect(await toolsState(page)).toBe("closed");

        // The close button and Escape still close it, and it opens again.
        await reopen();
        await closeTools(page);
        await reopen();
        await page.keyboard.press("Escape");
        expect(await toolsState(page)).toBe("closed");
        await reopen();
        expect(outside).toEqual([]);
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );

  test(
    "the search card's handle closes it on a tap or a drag down its heading",
    async () => {
      const { context, page } = await open("offered");
      try {
        const card = page.locator("#search-card");
        await page.tap("#pill");
        await card.waitFor();
        await settled(page, "#search-card");
        await screenshot(page, "search-card-open");
        await page.tap(".search-grab");
        await card.waitFor({ state: "hidden" });
        expect(await page.getAttribute("#pill", "aria-expanded")).toBe("false");

        await page.tap("#pill");
        await card.waitFor();
        const heading = await middle(page, "#search-title");
        await swipe(page, heading, { x: heading.x, y: heading.y + 20 });
        expect(await card.isVisible()).toBe(true);
        await swipe(page, heading, { x: heading.x, y: heading.y + 160 });
        await card.waitFor({ state: "hidden" });

        // Its close button still closes it.
        await page.tap("#pill");
        await card.waitFor();
        await page.tap("#search-close");
        await card.waitFor({ state: "hidden" });
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );
});
