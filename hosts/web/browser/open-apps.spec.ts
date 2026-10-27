/**
 * Headless-browser test of the products open in a tab: a product Browse
 * opens with TrUAPI's `navigateTo` joins the Apps shelf beside Browse's own
 * and keeps running until it is closed. Every product is a synthetic archive
 * served under a made-up name on a fake Asset Hub; every socket is closed and
 * every other request outside this host is refused, so no chain, backend or
 * peer is reached. It needs a Chromium: playwright-core finds the one it
 * installs, or set CHROMIUM_PATH.
 */
import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import type { Browser, BrowserContext, Frame, Page } from "playwright-core";
import { productLabel } from "../src/sandbox/mount.js";
import {
  TEST_MNEMONIC,
  type PublishedApp,
  type RunningHost,
  buildFixture,
  closeTools,
  openTools,
  publishApp,
  serveNames,
  startHost,
  testBrowser,
} from "./fixtures.js";

const STEP_MS = 60_000;
const TEST_MS = 180_000;
const LAUNCHER = "launcher.paseo";
const ONE = "one.paseo";
const TWO = "two.paseo";

let browser: Browser;
let host: RunningHost;
let app: PublishedApp;

beforeAll(async () => {
  host = await startHost();
  browser = await testBrowser();
  app = publishApp({
    "index.html":
      '<!doctype html><meta charset="utf-8"><title>launcher fixture</title><script type="module" src="/index.js"></script>',
    "index.js": await buildFixture("launcher-product.ts"),
  });
}, STEP_MS);

afterAll(async () => {
  await host?.close();
});

interface LauncherWindow {
  __launcher: { state: number; navigate(url: string): Promise<unknown> };
}

async function open(): Promise<{ context: BrowserContext; page: Page }> {
  const context = await browser.newContext({
    viewport: { width: 390, height: 844 },
  });
  context.setDefaultTimeout(STEP_MS);
  await context.routeWebSocket(/.*/, (socket) => socket.close());
  await serveNames(
    context,
    [LAUNCHER, ONE, TWO].map((name) => ({ record: `app.${name}`, app })),
  );
  await context.addInitScript((browse) => {
    if (!localStorage.getItem("truapi-web-signing-host:dock:v1"))
      localStorage.setItem(
        "truapi-web-signing-host:dock:v1",
        JSON.stringify({ browse }),
      );
  }, LAUNCHER);
  const page = await context.newPage();
  await page.goto(host.origin);
  await page
    .locator("#core-status")
    .filter({ hasText: "Core ready" })
    .waitFor({ state: "attached" });
  await openTools(page);
  await page
    .locator("#import")
    .evaluate((details) => ((details as HTMLDetailsElement).open = true));
  await page.fill("#mnemonic", TEST_MNEMONIC);
  await page.click("#add-wallet");
  await page.locator("#session").filter({ hasNotText: "Signed out" }).waitFor();
  await closeTools(page);
  return { context, page };
}

/** The running frame of `productId`, once its fixture is up. */
async function frameOf(page: Page, productId: string): Promise<Frame> {
  const label = `/${productLabel(productId)}/`;
  const deadline = Date.now() + STEP_MS;
  while (Date.now() < deadline) {
    for (const frame of page.frames()) {
      if (frame.isDetached() || !frame.url().includes(label)) continue;
      const ready = await frame
        .evaluate(() => "__launcher" in window)
        .catch(() => false);
      if (ready) return frame;
    }
    await page.waitForTimeout(100);
  }
  throw new Error(`${productId} did not start`);
}

const stateOf = (frame: Frame) =>
  frame.evaluate(() => (window as unknown as LauncherWindow).__launcher.state);

const navigate = (frame: Frame, url: string) =>
  frame.evaluate(
    (target) =>
      (window as unknown as LauncherWindow).__launcher.navigate(target),
    url,
  );

