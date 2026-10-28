/**
 * Headless-browser test of `contacts.pick` for test users: each tab of the
 * normal host signs a test user in to the core with a public test phrase,
 * runs Example Chat as an ordinary product, and the host's own picker lists
 * that user's made-up contacts. No wallet is used, every socket is closed and
 * every request outside this host and its loopback fixture product is
 * refused. It needs a Chromium: playwright-core finds the one it installs, or
 * set CHROMIUM_PATH.
 */
import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { createServer, type Server } from "node:http";
import type { Browser, BrowserContext, Frame, Page } from "playwright-core";
import {
  buildFixture,
  freePort,
  type RunningHost,
  showAddress,
  startHost,
  testBrowser,
  closeTools,
  openTools,
  type DevTab,
  useInvitationDemo,
} from "./support/fixtures.js";

const STEP_MS = 60_000;
const TEST_MS = 240_000;
const HANDLE = /^0x[0-9a-f]{64}$/;

let browser: Browser;
let host: RunningHost;
/** A second product, served by URL on a port of its own: `contacts-product.ts`. */
let second: { url: string; server: Server };

beforeAll(async () => {
  host = await startHost();
  browser = await testBrowser();
  const code = await buildFixture("contacts-product.ts");
  const port = await freePort();
  const server = createServer((request, response) => {
    if (request.url === "/index.js") {
      response.writeHead(200, { "content-type": "text/javascript" });
      return void response.end(code);
    }
    response.writeHead(200, { "content-type": "text/html; charset=utf-8" });
    response.end(
      '<!doctype html><meta charset="utf-8"><title>second product</title><script type="module" src="/index.js"></script>',
    );
  });
  await new Promise<void>((resolve) =>
    server.listen(port, "127.0.0.1", () => resolve()),
  );
  second = { url: `http://localhost:${port}/`, server };
}, STEP_MS);

afterAll(async () => {
  second?.server.close();
  await host?.close();
});

