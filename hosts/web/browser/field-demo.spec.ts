/**
 * The draft Locket bridge (dev server only) between two host tabs, Alice's
 * and Bob's, on the simulated Statement Store (`?field-demo=simulation`):
 * two providers on their own origins and Small Market, which posts its own
 * listings and orders through its TrUAPI Statement Store. The host binds
 * Bob's recipient key with the core, checks it in Alice's tab, asks her once
 * which details to send, seals them, and draws them in both markets; Bob's
 * host opens the order again after a restart.
 *
 * The field apps run on this test's own Vite servers, and the browser reaches
 * them under the exact origins the host allows (`LOCKET_APPS` in
 * `tools/demos/field-demo.ts`), so the host's own allowlist decides their
 * roles. Every other request outside the host is refused and every socket is
 * closed: nothing reaches a chain and no allowance is claimed. The host and
 * the field apps use their own Vite caches, so a running dev server is not
 * disturbed. It needs a Chromium: playwright-core finds the one it installs,
 * or set CHROMIUM_PATH.
 */
import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
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
  /** What the dev server answered for each field image, as the network saw it. */
  images: ImageAnswer[];
}

interface ImageAnswer {
  token: string;
  status: number;
  body: string;
  headers: Record<string, string>;
}

/** The runs, by browser context, so a page finds the images its context saw. */
const runs = new Map<BrowserContext, Run>();
const tokenOf = (url: string) => /([0-9a-f]{64})\.svg$/.exec(url)?.[1] ?? "";

/** Record the field images `context` receives into `into`. */
function recordImages(context: BrowserContext, into: ImageAnswer[]): void {
  context.on("response", async (response) => {
    const token = tokenOf(response.url());
    if (token === "" || !response.url().includes("/__field-images/")) return;
    into.push({
      token,
      status: response.status(),
      headers: response.headers(),
      body: response.status() === 200 ? await response.text() : "",
    });
  });
}

