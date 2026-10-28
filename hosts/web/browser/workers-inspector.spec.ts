/**
 * Headless-browser test of the inspector's Workers view, with two synthetic
 * product workers run by the ordinary loader from a stood-in Asset Hub and
 * gateway. Every request outside this host is refused and every socket
 * closed, so no chain is reached. It needs a Chromium: playwright-core finds
 * the one it installs, or set CHROMIUM_PATH.
 */
import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import {
  type Browser,
  type BrowserContext,
  type Frame,
  type Page,
} from "playwright-core";
import {
  TEST_MNEMONIC,
  buildFixture,
  openTools,
  publishApp,
  serveNames,
  startHost,
  testBrowser,
  type RunningHost,
} from "./support/fixtures.js";

const STEP_MS = 60_000;
const TEST_MS = 180_000;
const A = "a.paseo";
const B = "b.paseo";
/** A BIP-39 test vector phrase. It holds nothing. */
const OTHER_MNEMONIC =
  "letter advice cage absurd amount doctor acoustic avoid letter advice cage above";
const MANIFEST = JSON.stringify({
  $v: 1,
  kind: "worker",
  appVersion: [0, 1, 0],
  entrypoint: "index.js",
  includes: { chat: true },
});

let browser: Browser;
let host: RunningHost;

beforeAll(async () => {
  host = await startHost();
  browser = await testBrowser();
}, STEP_MS);

afterAll(async () => {
  await host?.close();
});

async function offlineContext(): Promise<BrowserContext> {
  const context = await browser.newContext({
    viewport: { width: 1280, height: 900 },
  });
  context.setDefaultTimeout(STEP_MS);
  await context.grantPermissions(["clipboard-read", "clipboard-write"]);
  await context.routeWebSocket(/.*/, (socket) => socket.close());
  // Registered first, so the stood-in names below take precedence.
  await context.route(
    (url) => !url.href.startsWith(host.origin),
    (route) => route.abort(),
  );
  await serveNames(context, [
    {
      record: `worker.${A}`,
      app: publishApp({ "index.js": await buildFixture("console-worker.ts") }),
      text: { executable: MANIFEST },
    },
    {
      record: `worker.${B}`,
      app: publishApp({
        "index.js": await buildFixture("console-spoof-worker.ts"),
      }),
      text: { executable: MANIFEST },
    },
  ]);
  return context;
}

async function openHost(context: BrowserContext): Promise<Page> {
  const page = await context.newPage();
  await page.goto(host.origin);
  await page
    .locator("#core-status")
    .filter({ hasText: "Core ready" })
    .waitFor({ state: "attached" });
  return page;
}

async function signInWith(page: Page, mnemonic: string): Promise<void> {
  await openTools(page, "account");
  await page
    .locator("#import")
    .evaluate((details) => ((details as HTMLDetailsElement).open = true));
  await page.fill("#mnemonic", mnemonic);
  await page.click("#add-wallet");
  await page.locator("#session").filter({ hasNotText: "Signed out" }).waitFor();
}

async function addWorker(page: Page, name: string): Promise<void> {
  await openTools(page, "product");
  await page.fill("#worker-name", name);
  await page.press("#worker-name", "Enter");
  await page
    .locator(`#worker-list [data-worker="${name}"][data-state="running"]`)
    .waitFor();
}

const view = (page: Page) => page.locator("#workers-inspector");

/** The selected worker's console lines, as their text reads. */
const lines = (page: Page) =>
  view(page)
    .locator(".wi-line")
    .evaluateAll((items) =>
      items.map((item) => (item as HTMLElement).innerText.replace(/\s+/g, " ")),
    );

async function select(page: Page, name: string): Promise<void> {
  await view(page).locator(`.wi-row[data-worker="${name}"]`).click();
  await view(page).locator(".wi-name", { hasText: name }).waitFor();
}

