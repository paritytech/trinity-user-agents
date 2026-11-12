/**
 * The draft field bridge (dev server only) between two host tabs, Alice's
 * and Bob's, on the simulated Statement Store (`?field-demo=simulation`):
 * two providers on their own origins and Small Market, which posts its own
 * listings and orders through its TrUAPI Statement Store. The host binds
 * Bob's receiving key with the core, checks it in Alice's tab, asks her once
 * which details to send, seals them, and draws them in both markets; Bob's
 * host opens the order again after a restart.
 *
 * The field apps run on this test's own Vite servers, and the browser reaches
 * them under the exact origins the host allows (`FIELD_APPS` in
 * `tools/demos/field-demo.ts`), so the host's own allowlist decides their
 * roles. Every other request outside the host is refused and every socket is
 * closed: nothing reaches a chain and no allowance is claimed. The host and
 * the field apps use their own Vite caches, so a running dev server is not
 * disturbed. It needs a Chromium: playwright-core finds the one it installs,
 * or set CHROMIUM_PATH.
 */
import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import type { Browser, BrowserContext, Frame, Page } from "playwright-core";
import { createServer, type ViteDevServer } from "vite";
import {
  freePort,
  startHost,
  testBrowser,
  type RunningHost,
} from "./support/fixtures.js";

const TIMEOUT = 240_000;
const hostRoot = fileURLToPath(new URL("..", import.meta.url));

const PROVIDER = "http://localhost:5182/provider.html";
const SECOND_PROVIDER = "http://localhost:5185/provider.html";
const MARKET = "http://localhost:5183/market.html";

/** Synthetic values, one set per provider. None may reach a market. */
const HOME = ["Ada Example", "24 Sample Lane", "Example City 01234"];
const HOME_PHONE = "+1 555 010 0123";
const TRAVEL = ["Ada Example", "c/o Fixture Hotel", "Test Town 98765"];
const TRAVEL_PHONE = "+1 555 010 0199";
const SECRETS = [...HOME, ...TRAVEL.slice(1), HOME_PHONE, TRAVEL_PHONE];

let host: RunningHost;
let browser: Browser;
let cacheRoot: string;
const fieldServers: ViteDevServer[] = [];
/** Where the browser's field origins are served from in this test. */
const served = new Map<string, string>();

/** One field app server, on loopback, with its own cache and no HMR socket. */
async function fieldServer(name?: string): Promise<string> {
  const port = await freePort();
  if (name === undefined) delete process.env.VITE_FIELD_PROVIDER_NAME;
  else process.env.VITE_FIELD_PROVIDER_NAME = name;
  try {
    const server = await createServer({
      configFile: join(hostRoot, "field-products.config.ts"),
      cacheDir: join(cacheRoot, `field-${port}`),
      logLevel: "error",
      server: { host: "127.0.0.1", port, strictPort: true, hmr: false },
    });
    await server.listen();
    fieldServers.push(server);
  } finally {
    delete process.env.VITE_FIELD_PROVIDER_NAME;
  }
  return `http://127.0.0.1:${port}`;
}

beforeAll(async () => {
  cacheRoot = mkdtempSync(join(tmpdir(), "web-host-field-demo-"));
  host = await startHost({ cacheDir: join(cacheRoot, "host") });
  const first = await fieldServer();
  served.set("http://localhost:5182", first);
  served.set("http://localhost:5183", first);
  served.set("http://localhost:5185", await fieldServer("Travel notebook"));
  browser = await testBrowser();
}, TIMEOUT);

afterAll(async () => {
  for (const server of fieldServers) await server.close();
  await host?.close();
  rmSync(cacheRoot, { recursive: true, force: true });
});

interface Run {
  context: BrowserContext;
  /** Page errors, and console lines that name a protected value. */
  errors: string[];
  leaks: string[];
  /** Requests refused because they left the host and the field apps. */
  outside: string[];
  /** Titles of the host sheets each tab opened, in order. */
  sheets: Record<string, string[]>;
}

