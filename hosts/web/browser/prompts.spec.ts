/**
 * Headless-browser test of the host's prompts: drawn as the app's bottom
 * sheets over the app's rectangle only, at the app's scale, with the
 * dialog's focus, Escape and scrim answers. A test user tab gives the core a
 * session from a public test phrase; a fixture product asks for a contact and
 * for the camera permission, which the test denies. Every socket is closed
 * and every request outside this host and the fixture is refused. It needs a
 * Chromium: playwright-core finds the one it installs, or set CHROMIUM_PATH.
 */
import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { createServer, type Server } from "node:http";
import { join } from "node:path";
import type { Browser, BrowserContext, Frame, Page } from "playwright-core";
import {
  buildFixture,
  freePort,
  type RunningHost,
  choosePreview,
  showAddress,
  startHost,
  testBrowser,
  closeTools,
  openTools,
  type DevTab,
  useInvitationDemo,
} from "./support/fixtures.js";

const STEP_MS = 60_000;
const TEST_MS = 180_000;

let browser: Browser;
let host: RunningHost;
let product: { url: string; server: Server };

beforeAll(async () => {
  host = await startHost();
  browser = await testBrowser();
  const code = await buildFixture("prompt-product.ts");
  const port = await freePort();
  const server = createServer((request, response) => {
    if (request.url === "/index.js") {
      response.writeHead(200, { "content-type": "text/javascript" });
      return void response.end(code);
    }
    response.writeHead(200, { "content-type": "text/html; charset=utf-8" });
    response.end(
      '<!doctype html><meta charset="utf-8"><title>prompt fixture</title><button id="focus-me">Here</button><script type="module" src="/index.js"></script>',
    );
  });
  await new Promise<void>((resolve) =>
    server.listen(port, "127.0.0.1", () => resolve()),
  );
  product = { url: `http://localhost:${port}/`, server };
}, STEP_MS);

afterAll(async () => {
  product?.server.close();
  await host?.close();
});

type Calls = { pick(): Promise<unknown>; camera(): Promise<unknown> };

