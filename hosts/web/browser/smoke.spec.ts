/**
 * Headless-browser smoke test of the TrUAPI Web Host, against the host page
 * from this tree, the sandbox loader and worker, and the real core. Everything runs on loopback: the chain record and the gateway
 * content of a name are stood in for inside the browser context, and the wallet
 * is a public test phrase. It needs a Chromium: playwright-core finds the one
 * it installs, or set CHROMIUM_PATH.
 *
 * A product's own requests follow the browser's rules, so the counting server
 * sees them. A request is judged by whether it reached that server, never by
 * what the page says about it.
 */
import {
  afterAll,
  beforeAll,
  beforeEach,
  describe,
  expect,
  test,
} from "bun:test";
import {
  type Browser,
  type BrowserContext,
  type Frame,
  type Page,
  chromium,
} from "playwright-core";
import {
  type CountingTarget,
  type PublishedApp,
  type RunningHost,
  type UrlProduct,
  TEST_MNEMONIC,
  productHtml,
  publishApp,
  serveName,
  startCountingTarget,
  startHost,
  startUrlProduct,
} from "./fixtures.js";

const NAME = "smoke.paseo";
const STEP_MS = 60_000;
const TEST_MS = 180_000;

interface ProductWindow {
  __first: { state: "pending" | "sent" | "refused"; message?: string };
  __ask(path: string): Promise<"sent" | "refused">;
  __loadedAt: number;
  __mark?: number;
}

let browser: Browser;
let host: RunningHost;
let target: CountingTarget;
let app: PublishedApp;
let urlProduct: UrlProduct;

beforeAll(async () => {
  host = await startHost();
  target = await startCountingTarget();
  app = publishApp({
    "index.html": productHtml(target.origin, { rootRelativeScript: true }),
    "root.js": "document.body.dataset.rootRelative = 'loaded';",
  });
  urlProduct = await startUrlProduct(target.origin);
  browser = await chromium.launch({
    executablePath: process.env.CHROMIUM_PATH || undefined,
  });
}, STEP_MS);

afterAll(async () => {
  await browser?.close();
  urlProduct?.close();
  target?.close();
  await host?.close();
});

beforeEach(() => {
  target.hits.length = 0;
});

/** A fresh browser profile in which the fixture name resolves. */
async function newContext(viewport?: {
  width: number;
  height: number;
}): Promise<BrowserContext> {
  const context = await browser.newContext({ viewport });
  context.setDefaultTimeout(STEP_MS);
  await serveName(context, NAME, app);
  return context;
}

async function openHost(context: BrowserContext): Promise<Page> {
  const page = await context.newPage();
  await page.goto(host.origin);
  await page
    .locator("#core-status")
    .filter({ hasText: "Core ready" })
    .waitFor({ state: "attached" });
  return page;
}

async function openMenu(page: Page): Promise<void> {
  if ((await page.getAttribute("body", "data-menu")) !== "open")
    await page.click("#menu-toggle");
}

async function openSection(page: Page, selector: string): Promise<void> {
  await openMenu(page);
  await page
    .locator(selector)
    .evaluate((details) => ((details as HTMLDetailsElement).open = true));
}

/** Import the public test phrase, which signs the tab in. */
async function signIn(page: Page): Promise<void> {
  await openSection(page, "#import");
  await page.fill("#mnemonic", TEST_MNEMONIC);
  await page.click("#add-wallet");
  await page.locator("#session").filter({ hasNotText: "Signed out" }).waitFor();
}

async function openAddress(page: Page, address: string): Promise<void> {
  await page.fill("#address", address);
  await page.press("#address", "Enter");
}

/** The product page's frame once its own script ran. */
async function productFrame(page: Page, origin: string): Promise<Frame> {
  const deadline = Date.now() + STEP_MS;
  while (Date.now() < deadline) {
    const frame = page
      .frames()
      .find(
        (candidate) =>
          candidate.url().startsWith(origin) &&
          !candidate.url().includes("/truapi-sandbox/"),
      );
    const ready = await frame
      ?.evaluate(() => "__ask" in window)
      .catch(() => false);
    if (frame && ready) return frame;
    await page.waitForTimeout(200);
  }
  throw new Error(`no product page appeared at ${origin}`);
}

