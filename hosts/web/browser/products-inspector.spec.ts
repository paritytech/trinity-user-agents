/**
 * Headless-browser test of the inspector's Products view: a synthetic product
 * opened by a made-up name, through the ordinary loader, from a stood-in Asset
 * Hub and gateway, and Contact Chat's worker, served by the host itself. Every
 * other request outside this host is refused and every socket closed, so no
 * chain or gateway is reached. It needs a Chromium: playwright-core finds the
 * one it installs, or set CHROMIUM_PATH. Set PRODUCTS_SCREENSHOTS to a folder
 * to keep screenshots of the view.
 */
import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import type { Browser, Frame, Page } from "playwright-core";
import { productLabel } from "../src/products/loader/mount.js";
import { PRODUCT_LOAD } from "../src/products/load-observer.js";
import { size } from "../tools/products/view.js";
import {
  TEST_MNEMONIC,
  closeTools,
  openTools,
  publishApp,
  serveNames,
  showAddress,
  startHost,
  testBrowser,
  type RunningHost,
} from "./support/fixtures.js";

const STEP_MS = 60_000;
/** A BIP-39 test vector phrase. It holds nothing, and Contact Chat runs for it. */
const SYNTHETIC_MNEMONIC =
  "letter advice cage absurd amount doctor acoustic avoid letter advice cage above";
