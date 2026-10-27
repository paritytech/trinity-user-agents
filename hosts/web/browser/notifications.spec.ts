/**
 * Headless-browser test of the Notifications screen, a prototype the dev
 * server offers with made-up requests, which have no owner to accept them.
 * It signs in with synthetic phrases,
 * closes every socket and refuses every request outside this host, so no
 * chain, backend or peer is reached. It needs a Chromium: playwright-core
 * finds the one it installs, or set CHROMIUM_PATH.
 */
import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { join } from "node:path";
import type { Browser, BrowserContext, Page } from "playwright-core";
import { type RunningHost, startHost, testBrowser } from "./fixtures.js";

const STEP_MS = 60_000;
const TEST_MS = 180_000;
/** BIP-39 test vector phrases. They hold nothing. */
const PHRASE_A =
  "letter advice cage absurd amount doctor acoustic avoid letter advice cage above";
const PHRASE_B =
  "legal winner thank year wave sausage worth useful legal winner thank yellow";

let browser: Browser;
let host: RunningHost;

beforeAll(async () => {
  host = await startHost();
  browser = await testBrowser();
}, STEP_MS);

afterAll(async () => {
  await host?.close();
});

async function open(
  viewport = { width: 1280, height: 800 },
  /** A product served on loopback, saved as what Browse opens. */
  browse?: string,
): Promise<{
  context: BrowserContext;
  page: Page;
  outside: string[];
}> {
  const context = await browser.newContext({ viewport });
  if (browse !== undefined)
    await context.addInitScript((address) => {
      if (!localStorage.getItem("truapi-web-signing-host:dock:v1"))
        localStorage.setItem(
          "truapi-web-signing-host:dock:v1",
          JSON.stringify({ browse: address }),
        );
    }, browse);
  context.setDefaultTimeout(STEP_MS);
  // From the first page on: a clock installed under a running light client breaks its timers.
  await context.clock.install();
  await context.routeWebSocket(/.*/, (socket) => socket.close());
  const outside: string[] = [];
  await context.route(/.*/, (route) => {
    const url = route.request().url();
    if (
      url.startsWith(host.origin) ||
      (browse !== undefined && url.startsWith(browse))
    )
      return route.fallback();
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

async function setMenu(page: Page, open: boolean): Promise<void> {
  if (((await page.getAttribute("body", "data-menu")) === "open") === open)
    return;
  // On a phone the open menu covers its toggle, and Escape closes it.
  const covered =
    !open &&
    (await page
      .click("#menu-toggle", { trial: true, timeout: 1000 })
      .then(() => false)
      .catch(() => true));
  if (covered) await page.keyboard.press("Escape");
  else await page.click("#menu-toggle");
  await page.waitForFunction(
    (want) => document.body.dataset.menu === want,
    open ? "open" : "closed",
  );
}

async function signIn(page: Page, phrase: string): Promise<void> {
  await setMenu(page, true);
  await page
    .locator("#import")
    .evaluate((details) => ((details as HTMLDetailsElement).open = true));
  await page.fill("#mnemonic", phrase);
  await page.click("#add-wallet");
  await page.locator("#session").filter({ hasNotText: "Signed out" }).waitFor();
  await setMenu(page, false);
}

/** Sample notifications come from the menu's Developer section. */
async function seed(page: Page): Promise<void> {
  await setMenu(page, true);
  await page.click('[data-action="notifications-demo-seed"]');
  await setMenu(page, false);
}

const tab = '#dock [data-destination="notifications"]';
const row = (id: string) => `[data-notification="${id}"].notification-row`;

/** The dock's Notifications entry. */
async function openNotifications(page: Page): Promise<void> {
  await page.click(tab);
}

/** The count on the dock's Notifications entry, whichever screen shows. */
const badgeCount = (page: Page): Promise<number> =>
  page
    .locator(`${tab} [data-badge]`)
    .evaluate((node) =>
      (node as HTMLElement).hidden ? 0 : Number(node.textContent),
    );

/** Read until `read` gives `want`, or fail with the last value. */
async function eventually<T>(read: () => Promise<T>, want: T): Promise<void> {
  const end = Date.now() + STEP_MS;
  let last: T | undefined;
  while (Date.now() < end) {
    last = await read();
    if (JSON.stringify(last) === JSON.stringify(want)) return;
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  expect(last).toEqual(want);
}

const seenIds = (page: Page, seen: boolean) =>
  page
    .locator(`.notification-row[data-seen="${seen}"]`)
    .evaluateAll((rows) =>
      rows.map((node) => (node as HTMLElement).dataset.notification),
    );

/** Pretend the page is in a background tab, or bring it back. */
const setPageVisible = (page: Page, visible: boolean) =>
  page.evaluate((shown) => {
    Object.defineProperty(document, "visibilityState", {
      configurable: true,
      get: () => (shown ? "visible" : "hidden"),
    });
    document.dispatchEvent(new Event("visibilitychange"));
  }, visible);

const groupIds = (page: Page, group: "pending" | "earlier") =>
  page
    .locator(`[data-group="${group}"] .notification-row`)
    .evaluateAll((rows) =>
      rows.map((node) => (node as HTMLElement).dataset.notification),
    );

async function screenshot(page: Page, name: string): Promise<void> {
  const directory = process.env.NOTIFICATIONS_SCREENSHOTS;
  if (directory)
    await page.screenshot({ path: join(directory, `${name}.png`) });
}

describe("the Notifications screen", () => {
  test(
    "marks a row seen only once it shows in a visible page, and Dismiss takes it off the list without an answer",
    async () => {
      const { context, page, outside } = await open({
        width: 1280,
        height: 600,
      });
      try {
        // Signed out: the tab is there, with nothing in it and no badge.
        await openNotifications(page);
        await page
          .locator("#notifications-screen", {
            hasText: "Sign in to see notifications",
          })
          .waitFor();
        expect(await badgeCount(page)).toBe(0);

        await signIn(page, PHRASE_A);
        expect(
          await page.evaluate(() =>
            Object.keys(localStorage).filter((key) =>
              key.includes(":notifications:"),
            ),
          ),
        ).toEqual([]);

        // Added while Chats is in front: four unseen, the badge counts them.
        await page.click('#dock [data-destination="chat"]');
        await seed(page);
        expect(await badgeCount(page)).toBe(4);

        // A page in a background tab marks nothing seen.
        await setPageVisible(page, false);
        await openNotifications(page);
        await page.locator(row("demo-invite-ada")).waitFor();
        await page.waitForTimeout(300);
        expect(await badgeCount(page)).toBe(4);

        // Once the page shows, only the rows on screen are seen.
        await setPageVisible(page, true);
        await eventually(async () => (await badgeCount(page)) < 4, true);
        const unseen = await seenIds(page, false);
        expect(unseen.length).toBeGreaterThan(0);
        expect(await badgeCount(page)).toBe(unseen.length);
        expect(await seenIds(page, true)).toContain("demo-invite-ada");
        await screenshot(page, "list");

        // A jump to the end skips the rows between: those stay unseen.
        await page
          .locator("#notifications-screen .chat-body")
          .evaluate((body) => (body.scrollTop = body.scrollHeight));
        await page.waitForTimeout(300);
        expect(await badgeCount(page)).toBeGreaterThan(0);
        // Scrolled into view one by one, the rest are seen too.
        for (const id of unseen)
          await page
            .locator(row(id!))
            .evaluate((node) => node.scrollIntoView({ block: "center" }));
        await eventually(() => badgeCount(page), 0);
        expect(await seenIds(page, false)).toEqual([]);

        // Seen is not answered: all four still wait.
        expect(await groupIds(page, "pending")).toEqual([
          "demo-invite-ada",
          "demo-share-grace",
          "demo-invite-soon",
          "demo-share-long",
        ]);
        // Made-up requests have no owner to accept them: Dismiss only, and no row opens.
        const ada = page.locator(row("demo-invite-ada"));
        expect(
          await ada
            .locator("[data-action]")
            .evaluateAll((nodes) =>
              nodes.map((node) => (node as HTMLElement).dataset.action),
            ),
        ).toEqual(["notification-dismiss"]);
        expect(
          await page.locator('[data-action="notification-open"]').count(),
        ).toBe(0);
        // The app screen carries no demo notes or developer controls.
        expect(
          await page
            .locator(
              "#notifications-screen [data-action^='notifications-demo']",
            )
            .count(),
        ).toBe(0);

        // Dismiss: off the list, kept in storage as it was, still waiting.
        await ada.locator('[data-action="notification-dismiss"]').click();
        await page
          .locator(row("demo-invite-ada"))
          .waitFor({ state: "detached" });
        const stored = await page.evaluate(() => {
          const key = Object.keys(localStorage).find((name) =>
            name.includes(":notifications:"),
          )!;
          return (
            JSON.parse(localStorage.getItem(key)!) as {
              id: string;
              state: { tag: string };
              dismissedAt?: number;
            }[]
          ).find((item) => item.id === "demo-invite-ada");
        });
        expect(stored?.state.tag).toBe("pending");
        expect(typeof stored?.dismissedAt).toBe("number");

        // A reload keeps what was seen and dismissed.
        await page.reload();
        await ready(page);
        await page
          .locator("#session")
          .filter({ hasNotText: "Signed out" })
          .waitFor();
        await openNotifications(page);
        await page.locator(row("demo-share-grace")).waitFor();
        expect(await page.locator(row("demo-invite-ada")).count()).toBe(0);
        expect(await badgeCount(page)).toBe(0);

        // Another wallet sees none of it; switching back shows it again.
        await signIn(page, PHRASE_B);
        await openNotifications(page);
        await page
          .locator("#notifications-screen", { hasText: "No notifications" })
          .waitFor();
        await signIn(page, PHRASE_A);
        await openNotifications(page);
        await page.locator(row("demo-share-grace")).waitFor();

        // Signing out takes the list off the screen and the badge off the tab.
        await setMenu(page, true);
        await page.click("#sign-out");
        await page
          .locator("#notifications-screen", {
            hasText: "Sign in to see notifications",
          })
          .waitFor();
        expect(await badgeCount(page)).toBe(0);
        expect(outside).toEqual([]);
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );

  test(
    "expires a request on time and offers no action after, and Clear takes only the demo's requests",
    async () => {
      const { context, page, outside } = await open();
      try {
        await signIn(page, PHRASE_A);
        await openNotifications(page);
        await seed(page);
        const soon = page.locator(row("demo-invite-soon"));
        await soon.waitFor();
        expect(await soon.innerText()).toContain("Expires in 2 min");

        // Two minutes on, it is expired, under Earlier, with no action left.
        await context.clock.fastForward("02:01");
        await eventually(() => soon.getAttribute("data-status"), "expired");
        expect(await groupIds(page, "earlier")).toContain("demo-invite-soon");
        expect(await soon.locator("[data-action]").count()).toBe(0);

        // Clear removes the demo's requests and the stored list with them.
        await setMenu(page, true);
        await page.click('[data-action="notifications-demo-clear"]');
        await setMenu(page, false);
        await page
          .locator("#notifications-screen", { hasText: "No notifications" })
          .waitFor();
        expect(await badgeCount(page)).toBe(0);
        expect(
          await page.evaluate(() =>
            Object.keys(localStorage).filter((key) =>
              key.includes(":notifications:"),
            ),
          ),
        ).toEqual([]);
        expect(outside).toEqual([]);
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );

  test(
    "fits a phone: four dock entries, long rows wrap, Dismiss is a full tap target, nothing scrolls sideways",
    async () => {
      const { context, page } = await open({ width: 360, height: 740 });
      try {
        await signIn(page, PHRASE_A);
        const dock = await page
          .locator("#dock button:not([hidden])")
          .evaluateAll((buttons) =>
            buttons.map(
              (button) => button.querySelector("span:last-child")?.textContent,
            ),
          );
        expect(dock).toEqual(["Chat", "Browse", "Pocket", "Notifications"]);
        await openNotifications(page);
        await seed(page);
        const long = page.locator(row("demo-share-long"));
        await long.waitFor();
        expect(await long.innerText()).toContain("Maximiliana");
        const sideways = () =>
          page.evaluate(() => {
            const body = document.querySelector<HTMLElement>(
              "#notifications-screen .chat-body",
            )!;
            return body.scrollWidth - body.clientWidth;
          });
        expect(await sideways()).toBeLessThanOrEqual(0);
        const dismiss = await page
          .locator(row("demo-invite-ada"))
          .locator('[data-action="notification-dismiss"]')
          .boundingBox();
        expect(dismiss!.x + dismiss!.width).toBeLessThanOrEqual(360);
        expect(dismiss!.height).toBeGreaterThanOrEqual(40);
        await screenshot(page, "phone-list");
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );
});
