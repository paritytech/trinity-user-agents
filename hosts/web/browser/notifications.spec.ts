/**
 * Headless-browser test of the Inbox, a prototype the dev server offers with
 * made-up requests, which have no owner to accept them. Chat's Inbox tab and
 * the Apps shelf's Inbox open it. It signs in with synthetic phrases,
 * closes every socket and refuses every request outside this host, so no
 * chain, backend or peer is reached. It needs a Chromium: playwright-core
 * finds the one it installs, or set CHROMIUM_PATH.
 */
import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { join } from "node:path";
import type { Browser, BrowserContext, Page } from "playwright-core";
import {
  type RunningHost,
  startHost,
  startUrlProduct,
  testBrowser,
  closeTools,
  openTools,
  previewFill,
  type DevTab,
} from "./fixtures.js";

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
  /** The preview size the tab starts on; a new tab's own is Phone. */
  preview: "phone" | "fill" = "phone",
): Promise<{
  context: BrowserContext;
  page: Page;
  outside: string[];
}> {
  const context = await browser.newContext({ viewport });
  if (preview === "fill") await previewFill(context);
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

async function setMenu(
  page: Page,
  open: boolean,
  tab: DevTab = "account",
): Promise<void> {
  if (open) await openTools(page, tab);
  else await closeTools(page);
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
  await setMenu(page, true, "more");
  await page.click('[data-action="notifications-demo-seed"]');
  await setMenu(page, false);
}

const inboxTab = '#chat-tabs [data-chat-tab="inbox"]';
const row = (id: string) => `[data-notification="${id}"].notification-row`;

/** Chat, at its Inbox tab. */
async function openInbox(page: Page): Promise<void> {
  await page.click('#dock [data-destination="chat"]');
  await page.click(inboxTab);
}

/** The Inbox's count, kept on Chat's Inbox tab whichever screen shows. */
const badgeCount = (page: Page): Promise<number> =>
  page
    .locator(`${inboxTab} [data-badge]`)
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

/**
 * Scroll the row `id` into the middle of the list, and wait until it is seen.
 * The screen draws its list again each time a row is marked seen, so the row
 * is looked up and scrolled in one step in the page: a node found earlier may
 * already be detached, and scrolling a detached node moves nothing.
 */
async function scrollUntilSeen(page: Page, id: string): Promise<void> {
  const selector = row(id);
  const end = Date.now() + STEP_MS;
  while (Date.now() < end) {
    const seen = await page.evaluate((selector) => {
      const node = document.querySelector<HTMLElement>(selector);
      if (node === null) return null;
      if (node.dataset.seen === "true") return true;
      node.scrollIntoView({ block: "center" });
      return false;
    }, selector);
    if (seen === true) return;
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  expect(await page.getAttribute(selector, "data-seen")).toBe("true");
}

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

describe("the Inbox", () => {
  test(
    "marks a row seen only once it shows in a visible page, and Dismiss takes it off the list without an answer",
    async () => {
      // At full scale the list is taller than its view, so a jump skips rows.
      const { context, page, outside } = await open(
        { width: 1280, height: 600 },
        undefined,
        "fill",
      );
      try {
        // Signed out: the tab is there, with nothing in it and no badge.
        await openInbox(page);
        await page
          .locator("#notifications-screen", {
            hasText: "Sign in to see your Inbox",
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
        await page.click('#chat-tabs [data-chat-tab="chats"]');
        await seed(page);
        expect(await badgeCount(page)).toBe(4);

        // A page in a background tab marks nothing seen.
        await setPageVisible(page, false);
        await openInbox(page);
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
        for (const id of unseen) await scrollUntilSeen(page, id!);
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
        await openInbox(page);
        await page.locator(row("demo-share-grace")).waitFor();
        expect(await page.locator(row("demo-invite-ada")).count()).toBe(0);
        expect(await badgeCount(page)).toBe(0);

        // Another wallet sees none of it; switching back shows it again.
        await signIn(page, PHRASE_B);
        await openInbox(page);
        await page
          .locator("#notifications-screen", { hasText: "Your Inbox is empty" })
          .waitFor();
        await signIn(page, PHRASE_A);
        await openInbox(page);
        await page.locator(row("demo-share-grace")).waitFor();

        // Signing out takes the list off the screen and the badge off the tab.
        await setMenu(page, true);
        await page.click("#sign-out");
        await page
          .locator("#notifications-screen", {
            hasText: "Sign in to see your Inbox",
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
    "Chat's Inbox tab and the Apps shelf open one Inbox, and every count reads the same items",
    async () => {
      const product = await startUrlProduct(host.origin);
      const { context, page, outside } = await open(
        { width: 1280, height: 800 },
        product.url,
      );
      const apps = page.locator("#apps-toggle");
      const shelf = page.locator("#apps-shelf");
      const shelfCount = (selector: string) =>
        shelf
          .locator(`${selector} [data-badge]`)
          .evaluate((node) =>
            (node as HTMLElement).hidden ? 0 : Number(node.textContent),
          );
      const appsOpen = () =>
        apps
          .locator("[data-badge]")
          .evaluate((node) =>
            (node as HTMLElement).hidden ? 0 : Number(node.textContent),
          );
      const inboxDot = () =>
        apps
          .locator("[data-inbox-dot]")
          .evaluate((node) => !(node as HTMLElement).hidden);
      try {
        await signIn(page, PHRASE_A);

        // Two samples are made to come through the product Browse opens,
        // whose id is its loopback address.
        const productId = new URL(product.url).host;
        await page.click('#dock [data-destination="chat"]');
        await seed(page);
        await page.evaluate((id) => {
          const key = Object.keys(localStorage).find((name) =>
            name.includes(":notifications:"),
          )!;
          const items = JSON.parse(localStorage.getItem(key)!) as {
            id: string;
            product: { id: string; name: string };
          }[];
          for (const item of items)
            if (item.id === "demo-invite-ada" || item.id === "demo-invite-soon")
              item.product = { id, name: "Loopback Product" };
          localStorage.setItem(key, JSON.stringify(items));
        }, productId);
        await page.reload();
        await ready(page);
        await page
          .locator("#session")
          .filter({ hasNotText: "Signed out" })
          .waitFor();

        // Browse runs it, then keeps it running behind Chat.
        await page.click('#dock [data-destination="browse"]');
        await page.locator("body[data-product='open']").waitFor();
        await page.click('#dock [data-destination="chat"]');
        expect(await badgeCount(page)).toBe(4);
        expect(await appsOpen()).toBe(1);
        expect(await inboxDot()).toBe(true);
        expect(await apps.getAttribute("aria-label")).toBe(
          "Apps, 1 open. Inbox has 4 new",
        );

        // J: the shelf counts the same items, in all and per product.
        await apps.click();
        await shelf.waitFor();
        expect(
          await page.evaluate(
            () =>
              (document.activeElement as HTMLElement | null)?.dataset.action,
          ),
        ).toBe("shelf-inbox");
        expect(await shelfCount('[data-action="shelf-inbox"]')).toBe(4);
        const app = shelf.locator(`[data-product-id="${productId}"]`);
        expect(await app.getAttribute("data-front")).toBe("false");
        expect(await shelfCount(`[data-product-id="${productId}"]`)).toBe(2);
        expect(
          await shelf.locator(".shelf-status").getAttribute("data-state"),
        ).toBe("ready");
        await screenshot(page, "apps-shelf");

        // Escape closes it and gives focus back to Apps.
        await page.keyboard.press("Escape");
        await shelf.waitFor({ state: "hidden" });
        expect(await page.evaluate(() => document.activeElement?.id)).toBe(
          "apps-toggle",
        );

        // From the keyboard, the shelf's Inbox is Chat at its Inbox tab.
        await page.keyboard.press("Enter");
        await shelf.waitFor();
        await page.keyboard.press("Enter");
        await page.locator("body[data-surface='inbox']").waitFor();
        expect(
          await page.getAttribute(
            '#dock [data-destination="chat"]',
            "aria-current",
          ),
        ).toBe("true");
        expect(await page.getAttribute(inboxTab, "aria-selected")).toBe("true");
        expect(
          await page.evaluate(
            () =>
              (document.activeElement as HTMLElement | null)?.dataset.chatTab,
          ),
        ).toBe("inbox");
        await screenshot(page, "inbox-from-shelf");

        // Seen in the Inbox: the tab, the dot, the shelf and the product
        // badge clear together. Apps still counts the open product. Rows
        // below the fold are seen once scrolled into view.
        for (const id of await seenIds(page, false))
          await scrollUntilSeen(page, id!);
        await eventually(() => badgeCount(page), 0);
        expect(await inboxDot()).toBe(false);
        expect(await appsOpen()).toBe(1);
        await apps.click();
        expect(await shelfCount('[data-action="shelf-inbox"]')).toBe(0);
        expect(await shelfCount(`[data-product-id="${productId}"]`)).toBe(0);
        await page.keyboard.press("Escape");

        // H: the arrow keys move between Chats and the Inbox in place.
        await page.focus(inboxTab);
        await page.keyboard.press("ArrowLeft");
        await page.locator("body[data-surface='chat']").waitFor();
        expect(await page.locator("#chat-screen #chat-tabs").count()).toBe(1);
        await page.keyboard.press("ArrowRight");
        await page.locator("body[data-surface='inbox']").waitFor();

        // Dismissed in the Inbox, gone from the list the shelf opens.
        await page
          .locator(row("demo-invite-ada"))
          .locator('[data-action="notification-dismiss"]')
          .click();
        await page
          .locator(row("demo-invite-ada"))
          .waitFor({ state: "detached" });
        await apps.click();
        await shelf.locator('[data-action="shelf-inbox"]').click();
        await page.locator(row("demo-share-grace")).waitFor();
        expect(await page.locator(row("demo-invite-ada")).count()).toBe(0);

        // The shelf brings the product back in front, and closes it.
        await apps.click();
        await app.locator('[data-action="shelf-open"]').click();
        await page.locator("body[data-product='open']").waitFor();
        expect(
          await page.getAttribute(
            '#dock [data-destination="browse"]',
            "aria-current",
          ),
        ).toBe("true");

        // In front, Browse's product has Minimize and More, not Close.
        expect(await page.locator("#app-minimize").isVisible()).toBe(true);
        expect(await page.locator("#app-close").isVisible()).toBe(false);
        await page.click("#app-more");
        expect(
          await page.locator("#app-more-menu [role=menuitem]").allInnerTexts(),
        ).toEqual(["Product details", "Close"]);
        await page.keyboard.press("Escape");
        expect(await page.evaluate(() => document.activeElement?.id)).toBe(
          "app-more",
        );
        // Minimize goes back to the last host screen; the product keeps running.
        await page.click("#app-minimize");
        await page.locator("body[data-surface='inbox']").waitFor();
        expect(await appsOpen()).toBe(1);
        expect(await page.locator("#app-chrome").isHidden()).toBe(true);
        await apps.click();
        expect(await app.getAttribute("data-front")).toBe("false");
        await app.locator('[data-action="shelf-open"]').click();
        await page.locator("body[data-product='open']").waitFor();

        await apps.click();
        expect(await app.getAttribute("data-front")).toBe("true");
        await app.locator('[data-action="shelf-close"]').click();
        await shelf
          .locator(".shelf-empty", { hasText: "No open apps" })
          .waitFor();
        expect(await appsOpen()).toBe(0);
        await page.keyboard.press("Escape");

        // Another wallet has its own Inbox: nothing counted, on any entry.
        await signIn(page, PHRASE_B);
        expect(await badgeCount(page)).toBe(0);
        expect(await inboxDot()).toBe(false);
        expect(outside).toEqual([]);
      } finally {
        await context.close();
        product.close();
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
        await openInbox(page);
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
        await setMenu(page, true, "more");
        await page.click('[data-action="notifications-demo-clear"]');
        await setMenu(page, false);
        await page
          .locator("#notifications-screen", { hasText: "Your Inbox is empty" })
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
    "fits a phone: the Docklet's entries, long rows wrap, Dismiss is a full tap target, nothing scrolls sideways",
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
        expect(dock).toEqual(["Chat", "Pocket", "Open", "Browse", "Apps"]);
        await openInbox(page);
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