const TEST_MS = 180_000;
const NAME = "one.paseo";
const FILES = {
  "index.html":
    '<!doctype html><meta charset="utf-8"><title>one</title><script src="/app.js"></script>',
  // Lines that differ, so the archive's chunks are not shared and its size is the files'.
  "app.js": `window.__one = true;\n${Array.from(
    { length: 1500 },
    (_, line) => `// ${line} ${(line * 7919) % 104_729}`,
  ).join("\n")}`,
};
const FILE_BYTES = Object.values(FILES).reduce(
  (sum, text) => sum + new TextEncoder().encode(text).length,
  0,
);

let browser: Browser;
let host: RunningHost;

beforeAll(async () => {
  host = await startHost();
  browser = await testBrowser();
}, STEP_MS);

afterAll(async () => {
  await host?.close();
});

/** The running frame of {@link NAME}, once its script ran. */
async function productFrame(page: Page): Promise<Frame> {
  const label = `/${productLabel(NAME)}/`;
  const deadline = Date.now() + STEP_MS;
  while (Date.now() < deadline) {
    for (const frame of page.frames()) {
      if (frame.isDetached() || !frame.url().includes(label)) continue;
      if (await frame.evaluate(() => "__one" in window).catch(() => false))
        return frame;
    }
    await page.waitForTimeout(100);
  }
  throw new Error(`${NAME} did not start`);
}

async function openByName(page: Page): Promise<Frame> {
  await closeTools(page);
  await showAddress(page);
  await page.fill("#address", NAME);
  await page.press("#address", "Enter");
  return productFrame(page);
}

const view = (page: Page) => page.locator("#products-inspector");
const row = (page: Page, product: string) =>
  view(page).locator(`.pi-row[data-product="${product}"]`);

/** A row's cells, by column. */
async function cells(page: Page, product: string) {
  return row(page, product).evaluate((node) =>
    Object.fromEntries(
      [...node.querySelectorAll<HTMLElement>(".si-cell[data-label]")].map(
        (cell) => [
          cell.dataset.label,
          cell.textContent!.replace(cell.dataset.label!, "").trim(),
        ],
      ),
    ),
  );
}

async function showProducts(page: Page): Promise<void> {
  await openTools(page, "inspector");
  await page.click('[data-inspector-view="products"]');
  await view(page).waitFor();
}

describe("the Products view", () => {
  test(
    "shows what the loader fetched for a product and for Contact Chat, keeps the loader's summary, and a reopen from the verified copy",
    async () => {
      const context = await browser.newContext({
        viewport: { width: 1280, height: 900 },
      });
      context.setDefaultTimeout(STEP_MS);
      await context.routeWebSocket(/.*/, (socket) => socket.close());
      await context.route(
        (url) => !url.href.startsWith(host.origin),
        (route) => route.abort(),
      );
      const app = publishApp(FILES);
      await serveNames(context, [{ record: `app.${NAME}`, app }]);
      // CAR requests to the host's own origin, which has no CAR to send.
      const carToHost: string[] = [];
      context.on("request", (request) => {
        const url = new URL(request.url());
        if (
          url.origin === host.origin &&
          url.searchParams.get("format") === "car"
        )
          carToHost.push(request.url());
      });
      const page = await context.newPage();
      const shots = process.env.PRODUCTS_SCREENSHOTS;
      try {
        await page.goto(host.origin);
        await page
          .locator("#core-status")
          .filter({ hasText: "Core ready" })
          .waitFor({ state: "attached" });
        await openTools(page, "account");
        await page
          .locator("#import")
          .evaluate((details) => ((details as HTMLDetailsElement).open = true));
        await page.fill("#mnemonic", SYNTHETIC_MNEMONIC);
        await page.click("#add-wallet");
        await page
          .locator("#session")
          .filter({ hasNotText: "Signed out" })
          .waitFor();

        const product = await openByName(page);
        // The product, on this origin, posts a summary of its own: the host
        // keeps the loader's, the first its frame sent.
        await product.evaluate(
          (type) =>
            window.parent.postMessage(
              { type, summary: { outcome: "failed", totalMs: 1 } },
              location.origin,
            ),
          PRODUCT_LOAD,
        );
        await showProducts(page);
        await row(page, NAME)
          .and(page.locator('[data-outcome="fetched"]'))
          .waitFor();
        expect(await cells(page, NAME)).toMatchObject({
          Size: size(FILE_BYTES),
          Files: "2",
          Transport: "CAR",
          Outcome: "fetched",
        });
        const blocks = Number((await cells(page, NAME)).Arrived);
        expect(blocks).toBeGreaterThan(1);

        // Contact Chat's worker is loaded from the host itself, and says so.
        const chat = view(page).locator(".pi-row", {
          hasText: "served by this host",
        });
        await chat
          .and(page.locator(":not([data-outcome=waiting])"))
          .first()
          .waitFor();
        expect(await chat.count()).toBe(1);
        // A fresh context has no verified copy, so its blocks were fetched,
        // block by block: the host's own origin is not asked for a CAR.
        const chatName = (await chat.getAttribute("data-product"))!;
        expect(await cells(page, chatName)).toMatchObject({
          Transport: "raw · host",
          Outcome: "fetched",
        });
        await chat.click();
        expect(await view(page).locator(".si-detail").innerText()).toContain(
          "not tried (host origin)",
        );
        await chat.click();
        expect(carToHost).toEqual([]);

        // A row opened from the keyboard keeps the focus through the redraw.
        await row(page, NAME).focus();
        await page.keyboard.press("Enter");
        const detail = view(page).locator(".si-detail");
        await detail.waitFor();
        expect(
          await page.evaluate(
            () =>
              (document.activeElement as HTMLElement | null)?.dataset.product,
          ),
        ).toBe(NAME);
        const text = await detail.innerText();
        for (const expected of [
          `app.${NAME}`,
          app.contentCid,
          "archive (one CAR file)",
          "Ended",
          "complete",
          "not fired",
          `${blocks} blocks`,
          `${FILE_BYTES} B`,
          "app.js",
        ])
          expect({ expected, found: text.includes(expected) }).toEqual({
            expected,
            found: true,
          });
        if (shots) {
          await page.screenshot({ path: `${shots}/products-desktop.png` });
          // At the bottom the dock is wide enough for the table with its header.
          await page.click('.devtools-side [data-dev-side="bottom"]');
          await page.waitForTimeout(300);
          await page.screenshot({ path: `${shots}/products-wide.png` });
          await page.click('.devtools-side [data-dev-side="right"]');
        }

        // The same mount again: the verified copy, nothing fetched, the same files.
        await closeTools(page);
        await page.click("#app-more");
        await page.click('#app-more-menu [data-app-action="close"]');
        await openByName(page);
        await showProducts(page);
        await row(page, NAME)
          .and(page.locator('[data-outcome="cached"]'))
          .waitFor();
        expect(await cells(page, NAME)).toMatchObject({
          Size: size(FILE_BYTES),
          Files: "2",
          Arrived: "–",
          Transport: "cache",
          Outcome: "cached",
        });

        // The tab's product comes back after a reload, which opens it at boot,
        // and when another wallet signs in, which opens it again around the
        // session change, and its row is there after both. Here the session
        // starts before the product opens both times; the unit tests of the
        // log cover the other order.
        await page.reload();
        await page
          .locator("#core-status")
          .filter({ hasText: "Core ready" })
          .waitFor({ state: "attached" });
        await productFrame(page);
        await showProducts(page);
        await row(page, NAME)
          .and(
            page.locator('[data-outcome="fetched"], [data-outcome="cached"]'),
          )
          .waitFor();
        await openTools(page, "account");
        await page
          .locator("#import")
          .evaluate((details) => ((details as HTMLDetailsElement).open = true));
        await page.fill("#mnemonic", TEST_MNEMONIC);
        await page.click("#add-wallet");
        await productFrame(page);
        await showProducts(page);
        await row(page, NAME)
          .and(page.locator('[data-outcome="cached"]'))
          .waitFor();
        // Nothing later clears it.
        await page.waitForTimeout(1000);
        expect(await row(page, NAME).count()).toBe(1);
        expect(await row(page, NAME).getAttribute("data-outcome")).toBe(
          "cached",
        );

        // On a phone the rows fit the dock, and the inspector's tabs scroll
        // to Products.
        await page.setViewportSize({ width: 390, height: 844 });
        await page.waitForTimeout(200);
        const overflow = await page.evaluate(() => {
          const tabs = document.querySelector(".inspector-views")!;
          const panel = document.getElementById("products-inspector")!;
          return {
            tabs: getComputedStyle(tabs).overflowX,
            panel: panel.scrollWidth - panel.clientWidth,
            page: document.documentElement.scrollWidth - window.innerWidth,
          };
        });
        expect(overflow).toEqual({ tabs: "auto", panel: 0, page: 0 });
        await page.click('[data-inspector-view="activity"]');
        await page.click('[data-inspector-view="products"]');
        await row(page, NAME).waitFor();
        if (shots)
          await page.screenshot({ path: `${shots}/products-phone.png` });
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );
});
