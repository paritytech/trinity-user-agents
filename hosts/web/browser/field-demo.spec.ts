/**
 * The two-product field demo (dev server only) between two host tabs, Alice's
 * and Bob's, on the simulated Statement Store (`?field-demo=simulation`): two
 * providers on their own origins, Small Market, the host's sheets, the
 * host-drawn address and printing.
 *
 * The field apps run on this test's own Vite servers, and the browser reaches
 * them under the exact origins the host allows (`FIELD_APPS` in
 * `tools/demos/field-demo.ts`), so the host's own allowlist decides their
 * roles. Every other request outside the host is refused and every socket is
 * closed: nothing reaches a chain and no allowance is used. The host and the
 * field apps use their own Vite caches, so a running dev server is not
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

/** Save `address` and `phone` in the provider at `url`. No host sheet asks. */
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
  await provider.getByLabel("Phone").fill(phone);
  const before = run.sheets.alice!.length;
  await provider
    .getByRole("button", { name: "Save to my Polkadot host" })
    .click();
  await provider.locator("#saved:not([hidden])").waitFor();
  expect(run.sheets.alice!.length).toBe(before);
}

/** List `item` from Bob's market; answers the delivery key his host showed. */
async function list(page: Page, item: string): Promise<string> {
  await open(page, MARKET);
  const market = app(page, MARKET);
  await market.getByRole("tab", { name: "Sell" }).click();
  await market.getByLabel("What are you selling?").fill(item);
  await market.getByRole("button", { name: "List item" }).click();
  const key = (await sheet(page).textContent())!.match(/0x[0-9a-f]{64}/)![0];
  await sheet(page)
    .getByRole("button", { name: "Publish", exact: true })
    .click();
  await market.locator("#toast").filter({ hasText: "Listed" }).waitFor();
  return key;
}

/** Open `item` in Alice's shop, from the list even if an item is still open. */
async function openItem(page: Page, item: string): Promise<void> {
  const market = app(page, MARKET);
  if (await market.locator("#item-screen:not([hidden])").count())
    await market.locator("#item-back").click();
  await market.getByRole("tab", { name: "Shop" }).click();
  await market.locator(".card", { hasText: item }).click();
}

/** No market document, storage or the simulated store holds a protected value. */
async function noPlaintext(pages: Page[]): Promise<string[]> {
  const found: string[] = [];
  for (const page of pages) {
    const market = appFrame(page, MARKET);
    if (market !== undefined) {
      const text = await market.evaluate(
        () =>
          document.documentElement.outerHTML +
          JSON.stringify({ ...localStorage }) +
          JSON.stringify({ ...sessionStorage }),
      );
      for (const secret of SECRETS)
        if (text.includes(secret)) found.push(`market: ${secret}`);
    }
    // The simulated Statement Store lives in the host origin's storage.
    const store = await page.evaluate(() =>
      JSON.stringify({ ...localStorage }),
    );
    for (const secret of SECRETS)
      if (store.includes(secret)) found.push(`store: ${secret}`);
  }
  return found;
}

