/**
 * The production build, served as a static site: Contact Chat's worker
 * archive and the Browse seed are beside the page and check against their
 * CIDs, the dev server's demos and controls are not in it, and a fresh wallet
 * signed in to it runs Contact Chat on the network delivery with nothing
 * turned on. Every socket is closed and every request outside the preview
 * server is refused, so no chain is reached and nothing is published. It needs
 * a Chromium: playwright-core finds the one it installs, or set CHROMIUM_PATH.
 * The build itself fetches the Browse seed from Paseo Next v2, read-only; with
 * WEB_SIGNING_SEED_BROWSE=0 it builds offline and the test checks that no seed
 * was written.
 */
import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import {
  existsSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import type { Browser, Page } from "playwright-core";
import { build, preview, type PreviewServer } from "vite";
import {
  CONTACT_CHAT_PATH,
  type ContactChatWorkerAnswer,
} from "../src/chat/contact-chat/route.js";
import {
  cidToString,
  parseCidString,
  verifyBlock,
} from "../src/products/archive/cid.js";
import {
  ARCHIVE_LIMITS,
  loadArchive,
} from "../src/products/archive/gateway.js";
import { SEED_MANIFEST_PATH } from "../src/products/seed.js";
import { SEED_ENV, type SeedManifest } from "../browse-seed-plugin.js";
import { PRODUCT_LOAD } from "../src/products/load-observer.js";
import {
  closeTools,
  freePort,
  openTools,
  reveal,
  testBrowser,
} from "./support/fixtures.js";

const STEP_MS = 60_000;
const TEST_MS = 180_000;
const hostRoot = fileURLToPath(new URL("..", import.meta.url));
/** A synthetic phrase: no chain records, and no value on any network. */
const SYNTHETIC_MNEMONIC =
  "letter advice cage absurd amount doctor acoustic avoid letter advice cage above";

let outDir: string;
let server: PreviewServer;
let origin: string;
let browser: Browser;

beforeAll(async () => {
  outDir = mkdtempSync(join(tmpdir(), "web-host-build-"));
  // bun test runs with NODE_ENV=test, which Vite would build as development.
  const nodeEnv = process.env.NODE_ENV;
  process.env.NODE_ENV = "production";
  try {
    await build({
      root: hostRoot,
      logLevel: "error",
      build: { outDir, emptyOutDir: true },
    });
  } finally {
    process.env.NODE_ENV = nodeEnv;
  }
  const port = await freePort();
  server = await preview({
    root: hostRoot,
    logLevel: "error",
    build: { outDir },
    preview: { host: "127.0.0.1", port, strictPort: true },
  });
  origin = `http://localhost:${port}`;
  browser = await testBrowser();
}, TEST_MS);

afterAll(async () => {
  await new Promise<void>((resolve) =>
    server?.httpServer.close(() => resolve()),
  );
  rmSync(outDir, { recursive: true, force: true });
});

/**
 * Every file the build wrote, as text, but the WebAssembly, images, fonts and
 * the Browse seed's blocks, which are the Browse app's code, not this host's.
 */
function builtText(): string {
  const seeded = new Set(
    existsSync(join(outDir, SEED_MANIFEST_PATH))
      ? (
          JSON.parse(
            readFileSync(join(outDir, SEED_MANIFEST_PATH), "utf8"),
          ) as SeedManifest
        ).blocks
      : [],
  );
  const texts: string[] = [];
  const walk = (dir: string) => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const path = join(dir, entry.name);
      if (entry.isDirectory()) walk(path);
      else if (
        !/\.(wasm|png|webp|woff2)$/.test(entry.name) &&
        !seeded.has(entry.name)
      )
        texts.push(readFileSync(path, "latin1"));
    }
  };
  walk(outDir);
  return texts.join("\n");
}

