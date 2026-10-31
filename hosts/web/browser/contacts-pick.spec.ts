/**
 * Headless-browser test of `contacts.pick` for test users: each tab of the
 * normal host signs a test user in to the core with a public test phrase and
 * opens a product, `fixtures/products/contacts-product.ts`, served by URL on
 * a loopback port; the host's own picker lists that user's made-up contacts.
 * The same module on a second port is a second product. No wallet is used,
 * every socket is closed and every request outside this host and its
 * loopback fixture products is refused. It needs a Chromium: playwright-core
 * finds the one it installs, or set CHROMIUM_PATH.
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
  useInvitationDemo,
} from "./support/fixtures.js";
import {
  TEST_USERS,
  testUserAccount,
} from "../fixtures/invitations/test-users.js";

const STEP_MS = 60_000;
const TEST_MS = 240_000;
const HANDLE = /^0x[0-9a-f]{64}$/;
/** A test user's identity account, as it would show in hex. */
const accountOf = (id: string): string =>
  testUserAccount(TEST_USERS.find((user) => user.id === id)!, "paseo").slice(2);

let browser: Browser;
let host: RunningHost;
/** Two products, each `contacts-product.ts` served by URL on a port of its own. */
let products: { url: string; server: Server }[] = [];

async function serveProduct(
  code: string,
): Promise<{ url: string; server: Server }> {
  const port = await freePort();
  const server = createServer((request, response) => {
    if (request.url === "/index.js") {
      response.writeHead(200, { "content-type": "text/javascript" });
      return void response.end(code);
    }
    response.writeHead(200, { "content-type": "text/html; charset=utf-8" });
    response.end(
      '<!doctype html><meta charset="utf-8"><title>contacts product</title><script type="module" src="/index.js"></script>',
    );
  });
  await new Promise<void>((resolve) =>
    server.listen(port, "127.0.0.1", () => resolve()),
  );
  return { url: `http://localhost:${port}/`, server };
}

beforeAll(async () => {
  host = await startHost();
  browser = await testBrowser();
  const code = await buildFixture("contacts-product.ts");
  products = [await serveProduct(code), await serveProduct(code)];
}, STEP_MS);