/** The page's own requests are not gated: each one reaches the target. */
async function expectRequestsAreNotGated(frame: Frame): Promise<void> {
  await frame.waitForFunction(
    () => (window as unknown as ProductWindow).__first.state !== "pending",
  );
  expect(
    await frame.evaluate(() => (window as unknown as ProductWindow).__first),
  ).toEqual({ state: "sent" });
  expect(
    await frame.evaluate(() =>
      (window as unknown as ProductWindow).__ask("/second"),
    ),
  ).toBe("sent");
  expect(target.hits).toEqual(["/first", "/second"]);
}

async function openUrlProduct(page: Page): Promise<Frame> {
  await openAddress(page, urlProduct.url);
  return productFrame(page, new URL(urlProduct.url).origin);
}

describe("a product opened by name", () => {
  test(
    "loads from its archive under the host's own path, and its requests are not gated",
    async () => {
      const context = await newContext();
      try {
        const page = await openHost(context);
        await signIn(page);
        await openAddress(page, NAME);
        const frame = await productFrame(page, `${host.origin}/product/`);

        // Same origin as the host, but a frame of its own: its web storage
        // starts empty and does not see the saved wallets.
        expect(
          await frame.evaluate(() => ({
            origin: location.origin,
            localKeys: document.body.dataset.localKeys,
            rootRelative: document.body.dataset.rootRelative,
          })),
        ).toEqual({
          origin: host.origin,
          localKeys: "[]",
          rootRelative: "loaded",
        });
        expect(await page.locator("#address-id").innerText()).toContain(NAME);

        const hostState = await page.evaluate(async () => ({
          keys: Object.keys(localStorage).length,
          workers: (await navigator.serviceWorker.getRegistrations()).length,
        }));
        expect(hostState.keys).toBeGreaterThan(0);
        expect(hostState.workers).toBe(0);

        await expectRequestsAreNotGated(frame);
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );
});

describe("a product opened by URL", () => {
  test(
    "loads from its own origin, with empty web storage, and its requests are not gated",
    async () => {
      const context = await newContext();
      try {
        const page = await openHost(context);
        await signIn(page);
        const frame = await openUrlProduct(page);

        expect(new URL(frame.url()).origin).not.toBe(host.origin);
        expect(
          await frame.evaluate(() => document.body.dataset.localKeys),
        ).toBe("[]");
        await expectRequestsAreNotGated(frame);
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );

  test(
    "the host refuses to open a product on its own origin",
    async () => {
      const context = await newContext();
      try {
        const page = await openHost(context);
        await signIn(page);
        await openAddress(page, host.origin);
        await page
          .locator("#product-status")
          .filter({ hasText: "own origin" })
          .waitFor();
        expect(await page.locator("#product-frame iframe").count()).toBe(0);
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );

  test(
    "opening the menu and inspector, or resizing, does not reload the product",
    async () => {
      const context = await newContext();
      try {
        const page = await openHost(context);
        await signIn(page);
        const frame = await openUrlProduct(page);

        const before = await frame.evaluate(() => {
          (window as unknown as ProductWindow).__mark = 7;
          return (window as unknown as ProductWindow).__loadedAt;
        });
        await page.locator("#product-frame iframe").evaluate((element) => {
          element.setAttribute("data-mark", "kept");
        });

        await page.click("#inspector-toggle");
        await page.click("#menu-toggle");
        await page.click("#menu-toggle");
        await page.click("#inspector-toggle");
        await page.selectOption("#viewport", { label: "Phone 393 × 852" });

        expect(
          await frame.evaluate(() => ({
            mark: (window as unknown as ProductWindow).__mark,
            loadedAt: (window as unknown as ProductWindow).__loadedAt,
            width: innerWidth,
          })),
        ).toEqual({ mark: 7, loadedAt: before, width: 393 });
        expect(
          await page.locator("#product-frame iframe").getAttribute("data-mark"),
        ).toBe("kept");
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );
});

describe("the product id", () => {
  test(
    "a product id entered in the menu is shown as entered, and Reset clears it",
    async () => {
      const context = await newContext();
      try {
        const page = await openHost(context);
        await page.fill("#address", urlProduct.url);
        const effective = (source: string) =>
          page.locator(`#product-id-effective[data-source="${source}"]`);
        await effective("derived").waitFor({ state: "attached" });

        await openMenu(page);
        await page.fill("#product-id", "entered.paseo");
        await effective("entered").waitFor({ state: "attached" });
        expect(await effective("entered").innerText()).toContain(
          "entered.paseo",
        );

        await page.click("#product-id-reset");
        await effective("derived").waitFor({ state: "attached" });
        expect(await page.inputValue("#product-id")).toBe("");
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );
});

/** What the loading pill showed, recorded in the page from before Open. */
interface LoadingLog {
  states: string[];
  texts: string[];
  inStage: boolean[];
}

async function recordLoading(page: Page): Promise<void> {
  await page.evaluate(() => {
    const log: LoadingLog = {
      states: [document.body.dataset.loading ?? ""],
      texts: [],
      inStage: [],
    };
    (window as unknown as { __loading: LoadingLog }).__loading = log;
    const pill = document.getElementById("stage-loading-pill")!;
    const stage = document.getElementById("stage")!;
    new MutationObserver(() => {
      const state = document.body.dataset.loading ?? "";
      if (state === log.states[log.states.length - 1]) return;
      log.states.push(state);
      if (state !== "on") return;
      const box = pill.getBoundingClientRect();
      const area = stage.getBoundingClientRect();
      log.inStage.push(
        box.width > 0 &&
          box.left >= area.left &&
          box.right <= area.right &&
          box.top >= area.top &&
          box.bottom <= area.bottom,
      );
    }).observe(document.body, {
      attributes: true,
      attributeFilter: ["data-loading"],
    });
    new MutationObserver(() => {
      const text = pill.innerText.trim();
      if (text !== "") log.texts.push(text);
    }).observe(document.getElementById("stage-loading-text")!, {
      childList: true,
    });
  });
}

const loadingLog = (page: Page): Promise<LoadingLog> =>
  page.evaluate(
    () => (window as unknown as { __loading: LoadingLog }).__loading,
  );

describe("the loading indicator", () => {
  for (const [label, viewport] of [
    ["desktop", { width: 1280, height: 800 }],
    ["mobile", { width: 390, height: 844 }],
  ] as const) {
    test(
      `${label}: shows from Open until the product's own document, and clears when a lookup fails`,
      async () => {
        const context = await newContext(viewport);
        try {
          const page = await openHost(context);
          await signIn(page);
          await recordLoading(page);

          await openAddress(page, NAME);
          const frame = await productFrame(page, `${host.origin}/product/`);
          await page.waitForFunction(
            () => document.body.dataset.loading === "off",
          );
          expect(await frame.evaluate(() => "__ask" in window)).toBe(true);
          const opened = await loadingLog(page);
          expect(opened).toEqual({
            states: ["off", "on", "off"],
            texts: ["Looking up name…", "Loading…"],
            inStage: [true],
          });
          expect(await page.locator("#stage-loading-pill").isHidden()).toBe(
            true,
          );

          // A click first: a fill goes to whichever frame holds focus, and the
          // product's frame does after it loads.
          await page.click("#address");
          await openAddress(page, "missing.paseo");
          await page
            .locator("#product-status")
            .filter({ hasText: "has no content" })
            .waitFor({ state: "attached" });
          expect(await page.getAttribute("body", "data-loading")).toBe("off");
          expect((await loadingLog(page)).states).toEqual([
            "off",
            "on",
            "off",
            "on",
            "off",
          ]);
        } finally {
          await context.close();
        }
      },
      TEST_MS,
    );
  }
});
