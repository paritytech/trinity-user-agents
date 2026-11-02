/**
 * The production build, served as a static site: Contact Chat's worker
 * archive is beside the page and checks against its CID, the dev server's
 * demos and controls are not in it, and a fresh wallet signed in to it runs
 * Contact Chat on the network delivery with nothing turned on. Every socket is
 * closed and every request outside the preview server is refused, so no chain
 * is reached and nothing is published. It needs a Chromium: playwright-core
 * finds the one it installs, or set CHROMIUM_PATH.
 */
import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { mkdtempSync, readFileSync, readdirSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import type { Browser, Page } from "playwright-core";
import { build, preview, type PreviewServer } from "vite";
import {
  CONTACT_CHAT_PATH,
  type ContactChatWorkerAnswer,
} from "../src/chat/contact-chat/route.js";
import { parseCidString, verifyBlock } from "../src/products/archive/cid.js";
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

/** Every file the build wrote, as text, but the WebAssembly and the archive's blocks. */
function builtText(): string {
  const texts: string[] = [];
  const walk = (dir: string) => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const path = join(dir, entry.name);
      if (entry.isDirectory()) walk(path);
      else if (!/\.(wasm|png|webp|woff2)$/.test(entry.name))
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

    const text = builtText();
    for (const absent of [
      "invitation-demo",
      "Invitation demo",
      "truapi-web-signing-host:dev:wallet-delivery",
      "truapi-web-signing-host:dev:reply-suite",
      "Add sample notifications",
      "alice.example",
      "__truapiNoSubmit",
      "__username-search",
    ])
      expect({ absent, found: text.includes(absent) }).toEqual({
        absent,
        found: false,
      });
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
          "Contact Chat's requests go on the network's day inbox: each post claims a notification slot.",
        );
        await workerRunning(page);
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
        }).toEqual({ delivery: 0, replyKey: 0, demo: 0 });
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

/** Contact Chat's worker, loaded from the build's own archive, once its script runs. */
async function workerRunning(page: Page): Promise<void> {
  const deadline = Date.now() + STEP_MS;
  while (Date.now() < deadline) {
    for (const frame of page.frames())
      if (
        frame.url().includes("__truapi-worker.html") &&
        (await frame
          .evaluate(
            () =>
              (
                window as unknown as {
                  __contactChatWorker?: { stage: string };
                }
              ).__contactChatWorker?.stage === "running",
          )
          .catch(() => false))
      )
        return;
    await page.waitForTimeout(250);
  }
  throw new Error("Contact Chat's worker did not run");
}