async function start(viewport: { width: number; height: number }): Promise<{
  context: BrowserContext;
  page: Page;
  frame: Frame;
  outside: string[];
}> {
  const context = await browser.newContext({ viewport });
  context.setDefaultTimeout(STEP_MS);
  await context.routeWebSocket(/.*/, (socket) => socket.close());
  const outside: string[] = [];
  await context.route(/.*/, (route) => {
    const url = route.request().url();
    if (url.startsWith(host.origin) || url.startsWith(product.url))
      return route.fallback();
    outside.push(url);
    return route.abort();
  });
  const page = await context.newPage();
  await page.goto(host.origin);
  await ready(page);
  await useInvitationDemo(page, "alice");
  await ready(page);
  // The tab has booted once the core signed the test user in. Nothing else
  // opens: the product below is the one the test user tries.
  await page
    .locator("#session")
    .filter({ hasText: "Signed in as Alice" })
    .waitFor({ state: "attached" });
  expect(await page.locator(".test-product iframe").count()).toBe(0);
  await setMenu(page, false);
  await showAddress(page);
  await page.fill("#address", product.url);
  await page.press("#address", "Enter");
  const deadline = Date.now() + STEP_MS;
  while (Date.now() < deadline) {
    for (const frame of page.frames())
      if (
        frame.url().startsWith(product.url) &&
        (await frame.evaluate(() => "__prompts" in window).catch(() => false))
      )
        return { context, page, frame, outside };
    await page.waitForTimeout(200);
  }
  throw new Error(
    `the prompt fixture did not start: menu ${await page.getAttribute("body", "data-menu")}, address ${await page.inputValue("#address")}, status ${await page.locator("#product-status").textContent()}, frames ${page
      .frames()
      .map((frame) => frame.url())
      .join(" ")}`,
  );
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

const call = (frame: Frame, name: keyof Calls) =>
  frame.evaluate(
    (method) => (window as unknown as { __prompts: Calls }).__prompts[method](),
    name,
  ) as Promise<{ outcome?: { tag: string }; granted?: boolean }>;

const sheet = (page: Page) => page.locator("dialog.prompt .prompt-sheet");

/** Where the dialog, its sheet and the app rectangle are, in page pixels, once the sheet has risen. */
async function geometry(page: Page) {
  await sheet(page).evaluate((node) =>
    Promise.all(node.getAnimations().map((animation) => animation.finished)),
  );
  return page.evaluate(() => {
    const box = (element: Element) => {
      const rect = element.getBoundingClientRect();
      return {
        left: rect.left,
        top: rect.top,
        right: rect.right,
        bottom: rect.bottom,
      };
    };
    const frame = document.getElementById("product-frame")!;
    const dialog = document.querySelector("dialog.prompt")!;
    return {
      app: box(frame),
      appScale:
        frame.getBoundingClientRect().width / Math.max(1, frame.offsetWidth),
      dialog: box(dialog),
      sheet: box(dialog.querySelector(".prompt-sheet")!),
      sheetColor: getComputedStyle(dialog.querySelector(".prompt-sheet")!)
        .backgroundColor,
      backdrop: getComputedStyle(dialog, "::backdrop").backgroundColor,
      tools: box(document.getElementById("devtools")!),
      sideways: document.documentElement.scrollWidth - window.innerWidth,
    };
  });
}

const inside = (
  inner: { left: number; top: number; right: number; bottom: number },
  outer: { left: number; top: number; right: number; bottom: number },
) =>
  inner.left >= outer.left - 1 &&
  inner.top >= outer.top - 1 &&
  inner.right <= outer.right + 1 &&
  inner.bottom <= outer.bottom + 1;

async function screenshot(page: Page, name: string): Promise<void> {
  const directory = process.env.PROMPT_SCREENSHOTS;
  if (directory)
    await page.screenshot({ path: join(directory, `${name}.png`) });
}

describe("the host's prompts", () => {
  test(
    "a contact picker is a dark sheet over the phone preview only, answered by row, Escape or the scrim",
    async () => {
      const { context, page, frame, outside } = await start({
        width: 1280,
        height: 860,
      });
      try {
        await choosePreview(page, "Phone 393 × 852");
        await setMenu(page, true);

        const picked = call(frame, "pick");
        await sheet(page).waitFor();
        const where = await geometry(page);
        // Over the app only: the dialog is the app's rectangle, the developer tools are outside it.
        expect(inside(where.dialog, where.app)).toBe(true);
        expect(inside(where.app, where.dialog)).toBe(true);
        expect(inside(where.sheet, where.app)).toBe(true);
        expect(where.tools.left).toBeGreaterThanOrEqual(where.app.right - 1);
        expect(where.backdrop).toBe("rgba(0, 0, 0, 0)");
        expect(where.sheetColor).toBe("rgb(23, 24, 28)");
        // A title, the product once, one row per contact, and Cancel.
        const text = await sheet(page).innerText();
        expect(text).toContain("Choose a contact");
        expect(
          text.match(new RegExp(new URL(product.url).host, "g")),
        ).toHaveLength(1);
        expect(text).not.toContain("0xb0b0");
        expect(
          await sheet(page).locator(".prompt-list button").allInnerTexts(),
        ).toEqual(["Bob", "Carol"]);
        // Focus starts in the search field, so the user can type at once.
        const search = sheet(page).getByRole("searchbox", {
          name: "Search contacts",
        });
        expect(
          await page.evaluate(() =>
            document.activeElement?.getAttribute("role"),
          ),
        ).toBe("searchbox");
        const shown = () =>
          sheet(page).locator(".prompt-list button:visible").allInnerTexts();
        // It filters by name, ignoring case; a name no contact has shows none and is not looked up.
        await search.fill("CAR");
        expect(await shown()).toEqual(["Carol"]);
        await search.fill("zed.01");
        expect(await shown()).toEqual([]);
        await sheet(page)
          .locator(".prompt-empty", { hasText: "No contact by that name." })
          .waitFor();
        await search.fill("ca");
        await screenshot(page, "picker-phone-preview");
        // Enter in the field picks nobody.
        await search.press("Enter");
        expect(await page.locator("dialog.prompt").count()).toBe(1);
        await sheet(page).getByRole("button", { name: "Carol" }).click();
        expect((await picked).outcome?.tag).toBe("Picked");
        expect(await page.locator("dialog.prompt").count()).toBe(0);

        // Escape and a tap on the scrim name nobody.
        const escaped = call(frame, "pick");
        await sheet(page).waitFor();
        await page.keyboard.press("Escape");
        expect((await escaped).outcome?.tag).toBe("Dismissed");
        const tapped = call(frame, "pick");
        await sheet(page).waitFor();
        const app = (await geometry(page)).app;
        await page.mouse.click(app.left + 20, app.top + 20);
        expect((await tapped).outcome?.tag).toBe("Dismissed");

        // A permission keeps its detail, and Deny is the answer the test gives.
        await page.focus("#pill");
        const asked = call(frame, "camera");
        await sheet(page).waitFor();
        const permission = await sheet(page).innerText();
        expect(permission).toContain("Device permission");
        expect(permission).toContain("Camera");
        expect(
          await sheet(page).locator(".prompt-actions button").allInnerTexts(),
        ).toEqual(["Deny", "Allow once", "Always allow"]);
        expect(inside((await geometry(page)).sheet, app)).toBe(true);
        await screenshot(page, "permission-phone-preview");
        await sheet(page).getByRole("button", { name: "Deny" }).click();
        expect((await asked).granted).toBe(false);

        // Focus goes back where it was before the prompt.
        expect(await page.evaluate(() => document.activeElement?.id)).toBe(
          "pill",
        );
        expect(outside).toEqual([]);
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );

  test(
    "a scaled preview scales the sheet with the app, and a phone browser shows it without sideways scroll",
    async () => {
      const small = await start({ width: 1100, height: 620 });
      try {
        await choosePreview(small.page, "Phone 393 × 852");
        const picked = call(small.frame, "pick");
        await sheet(small.page).waitFor();
        const where = await geometry(small.page);
        expect(where.appScale).toBeLessThan(1);
        expect(inside(where.sheet, where.app)).toBe(true);
        // The sheet is drawn at the app's scale, so it keeps its phone proportions.
        const sheetWidth = where.sheet.right - where.sheet.left;
        const appWidth = where.app.right - where.app.left;
        expect(
          Math.abs(sheetWidth - (appWidth - 16 * where.appScale)),
        ).toBeLessThan(2);
        await screenshot(small.page, "picker-scaled-preview");
        await small.page.keyboard.press("Escape");
        expect((await picked).outcome?.tag).toBe("Dismissed");
      } finally {
        await small.context.close();
      }

      const phone = await start({ width: 390, height: 844 });
      try {
        // The picker's search on a phone: full width, filters, nothing sideways.
        const picking = call(phone.frame, "pick");
        await sheet(phone.page).waitFor();
        const field = sheet(phone.page).getByRole("searchbox");
        await field.fill("bo");
        expect(
          await sheet(phone.page)
            .locator(".prompt-list button:visible")
            .allInnerTexts(),
        ).toEqual(["Bob"]);
        expect((await field.boundingBox())!.height).toBeGreaterThanOrEqual(44);
        expect((await geometry(phone.page)).sideways).toBeLessThanOrEqual(0);
        await screenshot(phone.page, "picker-phone");
        // Escape dismisses the sheet even with text in the field.
        await phone.page.keyboard.press("Escape");
        expect((await picking).outcome?.tag).toBe("Dismissed");

        const asked = call(phone.frame, "camera");
        await sheet(phone.page).waitFor();
        const where = await geometry(phone.page);
        expect(where.sideways).toBeLessThanOrEqual(0);
        expect(inside(where.sheet, where.app)).toBe(true);
        const deny = await sheet(phone.page)
          .getByRole("button", { name: "Deny" })
          .boundingBox();
        expect(deny!.height).toBeGreaterThanOrEqual(44);
        await screenshot(phone.page, "permission-phone");
        await phone.page.keyboard.press("Escape");
        expect((await asked).granted).toBe(false);
        expect(phone.outside).toEqual([]);
      } finally {
        await phone.context.close();
      }
    },
    TEST_MS,
  );
});