async function offline(): Promise<Run> {
  const context = await browser.newContext({
    viewport: { width: 1280, height: 900 },
  });
  context.setDefaultTimeout(60_000);
  const run: Run = {
    context,
    errors: [],
    leaks: [],
    outside: [],
    sheets: {},
    images: [],
  };
  runs.set(context, run);
  recordImages(context, run.images);
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
/**
 * Wait until a field image the host drew in `page` shows `text`: an image
 * loaded in one of its frames, whose SVG the dev server served with `text`.
 */
async function viewShows(page: Page, text: string): Promise<boolean> {
  const run = runs.get(page.context())!;
  for (let attempt = 0; attempt < 50; attempt++) {
    const loaded = (
      await Promise.all(
        page
          .frames()
          .map((frame) =>
            frame
              .evaluate(() =>
                [
                  ...document.querySelectorAll<HTMLImageElement>(
                    "img.protected-detail",
                  ),
                ]
                  .filter((image) => image.complete && image.naturalWidth > 0)
                  .map((image) => image.src),
              )
              .catch(() => [] as string[]),
          ),
      )
    ).flat();
    if (
      loaded.some((src) =>
        run.images.some(
          (answer) =>
            answer.token === tokenOf(src) &&
            answer.status === 200 &&
            answer.body.includes(text),
        ),
      )
    )
      return true;
    await page.waitForTimeout(200);
  }
  return false;
}

/** The titles of the Locket bridge's own sheets. Every other sheet is the core's. */
const FIELD_SHEETS = /^(Send your|Show your)/;

/**
 * Answer the core's sheets as they come, until `done` resolves: approve a
 * review, allow a permission. The Locket bridge's own sheets are left to the
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
    // The simulated store, the market's TrUAPI storage and the recipient
    // keys all live in the host origin's storage.
    const store = await page.evaluate(() =>
      JSON.stringify({ ...localStorage }),
    );
    for (const secret of SECRETS)
      if (store.includes(secret)) found.push(`host storage: ${secret}`);
  }
  return found;
}

describe("the draft Locket bridge demo", () => {
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

        // The market places the image but cannot read its pixels, and its URL loads once.
        const marketFrame = appFrame(alice, MARKET);
        const probe = await marketFrame.evaluate(async () => {
          const image =
            document.querySelector<HTMLImageElement>("#slip-address img")!;
          const canvas = document.createElement("canvas");
          canvas.width = image.naturalWidth;
          canvas.height = image.naturalHeight;
          const context = canvas.getContext("2d")!;
          context.drawImage(image, 0, 0);
          let pixels: string;
          try {
            context.getImageData(0, 0, 1, 1);
            pixels = "read";
          } catch (error) {
            pixels = (error as Error).name;
          }
          const replay = await new Promise<string>((resolve) => {
            const again = new Image();
            again.referrerPolicy = "origin";
            again.onload = () => resolve("loaded");
            again.onerror = () => resolve("refused");
            again.src = image.src;
          });
          return { pixels, replay };
        });
        // A new image of the same URL in the same document is drawn from the
        // browser's list of images that document already has; the server
        // served the URL once.
        const slipImage = await marketFrame.evaluate(
          () =>
            document.querySelector<HTMLImageElement>("#slip-address img")!.src,
        );
        expect({
          ...probe,
          served: run.images
            .filter((answer) => answer.token === tokenOf(slipImage))
            .map((answer) => answer.status),
        }).toEqual({
          pixels: "SecurityError",
          replay: "loaded",
          served: [200],
        });

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
        const restartedSheets = [...run.sheets.bob!];

        // The recipient keys are lost, as when site data is cleared: the
        // order cannot be opened, and the market says why.
        await bob.close();
        bob = await boot(run, "bob");
        await bob.evaluate(() => {
          for (const name of Object.keys(localStorage))
            if (name.startsWith("truapi-web-signing-host:fields:"))
              localStorage.removeItem(name);
        });
        await open(bob, MARKET);
        const keyless = app(bob, MARKET);
        await keyless.getByRole("tab", { name: /Orders/ }).click();
        await keyless.locator("#to-ship .row").first().click();
        await keyless
          .locator("#slip-address")
          .filter({ hasText: "receiving key this device does not have" })
          .waitFor();

        // Bound key, open and draw ask nothing; posting asks the core once
        // for the allowance and the submit permission.
        expect({
          sheets: {
            bob: bobFirstTab,
            restarted: restartedSheets,
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
    "no provider asks nothing, a reloaded market ends its open sheet, and sign-out closes what is drawn",
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

  test(
    "the SVG image: the market reads neither text nor pixels, an unused URL is a bearer capability on this machine, and release and sign-out refuse new loads",
    async () => {
      const run = await offline();
      const other = await browser.newContext();
      try {
        const bob = await boot(run, "bob");
        await list(bob, "Tin cup");
        const alice = await boot(run, "alice");
        await saveDetails(run, alice, PROVIDER, HOME, "");
        await open(alice, MARKET);
        const market = app(alice, MARKET);
        await openItem(alice, "Tin cup");
        await market.getByRole("button", { name: "Buy now" }).click();
        await sheet(alice)
          .getByRole("button", { name: /^Share address/ })
          .click();
        await approveCore(
          alice,
          market.locator("#order-screen:not([hidden])").waitFor(),
        );
        expect(await viewShows(alice, HOME[1]!)).toBe(true);

        const answers = run.images;
        recordImages(other, answers);
        // While held, an image request stops in the browser, so its token stays unused.
        let hold = false;
        const holdImages = (page: Page) =>
          page.route(/\/__field-images\/[0-9a-f]{64}\.svg$/, (route) =>
            hold ? route.abort() : route.fallback(),
          );
        await holdImages(alice);
        await holdImages(bob);
        const marketFrame = () => appFrame(alice, MARKET);
        const loaded = (frame: Frame, selector: string) =>
          frame.evaluate(
            (query) =>
              new Promise<boolean>((resolve) => {
                const image = document.querySelector<HTMLImageElement>(query)!;
                if (image.complete) resolve(image.naturalWidth > 0);
                else {
                  image.onload = () => resolve(true);
                  image.onerror = () => resolve(false);
                }
              }),
            selector,
          );
        /** Open Alice's receipt afresh; answers its address image's URL. */
        const reopen = async (): Promise<string> => {
          if (await market.locator("#order-screen:not([hidden])").count())
            await market.locator("#back").click();
          await market.getByRole("tab", { name: /Orders/ }).click();
          await market.locator("#bought .row").first().click();
          await market
            .locator("#slip-address img")
            .waitFor({ state: "attached" });
          return marketFrame().evaluate(
            () =>
              document.querySelector<HTMLImageElement>("#slip-address img")!
                .src,
          );
        };
        const unused = async (): Promise<string> => {
          hold = true;
          const url = await reopen();
          await alice.waitForTimeout(300);
          hold = false;
          return url;
        };
        /** Load `url` as an image in `frame`, as an honest embed does; answers what drew. */
        const loadIn = (frame: Frame, url: string) =>
          frame.evaluate(
            (src) =>
              new Promise<string>((resolve) => {
                const image = new Image();
                image.referrerPolicy = "origin";
                image.onload = () => resolve(`loaded ${image.naturalWidth}`);
                image.onerror = () => resolve("refused");
                image.src = src;
              }),
            url,
          );
        /**
         * A page of `origin` in `context`, outside the host: the field app's
         * page, served from loopback as the market is, since Chromium keeps a
         * page with no network address from requests to this machine.
         */
        const probePage = async (context: BrowserContext, origin: string) => {
          const page = await context.newPage();
          await page.goto(`${origin}/provider.html`);
          return page;
        };
        await other.route(/.*/, (route) => {
          const url = new URL(route.request().url());
          const target = served.get(url.origin);
          if (target !== undefined)
            return route.continue({
              url: `${target}${url.pathname}${url.search}`,
            });
          return url.origin === host.origin ? route.fallback() : route.abort();
        });

        // What was served: the address, uncached, with no CORS grant.
        const shown = await reopen();
        expect(await loaded(marketFrame(), "#slip-address img")).toBe(true);
        const first = answers.find((entry) => entry.token === tokenOf(shown))!;
        expect({
          status: first.status,
          body: HOME.every((line) => first.body.includes(line)),
          cache: first.headers["cache-control"],
          cors: Object.keys(first.headers).filter((name) =>
            name.startsWith("access-control"),
          ),
        }).toEqual({ status: 200, body: true, cache: "no-store", cors: [] });
        const out = process.env.FIELD_DEMO_OUT;
        if (out !== undefined) {
          mkdirSync(out, { recursive: true });
          await market
            .locator(".slip")
            .screenshot({ path: join(out, "receipt.png") });
        }

        // Printing the host page to PDF (headless Chromium's renderer, not an
        // OS print dialog) asks the dev server for nothing: the browser
        // prints the image it already holds.
        const beforePrint = answers.length;
        const pdf = await alice.pdf({ printBackground: true });
        await alice.waitForTimeout(500);
        expect({
          printed: pdf.length > 0,
          requests: answers.length - beforePrint,
        }).toEqual({ printed: true, requests: 0 });
        if (out !== undefined) writeFileSync(join(out, "host-page.pdf"), pdf);

        // The market's scripts cannot read the loaded image's pixels.
        const pixels = await marketFrame().evaluate(async () => {
          const image =
            document.querySelector<HTMLImageElement>("#slip-address img")!;
          const canvas = document.createElement("canvas");
          canvas.width = image.naturalWidth;
          canvas.height = image.naturalHeight;
          const context = canvas.getContext("2d")!;
          context.drawImage(image, 0, 0);
          const attempt = (run: () => unknown) => {
            try {
              run();
              return "read";
            } catch (error) {
              return (error as Error).name;
            }
          };
          const second = document.createElement("canvas").getContext("2d")!;
          second.drawImage(await createImageBitmap(image), 0, 0);
          return {
            size: [image.naturalWidth, image.naturalHeight],
            getImageData: attempt(() => context.getImageData(0, 0, 1, 1)),
            toDataURL: attempt(() => canvas.toDataURL()),
            bitmap: attempt(() => second.getImageData(0, 0, 1, 1)),
          };
        });
        expect(pixels).toEqual({
          size: [360, 150],
          getImageData: "SecurityError",
          toDataURL: "SecurityError",
          bitmap: "SecurityError",
        });

        // With an unused URL, nothing the market's scripts try returns the image.
        const stolen = await unused();
        const reads = await marketFrame().evaluate(async (url) => {
          const outcome = async (run: () => Promise<string>) => {
            try {
              return await run();
            } catch (error) {
              return (error as Error).name;
            }
          };
          return {
            cors: await outcome(async () =>
              (await fetch(url, { credentials: "include" })).text(),
            ),
            noCors: await outcome(async () => {
              const answer = await fetch(url, {
                mode: "no-cors",
                credentials: "include",
              });
              return `${answer.type} ${answer.status} [${await answer.text()}]`;
            }),
            xhr: await outcome(
              () =>
                new Promise((resolve, reject) => {
                  const request = new XMLHttpRequest();
                  request.open("GET", url);
                  request.onload = () => resolve(request.responseText);
                  request.onerror = () =>
                    reject(
                      Object.assign(new Error(), { name: "NetworkError" }),
                    );
                  request.send();
                }),
            ),
            documents: await Promise.all(
              (["iframe", "object", "embed"] as const).map(
                (tag) =>
                  new Promise<string>((resolve) => {
                    const element = document.createElement(tag);
                    if (element instanceof HTMLObjectElement)
                      element.data = url;
                    else (element as HTMLIFrameElement).src = url;
                    document.body.append(element);
                    setTimeout(() => {
                      let readable = "no";
                      try {
                        readable =
                          (element as HTMLIFrameElement).contentDocument
                            ?.documentElement?.outerHTML ?? "no";
                      } catch {
                        readable = "no";
                      }
                      element.remove();
                      resolve(`${tag}: ${readable}`);
                    }, 1_500);
                  }),
              ),
            ),
          };
        }, stolen);
        expect(reads).toEqual({
          cors: "TypeError",
          noCors: "opaque 0 []",
          xhr: "NetworkError",
          documents: ["iframe: no", "object: no", "embed: no"],
        });
        // Another product's origin in this browser, and a plain request, are refused.
        const foreign = await loadIn(
          (await probePage(run.context, "http://localhost:5182")).mainFrame(),
          stolen,
        );
        const plain = await fetch(
          stolen.replace("//localhost:", "//127.0.0.1:"),
        );
        // A page of the market's origin in another browser profile draws it,
        // once: nothing ties the request to this browser.
        const elsewhere = await probePage(other, "http://localhost:5183");
        const otherProfile = await loadIn(elsewhere.mainFrame(), stolen);
        const replay = await loadIn(marketFrame(), stolen);
        expect({
          foreign,
          plain: plain.status,
          otherProfile,
          replay,
        }).toEqual({
          foreign: "refused",
          plain: 404,
          otherProfile: "loaded 360",
          replay: "refused",
        });

        // The accepted limit: a client on this machine with the URL sends the
        // two headers and gets the SVG, value included.
        const forgedUrl = await unused();
        const forged = await fetch(
          forgedUrl.replace("//localhost:", "//127.0.0.1:"),
          {
            headers: {
              "sec-fetch-dest": "image",
              referer: "http://localhost:5183/",
            },
          },
        );
        expect({
          forged: forged.status,
          plaintext: (await forged.text()).includes(HOME[1]!),
        }).toEqual({ forged: 200, plaintext: true });

        // Release: Back on a sale releases its refs. An unused image no
        // longer loads; one the market kept stays drawn.
        await bob.bringToFront();
        const bobMarket = app(bob, MARKET);
        const bobFrame = appFrame(bob, MARKET);
        await bobMarket.getByRole("tab", { name: /Orders/ }).click();
        hold = true;
        await bobMarket.locator("#to-ship .row").first().click();
        await bobMarket
          .locator("#slip-address img")
          .waitFor({ state: "attached" });
        const bobUnused = await bobFrame.evaluate(
          () =>
            document.querySelector<HTMLImageElement>("#slip-address img")!.src,
        );
        await bob.waitForTimeout(300);
        hold = false;
        const revoked = bob.waitForResponse((response) =>
          response.url().endsWith("/__field-images/revoke"),
        );
        await bobMarket.locator("#back").click();
        await revoked;
        const releasedLoad = await loadIn(bobFrame, bobUnused);
        await bobMarket.locator("#to-ship .row").first().click();
        await bobMarket
          .locator("#slip-address img")
          .waitFor({ state: "attached" });
        expect(await loaded(bobFrame, "#slip-address img")).toBe(true);
        // As a market that keeps the element does.
        await bobFrame.evaluate(() => {
          const image =
            document.querySelector<HTMLImageElement>("#slip-address img")!;
          image.id = "kept-image";
          image.remove = () => {};
          document.body.append(image);
        });
        await bobMarket.locator("#back").click();
        await bob.waitForTimeout(500);
        const kept = await bobFrame.evaluate(() => {
          const image = document.getElementById(
            "kept-image",
          ) as HTMLImageElement | null;
          return image !== null && image.complete && image.naturalWidth > 0;
        });

        // Sign-out ends the unused images and closes the market's document.
        await alice.bringToFront();
        const beforeSignOut = await unused();
        await alice.getByRole("tab", { name: "Account", exact: true }).click();
        await alice.locator("#sign-out").click();
        await alice
          .locator("#session")
          .filter({ hasText: "Signed out" })
          .waitFor({ state: "attached" });
        await alice.waitForTimeout(500);
        const afterSignOut = await loadIn(
          (await probePage(run.context, "http://localhost:5183")).mainFrame(),
          beforeSignOut,
        );
        expect({
          releasedLoad,
          kept,
          afterSignOut,
          uncached: answers
            .filter((entry) => entry.status === 200)
            .every((entry) => entry.headers["cache-control"] === "no-store"),
          leaks: run.leaks,
          errors: run.errors,
        }).toEqual({
          releasedLoad: "refused",
          kept: true,
          afterSignOut: "refused",
          uncached: true,
          leaks: [],
          errors: [],
        });
      } finally {
        await other.close();
        await run.context.close();
      }
    },
    TIMEOUT,
  );
});
