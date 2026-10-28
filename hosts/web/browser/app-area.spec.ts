/**
 * Headless-browser test of where a product in front sits: its frame against
 * the host's circles, a device's island and the Docklet, in each preview frame,
 * drawn at full size and shrunk, and in a phone-sized window. The product is a
 * synthetic page on loopback with a field at its very top and a button fixed
 * to its bottom. No wallet signs in, every socket is closed and every request
 * outside this host and the product is refused. It needs a Chromium:
 * playwright-core finds the one it installs, or set CHROMIUM_PATH.
 */
import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { createServer, type Server } from "node:http";
import { join } from "node:path";
import type { Browser, BrowserContext, Frame, Page } from "playwright-core";
import {
  type RunningHost,
  choosePreview,
  closeTools,
  freePort,
  openTools,
  showAddress,
  startHost,
  testBrowser,
} from "./support/fixtures.js";

const STEP_MS = 60_000;
const TEST_MS = 180_000;

/** A page that puts content at both of its edges and reports its own insets. */
const PRODUCT_HTML = `<!doctype html>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover">
<title>edges fixture</title>
<style>
  body { margin: 0; font: 16px system-ui; }
  #top { display: block; box-sizing: border-box; width: 100%; height: 44px; margin: 0; }
  #filler { height: 1600px; }
  #bottom { position: fixed; right: 0; bottom: 0; left: 0; height: 44px; }
  #insets { padding-top: env(safe-area-inset-top); padding-bottom: env(safe-area-inset-bottom); }
</style>
<input id="top" placeholder="Search">
<div id="insets"></div>
<div id="filler"></div>
<button id="bottom">Bottom action</button>
<script>window.__alive = Math.random();</script>`;

let browser: Browser;
let host: RunningHost;
let product: { server: Server; url: string };

beforeAll(async () => {
  host = await startHost();
  browser = await testBrowser();
  const port = await freePort();
  const server = createServer((_request, response) => {
    response.writeHead(200, { "content-type": "text/html; charset=utf-8" });
    response.end(PRODUCT_HTML);
  });
  await new Promise<void>((resolve) =>
    server.listen(port, "127.0.0.1", () => resolve()),
  );
  product = { server, url: `http://localhost:${port}/` };
}, STEP_MS);

afterAll(async () => {
  product?.server.close();
  await host?.close();
});

async function open(viewport: {
  width: number;
  height: number;
}): Promise<{ context: BrowserContext; page: Page }> {
  const context = await browser.newContext({ viewport });
  context.setDefaultTimeout(STEP_MS);
  await context.routeWebSocket(/.*/, (socket) => socket.close());
  await context.route(/.*/, (route) => {
    const url = route.request().url();
    if (url.startsWith(host.origin) || url.startsWith(product.url))
      return route.fallback();
    return route.abort();
  });
  const page = await context.newPage();
  await page.goto(host.origin);
  await page
    .locator("#core-status")
    .filter({ hasText: "Core ready" })
    .waitFor({ state: "attached" });
  return { context, page };
}

/** Open the fixture as Browse's product, and return its frame once it runs. */
async function openProduct(page: Page): Promise<Frame> {
  await page.click('#dock [data-destination="browse"]');
  await showAddress(page);
  await page.fill("#address", product.url);
  await page.press("#address", "Enter");
  await page.locator("#app-chrome").waitFor();
  const deadline = Date.now() + STEP_MS;
  while (Date.now() < deadline) {
    const frame = page
      .frames()
      .find((candidate) => candidate.url() === product.url);
    if (frame !== undefined) {
      await frame.locator("#bottom").waitFor();
      return frame;
    }
    await page.waitForTimeout(100);
  }
  throw new Error("the fixture product did not load");
}

interface Box {
  x: number;
  y: number;
  width: number;
  height: number;
}

const bottomOf = (box: Box) => box.y + box.height;

async function box(page: Page, selector: string): Promise<Box> {
  const found = await page.locator(selector).boundingBox();
  if (found === null) throw new Error(`${selector} is not shown`);
  return found;
}

async function frameBox(frame: Frame, selector: string): Promise<Box> {
  const found = await frame.locator(selector).boundingBox();
  if (found === null) throw new Error(`${selector} is not shown`);
  return found;
}

/** The device frame's island, a pseudo-element: 37 px high, 11 px from the top. */
async function islandBottom(page: Page): Promise<number> {
  const app = await box(page, "#product-frame");
  const scale = await page
    .locator("#product-frame")
    .evaluate(
      (node) =>
        node.getBoundingClientRect().width / (node as HTMLElement).offsetWidth,
    );
  return app.y + (11 + 37) * scale;
}