describe("the two-product field demo", () => {
  test(
    "a buyer who opens later finds the listing, chooses one of two providers in one sheet, and only hosts draw the address",
    async () => {
      const run = await offline();
      try {
        const bob = await boot(run, "bob");
        await open(bob, MARKET);
        const bobMarket = app(bob, MARKET);
        await bobMarket.getByRole("tab", { name: "Sell" }).click();
        await bobMarket.getByLabel("What are you selling?").fill("Brass lamp");
        await bobMarket.getByRole("button", { name: "List item" }).click();
        expect(await sheet(bob).textContent()).toContain(
          "SIMULATED Statement Store",
        );
        await sheet(bob)
          .getByRole("button", { name: "Cancel", exact: true })
          .click();
        await bobMarket
          .locator("#toast")
          .filter({ hasText: "Listing cancelled" })
          .waitFor();
        const key = await list(bob, "Brass lamp");

        // Alice opens only now: the listing reaches her from the store.
        const alice = await boot(run, "alice");
        await saveDetails(run, alice, PROVIDER, HOME, HOME_PHONE);
        await saveDetails(run, alice, SECOND_PROVIDER, TRAVEL, TRAVEL_PHONE);
        expect(
          await app(alice, SECOND_PROVIDER).locator(".brand-name").innerText(),
        ).toBe("Travel notebook");

        await open(alice, MARKET);
        const aliceMarket = app(alice, MARKET);
        await openItem(alice, "Brass lamp");
        await aliceMarket.getByRole("button", { name: "Buy now" }).click();
        const share = sheet(alice);
        await share.waitFor();
        const shown = (await share.textContent())!;
        expect({
          title: await share.locator("h2").innerText(),
          key: shown.includes(key),
          home: shown.includes(
            `${HOME.join(", ")} · from Field notebook · localhost:5182`,
          ),
          travel: shown.includes(
            `${TRAVEL.join(", ")} · from Travel notebook · localhost:5185`,
          ),
        }).toEqual({
          title: "Share your address with the seller?",
          key: true,
          home: true,
          travel: true,
        });
        await share
          .getByRole("button", { name: "Cancel", exact: true })
          .click();
        await aliceMarket
          .locator("#toast")
          .filter({ hasText: "Purchase cancelled" })
          .waitFor();

        await aliceMarket.getByRole("button", { name: "Buy now" }).click();
        await sheet(alice)
          .getByRole("button", { name: /Travel notebook/ })
          .click();
        await aliceMarket.locator("#order-screen:not([hidden])").waitFor();
        expect(await viewShows(alice, TRAVEL[1]!)).toBe(true);

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
          replay.dataset.replay = "";
          document.body.append(replay);
          await new Promise((resolve) =>
            replay.addEventListener("load", resolve),
          );
          return { readable, contentDocument: frame.contentDocument === null };
        });
        expect(probe).toEqual({ readable: false, contentDocument: true });
        expect(await viewShows(alice, "Address unavailable")).toBe(true);

        // Bob's packing slip shows the chosen address, and printing asks nothing.
        await bob.bringToFront();
        await bobMarket.getByRole("tab", { name: /Orders/ }).click();
        await bobMarket.locator("#to-ship .row").first().click();
        expect(await viewShows(bob, TRAVEL[2]!)).toBe(true);
        expect(await viewShows(bob, HOME[1]!)).toBe(false);
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

        expect({
          sheets: run.sheets,
          plaintext: await noPlaintext([alice, bob]),
          leaks: run.leaks,
          errors: run.errors,
        }).toEqual({
          sheets: {
            bob: ["Publish “Brass lamp”?", "Publish “Brass lamp”?"],
            alice: [
              "Share your address with the seller?",
              "Share your address with the seller?",
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
    "no provider asks nothing, a reloaded market ends its open sheet and old views, and sign-out blanks what is drawn",
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
          .filter({ hasText: "No address saved" })
          .waitFor();
        expect(run.sheets.alice).toEqual([]);

        await saveDetails(run, alice, PROVIDER, HOME, HOME_PHONE);
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
          .getByRole("button", { name: "Keys match, share address" })
          .click();
        await market.locator("#order-screen:not([hidden])").waitFor();
        expect(await viewShows(alice, HOME[1]!)).toBe(true);
        const oldView = await appFrame(alice, MARKET).evaluate(
          () =>
            [...document.querySelectorAll("iframe")].find((frame) =>
              frame.src.includes("/field-view.html"),
            )!.src,
        );

        // A view token outlives neither its market document nor the session.
        await appFrame(alice, MARKET).evaluate(() => location.reload());
        await market.locator(".card", { hasText: "Oak shelf" }).waitFor();
        await appFrame(alice, MARKET).evaluate((src) => {
          const replay = document.createElement("iframe");
          replay.src = src;
          document.body.append(replay);
        }, oldView);
        expect(await viewShows(alice, "Address unavailable")).toBe(true);
        await market.getByRole("tab", { name: /Orders/ }).click();
        await market.locator("#bought .row").first().click();
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
