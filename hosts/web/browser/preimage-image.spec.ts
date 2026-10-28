/**
 * Headless-browser test of preimage images: a product looks a preimage up
 * with TrUAPI, as Browse looks up an app's icon, and the host reads it from
 * the network's Bulletin gateway. The product is a synthetic archive served
 * under a made-up name on a fake Asset Hub, the gateway is stood in, every
 * socket is closed and every other request outside this host is refused, so
 * no chain, backend or peer is reached. It needs a Chromium: playwright-core
 * finds the one it installs, or set CHROMIUM_PATH.
 */
import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { blake2b } from "@noble/hashes/blake2.js";
import type { Browser, Frame, Page } from "playwright-core";
import { PASEO_DOTNS, bytesToHex } from "../src/products/dotns.js";
import { preimageCid } from "../src/products/preimage.js";
import { productLabel } from "../src/products/loader/mount.js";
import {
  type PublishedApp,
  type RunningHost,
  buildFixture,
  publishApp,
  serveNames,
  startHost,
  testBrowser,
  openCatalog,
} from "./support/fixtures.js";

const STEP_MS = 60_000;
const TEST_MS = 120_000;
const SHOW_MS = 20_000;
const PRODUCT = "icons.paseo";
/** A 3 by 2 pixel PNG, made for this test. */
const ICON = Buffer.from(
  "iVBORw0KGgoAAAANSUhEUgAAAAMAAAACCAIAAAASFvFNAAAAEElEQVR4nGN4xlAFQQxwFgBVHAhBX13DgwAAAABJRU5ErkJggg==",
  "base64",
);
const ICON_KEY = blake2b(ICON, { dkLen: 32 });

let browser: Browser;
let host: RunningHost;
let app: PublishedApp;

beforeAll(async () => {
  host = await startHost();
  browser = await testBrowser();
  app = publishApp({
    "index.html":
      '<!doctype html><meta charset="utf-8"><title>icons fixture</title><script type="module" src="/index.js"></script>',
    "index.js": await buildFixture("preimage-product.ts"),
  });
}, STEP_MS);

afterAll(async () => {
  await host?.close();
});

interface IconWindow {
  __icon: { show(key: string): Promise<unknown> };
}

/** The running frame of the product, once its fixture is up. */
async function productFrame(page: Page): Promise<Frame> {
  const label = `/${productLabel(PRODUCT)}/`;
  const deadline = Date.now() + STEP_MS;
  while (Date.now() < deadline) {
    for (const frame of page.frames()) {
      if (frame.isDetached() || !frame.url().includes(label)) continue;
      const ready = await frame
        .evaluate(() => "__icon" in window)
        .catch(() => false);
      if (ready) return frame;
    }
    await page.waitForTimeout(100);
  }
  throw new Error(`${PRODUCT} did not start`);
}

describe("preimage images", () => {
  test(
    "an image a product looks up by its preimage key is read from the Bulletin gateway",
    async () => {
      const context = await browser.newContext();
      context.setDefaultTimeout(STEP_MS);
      try {
        await context.routeWebSocket(/.*/, (socket) => socket.close());
        await serveNames(context, [{ record: `app.${PRODUCT}`, app }]);
        const asked: string[] = [];
        await context.route(
          `${PASEO_DOTNS.contentGateway}/ipfs/${preimageCid(ICON_KEY)}`,
          (route) => {
            asked.push(route.request().url());
            return route.fulfill({
              status: 200,
              headers: {
                "access-control-allow-origin": "*",
                "content-type": "image/png",
              },
              body: ICON,
            });
          },
        );
        await context.addInitScript((browse) => {
          localStorage.setItem(
            "truapi-web-signing-host:dock:v2",
            JSON.stringify({ browse }),
          );
        }, PRODUCT);
        const page = await context.newPage();
        await page.goto(host.origin);
        await openCatalog(page);
        const frame = await productFrame(page);

        // A miss leaves the product waiting, as Browse waits with a grey tile.
        const shown = await Promise.race([
          frame.evaluate(
            (key) => (window as unknown as IconWindow).__icon.show(key),
            `0x${bytesToHex(ICON_KEY)}`,
          ),
          new Promise((resolve) =>
            setTimeout(() => resolve({ error: "no image" }), SHOW_MS),
          ),
        ]);
        expect({ shown, asked }).toEqual({
          shown: { width: 3, height: 2 },
          asked: [
            `${PASEO_DOTNS.contentGateway}/ipfs/${preimageCid(ICON_KEY)}`,
          ],
        });
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );
});
