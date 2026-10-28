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
import { productLabel } from "../src/products/loader/mount.js";
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
  openCatalog,
  showAddress,
} from "./support/fixtures.js";

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
    if (!localStorage.getItem("truapi-web-signing-host:dock:v2"))
      localStorage.setItem(
        "truapi-web-signing-host:dock:v2",
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
    "a tab starts in Chat; Browse's landing opens Browse apps or a recent app once, and Minimize and Close come back to it",
    async () => {
      const { context, page } = await open();
      const launcherLabel = `/${productLabel(LAUNCHER)}/`;
      const launcherRequests: string[] = [];
      page.on("request", (request) => {
        if (request.url().includes(launcherLabel))
          launcherRequests.push(request.url());
      });
      // Names of this network read without their suffix, as the search field shows them.
      const shown = (name: string) => name.replace(/\.paseo$/, "");
      const surface = () => page.getAttribute("body", "data-surface");
      const landing = page.locator("#browse-screen");
      const browseApps = landing.locator('[data-action="open-catalog"]');
      const recent = () =>
        landing
          .locator('[data-action="open-recent"]')
          .evaluateAll((rows) =>
            rows.map((row) => (row as HTMLElement).dataset.address),
          );
      try {
        expect(await surface()).toBe("chat");

        // Browse shows its landing: Browse apps and no history, no field of
        // its own, and nothing loaded, the second time too.
        await page.click('#dock [data-destination="browse"]');
        await landing.waitFor();
        expect(await surface()).toBe("browse");
        expect(await browseApps.innerText()).toBe(
          `B\nBrowse apps\n${shown(LAUNCHER)}`,
        );
        expect(
          await landing.locator("input, [data-action=browse-search]").count(),
        ).toBe(0);
        expect(await recent()).toEqual([]);
        await page.click('#dock [data-destination="browse"]');
        await page.waitForTimeout(300);
        expect(await frames(page)).toBe(0);
        expect(launcherRequests).toEqual([]);
        const shots = process.env.BROWSE_SCREENSHOTS;
        if (shots)
          await page.screenshot({ path: `${shots}/browse-landing.png` });

        // Browse apps opens the product once, in Browse.
        await browseApps.click();
        const launcher = await frameOf(page, LAUNCHER);
        expect(await surface()).toBe("product");
        expect(await frames(page)).toBe(1);
        const state = await stateOf(launcher);

        // Minimize: the landing again, the app still running in the shelf.
        await page.click("#app-minimize");
        await landing.waitFor();
        expect(await surface()).toBe("browse");
        expect(await frames(page)).toBe(1);
        expect(await shelf(page)).toEqual([LAUNCHER]);
        expect(await recent()).toEqual([shown(LAUNCHER)]);
        // Choosing Browse again keeps the landing.
        await page.click('#dock [data-destination="browse"]');
        await page.waitForTimeout(300);
        expect(await surface()).toBe("browse");

        // The shelf brings the same frame back, in Browse.
        await page.click("#apps-toggle");
        await page
          .locator(
            `#apps-shelf [data-product-id="${LAUNCHER}"] [data-action="shelf-open"]`,
          )
          .click();
        await page.locator("#app-chrome").waitFor();
        expect(await frames(page)).toBe(1);
        expect(await stateOf(launcher)).toBe(state);
        expect(
          await page.getAttribute(
            '#dock [data-destination="browse"]',
            "aria-current",
          ),
        ).toBe("true");

        // Minimize again; Browse apps and its Recent row bring the same frame
        // back, never a second one.
        await page.click("#app-minimize");
        await landing.waitFor();
        await browseApps.click();
        await page.locator("#app-chrome").waitFor();
        expect(await frames(page)).toBe(1);
        expect(await stateOf(launcher)).toBe(state);
        await page.click("#app-minimize");
        await landing.waitFor();
        await landing
          .locator(
            `[data-action="open-recent"][data-address="${shown(LAUNCHER)}"]`,
          )
          .click();
        await page.locator("#app-chrome").waitFor();
        expect(await frames(page)).toBe(1);
        expect(await stateOf(launcher)).toBe(state);

        // Close ends it: the landing, and an empty shelf.
        await page.click("#app-more");
        await page.click('#app-more-menu [data-app-action="close"]');
        await landing.waitFor();
        expect(await frames(page)).toBe(0);
        expect(await shelf(page)).toEqual([]);

        // A product found by the Docklet's search opens in Browse and joins
        // Recent; Browse apps still opens Browse's own.
        await showAddress(page);
        await page.fill("#address", ONE);
        await page.press("#address", "Enter");
        await frameOf(page, ONE);
        await page.click("#app-more");
        await page.click('#app-more-menu [data-app-action="close"]');
        await landing.waitFor();
        expect(await recent()).toEqual([shown(ONE), shown(LAUNCHER)]);
        expect(await browseApps.innerText()).toContain(shown(LAUNCHER));

        // On a phone it fits, with nothing scrolling sideways.
        const box = await landing.boundingBox();
        expect(box!.x + box!.width).toBeLessThanOrEqual(390);
        expect(
          await page.evaluate(
            () => document.documentElement.scrollWidth - window.innerWidth,
          ),
        ).toBeLessThanOrEqual(0);

        // A network with no Browse app of its own offers none.
        await openTools(page);
        await Promise.all([
          page.waitForEvent("load"),
          page.selectOption("#network", "previewnet"),
        ]);
        await page
          .locator("#core-status")
          .filter({ hasText: "Core ready" })
          .waitFor({ state: "attached" });
        await closeTools(page);
        await page.click('#dock [data-destination="browse"]');
        await landing.waitFor();
        expect(await browseApps.count()).toBe(0);
        expect(await landing.innerText()).toContain("No recent apps");
        expect(await frames(page)).toBe(0);
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );

  test(
    "Browse apps opens the network's Browse app, not the product earlier builds saved for Browse",
    async () => {
      const context = await browser.newContext({
        viewport: { width: 390, height: 844 },
      });
      context.setDefaultTimeout(STEP_MS);
      await context.routeWebSocket(/.*/, (socket) => socket.close());
      await serveNames(
        context,
        ["browse.paseo", "doom.paseo"].map((name) => ({
          record: `app.${name}`,
          app,
        })),
      );
      // What an earlier build left: Doom saved as Browse's product, and in
      // the signed-out history.
      await context.addInitScript(() => {
        if (localStorage.getItem("truapi-web-signing-host:dock:v1")) return;
        localStorage.setItem(
          "truapi-web-signing-host:dock:v1",
          JSON.stringify({ browse: "doom.paseo" }),
        );
        localStorage.setItem(
          "truapi-web-signing-host:recents:v1:signed-out",
          JSON.stringify([
            {
              address: "doom.paseo",
              productId: "doom.paseo",
              entered: false,
              at: Date.now() - 60_000,
            },
          ]),
        );
      });
      try {
        const page = await context.newPage();
        await page.goto(host.origin);
        await page
          .locator("#core-status")
          .filter({ hasText: "Core ready" })
          .waitFor({ state: "attached" });
        await page.click('#dock [data-destination="browse"]');
        const landing = page.locator("#browse-screen");
        const browseApps = landing.locator('[data-action="open-catalog"]');
        expect(await browseApps.innerText()).toBe("B\nBrowse apps\nbrowse");
        expect(
          await landing
            .locator('[data-action="open-recent"]')
            .evaluateAll((rows) =>
              rows.map((row) => (row as HTMLElement).dataset.address),
            ),
        ).toEqual(["doom"]);

        await browseApps.click();
        await frameOf(page, "browse.paseo");
        expect(await frames(page)).toBe(1);
        // The old entry is left as it was.
        expect(
          await page.evaluate(() =>
            localStorage.getItem("truapi-web-signing-host:dock:v1"),
          ),
        ).toBe('{"browse":"doom.paseo"}');
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );

  test(
    "an app Browse opens joins the shelf and runs until it is closed",
    async () => {
      const { context, page } = await open();
      try {
        await openCatalog(page);
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
