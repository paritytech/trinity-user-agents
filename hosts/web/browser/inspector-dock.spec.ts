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
  showAddress,
  startHost,
  startUrlProduct,
  testBrowser,
} from "./support/fixtures.js";

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

/** The side switch's segments and Hide: their size, whether they sit inside their track, and their icon centred. */
const roundButtons = (page: Page) =>
  page
    .locator(".devtools-side button:visible, #devtools-close:visible")
    .evaluateAll((buttons) =>
      buttons.map((button) => {
        const own = button.getBoundingClientRect();
        const icon = button.querySelector("svg")!.getBoundingClientRect();
        const track = button.parentElement!.getBoundingClientRect();
        return {
          name: button.getAttribute("aria-label"),
          pressed: button.getAttribute("aria-pressed"),
          size: [own.width, own.height],
          round: getComputedStyle(button).borderRadius !== "0px",
          // A segment keeps at least 2 px of its track around it, so the
          // selected one never reaches the track's edge.
          insideTrack: button.classList.contains("devtools-close")
            ? true
            : own.top - track.top >= 2 &&
              track.bottom - own.bottom >= 2 &&
              own.left - track.left >= 2 &&
              track.right - own.right >= 2,
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

        // The inspector's four views, in order, fill the same tools, and the
        // arrow keys move between them.
        expect(
          await page
            .locator("[data-inspector-view]")
            .evaluateAll((tabs) => tabs.map((tab) => tab.textContent?.trim())),
        ).toEqual(["Activity", "TrUAPI", "Chat", "Workers"]);
        for (const [view, panel] of [
          ["activity", "#activity-inspector"],
          ["chat", "#chat-diagnostics"],
          ["workers", "#workers-inspector"],
          ["wire", "#inspector-mount"],
        ] as const) {
          await page.click(`[data-inspector-view="${view}"]`);
          expect((await box(page, panel)).width).toBe(1420);
          expect(
            await page
              .locator("#inspector > [role=tabpanel], #inspector-mount")
              .evaluateAll(
                (panels) =>
                  panels.filter((shown) => !(shown as HTMLElement).hidden)
                    .length,
              ),
          ).toBe(1);
        }
        // The Activity view holds what was logged before it was first shown.
        await page.keyboard.press("ArrowLeft");
        await page
          .locator('[data-inspector-view="activity"][aria-selected="true"]')
          .waitFor();
        expect(
          await page.evaluate(
            () => (document.activeElement as HTMLElement).dataset.inspectorView,
          ),
        ).toBe("activity");
        await page.locator("#log .activity-row").first().waitFor();
        await page.keyboard.press("ArrowLeft");
        await page
          .locator('[data-inspector-view="workers"][aria-selected="true"]')
          .waitFor();
        await page.keyboard.press("ArrowRight");
        await page.keyboard.press("ArrowRight");
        await page
          .locator('[data-inspector-view="wire"][aria-selected="true"]')
          .waitFor();
        // More no longer holds a log.
        expect(await page.locator("#dev-more #log").count()).toBe(0);

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
    "the side switch is a segmented control and Hide a round button, icons centred, on either side and in either colour scheme",
    async () => {
      const { context, page } = await openHost({ width: 1280, height: 800 });
      try {
        const expected = (right: boolean) => [
          {
            name: "Dock on the right",
            pressed: String(right),
            size: [36, 26],
            round: true,
            insideTrack: true,
            iconCentred: true,
          },
          {
            name: "Dock at the bottom",
            pressed: String(!right),
            size: [36, 26],
            round: true,
            insideTrack: true,
            iconCentred: true,
          },
          {
            name: "Hide the developer tools (Focus)",
            pressed: null,
            size: [28, 28],
            round: true,
            insideTrack: true,
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
    "the left edge sets the width on the right, apart from the height at the bottom, and the product keeps running",
    async () => {
      const product = await startUrlProduct(host.origin);
      const { context, page } = await openHost({ width: 1440, height: 900 });
      const handle = "#devtools-resize";
      const width = async () => (await box(page, "#devtools")).width;
      const height = async () => (await box(page, "#devtools")).height;
      const aria = () =>
        page.locator(handle).evaluate((node) => ({
          orientation: node.getAttribute("aria-orientation"),
          label: node.getAttribute("aria-label"),
          now: node.getAttribute("aria-valuenow"),
          min: node.getAttribute("aria-valuemin"),
          max: node.getAttribute("aria-valuemax"),
        }));
      try {
        await page.click('[data-dev-tab="preview"]');
        await page.selectOption("#viewport", { label: PHONE });
        await closeTools(page);
        // A product opens only once the core runs.
        await page
          .locator("#core-status")
          .filter({ hasText: "Core ready" })
          .waitFor({ state: "attached" });
        await showAddress(page);
        await page.fill("#address", product.url);
        await page.press("#address", "Enter");
        const origin = new URL(product.url).origin;
        const deadline = Date.now() + STEP_MS;
        let frame = null;
        while (frame === null) {
          if (Date.now() > deadline)
            throw new Error("the product did not open");
          frame =
            page
              .frames()
              .find((candidate) => candidate.url().startsWith(origin)) ?? null;
          if (frame === null) await page.waitForTimeout(100);
        }
        await frame.waitForFunction(() => "__loadedAt" in window);
        const loadedAt = await frame.evaluate(
          () => (window as unknown as { __loadedAt: number }).__loadedAt,
        );
        await openTools(page, "inspector");
        await page.click('.devtools-side [data-dev-side="right"]');
        expect(await width()).toBe(480);
        expect(await aria()).toEqual({
          orientation: "vertical",
          label: "Developer tools width",
          now: "480",
          min: "320",
          max: "1080",
        });

        // Dragged 100 px to the left, the tools are 100 px wider.
        const edge = await box(page, handle);
        const y = edge.top + edge.height / 2;
        await page.mouse.move(edge.left + 5, y);
        await page.mouse.down();
        await page.mouse.move(edge.left - 45, y);
        await page.mouse.move(edge.left - 95, y);
        await page.mouse.up();
        expect(await width()).toBe(580);
        expect((await aria()).now).toBe("580");
        expect(
          await page.evaluate(() => [
            document.body.style.userSelect,
            document.body.style.cursor,
          ]),
        ).toEqual(["", ""]);
        await productFitted(page);
        expect(await productSize(page)).toEqual({ width: 393, height: 852 });

        // The keyboard: Left widens, Right narrows, Home and End are the bounds.
        await page.focus(handle);
        await page.keyboard.press("ArrowLeft");
        expect(await width()).toBe(604);
        await page.keyboard.press("ArrowRight");
        await page.keyboard.press("ArrowRight");
        expect(await width()).toBe(556);
        await page.keyboard.press("End");
        expect(await width()).toBe(1080);
        await page.keyboard.press("Home");
        expect(await width()).toBe(320);
        for (let step = 0; step < 15; step += 1)
          await page.keyboard.press("ArrowLeft");
        expect(await width()).toBe(680);

        // A narrower window keeps 360 px for the app; a wider one gives the width back.
        await page.setViewportSize({ width: 960, height: 900 });
        await page.waitForFunction(
          () => document.getElementById("devtools")!.offsetWidth === 600,
        );
        await page.setViewportSize({ width: 1440, height: 900 });
        await page.waitForFunction(
          () => document.getElementById("devtools")!.offsetWidth === 680,
        );

        // At the bottom the same edge sets the height, and leaves the width alone.
        await page.click('.devtools-side [data-dev-side="bottom"]');
        const bottomHeight = await height();
        expect((await aria()).orientation).toBe("horizontal");
        expect((await aria()).label).toBe("Developer tools height");
        await page.focus(handle);
        await page.keyboard.press("ArrowUp");
        expect(await height()).toBe(bottomHeight + 24);
        await page.click('.devtools-side [data-dev-side="right"]');
        expect(await width()).toBe(680);

        // Neither drag nor key reloaded the product.
        expect(
          await frame.evaluate(
            () => (window as unknown as { __loadedAt: number }).__loadedAt,
          ),
        ).toBe(loadedAt);

        // Both sizes are kept for the next visit.
        await page.reload();
        await scriptReady(page);
        await openTools(page, "inspector");
        expect(await width()).toBe(680);
        await page.click('.devtools-side [data-dev-side="bottom"]');
        expect(await height()).toBe(bottomHeight + 24);
      } finally {
        await context.close();
        product.close();
      }
    },
    TEST_MS,
  );

  test(
    "a new tab previews a phone, and a size the tab chose stays chosen",
    async () => {
      const { context, page } = await openHost({ width: 1440, height: 900 });
      try {
        const preview = () =>
          page.locator("#product-frame").evaluate((element) => ({
            choice: (document.getElementById("viewport") as HTMLSelectElement)
              .value,
            frame: (element as HTMLElement).dataset.frame ?? null,
            width: (element as HTMLElement).offsetWidth,
            height: (element as HTMLElement).offsetHeight,
          }));
        expect(await preview()).toEqual({
          choice: "phone",
          frame: "device",
          width: 393,
          height: 852,
        });

        // Desktop, chosen in the Preview tab, survives a reload of the tab.
        await openTools(page, "preview");
        await page.click('#dev-preview [data-viewport="fill"]');
        await page.reload();
        await scriptReady(page);
        const filled = await preview();
        expect(filled).toMatchObject({ choice: "fill", frame: null });
        expect(filled.width).toBeGreaterThan(393);

        // A narrow window fills with the app, not a phone inside a phone.
        await page.evaluate(() => sessionStorage.clear());
        await page.setViewportSize({ width: 600, height: 900 });
        await page.reload();
        await scriptReady(page);
        expect(await preview()).toEqual({
          choice: "phone",
          frame: null,
          width: 600,
          height: (await box(page, "#product-frame")).height,
        });
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
        // The sheet is not resized.
        expect(await page.locator("#devtools-resize").isVisible()).toBe(false);
        // The sheet has no side switch, and Hide keeps its shape.
        expect(await roundButtons(page)).toEqual([
          {
            name: "Hide the developer tools (Focus)",
            pressed: null,
            size: [28, 28],
            round: true,
            insideTrack: true,
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
