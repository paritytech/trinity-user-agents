/**
 * Headless-browser test of the menu's Workers section and the Developer
 * menu's Chat demo control, which replace the Workers page the Chat screen
 * had. It signs in with a synthetic phrase, closes every socket and refuses
 * every request outside this host, so no chain, backend or peer is reached.
 * The worker it runs is the dev server's synthetic chat product. It needs a
 * Chromium: playwright-core finds the one it installs, or set CHROMIUM_PATH.
 */
import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { join } from "node:path";
import type { Browser, BrowserContext, Page } from "playwright-core";
import {
  type RunningHost,
  startHost,
  testBrowser,
  closeTools,
  openTools,
  type DevTab,
} from "./support/fixtures.js";

const STEP_MS = 60_000;
const TEST_MS = 180_000;
/** A BIP-39 test vector phrase. It holds nothing. */
const PHRASE =
  "letter advice cage absurd amount doctor acoustic avoid letter advice cage above";

let browser: Browser;
let host: RunningHost;

beforeAll(async () => {
  host = await startHost();
  browser = await testBrowser();
}, STEP_MS);

afterAll(async () => {
  await host?.close();
});

async function open(viewport: { width: number; height: number }): Promise<{
  context: BrowserContext;
  page: Page;
  outside: string[];
}> {
  const context = await browser.newContext({ viewport });
  context.setDefaultTimeout(STEP_MS);
  await context.routeWebSocket(/.*/, (socket) => socket.close());
  const outside: string[] = [];
  await context.route(/.*/, (route) => {
    const url = route.request().url();
    if (url.startsWith(host.origin)) return route.fallback();
    outside.push(url);
    return route.abort();
  });
  const page = await context.newPage();
  await page.goto(host.origin);
  await ready(page);
  return { context, page, outside };
}

const ready = (page: Page) =>
  page
    .locator("#core-status")
    .filter({ hasText: "Core ready" })
    .waitFor({ state: "attached" });

async function setMenu(
  page: Page,
  open: boolean,
  tab: DevTab = "account",
): Promise<void> {
  if (open) await openTools(page, tab);
  else await closeTools(page);
}

async function signIn(page: Page): Promise<void> {
  await setMenu(page, true, "account");
  await page
    .locator("#import")
    .evaluate((details) => ((details as HTMLDetailsElement).open = true));
  await page.fill("#mnemonic", PHRASE);
  await page.click("#add-wallet");
  await page.locator("#session").filter({ hasNotText: "Signed out" }).waitFor();
}

const status = (page: Page) => page.locator("#worker-status").innerText();

/** Mark the worker frame, to tell a restarted one from the same one. */
const markWorkerFrame = (page: Page) =>
  page.evaluate(() => {
    const frames = document.querySelectorAll("#worker-frames iframe");
    frames.forEach((frame) => frame.setAttribute("data-marked", "yes"));
    return frames.length;
  });

const markedWorkerFrames = (page: Page) =>
  page.locator('#worker-frames iframe[data-marked="yes"]').count();

async function screenshot(page: Page, name: string): Promise<void> {
  const directory = process.env.WORKERS_SCREENSHOTS;
  if (directory)
    await page.screenshot({ path: join(directory, `${name}.png`) });
}

