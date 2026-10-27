/**
 * Headless-browser test of where the inspector sits, against the host page
 * from this tree with no wallet. Requests that leave loopback are refused, so
 * the page reaches no chain or gateway. It needs a Chromium: playwright-core
 * finds the one it installs, or set CHROMIUM_PATH.
 *
 * Where the developer tools sit follows the size of the browser window, never
 * the size chosen for the app: a phone-sized app on a wide screen still has
 * the tools docked beside or under it.
 */
import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { type Browser, type Page } from "playwright-core";
import {
  type RunningHost,
  closeTools,
  openTools,
  startHost,
  testBrowser,
} from "./fixtures.js";

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
  await scriptReady(page);
  return { context, page };
}

/** The page's script has drawn the chrome: the size menu has its choices. */
const scriptReady = (page: Page) =>
  page.waitForFunction(
    () =>
      (document.getElementById("viewport") as HTMLSelectElement).options
        .length > 0,
  );

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

const devtools = (page: Page) =>
  page.evaluate(() => ({
    open: document.body.dataset.devtools,
    view: document.body.dataset.devView,
    side: document.body.dataset.devSide,
  }));

/** The product's CSS size, which only the preview choice changes. */
const productSize = (page: Page) =>
  page.locator("#product-frame").evaluate((element) => ({
    width: (element as HTMLElement).offsetWidth,
    height: (element as HTMLElement).offsetHeight,
  }));

/** The panel's round buttons: each a 28 px circle, its icon inside it and centred. */
const roundButtons = (page: Page) =>
  page
    .locator(".devtools-side button:visible, #devtools-close:visible")
    .evaluateAll((buttons) =>
      buttons.map((button) => {
        const own = button.getBoundingClientRect();
        const icon = button.querySelector("svg")!.getBoundingClientRect();
        return {
          name: button.getAttribute("aria-label"),
          pressed: button.getAttribute("aria-pressed"),
          size: [own.width, own.height],
          round: getComputedStyle(button).borderRadius !== "0px",
          iconCentred:
            Math.abs(icon.left - own.left - (own.right - icon.right)) <= 1 &&
            Math.abs(icon.top - own.top - (own.bottom - icon.bottom)) <= 1,
        };
      }),
    );