async function offline(): Promise<Run> {
  const context = await browser.newContext({
    viewport: { width: 1280, height: 900 },
  });
  context.setDefaultTimeout(60_000);
  const run: Run = { context, errors: [], leaks: [], outside: [], sheets: {} };
  await context.routeWebSocket(/.*/, (socket) => socket.close());
  await context.route(/.*/, async (route) => {
    const url = new URL(route.request().url());
    const target = served.get(url.origin);
    // Continued, not fulfilled, so the browser still sees a loopback
    // server answer, as it does for the dev servers.
    if (target !== undefined)
      return route.continue({ url: `${target}${url.pathname}${url.search}` });
    if (url.origin !== host.origin) {
      run.outside.push(url.href);
      return route.abort();
    }
    return route.fallback();
  });
  await context.addInitScript(() => {
    if (window !== window.top) return;
    localStorage.setItem("truapi-web-signing-host:backup:switch", "off");
    sessionStorage.setItem("truapi-web-signing-host:dev:contact-chat", "off");
  });
  return run;
}

/** A host tab signed in as a built-in test account, with its sheets recorded. */
async function boot(run: Run, user: "alice" | "bob"): Promise<Page> {
  const page = await run.context.newPage();
  run.sheets[user] = [];
  page.on("pageerror", (error) => run.errors.push(`${user}: ${error.message}`));
  page.on("console", (line) => {
    if (SECRETS.some((secret) => line.text().includes(secret)))
      run.leaks.push(`${user}: ${line.text()}`);
  });
  await page.goto(`${host.origin}/?field-demo=simulation`);
  await page
    .locator("#core-status")
    .filter({ hasText: "Core ready" })
    .waitFor({ state: "attached", timeout: 120_000 });
  await page.locator("#wallet").selectOption(`test-account:${user}`);
  await page.locator("#sign-in").click();
  await page
    .locator("#session")
    .filter({ hasText: "Signed in as" })
    .waitFor({ state: "attached" });
  await page.exposeFunction("__fieldSheet", (title: string) =>
    run.sheets[user]!.push(title),
  );
  await page.evaluate(() => {
    const seen = new WeakSet<Element>();
    new MutationObserver(() => {
      for (const dialog of document.querySelectorAll("dialog[open]"))
        if (!seen.has(dialog)) {
          seen.add(dialog);
          (
            window as unknown as { __fieldSheet(title: string): void }
          ).__fieldSheet(dialog.querySelector("h2")?.textContent ?? "?");
        }
    }).observe(document.body, {
      subtree: true,
      childList: true,
      attributes: true,
    });
  });
  return page;
}

/** Open `url` as a product in `page`, from the Product tab, as a user would. */
async function open(page: Page, url: string): Promise<void> {
  await page.bringToFront();
  await page.getByRole("tab", { name: "Product", exact: true }).click();
  await page.locator("#product-address").fill(url);
  await page.locator("#open").click();
}

const app = (page: Page, url: string) =>
  page.frameLocator(`iframe[src^="${new URL(url).origin}/"]`);
const appFrame = (page: Page, url: string): Frame =>
  page.frames().find((frame) => frame.url().startsWith(new URL(url).origin))!;
const sheet = (page: Page) => page.locator("dialog[open]");
/** The host-origin frames that draw an address inside a product. */
const views = (page: Page): Frame[] =>
  page.frames().filter((frame) => frame.url().includes("/field-view.html"));

/** Wait until a host-drawn view in `page` shows `text`. */
async function viewShows(page: Page, text: string): Promise<boolean> {
  for (let attempt = 0; attempt < 50; attempt++) {
    const shown = await Promise.all(
      views(page).map((frame) =>
        frame
          .locator("body")
          .innerText()
          .catch(() => ""),
      ),
    );
    if (shown.some((body) => body.includes(text))) return true;
    await page.waitForTimeout(200);
  }
  return false;
}