describe("workers in the menu", () => {
  test(
    "the menu runs workers and the chat demo; the Chat screen keeps only chats",
    async () => {
      const { context, page, outside } = await open({
        width: 1280,
        height: 860,
      });
      try {
        // Signed out: nothing to run, and nothing starts.
        await setMenu(page, true, "product");
        expect(await status(page)).toBe(
          "Sign in with a wallet to run workers.",
        );
        expect(await page.locator("#worker-name").isDisabled()).toBe(true);
        expect(
          await page
            .locator('#chat-demo [data-action="demo-start"]')
            .isDisabled(),
        ).toBe(true);
        // What the demo is lives in a folded note, not on the screen.
        expect(
          await page.locator("#chat-demo details").getAttribute("open"),
        ).toBeNull();

        await signIn(page);
        await setMenu(page, true, "product");
        await page
          .locator("#worker-status", { hasText: "No workers." })
          .waitFor();

        // The Chat screen has no Workers page and no demo panel.
        await setMenu(page, false);
        await page.click('#dock [data-destination="chat"]');
        expect(
          await page
            .locator(
              '#chat-screen [data-action="manage-workers"], #chat-screen [data-action^="demo-"]',
            )
            .count(),
        ).toBe(0);

        // A typed name survives closing and opening the menu; an empty one is refused.
        await setMenu(page, true, "product");
        await page.fill("#worker-name", "draft.paseo");
        await setMenu(page, false);
        await setMenu(page, true, "product");
        expect(await page.inputValue("#worker-name")).toBe("draft.paseo");
        await page.fill("#worker-name", " ");
        await page.press("#worker-name", "Enter");
        await page
          .locator('#worker-status[data-state="error"]', {
            hasText: "Enter a product name.",
          })
          .waitFor();
        expect(await page.locator("#worker-frames iframe").count()).toBe(0);

        // More → Chat demo → Start runs it as a worker the host added, listed
        // under Product → Workers.
        await setMenu(page, true, "more");
        await page.click('#chat-demo [data-action="demo-start"]');
        await setMenu(page, true, "product");
        const PRODUCT = `localhost:${new URL(host.origin).port}`;
        const row = page.locator(`#worker-list [data-worker="${PRODUCT}"]`);
        await page
          .locator(
            `#worker-list [data-worker="${PRODUCT}"][data-state="running"]`,
          )
          .waitFor();
        expect(await row.innerText()).toContain("added by this host");
        // The host's own worker can be inspected, but not removed.
        expect(
          await row
            .locator("button")
            .evaluateAll((buttons) =>
              buttons.map((button) => button.textContent),
            ),
        ).toEqual(["Inspect"]);
        await setMenu(page, true, "more");
        await page.locator('#chat-demo[data-demo="running"]').waitFor();
        expect(
          await page
            .locator('#chat-demo [data-action="demo-start"]')
            .isHidden(),
        ).toBe(true);
        await screenshot(page, "menu-workers-desktop");

        // Its rooms are in Chat; opening and closing the menu restarts nothing.
        await setMenu(page, false);
        await page
          .locator(`[data-conversation^="chat-v2:${PRODUCT}/"]`)
          .first()
          .waitFor();
        expect(await markWorkerFrame(page)).toBe(1);
        for (let round = 0; round < 3; round += 1) {
          await setMenu(page, true, "product");
          await setMenu(page, false);
        }
        expect(await markedWorkerFrames(page)).toBe(1);

        // Restart gives a new frame, and only one; Stop takes it and its rooms away.
        await setMenu(page, true, "more");
        await page.click('#chat-demo [data-action="demo-restart"]');
        await setMenu(page, true, "product");
        await page
          .locator(
            `#worker-list [data-worker="${PRODUCT}"][data-state="running"]`,
          )
          .waitFor();
        expect(await markedWorkerFrames(page)).toBe(0);
        expect(await page.locator("#worker-frames iframe").count()).toBe(1);
        await setMenu(page, true, "more");
        await page.click('#chat-demo [data-action="demo-stop"]');
        await row.waitFor({ state: "detached" });
        expect(await page.locator("#worker-list").isHidden()).toBe(true);
        await page
          .locator(`[data-conversation^="chat-v2:${PRODUCT}/"]`)
          .first()
          .waitFor({
            state: "detached",
          });
        expect(await page.locator("#worker-frames iframe").count()).toBe(0);
        // The refusal stays until the name is edited.
        expect(await status(page)).toBe("Enter a product name.");
        await setMenu(page, true, "product");
        await page.fill("#worker-name", "x");
        expect(await status(page)).toBe("No workers.");
        expect(outside).toEqual([]);
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );

  test(
    "on a phone the menu drawer holds them, Escape closes it, and a test user tab runs no workers",
    async () => {
      const { context, page, outside } = await open({
        width: 390,
        height: 844,
      });
      try {
        await signIn(page);
        await setMenu(page, true, "product");
        await page.locator("#worker-name").scrollIntoViewIfNeeded();
        await page.fill("#worker-name", "myapp.paseo");
        await screenshot(page, "menu-workers-phone");
        expect(
          await page.evaluate(
            () => document.documentElement.scrollWidth <= window.innerWidth,
          ),
        ).toBe(true);
        await setMenu(page, false);
        await setMenu(page, true, "product");
        expect(await page.inputValue("#worker-name")).toBe("myapp.paseo");

        // A test user has no wallet session, so no workers or chat demo.
        await setMenu(page, true, "more");
        await Promise.all([
          page.waitForEvent("load"),
          page.selectOption("#test-user", "alice"),
        ]);
        await ready(page);
        await setMenu(page, true, "product");
        expect(await status(page)).toBe(
          "Sign in with a wallet to run workers.",
        );
        expect(
          await page
            .locator('#chat-demo [data-action="demo-start"]')
            .isDisabled(),
        ).toBe(true);
        expect(await page.locator("#worker-frames iframe").count()).toBe(0);
        expect(outside).toEqual([]);
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );
});