describe("the developer tools", () => {
  test(
    "sit on the right or at the bottom, and Focus hides them, while the app keeps its preview size",
    async () => {
      const { context, page } = await openHost({ width: 1440, height: 900 });
      try {
        expect(await devtools(page)).toEqual({
          open: "open",
          view: "account",
          side: "right",
        });
        // The tools' tabs, in the order the design gives them.
        expect(
          await page.locator('[role="tablist"] [data-dev-tab]').allInnerTexts(),
        ).toEqual(["Account", "Product", "Preview", "Inspector", "More"]);
        // The size controls live in the Preview tab only, and keep their
        // choice when the tab changes.
        expect(await page.locator("#dev-preview [data-viewport]").count()).toBe(
          3,
        );
        expect(await page.locator("[data-viewport]").count()).toBe(3);
        expect(await page.locator("#viewport-note").isVisible()).toBe(false);
        await page.click('[data-dev-tab="preview"]');
        expect(await page.locator("#viewport-note").innerText()).toContain(
          "App viewport",
        );
        // Auto fills a wide app area and gives a narrow one the Phone size.
        await page.click('#dev-preview [data-viewport="auto"]');
        await page.click('[data-dev-tab="account"]');
        await page.click('[data-dev-tab="preview"]');
        expect(
          await page.getAttribute(
            '#dev-preview [data-viewport="auto"]',
            "aria-pressed",
          ),
        ).toBe("true");
        expect(await page.getAttribute("#product-frame", "data-frame")).toBe(
          null,
        );
        await page.setViewportSize({ width: 1100, height: 900 });
        await page.waitForFunction(
          () => document.getElementById("product-frame")!.offsetWidth === 393,
        );
        await page.setViewportSize({ width: 1440, height: 900 });
        await page.waitForFunction(
          () =>
            document.getElementById("product-frame")!.dataset.frame ===
            undefined,
        );

        await page.click('[data-dev-tab="preview"]');
        await page.selectOption("#viewport", { label: PHONE });
        await page.click('[data-dev-tab="inspector"]');
        expect(await page.getAttribute("#product-frame", "data-frame")).toBe(
          "device",
        );

        // Right: a 480 px column the whole height of the window.
        await productFitted(page);
        expect(await box(page, "#devtools")).toMatchObject({
          left: 960,
          right: 1440,
          top: 0,
          bottom: 900,
        });
        expect((await box(page, "#stage")).right).toBeLessThanOrEqual(960);
        expect(await productSize(page)).toEqual({ width: 393, height: 852 });

        // Bottom: across the window, under the app.
        await page.click('.devtools-side [data-dev-side="bottom"]');
        await productFitted(page);
        const bottom = await box(page, "#devtools");
        const stage = await box(page, "#stage");
        expect([bottom.left, bottom.right, bottom.bottom]).toEqual([
          0, 1440, 900,
        ]);
        expect(bottom.top).toBeGreaterThanOrEqual(stage.bottom);
        expect(await productSize(page)).toEqual({ width: 393, height: 852 });
        const frame = await box(page, "#product-frame");
        expect(frame.top).toBeGreaterThanOrEqual(stage.top);
        expect(frame.bottom).toBeLessThanOrEqual(stage.bottom);

        // Both inspector views fill the same tools.
        await page.click('[data-inspector-view="chat"]');
        expect((await box(page, "#chat-diagnostics")).width).toBe(1420);
        await page.click('[data-inspector-view="wire"]');
        expect((await box(page, "#inspector-mount")).width).toBe(1420);

        // The top edge sets the height; the app shrinks to make room.
        const frameBefore = await box(page, "#product-frame");
        await page.focus("#devtools-resize");
        await page.keyboard.press("ArrowUp");
        await page.keyboard.press("ArrowUp");
        expect((await box(page, "#devtools")).height).toBe(bottom.height + 48);
        await page.waitForFunction(
          (height) =>
            document.getElementById("product-frame")!.getBoundingClientRect()
              .height < height,
          frameBefore.height,
        );

        // The side is kept for the next visit.
        await page.reload();
        await scriptReady(page);
        expect((await devtools(page)).side).toBe("bottom");

        // Focus: the app gets the whole window, and the tools come back as they were.
        await page.click('[data-dev-tab="inspector"]');
        await page.click("#dev-toggle");
        expect((await devtools(page)).open).toBe("closed");
        expect(await box(page, "#stage")).toMatchObject({
          width: 1440,
          height: 900,
        });
        expect(await productSize(page)).toEqual({ width: 393, height: 852 });
        await page.click("#dev-toggle");
        expect(await devtools(page)).toEqual({
          open: "open",
          view: "inspector",
          side: "bottom",
        });
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );

  test(
    "the side switch and Hide are round buttons with their icons centred, on either side and in either colour scheme",
    async () => {
      const { context, page } = await openHost({ width: 1280, height: 800 });
      try {
        const expected = (right: boolean) => [
          {
            name: "Dock on the right",
            pressed: String(right),
            size: [28, 28],
            round: true,
            iconCentred: true,
          },
          {
            name: "Dock at the bottom",
            pressed: String(!right),
            size: [28, 28],
            round: true,
            iconCentred: true,
          },
          {
            name: "Hide the developer tools (Focus)",
            pressed: null,
            size: [28, 28],
            round: true,
            iconCentred: true,
          },
        ];
        for (const scheme of ["dark", "light"] as const) {
          await page.emulateMedia({ colorScheme: scheme });
          await openTools(page);
          await page.click('.devtools-side [data-dev-side="right"]');
          expect(await roundButtons(page)).toEqual(expected(true));
          await page.click('.devtools-side [data-dev-side="bottom"]');
          expect((await devtools(page)).side).toBe("bottom");
          expect(await roundButtons(page)).toEqual(expected(false));
          // Hover and focus keep the shape.
          await page.hover('.devtools-side [data-dev-side="right"]');
          await page.focus('.devtools-side [data-dev-side="right"]');
          expect(await roundButtons(page)).toEqual(expected(false));
          await page.click('.devtools-side [data-dev-side="right"]');
          await closeTools(page);
        }
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );

  test(
    "a narrow window keeps the tools a sheet over the app",
    async () => {
      const { context, page } = await openHost({ width: 600, height: 900 });
      try {
        // The preview bar is hidden here; the tab still holds a Phone choice.
        await page.evaluate(() =>
          sessionStorage.setItem(
            "truapi-web-signing-host:tab-viewport",
            "phone",
          ),
        );
        await page.reload();
        await scriptReady(page);
        expect((await devtools(page)).open).toBe("closed");
        expect(
          await page.locator("#viewport").evaluate((select) => ({
            value: (select as HTMLSelectElement).value,
            hidden: (select as HTMLElement).offsetParent === null,
          })),
        ).toEqual({ value: "phone", hidden: true });
        expect((await box(page, "#product-frame")).width).toBe(600);
        await page.click("#dev-toggle");
        await page.click('[data-dev-tab="inspector"]');
        expect(
          await page
            .locator("#devtools")
            .evaluate((element) => getComputedStyle(element).position),
        ).toBe("fixed");
        expect(await page.locator("#scrim").isVisible()).toBe(true);
        expect((await box(page, "#devtools")).width).toBe(600);
        // The sheet has no side switch, and Hide keeps its shape.
        expect(await roundButtons(page)).toEqual([
          {
            name: "Hide the developer tools (Focus)",
            pressed: null,
            size: [28, 28],
            round: true,
            iconCentred: true,
          },
        ]);
        await page.keyboard.press("Escape");
        expect(await page.getAttribute("body", "data-inspector")).toBe(
          "closed",
        );
        expect(await page.evaluate(() => document.activeElement?.id)).toBe(
          "dev-toggle",
        );
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );
});

describe("the Pill", () => {
  test(
    "opens the search card with focus in the field, and Escape gives focus back",
    async () => {
      const { context, page } = await openHost({ width: 1280, height: 800 });
      try {
        await page
          .locator("#core-status")
          .filter({ hasText: "Core ready" })
          .waitFor({ state: "attached" });
        await page.focus("#pill");
        await page.keyboard.press("Enter");
        await page.locator("#search-card").waitFor();
        expect(await page.getAttribute("#pill", "aria-expanded")).toBe("true");
        expect(await page.evaluate(() => document.activeElement?.id)).toBe(
          "address",
        );
        // A typed address that cannot open says why, in the card.
        await page.fill("#address", "javascript:alert(1)");
        await page.press("#address", "Enter");
        expect(await page.locator("#search-status").innerText()).not.toBe("");
        await page.press("#address", "Escape");
        await page.locator("#search-card").waitFor({ state: "hidden" });
        expect(await page.evaluate(() => document.activeElement?.id)).toBe(
          "pill",
        );
        expect(await page.inputValue("#address")).toBe("");
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
      const label = document.querySelector(".dock-label");
      (window as unknown as { __firstPaint: unknown }).__firstPaint = {
        // Unstyled, a dock label is in the flow; the stylesheet takes it out.
        styled:
          label !== null && getComputedStyle(label).position === "absolute",
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
            largestIcon: 30,
          });
          expect(
            await box(page, '#dock [data-destination="chat"] svg'),
          ).toMatchObject({ width: 28, height: 28 });
          expect(await box(page, "#dev-toggle")).toMatchObject({
            width: 120,
            height: 8,
          });
          release();
          await page.locator("#core-status").waitFor({ state: "attached" });
          expect(
            await box(page, '#dock [data-destination="chat"] svg'),
          ).toMatchObject({ width: 28, height: 28 });
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
          largestIcon: 30,
        });
      } finally {
        release();
        await context.close();
      }
    },
    TEST_MS,
  );
});