describe("the production build", () => {
  test("serves Contact Chat's worker archive itself, and holds none of the dev server's demos or controls", () => {
    const answer = JSON.parse(
      readFileSync(join(outDir, CONTACT_CHAT_PATH), "utf8"),
    ) as ContactChatWorkerAnswer;
    expect(answer.kind).toBe("car");
    expect(JSON.parse(answer.manifest)).toMatchObject({
      kind: "worker",
      includes: { chat: true },
    });
    const blocks = readdirSync(join(outDir, "ipfs"));
    expect(blocks).toContain(answer.cid);
    for (const cid of blocks)
      expect({
        cid,
        verified: verifyBlock(
          parseCidString(cid),
          new Uint8Array(readFileSync(join(outDir, "ipfs", cid))),
        ),
      }).toEqual({ cid, verified: true });

    // The loader reports each load to the Products view in a build too.
    expect(
      readFileSync(join(outDir, "truapi-sandbox/page.js"), "utf8"),
    ).toContain(PRODUCT_LOAD);

    const text = builtText();
    for (const absent of [
      "invitation-demo",
      "Invitation demo",
      "truapi-web-signing-host:dev:wallet-delivery",
      "truapi-web-signing-host:dev:reply-suite",
      "truapi-web-signing-host:dev:contact-chat",
      "truapi-web-signing-host:dev:kettle-chat",
      "truapi-web-signing-host:dev:new-chats",
      "chat-product-switch",
      "Add sample notifications",
      "alice.example",
      "__truapiNoSubmit",
      "__truapiDemoPosting",
      "__truapiBackupSettled",
      "__postingControl",
      "__username-search",
      // The chat workers' test probe, and the tab's choice of one.
      "__contactChatWorker",
      "truapi-web-signing-host:dev:worker-probes",
    ])
      expect({ absent, found: text.includes(absent) }).toEqual({
        absent,
        found: false,
      });
  });

  test("ships the Browse seed's blocks, which load as the archive its CID names", async () => {
    const manifestPath = join(outDir, SEED_MANIFEST_PATH);
    if (process.env[SEED_ENV] === "0") {
      expect(existsSync(manifestPath)).toBe(false);
      return;
    }
    const manifest = JSON.parse(
      readFileSync(manifestPath, "utf8"),
    ) as SeedManifest;
    expect(manifest).toMatchObject({ record: "browse.paseo", kind: "car" });
    const read: string[] = [];
    const { files } = await loadArchive(
      manifest.cid,
      async (cid) => {
        const name = cidToString(cid);
        read.push(name);
        const data = new Uint8Array(readFileSync(join(outDir, "ipfs", name)));
        if (!verifyBlock(cid, data)) throw new Error(`${name} does not verify`);
        return data;
      },
      ARCHIVE_LIMITS,
    );
    expect(files.has("index.html")).toBe(true);
    expect(new Set(read)).toEqual(new Set(manifest.blocks));
    expect(builtText()).toContain(manifest.cid);
  });

  test(
    "runs a fresh wallet's Contact Chat on the network, with its Inbox, and no switch to turn it on",
    async () => {
      const context = await browser.newContext({
        viewport: { width: 1280, height: 860 },
      });
      context.setDefaultTimeout(STEP_MS);
      await context.routeWebSocket(/.*/, (socket) => socket.close());
      const outside: string[] = [];
      await context.route(/.*/, (route) => {
        const url = route.request().url();
        if (url.startsWith(origin)) return route.fallback();
        outside.push(url);
        return route.abort();
      });
      try {
        const page = await context.newPage();
        const errors: string[] = [];
        page.on("pageerror", (error) => errors.push(error.message));
        await page.goto(origin);
        await page
          .locator("#core-status")
          .filter({ hasText: "Core ready" })
          .waitFor({ state: "attached" });
        await signInWith(page, SYNTHETIC_MNEMONIC);

        await logged(
          page,
          "Host requests for Contact Chat: requests and replies go on the network's day inbox, and each post claims a notification slot.",
        );
        await workerRunning(page);
        // The Products view holds the loader's summary of Contact Chat's load.
        await openTools(page, "inspector");
        await page.click('[data-inspector-view="products"]');
        await page
          .locator(
            '#products-inspector .pi-row[data-outcome="fetched"], #products-inspector .pi-row[data-outcome="cached"]',
            { hasText: "served by this host" },
          )
          .waitFor();
        await closeTools(page);
        await page.click('#dock [data-destination="chat"]');
        expect(await page.locator('[aria-label="New chat"]').count()).toBe(1);
        expect(
          await page.locator('#chat-tabs [data-chat-tab="inbox"]').count(),
        ).toBe(1);

        await openTools(page, "more");
        expect({
          delivery: await page.locator("#wallet-delivery").count(),
          replyKey: await page.locator("#reply-key-suite").count(),
          demo: await page.locator("#invitation-demo").count(),
          chatTab: await page.locator('[data-dev-tab="chat"]').isHidden(),
          chatProducts: await page.locator("#chat-products").count(),
        }).toEqual({
          delivery: 0,
          replyKey: 0,
          demo: 0,
          chatTab: true,
          chatProducts: 0,
        });
        await closeTools(page);
        expect(errors).toEqual([]);
        expect(outside).toEqual([]);
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );
});

async function signInWith(page: Page, mnemonic: string): Promise<void> {
  await openTools(page, "account");
  await reveal(page, "#import");
  await page
    .locator("#import")
    .evaluate((details) => ((details as HTMLDetailsElement).open = true));
  await page.fill("#mnemonic", mnemonic);
  await page.click("#add-wallet");
  await page.locator("#session").filter({ hasNotText: "Signed out" }).waitFor();
  await closeTools(page);
}

/** Wait for the inspector's Activity view to hold a host row reading `text`. */
async function logged(page: Page, text: string): Promise<void> {
  await page
    .locator('#log .activity-row[data-category="host"]', { hasText: text })
    .waitFor({ state: "attached" });
}

/**
 * Contact Chat's worker, loaded from the build's own archive, once it says
 * on its console, in the Workers view, that it runs. Its frame keeps no
 * test probe.
 */
async function workerRunning(page: Page): Promise<void> {
  await openTools(page, "inspector");
  await page.click('[data-inspector-view="workers"]');
  const view = page.locator("#workers-inspector");
  await view.locator('.wi-row[data-worker^="contact-chat."]').click();
  await view
    .locator(".wi-line", { hasText: "Contact Chat is running." })
    .waitFor();
  await closeTools(page);
  const probes: boolean[] = [];
  for (const frame of page.frames())
    if (frame.url().includes("__truapi-worker.html"))
      probes.push(await frame.evaluate(() => "__contactChatWorker" in window));
  expect(probes.length).toBeGreaterThan(0);
  expect(probes).not.toContain(true);
}
