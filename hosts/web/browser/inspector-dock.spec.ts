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
import { type Browser, type Page, chromium } from "playwright-core";
import { type RunningHost, startHost } from "./fixtures.js";

const STEP_MS = 60_000;
const TEST_MS = 120_000;
const PHONE = "Phone 393 × 852";

let browser: Browser;
let host: RunningHost;

beforeAll(async () => {
  host = await startHost();
  browser = await chromium.launch({
    executablePath: process.env.CHROMIUM_PATH || undefined,
  });
}, STEP_MS);

afterAll(async () => {
  await browser?.close();
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