/** The worker frame whose document set `key` on its window. */
async function workerFrame(page: Page, key: string): Promise<Frame> {
  for (;;) {
    for (const frame of page.frames())
      if (
        frame.url().includes("__truapi-worker.html") &&
        (await frame.evaluate((key) => key in window, key).catch(() => false))
      )
        return frame;
    await page.waitForTimeout(100);
  }
}

describe("the Workers view", () => {
  test(
    "shows each worker's state and its own console from the first line, bounded, and leaves the worker as it was",
    async () => {
      const context = await offlineContext();
      try {
        const page = await openHost(context);
        const pageConsole: string[] = [];
        page.on("console", (message) => pageConsole.push(message.text()));
        await signInWith(page, TEST_MNEMONIC);
        await addWorker(page, A);
        await addWorker(page, B);

        // The menu's Inspect opens the Inspector at this worker.
        await page.click(`#worker-list [data-worker="${A}"] >> text=Inspect`);
        await page
          .locator('[data-inspector-view="workers"][aria-selected="true"]')
          .waitFor();
        await view(page).locator(".wi-name", { hasText: A }).waitFor();
        expect(
          await view(page).locator(".wi-badge").getAttribute("data-state"),
        ).toBe("running");
        const facts = await view(page).locator(".wi-facts").innerText();
        expect(facts).toContain(`worker.${A}`);
        expect(facts).toContain("Added by");
        expect(facts).toContain("Up for");

        await view(page)
          .locator(".wi-line", { hasText: "not forwarded" })
          .waitFor();
        await view(page)
          .locator(".wi-line", { hasText: "uncaught later" })
          .waitFor();
        const shown = await lines(page);
        // The first statement's line is there, so the capture ran before it.
        expect(shown[0]).toContain("log worker booting {step: 1}");
        expect(shown).toContainEqual(
          expect.stringContaining(
            'warn circular {name: "loop", self: [Circular]}',
          ),
        );
        expect(shown).toContainEqual(
          expect.stringContaining("info binary Uint8Array(4096 bytes)"),
        );
        expect(shown).toContainEqual(
          expect.stringContaining("Error: first failure"),
        );
        expect(shown).toContainEqual(
          expect.stringContaining("rejection Error: nobody waits"),
        );
        expect(shown).toContainEqual(
          expect.stringContaining("uncaught Error: uncaught later"),
        );
        // A flood is cut at 50 lines a second and the rest counted.
        expect(shown.filter((line) => line.includes("flood")).length).toBe(45);
        expect(shown).toContainEqual(
          expect.stringContaining(
            "75 console lines not forwarded: more than 50 a second.",
          ),
        );
        // The worker's own console still prints, its calls return as before,
        // and its code went on past them to reach the core.
        expect(
          pageConsole.some((text) => text.startsWith("worker booting")),
        ).toBe(true);
        expect(
          await (
            await workerFrame(page, "__consoleWorker")
          ).evaluate(
            () =>
              (window as unknown as { __consoleWorker: unknown })
                .__consoleWorker,
          ),
        ).toEqual({ returned: true, hasClient: true });

        // A line is put down to the frame that posted it, whatever it claims.
        expect(shown.join("\n")).not.toContain("spoof from b");
        await select(page, B);
        await view(page).locator(".wi-line", { hasText: "b is up" }).waitFor();
        expect((await lines(page)).join("\n")).toContain("spoof from b");

        // Filters show only warnings and errors.
        await select(page, A);
        await view(page).locator('.wi-chip:text-is("Errors")').click();
        const errors = await lines(page);
        expect(errors.length).toBe(3);
        expect(
          errors.every((line) => /error|uncaught|rejection/.test(line)),
        ).toBe(true);
        await view(page).locator('.wi-chip:text-is("All")').click();
        expect(
          await view(page).locator(`.wi-row[data-worker="${A}"]`).innerText(),
        ).toContain("3 errors");

        // Copy writes the lines shown, each with its run.
        await view(page).locator('.wi-chip:text-is("Copy")').click();
        const copied = await page.evaluate(() =>
          navigator.clipboard.readText(),
        );
        expect(copied.split("\n")[0]).toMatch(
          /^\d\d:\d\d:\d\d\.\d{3} run \d+ log worker booting \{step: 1\}$/,
        );
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );

  test(
    "narrows the TrUAPI view to the worker, restarts it with its earlier lines labelled, and forgets a removed worker and an ended session",
    async () => {
      const context = await offlineContext();
      try {
        const page = await openHost(context);
        await signInWith(page, TEST_MNEMONIC);
        await addWorker(page, A);
        await addWorker(page, B);
        await page.click(`#worker-list [data-worker="${A}"] >> text=Inspect`);
        await view(page)
          .locator(".wi-line", { hasText: "worker booting" })
          .waitFor();

        // Traces opens the TrUAPI view on this worker's channel only.
        await page.waitForFunction(
          () =>
            document.querySelectorAll("#inspector-mount .ins-chan").length >= 3,
        );
        await view(page).locator('.wi-action:text-is("Traces")').click();
        await page
          .locator('[data-inspector-view="wire"][aria-selected="true"]')
          .waitFor();
        await page
          .locator(
            `#inspector-mount .ins-chan.active[data-channel="${A} (worker)"]`,
          )
          .waitFor();
        const rows = await page
          .locator("#inspector-mount .td-op")
          .evaluateAll((ops) =>
            ops.map((op) => (op as HTMLElement).dataset.channelId),
          );
        expect(rows.length).toBeGreaterThan(0);
        expect(rows.every((channel) => channel === `${A} (worker)`)).toBe(true);

        // Restart starts a new frame; the first run's lines stay, labelled.
        await page.click('[data-inspector-view="workers"]');
        await select(page, A);
        const restart = view(page).locator('.wi-action:text-is("Restart")');
        expect(await restart.isVisible()).toBe(true);
        const firstRun = await view(page)
          .locator(".wi-line")
          .first()
          .getAttribute("data-run");
        await restart.click();
        await view(page)
          .locator(`.wi-line[data-new-run="true"]`)
          .first()
          .waitFor();
        const runs = await view(page)
          .locator(".wi-line")
          .evaluateAll((items) => [
            ...new Set(items.map((item) => (item as HTMLElement).dataset.run)),
          ]);
        expect(runs[0]).toBe(firstRun!);
        expect(runs.length).toBe(2);
        expect(await page.locator("#worker-frames iframe").count()).toBe(2);

        // A removed worker leaves the view with its lines.
        await openTools(page, "product");
        await page.click(`#worker-list [data-worker="${B}"] >> text=Remove`);
        await openTools(page, "inspector");
        await view(page)
          .locator(`.wi-row[data-worker="${B}"]`)
          .waitFor({ state: "detached" });

        // Signing out ends the session's workers and forgets their lines.
        await openTools(page, "account");
        await page.click("#sign-out");
        await page.waitForFunction(
          () => document.querySelectorAll("#worker-frames iframe").length === 0,
        );
        await openTools(page, "inspector");
        await view(page).locator(".wi-list-pane .wi-empty").waitFor();
        expect(await view(page).locator(".wi-row").count()).toBe(0);

        // Another wallet sees none of it.
        await signInWith(page, OTHER_MNEMONIC);
        await openTools(page, "inspector");
        expect(await view(page).locator(".wi-row").count()).toBe(0);
        expect(await view(page).locator(".wi-line").count()).toBe(0);

        // Back on the first wallet, its worker starts again with a new run
        // and none of the last session's lines.
        await signInWith(page, TEST_MNEMONIC);
        await openTools(page, "inspector");
        await page.click('[data-inspector-view="workers"]');
        await select(page, A);
        await view(page)
          .locator(".wi-line", { hasText: "worker booting" })
          .waitFor();
        const after = await view(page)
          .locator(".wi-line")
          .evaluateAll((items) => [
            ...new Set(items.map((item) => (item as HTMLElement).dataset.run)),
          ]);
        expect(after.length).toBe(1);
        expect(runs).not.toContain(after[0]);
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );
});