/** Everything a product's content must stay clear of, as drawn: the circles, the island and the Docklet. */
async function expectClear(page: Page, frame: Frame): Promise<void> {
  const top = await frameBox(frame, "#top");
  const minimize = await box(page, "#app-minimize");
  const more = await box(page, "#app-more");
  expect(top.y).toBeGreaterThanOrEqual(
    Math.max(bottomOf(minimize), bottomOf(more)) - 0.5,
  );
  if ((await page.getAttribute("#product-frame", "data-frame")) === "device")
    expect(top.y).toBeGreaterThanOrEqual((await islandBottom(page)) - 0.5);
  const bottom = await frameBox(frame, "#bottom");
  // The Pill rises above the bar, so both bound the product.
  const dock = await box(page, "#dock");
  const pill = await box(page, "#pill");
  expect(bottomOf(bottom)).toBeLessThanOrEqual(Math.min(dock.y, pill.y) + 0.5);
}

async function screenshot(page: Page, name: string): Promise<void> {
  const directory = process.env.APP_AREA_SCREENSHOTS;
  if (directory)
    await page.screenshot({ path: join(directory, `${name}.png`) });
}

describe("a product in front", () => {
  test(
    "starts below the circles and the island and ends above the Docklet in every frame, at full size and shrunk",
    async () => {
      for (const height of [1000, 640]) {
        const { context, page } = await open({ width: 1280, height });
        try {
          await choosePreview(page, "Phone 393 × 852");
          await closeTools(page);
          const frame = await openProduct(page);
          for (const kind of ["device", "round", "none"]) {
            await openTools(page, "preview");
            await page.click(`#dev-preview [data-frame="${kind}"]`);
            await closeTools(page);
            await page
              .locator(`#product-frame[data-frame="${kind}"]`)
              .waitFor({ state: "attached" });
            await expectClear(page, frame);
            await screenshot(page, `phone-${kind}-${height}`);
          }
          // The product is told of no inset of its own, so it adds none.
          expect(
            await frame
              .locator("#insets")
              .evaluate((node) => getComputedStyle(node).paddingTop),
          ).toBe("0px");
        } finally {
          await context.close();
        }
      }
    },
    TEST_MS,
  );

  test(
    "fills a phone-sized window between the circles and the Docklet, and the Preview tab says what it gets",
    async () => {
      const { context, page } = await open({
        width: 390,
        height: 844,
      });
      try {
        const frame = await openProduct(page);
        await expectClear(page, frame);
        expect(
          await frame.evaluate(() => [window.innerWidth, window.innerHeight]),
        ).toEqual([390, 844 - 68 - 84]);
        await openTools(page, "preview");
        expect(await page.locator("#viewport-app-area").textContent()).toBe(
          "Product frame 390 × 692 · the host keeps 68 px on top and 84 px below",
        );
        await closeTools(page);
        await screenshot(page, "phone-window");
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );

  test(
    "More opens its menu clear of the island, from the keyboard too, and Minimize keeps the product running",
    async () => {
      const { context, page } = await open({
        width: 1280,
        height: 1000,
      });
      try {
        await choosePreview(page, "Phone 393 × 852");
        await openTools(page, "preview");
        await page.click('#dev-preview [data-frame="device"]');
        await closeTools(page);
        const frame = await openProduct(page);
        const alive = await frame.evaluate(
          () => (window as unknown as { __alive: number }).__alive,
        );

        await page.focus("#app-more");
        await page.keyboard.press("Enter");
        await page.locator("#app-more-menu").waitFor();
        expect(
          await page.evaluate(() =>
            document.activeElement?.textContent?.trim(),
          ),
        ).toBe("Product details");
        const menu = await box(page, "#app-more-menu");
        const app = await box(page, "#product-frame");
        expect(menu.y).toBeGreaterThanOrEqual(await islandBottom(page));
        expect(menu.x + menu.width).toBeLessThanOrEqual(app.x + app.width);
        await screenshot(page, "more-menu");
        await page.keyboard.press("Escape");
        expect(await page.locator("#app-more-menu").isHidden()).toBe(true);
        expect(await page.evaluate(() => document.activeElement?.id)).toBe(
          "app-more",
        );

        await page.click("#app-minimize");
        await page.locator("#app-chrome").waitFor({ state: "hidden" });
        expect(
          await page.getAttribute("#product-frame", "data-app-chrome"),
        ).toBe("false");
        // Browse shows its landing; the Apps shelf brings the product back.
        expect(await page.getAttribute("body", "data-surface")).toBe("browse");
        await page.click("#apps-toggle");
        await page.locator('#apps-shelf [data-action="shelf-open"]').click();
        await page.locator("#app-chrome").waitFor();
        expect(
          await frame.evaluate(
            () => (window as unknown as { __alive: number }).__alive,
          ),
        ).toBe(alive);
        await expectClear(page, frame);
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );
});
