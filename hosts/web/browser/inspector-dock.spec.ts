/**
 * Headless-browser test of where the inspector sits, against the host page
 * from this tree with no wallet. Requests that leave loopback are refused, so
 * the page reaches no chain or gateway. It needs a Chromium: playwright-core
 * finds the one it installs, or set CHROMIUM_PATH.
 *
 * Where the inspector sits follows the size of the browser window, never the
 * size simulated for the product: a phone-shaped product on a wide screen
 * still has the inspector docked under it.
 */
import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { type Browser, type Page } from "playwright-core";
import { type RunningHost, startHost, testBrowser } from "./fixtures.js";

const STEP_MS = 60_000;
const TEST_MS = 120_000;
const PHONE = "Phone 393 × 852";

let browser: Browser;
let host: RunningHost;

beforeAll(async () => {
  host = await startHost();
  browser = await testBrowser();
}, STEP_MS);

afterAll(async () => {
  await host?.close();
});

async function openHost(viewport: { width: number; height: number }) {
  const context = await browser.newContext({ viewport });
  context.setDefaultTimeout(STEP_MS);
  await context.route("**/*", (route) =>
    new URL(route.request().url()).hostname === "localhost"
      ? route.continue()
      : route.abort(),
  );
  const page = await context.newPage();
  await page.goto(host.origin);
  await page.locator("#inspector-toggle").waitFor();
  return { context, page };
}

interface Box {
  left: number;
  right: number;
  top: number;
  bottom: number;
  width: number;
  height: number;
}

const box = (page: Page, selector: string): Promise<Box> =>
  page.locator(selector).evaluate((element) => {
    const { left, right, top, bottom, width, height } =
      element.getBoundingClientRect();
    return { left, right, top, bottom, width, height };
  });

/**
 * Wait until the stage's resize observer has refit the product after a
 * layout change. The observer runs after layout, so for one frame the
 * product can still have the scale it had before.
 */
const productFitted = (page: Page) =>
  page.waitForFunction(() => {
    const frame = document.getElementById("product-frame")!;
    const stage = document.getElementById("stage")!.getBoundingClientRect();
    const box = frame.getBoundingClientRect();
    return box.top >= stage.top && box.bottom <= stage.bottom;
  });

async function setOpen(
  page: Page,
  panel: "menu" | "inspector",
  open: boolean,
): Promise<void> {
  const state = open ? "open" : "closed";
  if ((await page.getAttribute("body", `data-${panel}`)) !== state)
    await page.click(`#${panel}-toggle`);
  expect(await page.getAttribute("body", `data-${panel}`)).toBe(state);
}

