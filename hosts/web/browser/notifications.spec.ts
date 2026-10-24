/**
 * Headless-browser test of the Notifications screen, a prototype the dev
 * server offers with made-up requests. It signs in with synthetic phrases,
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

async function open(viewport = { width: 1280, height: 800 }): Promise<{
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

const tab = '#dock [data-destination="notifications"]';
const badge = `${tab} [data-badge]`;
const row = (id: string) => `[data-notification="${id}"].notification-row`;

async function badgeCount(page: Page): Promise<number> {
  return (await page.locator(badge).isHidden())
    ? 0
    : Number(await page.locator(badge).innerText());
}

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
    "shows demo requests only when asked, marks one seen when opened, and keeps its answer",
    async () => {
      const { context, page, outside } = await open();
      try {
        // Signed out: the tab is there, with nothing in it and no badge.
        await page.click(tab);
        await page
          .locator("#notifications-screen", {
            hasText: "Sign in to see notifications",
          })
          .waitFor();
        expect(await badgeCount(page)).toBe(0);

        await signIn(page, PHRASE_A);
        await page.click(tab);
        await page
          .locator("#notifications-screen", { hasText: "No notifications" })
          .waitFor();
        // Nothing is stored for a wallet that has not asked for the demo.
        expect(
          await page.evaluate(() =>
            Object.keys(localStorage).filter((key) =>
              key.includes(":notifications:"),
            ),
          ),
        ).toEqual([]);

        await page.click('[data-action="notifications-demo-seed"]');
        await page.locator(row("demo-invite-ada")).waitFor();
        expect(await groupIds(page, "pending")).toEqual([
          "demo-invite-ada",
          "demo-share-grace",
          "demo-invite-soon",
          "demo-share-long",
        ]);
        expect(await groupIds(page, "earlier")).toEqual([
          "demo-invite-accepted",
          "demo-share-declined",
          "demo-invite-expired",
        ]);
        expect(
          await page.locator(".notification-demo").first().innerText(),
        ).toContain("Demo data, made up by this host");
        // The list shows them; it does not mark any seen.
        expect(await badgeCount(page)).toBe(4);
        await screenshot(page, "list");

        // Opening one marks that one seen, and only that one.
        await page.click(row("demo-invite-ada"));
        const detail = page.locator(".notification-detail");
        await detail.waitFor();
        expect(await page.locator(".notification-heading").innerText()).toBe(
          "Chat invitation",
        );
        const text = await detail.innerText();
        expect(text).toContain("Ada");
        expect(text).toContain("ada.example");
        expect(text).toContain("Example Chat (chat.example)");
        expect(text).toContain("Ada wants to start a chat with you.");
        expect(text).toContain("no permission or contact is created");
        expect(await badgeCount(page)).toBe(3);
        expect(await detail.getAttribute("data-status")).toBe("pending");
        await screenshot(page, "detail");

        // Seen is not answered: back on the list it still waits, without its dot.
        await page.click('[data-action="notifications-back"]');
        await page.locator(row("demo-invite-ada")).waitFor();
        expect(
          await page.locator(row("demo-invite-ada")).getAttribute("data-seen"),
        ).toBe("true");
        expect(await groupIds(page, "pending")).toContain("demo-invite-ada");
        // Focus comes back to the row the detail was opened from.
        expect(
          await page.evaluate(
            () =>
              (document.activeElement as HTMLElement | null)?.dataset
                .notification,
          ),
        ).toBe("demo-invite-ada");

        // Accepting ends it once; it moves to Earlier, and its detail offers no answer again.
        await page.click(row("demo-invite-ada"));
        await page.click('[data-action="notification-accept"]');
        await detail
          .locator(".notification-status", { hasText: "Accepted" })
          .waitFor();
        expect(
          await page.locator('[data-action="notification-accept"]').count(),
        ).toBe(0);
        expect(
          await page.locator('[data-action="notification-decline"]').count(),
        ).toBe(0);
        expect(await badgeCount(page)).toBe(3);
        await page.click('[data-action="notifications-back"]');
        expect((await groupIds(page, "earlier"))[0]).toBe("demo-invite-ada");

        // Declining another, opened from the keyboard.
        await page.locator(row("demo-share-grace")).focus();
        await page.keyboard.press("Enter");
        await detail.waitFor();
        await page.click('[data-action="notification-decline"]');
        await detail
          .locator(".notification-status", { hasText: "Declined" })
          .waitFor();
        await page.click('[data-action="notifications-back"]');
        expect(await groupIds(page, "pending")).toEqual([
          "demo-invite-soon",
          "demo-share-long",
        ]);
        expect(await badgeCount(page)).toBe(2);

        // A reload keeps the waiting requests and the history, with what was seen.
        await page.reload();
        await ready(page);
        // The tab signs its wallet back in by itself after a reload.
        await page
          .locator("#session")
          .filter({ hasNotText: "Signed out" })
          .waitFor();
        await page.click(tab);
        await page.locator(row("demo-invite-ada")).waitFor();
        expect(await groupIds(page, "pending")).toEqual([
          "demo-invite-soon",
          "demo-share-long",
        ]);
        expect(await groupIds(page, "earlier")).toEqual([
          "demo-invite-ada",
          "demo-share-grace",
          "demo-invite-accepted",
          "demo-share-declined",
          "demo-invite-expired",
        ]);
        expect(await badgeCount(page)).toBe(2);

        // Another wallet sees none of it; switching back shows it again.
        await signIn(page, PHRASE_B);
        await page.click(tab);
        await page
          .locator("#notifications-screen", { hasText: "No notifications" })
          .waitFor();
        expect(await badgeCount(page)).toBe(0);
        await signIn(page, PHRASE_A);
        await page.click(tab);
        await page.locator(row("demo-invite-soon")).waitFor();
        expect(await badgeCount(page)).toBe(2);

        // Signing out takes the list off the screen and the badge off the tab.
        await setMenu(page, true);
        await page.click("#sign-out");
        await page
          .locator("#notifications-screen", {
            hasText: "Sign in to see notifications",
          })
          .waitFor();
        expect(await badgeCount(page)).toBe(0);

        // Nothing here reached outside this host.
        expect(outside).toEqual([]);
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );

  test(
    "expires a request on time and offers no answer after, and Clear takes only the demo's requests",
    async () => {
      const { context, page, outside } = await open();
      try {
        await page.clock.install();
        await signIn(page, PHRASE_A);
        await page.click(tab);
        await page.click('[data-action="notifications-demo-seed"]');
        await page.click(row("demo-invite-soon"));
        const detail = page.locator(".notification-detail");
        await detail.locator('[data-action="notification-accept"]').waitFor();
        expect(await detail.innerText()).toContain("in 2 min");

        // Two minutes on, the open detail draws it as expired, with no answer to give.
        await page.clock.fastForward("02:01");
        await detail
          .locator(".notification-status", { hasText: "Expired" })
          .waitFor();
        expect(
          await page.locator('[data-action="notification-accept"]').count(),
        ).toBe(0);
        await page.click('[data-action="notifications-back"]');
        expect(await groupIds(page, "earlier")).toContain("demo-invite-soon");
        expect(
          await page
            .locator(row("demo-invite-soon"))
            .getAttribute("data-status"),
        ).toBe("expired");

        // Clear removes the demo's requests and the stored list with them.
        await page.click('[data-action="notifications-demo-clear"]');
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
    "fits a phone: four dock entries, long names and purposes wrap, nothing scrolls sideways",
    async () => {
      const { context, page } = await open({ width: 360, height: 740 });
      try {
        await signIn(page, PHRASE_A);
        const dock = await page
          .locator("#dock button:not([hidden])")
          .evaluateAll((buttons) =>
            buttons.map((button) => {
              const box = button.getBoundingClientRect();
              return {
                width: Math.round(box.width),
                text: button.querySelector("span:last-child")?.textContent,
              };
            }),
          );
        expect(dock.map((entry) => entry.text)).toEqual([
          "Chat",
          "Browse",
          "Pocket",
          "Notifications",
        ]);
        expect(
          await page.evaluate(() => {
            const label = [
              ...document.querySelectorAll("#dock button span"),
            ].find((node) => node.textContent === "Notifications")!;
            return label.scrollWidth <= label.clientWidth + 1;
          }),
        ).toBe(true);
        // The new icon has its own size before the stylesheet applies.
        expect(
          await page
            .locator(`${tab} svg`)
            .evaluate((svg) => [
              svg.getAttribute("width"),
              svg.getAttribute("height"),
            ]),
        ).toEqual(["22", "22"]);

        await page.click(tab);
        await page.click('[data-action="notifications-demo-seed"]');
        await page.locator(row("demo-share-long")).waitFor();
        const sideways = () =>
          page.evaluate(() => {
            const body = document.querySelector<HTMLElement>(
              "#notifications-screen .chat-body",
            )!;
            return body.scrollWidth - body.clientWidth;
          });
        expect(await sideways()).toBeLessThanOrEqual(0);
        await screenshot(page, "phone-list");

        await page.click(row("demo-share-long"));
        const detail = page.locator(".notification-detail");
        await detail.waitFor();
        expect(await detail.innerText()).toContain(
          "maximiliana-konstantinopoulou-abernathy.example",
        );
        expect(await sideways()).toBeLessThanOrEqual(0);
        const accept = await page
          .locator('[data-action="notification-accept"]')
          .boundingBox();
        expect(accept!.x + accept!.width).toBeLessThanOrEqual(360);
        expect(accept!.height).toBeGreaterThanOrEqual(44);
        await screenshot(page, "phone-detail");
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );
});