/** The titles of the field bridge's own sheets. Every other sheet is the core's. */
const FIELD_SHEETS = /^(Send your|Show your)/;

/**
 * Answer the core's sheets as they come, until `done` resolves: approve a
 * review, allow a permission. The field bridge's own sheets are left to the
 * test.
 */
async function approveCore(page: Page, done: Promise<unknown>): Promise<void> {
  let finished = false;
  void done.finally(() => (finished = true)).catch(() => {});
  while (!finished) {
    const open = sheet(page);
    if ((await open.count()) > 0) {
      const title =
        (await open
          .locator("h2")
          .innerText()
          .catch(() => "")) ?? "";
      if (!FIELD_SHEETS.test(title)) {
        const approve = open.getByRole("button", {
          name: /^(Approve|Always allow)$/,
        });
        if ((await approve.count()) > 0) await approve.first().click();
      }
    }
    await page.waitForTimeout(150);
  }
  await done;
}

/** Save `address` and, when given, `phone` in the provider at `url`. No host sheet asks. */
async function saveDetails(
  run: Run,
  page: Page,
  url: string,
  address: string[],
  phone: string,
): Promise<void> {
  await open(page, url);
  const provider = app(page, url);
  await provider.getByLabel("Shipping address").fill(address.join("\n"));
  await provider.getByLabel("Phone (optional)").fill(phone);
  const before = run.sheets.alice!.length;
  await provider
    .getByRole("button", { name: "Save to my Polkadot host" })
    .click();
  await provider.locator("#saved:not([hidden])").waitFor();
  expect(run.sheets.alice!.length).toBe(before);
}

/** List `item` from Bob's market, approving the core's posting allowance. */
async function list(page: Page, item: string): Promise<void> {
  await open(page, MARKET);
  const market = app(page, MARKET);
  await market.getByRole("tab", { name: "Sell" }).click();
  await market.getByLabel("What are you selling?").fill(item);
  await market.getByRole("button", { name: "List item" }).click();
  await approveCore(
    page,
    market.locator("#toast").filter({ hasText: "Listed" }).waitFor(),
  );
}

/** Open `item` in Alice's shop, from the list even if an item is still open. */
async function openItem(page: Page, item: string): Promise<void> {
  const market = app(page, MARKET);
  if (await market.locator("#item-screen:not([hidden])").count())
    await market.locator("#item-back").click();
  await market.getByRole("tab", { name: "Shop" }).click();
  await market.locator(".card", { hasText: item }).click();
}

/** No market document or storage, and no host storage, holds a protected value. */
async function noPlaintext(pages: Page[]): Promise<string[]> {
  const found: string[] = [];
  for (const page of pages) {
    const market = appFrame(page, MARKET);
    if (market !== undefined) {
      const text = await market.evaluate(
        () =>
          document.documentElement.outerHTML +
          JSON.stringify({ ...localStorage }) +
          JSON.stringify({ ...sessionStorage }) +
          JSON.stringify(window.__fieldDemoReceived),
      );
      for (const secret of SECRETS)
        if (text.includes(secret)) found.push(`market: ${secret}`);
    }
    // The simulated store, the market's TrUAPI storage and the receiving
    // keys all live in the host origin's storage.
    const store = await page.evaluate(() =>
      JSON.stringify({ ...localStorage }),
    );
    for (const secret of SECRETS)
      if (store.includes(secret)) found.push(`host storage: ${secret}`);
  }
  return found;
}