describe("the inspector dock", () => {
  test(
    "a wide window docks it under a phone-sized product, across the width the menu leaves",
    async () => {
      const { context, page } = await openHost({ width: 1440, height: 900 });
      try {
        await page.selectOption("#viewport", { label: PHONE });
        await setOpen(page, "inspector", true);

        for (const menuOpen of [true, false]) {
          await setOpen(page, "menu", menuOpen);
          await productFitted(page);
          const inspector = await box(page, "#inspector");
          const stage = await box(page, "#stage");
          const frame = await box(page, "#product-frame");
          const right = menuOpen ? (await box(page, "#menu")).left : 1440;
          expect(inspector.left).toBe(0);
          expect(inspector.right).toBe(right);
          expect(inspector.top).toBeGreaterThanOrEqual(stage.bottom);
          expect(inspector.bottom).toBe(900);
          // The product keeps its CSS size and is shrunk to fit above.
          expect(
            await page.locator("#product-frame").evaluate((element) => ({
              width: (element as HTMLElement).offsetWidth,
              height: (element as HTMLElement).offsetHeight,
            })),
          ).toEqual({ width: 393, height: 852 });
          expect(frame.top).toBeGreaterThanOrEqual(stage.top);
          expect(frame.bottom).toBeLessThanOrEqual(stage.bottom);
        }

        // Both views fill the same dock.
        await page.click('[data-inspector-view="chat"]');
        const chat = await box(page, "#chat-diagnostics");
        expect(chat.width).toBe(1440);
        await page.click('[data-inspector-view="wire"]');
        expect((await box(page, "#inspector-mount")).width).toBe(1440);

        // The top edge sets the height; the product shrinks to make room.
        const before = await box(page, "#inspector");
        const frameBefore = await box(page, "#product-frame");
        await page.focus("#inspector-resize");
        await page.keyboard.press("ArrowUp");
        await page.keyboard.press("ArrowUp");
        const after = await box(page, "#inspector");
        expect(after.height).toBe(before.height + 48);
        // The stage's resize observer refits the product after layout.
        await page.waitForFunction(
          (height) =>
            document.getElementById("product-frame")!.getBoundingClientRect()
              .height < height,
          frameBefore.height,
        );
        expect(await page.locator("#inspector-resize").isVisible()).toBe(true);
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );

  test(
    "a narrow window keeps the inspector a bottom sheet over the product",
    async () => {
      const { context, page } = await openHost({ width: 600, height: 900 });
      try {
        // The size menu is hidden here; the tab still holds a Phone choice.
        await page.evaluate(() =>
          sessionStorage.setItem("truapi-web-signing-host:tab-viewport", "1"),
        );
        await page.reload();
        await page.locator("#inspector-toggle").waitFor();
        expect(
          await page.locator("#viewport").evaluate((select) => ({
            value: (select as HTMLSelectElement).value,
            hidden: (select as HTMLElement).offsetParent === null,
          })),
        ).toEqual({ value: "1", hidden: true });
        expect((await box(page, "#product-frame")).width).toBe(600);
        await setOpen(page, "inspector", true);
        expect(
          await page
            .locator("#inspector")
            .evaluate((element) => getComputedStyle(element).position),
        ).toBe("fixed");
        expect(await page.locator("#scrim").isVisible()).toBe(true);
        expect((await box(page, "#inspector")).width).toBe(600);
        await page.keyboard.press("Escape");
        expect(await page.getAttribute("body", "data-inspector")).toBe(
          "closed",
        );
        expect(await page.evaluate(() => document.activeElement?.id)).toBe(
          "inspector-toggle",
        );
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );
});

describe("the first paint", () => {
  const VIEWPORTS = [
    { width: 1280, height: 800 },
    { width: 390, height: 844 },
  ];

  /**
   * Record, at the browser's first contentful paint, what the page looked
   * like: whether the stylesheet had applied and how large each icon was.
   */
  function recordFirstPaint(): void {
    new PerformanceObserver((list, observer) => {
      if (!list.getEntries().some((e) => e.name === "first-contentful-paint"))
        return;
      observer.disconnect();
      const brand = document.querySelector(".brand");
      (window as unknown as { __firstPaint: unknown }).__firstPaint = {
        // Unstyled, the brand's span is `inline`; the stylesheet makes it a flex box.
        styled: brand !== null && getComputedStyle(brand).display !== "inline",
        largestIcon: Math.max(
          ...[...document.querySelectorAll("svg")].map((icon) => {
            const { width, height } = icon.getBoundingClientRect();
            return Math.max(width, height);
          }),
        ),
      };
    }).observe({ type: "paint", buffered: true });
  }

  /** A fresh page that holds back requests matching `held` until released. */
  async function openHeld(
    viewport: { width: number; height: number },
    held: RegExp,
  ) {
    const context = await browser.newContext({ viewport });
    context.setDefaultTimeout(STEP_MS);
    let release!: () => void;
    const gate = new Promise<void>((resolve) => (release = resolve));
    await context.route(held, async (route) => {
      await gate;
      await route.fallback();
    });
    await context.route("**/*", (route) =>
      new URL(route.request().url()).hostname === "localhost"
        ? route.fallback()
        : route.abort(),
    );
    await context.addInitScript(recordFirstPaint);
    const page = await context.newPage();
    return { context, page, release };
  }

  const firstPaint = (page: Page) =>
    page
      .waitForFunction(() => "__firstPaint" in window)
      .then(() =>
        page.evaluate(
          () =>
            (
              window as unknown as {
                __firstPaint: { styled: boolean; largestIcon: number };
              }
            ).__firstPaint,
        ),
      );

  test(
    "paints the styled page first, while the page's script is still on its way",
    async () => {
      for (const viewport of VIEWPORTS) {
        const { context, page, release } = await openHeld(
          viewport,
          /\/src\/main\.ts(\?.*)?$/,
        );
        try {
          await page.goto(host.origin, { waitUntil: "commit" });
          expect(await firstPaint(page)).toEqual({
            styled: true,
            largestIcon: 22,
          });
          expect(await box(page, ".brand svg")).toMatchObject({
            width: 22,
            height: 22,
          });
          expect(await box(page, "#menu-toggle svg")).toMatchObject({
            width: 18,
            height: 18,
          });
          release();
          await page.locator("#core-status").waitFor({ state: "attached" });
          expect(
            await box(page, '#dock [data-destination="chat"] svg'),
          ).toMatchObject({ width: 22, height: 22 });
        } finally {
          release();
          await context.close();
        }
      }
    },
    TEST_MS,
  );

  test(
    "paints nothing until the stylesheet arrives",
    async () => {
      const { context, page, release } = await openHeld(
        VIEWPORTS[1],
        /\/src\/style\.css(\?.*)?$/,
      );
      try {
        await page.goto(host.origin, { waitUntil: "commit" });
        await page.waitForTimeout(1500);
        expect(
          await page.evaluate(() =>
            performance.getEntriesByType("paint").map((entry) => entry.name),
          ),
        ).toEqual([]);
        release();
        expect(await firstPaint(page)).toEqual({
          styled: true,
          largestIcon: 22,
        });
      } finally {
        release();
        await context.close();
      }
    },
    TEST_MS,
  );
});