/** What the Apps shelf lists, in the order the apps were opened, with the one in front marked. */
async function shelf(page: Page): Promise<string[]> {
  await page.click("#apps-toggle");
  const apps = await page
    .locator("#apps-shelf [data-product-id]")
    .evaluateAll((items) =>
      items.map(
        (item) =>
          `${(item as HTMLElement).dataset.productId}${(item as HTMLElement).dataset.front === "true" ? " (front)" : ""}`,
      ),
    );
  await page.keyboard.press("Escape");
  return apps;
}

/** How many product frames the page holds. */
const frames = (page: Page) => page.locator("#product-slot iframe").count();

describe("open apps", () => {
  test(
    "an app Browse opens joins the shelf and runs until it is closed",
    async () => {
      const { context, page } = await open();
      try {
        await page.click('#dock [data-destination="browse"]');
        const launcher = await frameOf(page, LAUNCHER);
        const launcherState = await stateOf(launcher);

        // Browse opens another product: it comes in front, Browse's keeps running.
        expect(await navigate(launcher, ONE)).toEqual({ ok: true });
        const one = await frameOf(page, ONE);
        const oneState = await stateOf(one);
        expect(await shelf(page)).toEqual([LAUNCHER, `${ONE} (front)`]);
        expect(await page.locator("#apps-toggle").innerText()).toContain("2");
        // It runs behind, so it gets Minimize, as Browse's own does.
        expect(await page.locator("#app-minimize").isVisible()).toBe(true);
        expect(await page.locator("#app-close").isVisible()).toBe(false);

        // Back to Browse: the app stays open, and Browse's document is the same.
        await page.click('#dock [data-destination="browse"]');
        expect(await shelf(page)).toEqual([`${LAUNCHER} (front)`, ONE]);
        expect(await stateOf(launcher)).toBe(launcherState);

        // A third app keeps the other two.
        expect(await navigate(launcher, TWO)).toEqual({ ok: true });
        const two = await frameOf(page, TWO);
        expect(await shelf(page)).toEqual([LAUNCHER, ONE, `${TWO} (front)`]);
        expect(await frames(page)).toBe(3);

        // The shelf brings the first app back as it was, without a reload.
        await page.click("#apps-toggle");
        await page
          .locator(
            `#apps-shelf [data-product-id="${ONE}"] [data-action="shelf-open"]`,
          )
          .click();
        expect(await shelf(page)).toEqual([LAUNCHER, `${ONE} (front)`, TWO]);
        expect(await stateOf(one)).toBe(oneState);

        // Opening an app already open brings it in front; no second copy starts.
        await page.click("#apps-toggle");
        await page
          .locator(
            `#apps-shelf [data-product-id="${TWO}"] [data-action="shelf-open"]`,
          )
          .click();
        expect(await navigate(two, ONE)).toEqual({ ok: true });
        await page.waitForFunction(
          () => document.querySelectorAll("#product-slot iframe").length === 3,
        );
        expect(await shelf(page)).toEqual([LAUNCHER, `${ONE} (front)`, TWO]);
        expect(await stateOf(one)).toBe(oneState);

        // Minimize keeps it open; Close in More ends only it.
        await page.click("#app-minimize");
        await page.locator("#app-chrome").waitFor({ state: "hidden" });
        expect(await shelf(page)).toEqual([LAUNCHER, ONE, TWO]);
        await page.click("#apps-toggle");
        await page
          .locator(
            `#apps-shelf [data-product-id="${ONE}"] [data-action="shelf-open"]`,
          )
          .click();
        await page.click("#app-more");
        await page.click('#app-more-menu [data-app-action="close"]');
        await page.waitForFunction(
          () => document.querySelectorAll("#product-slot iframe").length === 2,
        );
        expect(await shelf(page)).toEqual([LAUNCHER, TWO]);
        expect(await stateOf(two)).toBeDefined();

        // Signing out ends every app.
        await openTools(page);
        await page.click("#sign-out");
        await page.locator("#session", { hasText: "Signed out" }).waitFor();
        await closeTools(page);
        await page.waitForFunction(
          () => document.querySelectorAll("#product-slot iframe").length === 0,
        );
        expect(await shelf(page)).toEqual([]);
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );
});