describe("the draft field bridge demo", () => {
  test(
    "a buyer sends address and optional phone to a bound key, only hosts draw them, and the seller's host opens the order again after a restart",
    async () => {
      const run = await offline();
      try {
        let bob = await boot(run, "bob");
        await list(bob, "Brass lamp");

        // Alice opens only now: the listing reaches her from the store.
        const alice = await boot(run, "alice");
        await saveDetails(run, alice, PROVIDER, HOME, HOME_PHONE);
        await saveDetails(run, alice, SECOND_PROVIDER, TRAVEL, "");
        expect(
          await app(alice, SECOND_PROVIDER).locator(".brand-name").innerText(),
        ).toBe("Travel notebook");

        await open(alice, MARKET);
        const aliceMarket = app(alice, MARKET);
        await openItem(alice, "Brass lamp");
        await aliceMarket.getByRole("button", { name: "Buy now" }).click();
        const share = sheet(alice);
        await share.waitFor();
        const choices = await share.getByRole("button").allInnerTexts();
        expect({
          title: await share.locator("h2").innerText(),
          choices: choices.filter((label) => label.startsWith("Share")),
          checked: (await share.textContent())!.includes("signed for it"),
        }).toEqual({
          title: "Send your address?",
          choices: [
            `Share address: ${HOME.join(", ")} · phone: ${HOME_PHONE} · from Field notebook · localhost:5182`,
            `Share address: ${HOME.join(", ")} · from Field notebook · localhost:5182`,
            `Share address: ${TRAVEL.join(", ")} · from Travel notebook · localhost:5185`,
          ],
          checked: true,
        });
        await share
          .getByRole("button", { name: "Cancel", exact: true })
          .click();
        await aliceMarket
          .locator("#toast")
          .filter({ hasText: "You chose not to share" })
          .waitFor();

        await aliceMarket.getByRole("button", { name: "Buy now" }).click();
        await sheet(alice)
          .getByRole("button", {
            name: new RegExp(`phone: ${HOME_PHONE.replace(/\+/g, "\\+")}`),
          })
          .click();
        await approveCore(
          alice,
          aliceMarket.locator("#order-screen:not([hidden])").waitFor(),
        );
        expect(await viewShows(alice, HOME[1]!)).toBe(true);
        expect(await viewShows(alice, HOME_PHONE)).toBe(true);

        // The market can place the view but not read it, and its token opens once.
        const marketFrame = appFrame(alice, MARKET);
        const probe = await marketFrame.evaluate(async () => {
          const frame = [...document.querySelectorAll("iframe")].find((each) =>
            each.src.includes("/field-view.html"),
          )!;
          let readable: boolean;
          try {
            readable = frame.contentWindow!.document.body !== null;
          } catch {
            readable = false;
          }
          const replay = document.createElement("iframe");
          replay.src = frame.src;
          document.body.append(replay);
          await new Promise((resolve) =>
            replay.addEventListener("load", resolve),
          );
          return { readable, contentDocument: frame.contentDocument === null };
        });
        expect(probe).toEqual({ readable: false, contentDocument: true });
        expect(await viewShows(alice, "Unavailable. Open it again.")).toBe(
          true,
        );

        // Bob's packing slip shows what Alice chose, and printing asks nothing.
        await bob.bringToFront();
        const bobMarket = app(bob, MARKET);
        await bobMarket.getByRole("tab", { name: /Orders/ }).click();
        await bobMarket.locator("#to-ship .row").first().click();
        expect(await viewShows(bob, HOME[2]!)).toBe(true);
        expect(await viewShows(bob, HOME_PHONE)).toBe(true);
        expect(await viewShows(bob, TRAVEL[1]!)).toBe(false);
        await bob.evaluate(() => {
          const counter = window as unknown as { __printed: number };
          counter.__printed = 0;
          new MutationObserver((records) => {
            for (const record of records)
              for (const node of record.addedNodes)
                if (
                  node instanceof HTMLIFrameElement &&
                  node.dataset.fieldLabel !== undefined
                ) {
                  const label = node.contentWindow!;
                  label.print = () => {
                    counter.__printed += 1;
                    label.dispatchEvent(new Event("afterprint"));
                  };
                }
          }).observe(document.body, { childList: true });
        });
        const bobSheets = run.sheets.bob!.length;
        await bobMarket
          .getByRole("button", { name: "Print shipping label" })
          .click();
        await bobMarket.locator("#toast", { hasText: "printer" }).waitFor();
        expect(
          await bob.evaluate(
            () => (window as unknown as { __printed: number }).__printed,
          ),
        ).toBe(1);
        expect(run.sheets.bob!.length).toBe(bobSheets);

        // Bob restarts: a new host tab, with only what the browser kept.
        const bobFirstTab = run.sheets.bob!;
        await bob.close();
        bob = await boot(run, "bob");
        await open(bob, MARKET);
        const restarted = app(bob, MARKET);
        await restarted.getByRole("tab", { name: /Orders/ }).click();
        await restarted.locator("#to-ship .row").first().click();
        expect(await viewShows(bob, HOME[1]!)).toBe(true);

        // Bound key, open and draw ask nothing; posting asks the core once
        // for the allowance and the submit permission.
        expect({
          sheets: {
            bob: bobFirstTab,
            restarted: run.sheets.bob,
            alice: run.sheets.alice,
          },
          plaintext: await noPlaintext([alice, bob]),
          leaks: run.leaks,
          errors: run.errors,
        }).toEqual({
          sheets: {
            bob: ["Allocate resources", "Remote permission"],
            restarted: [],
            alice: [
              "Send your address?",
              "Send your address?",
              "Allocate resources",
              "Remote permission",
            ],
          },
          plaintext: [],
          leaks: [],
          errors: [],
        });
      } finally {
        await run.context.close();
      }
    },
    TIMEOUT,
  );

  test(
    "no provider asks nothing, a reloaded market ends its open sheet, and sign-out blanks what is drawn",
    async () => {
      const run = await offline();
      try {
        const bob = await boot(run, "bob");
        await list(bob, "Oak shelf");
        const alice = await boot(run, "alice");
        await open(alice, MARKET);
        const market = app(alice, MARKET);
        await openItem(alice, "Oak shelf");
        await market.getByRole("button", { name: "Buy now" }).click();
        await market
          .locator("#toast")
          .filter({ hasText: "No details app holds your address" })
          .waitFor();
        expect(run.sheets.alice).toEqual([]);

        await saveDetails(run, alice, PROVIDER, HOME, "");
        await open(alice, MARKET);
        await openItem(alice, "Oak shelf");
        await market.getByRole("button", { name: "Buy now" }).click();
        await sheet(alice).waitFor();
        // The market's document goes away: its bridge, and the sheet it asked for, end.
        await appFrame(alice, MARKET).evaluate(() => location.reload());
        await sheet(alice).waitFor({ state: "detached" });
        await bob.bringToFront();
        const bobMarket = app(bob, MARKET);
        await bobMarket.getByRole("tab", { name: /Orders/ }).click();
        await bob.waitForTimeout(2_500);
        expect(await bobMarket.locator("#to-ship .row").count()).toBe(0);

        await alice.bringToFront();
        await openItem(alice, "Oak shelf");
        await market.getByRole("button", { name: "Buy now" }).click();
        await sheet(alice)
          .getByRole("button", { name: /^Share address/ })
          .click();
        await approveCore(
          alice,
          market.locator("#order-screen:not([hidden])").waitFor(),
        );
        expect(await viewShows(alice, HOME[1]!)).toBe(true);
        await alice.getByRole("tab", { name: "Account", exact: true }).click();
        await alice.locator("#sign-out").click();
        await alice
          .locator("#session")
          .filter({ hasText: "Signed out" })
          .waitFor({ state: "attached" });
        expect(await viewShows(alice, HOME[1]!)).toBe(false);

        expect({
          plaintext: await noPlaintext([bob]),
          leaks: run.leaks,
          errors: run.errors,
        }).toEqual({ plaintext: [], leaks: [], errors: [] });
      } finally {
        await run.context.close();
      }
    },
    TIMEOUT,
  );
});