afterAll(async () => {
  for (const product of products) product.server.close();
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
    if (
      url.startsWith(host.origin) ||
      products.some((product) => url.startsWith(product.url))
    )
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

/** More → Invitation demo → the user. The tab reloads as that user. */
async function actAs(page: Page, user: string): Promise<void> {
  await useInvitationDemo(page, user);
  await ready(page);
  await page
    .locator("#session")
    .filter({ hasNotText: "Signed out" })
    .waitFor({ state: "attached" });
  await closeTools(page);
}

const picker = (page: Page) =>
  page.locator("dialog.prompt", { hasText: "Choose a contact" });

/** Open the product at `url` by its address, and answer its frame once its own script ran. */
async function productFrame(page: Page, url: string): Promise<Frame> {
  await showAddress(page);
  await page.fill("#address", url);
  await page.press("#address", "Enter");
  const deadline = Date.now() + STEP_MS;
  while (Date.now() < deadline) {
    for (const frame of page.frames()) {
      if (!frame.url().startsWith(url)) continue;
      if (await frame.evaluate(() => "__contacts" in window).catch(() => false))
        return frame;
    }
    await page.waitForTimeout(200);
  }
  throw new Error(`the product at ${url} did not start`);
}

/** What the product's `contacts.pick` answered, as the core returned it. */
type PickResult = { outcome?: unknown; error?: unknown };

const pickIn = (frame: Frame): Promise<PickResult> =>
  frame.evaluate(() =>
    (
      window as unknown as {
        __contacts: { pick(): Promise<PickResult> };
      }
    ).__contacts.pick(),
  );

/** The outcome's tag and, when it picked someone, the handle. */
function outcomeOf(result: PickResult): { tag: string; handle?: string } {
  const outcome = result.outcome as
    | { tag: string; value?: { handle: { bytes: string } } }
    | undefined;
  if (outcome === undefined) throw new Error(JSON.stringify(result.error));
  return { tag: outcome.tag, handle: outcome.value?.handle.bytes };
}

/** The product asks `contacts.pick`; answer the host's picker with `choice`, or Escape. */
async function pickWith(
  page: Page,
  frame: Frame,
  choice: string,
): Promise<{ tag: string; handle?: string }> {
  const pending = pickIn(frame);
  await picker(page).waitFor();
  if (choice === "Escape") await page.keyboard.press("Escape");
  else
    await picker(page)
      .getByRole("button", { name: new RegExp(`^${choice}\\b`) })
      .click();
  await picker(page).waitFor({ state: "detached" });
  return outcomeOf(await pending);
}

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
        const product = await productFrame(alice, products[0]!.url);

        // The host draws its own picker for the product, with two contacts to choose from.
        const first = pickIn(product);
        await picker(alice).waitFor();
        expect(await picker(alice).innerText()).toContain(
          new URL(products[0]!.url).host,
        );
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
        expect(outcomeOf(await first).tag).toBe("Dismissed");

        // Each person is one handle, the same every time; another person is another.
        const bob = await pickWith(alice, product, "Bob");
        expect(bob.tag).toBe("Picked");
        expect(bob.handle).toMatch(HANDLE);
        const carol = await pickWith(alice, product, "Carol");
        expect(carol.handle).toMatch(HANDLE);
        expect(carol.handle).not.toBe(bob.handle);
        expect((await pickWith(alice, product, "Bob")).handle).toBe(bob.handle);

        // Cancel and Escape name nobody.
        expect((await pickWith(alice, product, "Cancel")).tag).toBe(
          "Dismissed",
        );
        expect((await pickWith(alice, product, "Escape")).tag).toBe(
          "Dismissed",
        );

        // Another product gets the same handle for the same person.
        const other = await productFrame(alice, products[1]!.url);
        const fromOther = pickIn(other);
        await picker(alice).waitFor();
        expect(await picker(alice).innerText()).toContain(
          new URL(products[1]!.url).host,
        );
        await picker(alice)
          .getByRole("button", { name: /^Bob\b/ })
          .click();
        const otherResult = await fromOther;
        expect(JSON.stringify(otherResult.outcome)).toBe(
          JSON.stringify({
            tag: "Picked",
            value: { handle: { bytes: bob.handle } },
          }),
        );

        // The products got handles only: no name and no account of a contact.
        const shown = JSON.stringify([bob, carol, otherResult]);
        for (const secret of [
          "Bob",
          "Carol",
          accountOf("bob"),
          accountOf("carol"),
        ])
          expect(shown).not.toContain(secret);

        // Another user's handle for the same contact differs: the key is per user.
        const bobTab = await open();
        await actAs(bobTab, "bob");
        const carolTab = await open();
        await actAs(carolTab, "carol");
        const fromBob = await pickWith(
          bobTab,
          await productFrame(bobTab, products[0]!.url),
          "Alice",
        );
        const fromCarol = await pickWith(
          carolTab,
          await productFrame(carolTab, products[0]!.url),
          "Alice",
        );
        expect(fromBob.handle).toMatch(HANDLE);
        expect(fromCarol.handle).toMatch(HANDLE);
        expect(fromBob.handle).not.toBe(fromCarol.handle);

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
        const product = await productFrame(bob, products[0]!.url);
        const results: { tag: string; handle?: string }[] = [];
        let pending: Promise<PickResult> = Promise.resolve({});
        const openPicker = async () => {
          pending = pickIn(product);
          await picker(bob).waitFor();
        };
        const closed = async () => {
          await picker(bob).waitFor({ state: "detached" });
          results.push(outcomeOf(await pending));
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
        const [carol] = results;
        expect(carol!.handle).toMatch(HANDLE);

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
        const [, again, alice] = results;
        expect(again!.handle).toBe(carol!.handle);
        expect(alice!.handle).toMatch(HANDLE);
        expect(alice!.handle).not.toBe(carol!.handle);

        // Carol did not become a contact: the next picker lists Alice only.
        await openPicker();
        expect(
          await picker(bob).locator(".prompt-list button").allInnerTexts(),
        ).toEqual(["Alice"]);
        await bob.keyboard.press("Escape");
        await closed();

        // The product got handles only.
        const shown = JSON.stringify(results);
        for (const secret of [
          "Alice",
          "carol",
          accountOf("alice"),
          accountOf("carol"),
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
        const frame = await productFrame(page, products[1]!.url);
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
