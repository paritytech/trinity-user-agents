/**
 * A build made with Kettle Chat (`WEB_SIGNING_KETTLE_CHAT=1`), for a private
 * test deployment, served by Vite's preview server on loopback. It needs a
 * Chromium and Kettle's WebAssembly (`npm run build:kettle`); without the
 * WebAssembly it is skipped. The Browse seed is left out
 * (`WEB_SIGNING_SEED_BROWSE=0`), every socket is closed and every request
 * outside this host is refused, so nothing reaches a chain.
 *
 * What it checks: the build serves Kettle Chat's worker archive beside
 * Contact Chat's, holds the Chat tab and none of the dev server's demos,
 * fixtures or request controls, and a wallet signed in with Kettle Chat
 * alone runs Kettle's worker from the build's own archive, with no Contact
 * Chat worker.
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
import type { Browser } from "playwright-core";
import { build, preview, type PreviewServer } from "vite";
import type { BundledWorkerAnswer } from "../src/products/worker-manifest.js";
import {
  KETTLE_CHAT_PATH,
  KETTLE_WASM,
  KETTLE_WASM_PACKAGE,
} from "../apps/kettle-chat/route.js";
import { parseCidString, verifyBlock } from "../src/products/archive/cid.js";
import { SEED_ENV } from "../browse-seed-plugin.js";
import { KETTLE_CHAT_ENV } from "../contact-chat-plugin.js";
import {
  closeTools,
  freePort,
  openTools,
  reveal,
  testBrowser,
  useChatProducts,
} from "./support/fixtures.js";

const STEP_MS = 60_000;
const TEST_MS = 240_000;
const hostRoot = fileURLToPath(new URL("..", import.meta.url));
/** A synthetic phrase: no chain records, and no value on any network. */
const SYNTHETIC_MNEMONIC =
  "letter advice cage absurd amount doctor acoustic avoid letter advice cage above";

const built = existsSync(
  new URL(`../${KETTLE_WASM_PACKAGE}/${KETTLE_WASM}`, import.meta.url),
);

let outDir: string;
let server: PreviewServer;
let origin: string;
let browser: Browser;

beforeAll(async () => {
  if (!built) return;
  outDir = mkdtempSync(join(tmpdir(), "web-host-kettle-build-"));
  const saved = {
    NODE_ENV: process.env.NODE_ENV,
    [KETTLE_CHAT_ENV]: process.env[KETTLE_CHAT_ENV],
    [SEED_ENV]: process.env[SEED_ENV],
  };
  // bun test runs with NODE_ENV=test, which Vite would build as development.
  Object.assign(process.env, {
    NODE_ENV: "production",
    [KETTLE_CHAT_ENV]: "1",
    [SEED_ENV]: "0",
  });
  try {
    await build({
      root: hostRoot,
      logLevel: "error",
      build: { outDir, emptyOutDir: true },
    });
  } finally {
    for (const [key, value] of Object.entries(saved))
      if (value === undefined) delete process.env[key];
      else process.env[key] = value;
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
  if (!built) return;
  await new Promise<void>((resolve) =>
    server?.httpServer.close(() => resolve()),
  );
  rmSync(outDir, { recursive: true, force: true });
}, STEP_MS);

/** Every file the build wrote, as text, but the WebAssembly and the archives' blocks. */
function builtText(): string {
  const texts: string[] = [];
  const walk = (dir: string) => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const path = join(dir, entry.name);
      if (entry.isDirectory()) {
        if (entry.name !== "ipfs") walk(path);
      } else if (!/\.(wasm|png|webp|woff2)$/.test(entry.name))
        texts.push(readFileSync(path, "latin1"));
    }
  };
  walk(outDir);
  return texts.join("\n");
}

describe("a build made with Kettle Chat", () => {
  test.skipIf(!built)(
    "serves Kettle Chat's worker archive, and holds the Chat tab but none of the dev server's demos or controls",
    () => {
      const answer = JSON.parse(
        readFileSync(join(outDir, KETTLE_CHAT_PATH), "utf8"),
      ) as BundledWorkerAnswer;
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
      expect(text).toContain("chat-product-switch");
      for (const absent of [
        "invitation-demo",
        "Invitation demo",
        "__truapiDemoPosting",
        "__truapiBackupSettled",
        "__postingControl",
        "truapi-web-signing-host:dev:reply-suite",
        "Add sample notifications",
        "alice.example",
        "__truapiNoSubmit",
        "__username-search",
        "truapi-web-signing-host:local-statements:",
        "truapi-web-signing-host:dev:worker-probes",
      ])
        expect({ absent, found: text.includes(absent) }).toEqual({
          absent,
          found: false,
        });
      // Neither chat worker's archive holds its test probe.
      const archives = blocks
        .map((cid) => readFileSync(join(outDir, "ipfs", cid), "latin1"))
        .join("\n");
      for (const absent of [
        "__kettleChatWorker",
        "__novaChatWorker",
        "__contactChatWorker",
      ])
        expect({ absent, found: archives.includes(absent) }).toEqual({
          absent,
          found: false,
        });
    },
  );

  test.skipIf(!built)(
    "a wallet with Kettle Chat alone runs Kettle's worker from the build's archive, and no Contact Chat worker",
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
        const ready = () =>
          page
            .locator("#core-status")
            .filter({ hasText: "Core ready" })
            .waitFor({ state: "attached" });
        await page.goto(origin);
        await ready();
        expect(await page.locator('[data-dev-tab="chat"]').isVisible()).toBe(
          true,
        );
        await openTools(page, "more");
        expect(await page.locator("#wallet-delivery").count()).toBe(0);
        await useChatProducts(page, { contact: false, kettle: true });
        await ready();

        await openTools(page, "account");
        await reveal(page, "#import");
        await page
          .locator("#import")
          .evaluate((details) => ((details as HTMLDetailsElement).open = true));
        await page.fill("#mnemonic", SYNTHETIC_MNEMONIC);
        await page.click("#add-wallet");
        await page
          .locator("#session")
          .filter({ hasNotText: "Signed out" })
          .waitFor();
        await closeTools(page);

        // Kettle Chat says on its console, in the Workers view, which Kettle
        // it runs and that it runs; no Contact Chat worker is there.
        await openTools(page, "inspector");
        await page.click('[data-inspector-view="workers"]');
        const view = page.locator("#workers-inspector");
        await view.locator('.wi-row[data-worker^="kettle-chat."]').click();
        await view
          .locator(".wi-line", {
            hasText:
              "Kettle Chat runs Kettle 4d0764d526e5e32dfc611323e5fa522651f864e6",
          })
          .waitFor();
        await view
          .locator(".wi-line", { hasText: "Kettle Chat is running." })
          .waitFor();
        expect(
          await view.locator('.wi-row[data-worker^="contact-chat."]').count(),
        ).toBe(0);
        // A build's chat workers keep no test probe.
        const frames = page
          .frames()
          .filter((frame) => frame.url().includes("__truapi-worker.html"));
        expect(frames.length).toBeGreaterThan(0);
        for (const frame of frames)
          expect(
            await frame.evaluate(() =>
              ["__kettleChatWorker", "__contactChatWorker"].some(
                (key) => key in window,
              ),
            ),
          ).toBe(false);
        await openTools(page, "chat");
        await page
          .locator('[data-chat-product-state="kettle-chat"]', {
            hasText: "Running",
          })
          .waitFor();
        expect(outside).toEqual([]);
        expect(errors).toEqual([]);
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );
});