async function session(): Promise<{
  context: BrowserContext;
  outside: string[];
  errors: string[];
  open(): Promise<Page>;
}> {
  const context = await browser.newContext({
    viewport: { width: 1280, height: 860 },
  });
  context.setDefaultTimeout(STEP_MS);
  await context.routeWebSocket(/.*/, (socket) => socket.close());
  const outside: string[] = [];
  await context.route(/.*/, (route) => {
    const url = route.request().url();
    if (url.startsWith(host.origin) || url.startsWith(second.url))
      return route.fallback();
    outside.push(url);
    return route.abort();
  });
  const errors: string[] = [];
  return {
    context,
    outside,
    errors,
    open: async () => {
      const page = await context.newPage();
      page.on("pageerror", (error) => errors.push(error.message));
      await page.goto(host.origin);
      await ready(page);
      return page;
    },
  };
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

/**
 * More → Invitation demo → the user, then More → Open Example Chat. The
 * tab reloads as that user, and Example Chat opens only when asked.
 */
async function actAs(page: Page, user: string): Promise<void> {
  await useInvitationDemo(page, user);
  await ready(page);
  await page
    .locator("#session")
    .filter({ hasNotText: "Signed out" })
    .waitFor({ state: "attached" });
  await setMenu(page, true, "more");
  await page.click('[data-action="open-example-chat"]');
  await page
    .frameLocator(".test-product iframe")
    .locator('[data-action="pick-contact"]')
    .waitFor();
  await setMenu(page, false);
}

const picker = (page: Page) =>
  page.locator("dialog.prompt", { hasText: "Choose a contact" });

/** Press Pick a contact in Example Chat and answer the host's picker with `choice`. */
async function pickInExampleChat(
  page: Page,
  choice: string,
): Promise<{ outcome: string; handles: string[] }> {
  const product = page.frameLocator(".test-product iframe");
  const status = product.locator('[data-card="contacts"] .note');
  const before = await product.locator('[data-list="picked"] .row').count();
  await product.locator('[data-action="pick-contact"]').click();
  await picker(page).waitFor();
  if (choice === "Escape") await page.keyboard.press("Escape");
  else
    await picker(page)
      .getByRole("button", { name: new RegExp(`^${choice}\\b`) })
      .click();
  await picker(page).waitFor({ state: "detached" });
  await product
    .locator('[data-action="pick-contact"]:not([disabled])')
    .waitFor();
  const handles = await product
    .locator('[data-list="picked"] .row')
    .evaluateAll((rows) => rows.map((row) => (row as HTMLElement).dataset.id!));
  const outcome =
    handles.length > before
      ? "Picked"
      : ((await status.getAttribute("data-outcome")) ?? "none");
  return { outcome, handles };
}

/** The second product's frame once its own script ran. */
async function secondProduct(page: Page): Promise<Frame> {
  await showAddress(page);
  await page.fill("#address", second.url);
  await page.press("#address", "Enter");
  const deadline = Date.now() + STEP_MS;
  while (Date.now() < deadline) {
    for (const frame of page.frames()) {
      if (!frame.url().startsWith(second.url)) continue;
      if (await frame.evaluate(() => "__contacts" in window).catch(() => false))
        return frame;
    }
    await page.waitForTimeout(200);
  }
  throw new Error("the second product did not start");
}

const pickIn = (frame: Frame) =>
  frame.evaluate(() =>
    (
      window as unknown as {
        __contacts: { pick(): Promise<{ outcome?: unknown; error?: unknown }> };
      }
    ).__contacts.pick(),
  );

describe("contacts.pick for test users", () => {
  test(
    "the host's picker lists the test user's contacts, and the product gets the core's handle, never a name or account",
    async () => {
      const { context, outside, errors, open } = await session();
      try {
        const alice = await open();
        await actAs(alice, "alice");
        expect(await alice.locator("#session").innerText()).toContain(
          "Signed in as Alice",
        );

        // The host draws its own picker for the product, with two contacts to choose from.
        const product = alice.frameLocator(".test-product iframe");
        await product.locator('[data-action="pick-contact"]').click();
        await picker(alice).waitFor();
        expect(await picker(alice).innerText()).toContain("example-chat.paseo");
        // One row per contact, by name only; Cancel sits below them.
        expect(
          await picker(alice).locator(".prompt-list button").allInnerTexts(),
        ).toEqual(["Bob", "Carol"]);
        expect(
          await picker(alice).locator(".prompt-actions button").allInnerTexts(),
        ).toEqual(["Cancel"]);
        if (process.env.INVITATION_SCREENSHOTS)
          await alice.screenshot({
            path: `${process.env.INVITATION_SCREENSHOTS}/picker.png`,
          });
        await alice.keyboard.press("Escape");
        await picker(alice).waitFor({ state: "detached" });

        // Each person is one handle, the same every time; another person is another.
        const bob = await pickInExampleChat(alice, "Bob");
        expect(bob.outcome).toBe("Picked");
        const bobHandle = bob.handles.at(-1)!;
        expect(bobHandle).toMatch(HANDLE);
        const carol = await pickInExampleChat(alice, "Carol");
        const carolHandle = carol.handles.at(-1)!;
        expect(carolHandle).toMatch(HANDLE);
        expect(carolHandle).not.toBe(bobHandle);
        expect((await pickInExampleChat(alice, "Bob")).handles.at(-1)).toBe(
          bobHandle,
        );

        // Cancel and Escape name nobody.
        expect((await pickInExampleChat(alice, "Cancel")).outcome).toBe(
          "Dismissed",
        );
        expect((await pickInExampleChat(alice, "Escape")).outcome).toBe(
          "Dismissed",
        );

        // The product saw handles only: no name and no account of a contact.
        const shown = await product.locator("main").innerText();
        for (const secret of ["Bob", "Carol", "b0".repeat(32), "c4".repeat(32)])
          expect(shown).not.toContain(secret);

        // Another product gets the same handle for the same person.
        const other = await secondProduct(alice);
        const fromOther = pickIn(other);
        await picker(alice).waitFor();
        expect(await picker(alice).innerText()).toContain(
          new URL(second.url).host,
        );
        await picker(alice)
          .getByRole("button", { name: /^Bob\b/ })
          .click();
        expect(JSON.stringify((await fromOther).outcome)).toBe(
          JSON.stringify({
            tag: "Picked",
            value: { handle: { bytes: bobHandle } },
          }),
        );

        // Another user's handle for the same contact differs: the key is per user.
        const bobTab = await open();
        await actAs(bobTab, "bob");
        const carolTab = await open();
        await actAs(carolTab, "carol");
        const fromBob = (await pickInExampleChat(bobTab, "Alice")).handles.at(
          -1,
        );
        const fromCarol = (
          await pickInExampleChat(carolTab, "Alice")
        ).handles.at(-1);
        expect(fromBob).toMatch(HANDLE);
        expect(fromCarol).toMatch(HANDLE);
        expect(fromBob).not.toBe(fromCarol);

        // No test user's data reached the saved wallets.
        expect(
          await alice.evaluate(() =>
            localStorage.getItem("truapi-web-signing-host:wallets:v1"),
          ),
        ).toBeNull();
        expect(outside).toEqual([]);
        expect(errors).toEqual([]);
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );

  test(
    "the picker finds people by username, and choosing one gives the product a handle without making them a contact",
    async () => {
      const { context, outside, errors, open } = await session();
      const searches: string[] = [];
      context.on("request", (request) => {
        if (request.url().includes("__username-search"))
          searches.push(request.url());
      });
      try {
        const bob = await open();
        await actAs(bob, "bob");
        const product = bob.frameLocator(".test-product iframe");
        const handles = () =>
          product
            .locator('[data-list="picked"] .row')
            .evaluateAll((rows) =>
              rows.map((row) => (row as HTMLElement).dataset.id!),
            );
        const openPicker = async () => {
          await product
            .locator('[data-action="pick-contact"]:not([disabled])')
            .click();
          await picker(bob).waitFor();
        };
        const closed = async () => {
          await picker(bob).waitFor({ state: "detached" });
          await product
            .locator('[data-action="pick-contact"]:not([disabled])')
            .waitFor();
        };
        const search = picker(bob).getByRole("searchbox");
        const found = picker(bob).locator(".prompt-lookup");

        // Nothing typed: the contacts only, and no search.
        await openPicker();
        expect(
          await picker(bob).locator(".prompt-lookup[hidden]").count(),
        ).toBe(1);

        // Alice is a contact: she is listed once, as a contact, not again as found.
        await search.fill("ali");
        await found.getByText("No one else by that username.").waitFor();
        expect(
          await picker(bob)
            .locator(".prompt-list button:not([hidden])")
            .allInnerTexts(),
        ).toEqual(["Alice"]);

        // Something that cannot be a username is not looked up.
        await search.fill("ali ce");
        await found
          .getByText(
            "Usernames are letters and digits, like alice or alice.06.",
          )
          .waitFor();

        // Carol is no contact of Bob's: she shows apart, under her username, and can be chosen.
        await search.fill("car");
        await found.getByRole("button", { name: /^carol\b/ }).waitFor();
        expect(await found.locator(".prompt-row").allInnerTexts()).toEqual([
          "carol",
        ]);
        if (process.env.INVITATION_SCREENSHOTS)
          await bob.screenshot({
            path: `${process.env.INVITATION_SCREENSHOTS}/picker-username.png`,
          });
        await found.getByRole("button", { name: /^carol\b/ }).click();
        await closed();
        const [carol] = await handles();
        expect(carol).toMatch(HANDLE);

        // The same person found again is the same handle; a contact is another.
        await openPicker();
        await search.fill("carol");
        await found.getByRole("button", { name: /^carol\b/ }).click();
        await closed();
        await openPicker();
        await picker(bob)
          .getByRole("button", { name: /^Alice\b/ })
          .click();
        await closed();
        const [, again, alice] = await handles();
        expect(again).toBe(carol);
        expect(alice).toMatch(HANDLE);
        expect(alice).not.toBe(carol);

        // Carol did not become a contact: the next picker lists Alice only.
        await openPicker();
        expect(
          await picker(bob).locator(".prompt-list button").allInnerTexts(),
        ).toEqual(["Alice"]);
        await bob.keyboard.press("Escape");
        await closed();

        // The product saw handles only.
        const shown = await product.locator("main").innerText();
        for (const secret of [
          "Alice",
          "carol",
          "a1".repeat(32),
          "c4".repeat(32),
        ])
          expect(shown).not.toContain(secret);

        // A test user searches the other test users only, never the username index.
        expect(searches).toEqual([]);
        expect(outside).toEqual([]);
        expect(errors).toEqual([]);
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );

  test(
    "with no session the core refuses the pick and the host draws no picker",
    async () => {
      const { context, outside, open } = await session();
      try {
        const page = await open();
        const frame = await secondProduct(page);
        const result = await pickIn(frame);
        expect(result.outcome).toBeUndefined();
        expect(JSON.stringify(result.error)).toContain("NotConnected");
        expect(await page.locator("dialog.prompt").count()).toBe(0);
        expect(outside).toEqual([]);
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );
});
