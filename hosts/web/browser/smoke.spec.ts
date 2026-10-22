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
} from "playwright-core";
import {
  type CountingTarget,
  type PublishedApp,
  type RunningHost,
  type UrlProduct,
  TEST_MNEMONIC,
  buildChatWorker,
  buildFixture,
  productHtml,
  publishApp,
  serveName,
  serveNames,
  startCountingTarget,
  startHost,
  startUrlProduct,
  testBrowser,
} from "./fixtures.js";
import { PASEO_DOTNS } from "../src/dotns.js";

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
  browser = await testBrowser();
}, STEP_MS);

afterAll(async () => {
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
  await signInWith(page, TEST_MNEMONIC);
}

/**
 * A BIP-39 test vector phrase, for checks the public development phrase is
 * refused for. It holds nothing.
 */
const SYNTHETIC_MNEMONIC =
  "letter advice cage absurd amount doctor acoustic avoid letter advice cage above";

async function signInWith(page: Page, mnemonic: string): Promise<void> {
  await openSection(page, "#import");
  await page.fill("#mnemonic", mnemonic);
  await page.click("#add-wallet");
  await page.locator("#session").filter({ hasNotText: "Signed out" }).waitFor();
}

async function openAddress(page: Page, address: string): Promise<void> {
  await page.fill("#address", address);
  // `fill` reports success on a field that cannot take focus, such as one
  // under a panel that covers it, and the text then goes nowhere.
  expect(await page.inputValue("#address")).toBe(address);
  await page.press("#address", "Enter");
}

/** Whether the address bar is inert, as it is while a panel covers it. */
const addressInert = (page: Page): Promise<boolean> =>
  page.evaluate(
    () => document.getElementById("address")!.closest("[inert]") !== null,
  );

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
          if (label === "mobile") {
            // On a narrow screen the menu covers the product, and the address
            // bar takes no input until the menu is closed, as for a user.
            expect(await addressInert(page)).toBe(true);
            await page.click('[data-close="menu"]');
          }
          expect(await addressInert(page)).toBe(false);
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

describe("the dock", () => {
  const destinations = (page: Page) =>
    page.evaluate(() =>
      Object.fromEntries(
        [...document.querySelectorAll<HTMLElement>("#dock button")].map(
          (button) => [
            button.dataset.destination,
            `${button.getAttribute("aria-current") === "true" ? "front" : "behind"}/${button.dataset.live === "true" ? "running" : "idle"}`,
          ],
        ),
      ),
    );

  test(
    "keeps Browse's product running while a host screen is in front",
    async () => {
      const context = await newContext();
      try {
        await context.addInitScript(
          (addresses) => {
            if (!localStorage.getItem("truapi-web-signing-host:dock:v1"))
              localStorage.setItem(
                "truapi-web-signing-host:dock:v1",
                JSON.stringify(addresses),
              );
          },
          { browse: urlProduct.url },
        );
        const page = await openHost(context);
        await signIn(page);
        expect(await destinations(page)).toEqual({
          chat: "behind/idle",
          browse: "behind/idle",
          pocket: "behind/idle",
        });

        await page.click('#dock [data-destination="browse"]');
        const browse = await productFrame(page, new URL(urlProduct.url).origin);
        await browse.evaluate(() => {
          (window as unknown as ProductWindow).__mark = 42;
        });

        // Chat is drawn by the host. The product behind it keeps running, hidden.
        await page.click('#dock [data-destination="chat"]');
        await page.locator("#chat-screen").waitFor({ state: "visible" });
        expect(await destinations(page)).toEqual({
          chat: "front/idle",
          browse: "behind/running",
          pocket: "behind/idle",
        });
        expect(
          await page
            .locator("#product-slot iframe")
            .evaluateAll((frames) =>
              frames.map((frame) => (frame as HTMLElement).style.display),
            ),
        ).toEqual(["none"]);

        await page.click('#dock [data-destination="pocket"]');
        await page.locator("#pocket-screen").waitFor({ state: "visible" });
        expect(await page.locator("#chat-screen").isHidden()).toBe(true);
        expect(await destinations(page)).toEqual({
          chat: "behind/idle",
          browse: "behind/running",
          pocket: "front/idle",
        });

        await page.click('#dock [data-destination="browse"]');
        expect(
          await browse.evaluate(
            () => (window as unknown as ProductWindow).__mark,
          ),
        ).toBe(42);
        expect(await page.inputValue("#address")).toBe(urlProduct.url);
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );

  test(
    "ends a held product when the wallet changes",
    async () => {
      const context = await newContext();
      try {
        await context.addInitScript(
          (addresses) => {
            if (!localStorage.getItem("truapi-web-signing-host:dock:v1"))
              localStorage.setItem(
                "truapi-web-signing-host:dock:v1",
                JSON.stringify(addresses),
              );
          },
          { browse: urlProduct.url },
        );
        const page = await openHost(context);
        await signIn(page);
        await page.click('#dock [data-destination="browse"]');
        await productFrame(page, new URL(urlProduct.url).origin);
        await page.click('#dock [data-destination="chat"]');
        expect(await page.locator("#product-slot iframe").count()).toBe(1);

        await openMenu(page);
        await page.click("#sign-out");
        await page.waitForFunction(
          () => document.querySelectorAll("#product-slot iframe").length === 0,
        );
        // Nothing was in front, so nothing is reopened for the new session.
        expect(await destinations(page)).toEqual({
          chat: "front/idle",
          browse: "behind/idle",
          pocket: "behind/idle",
        });
        // The screen follows the session change, which lands after the frame closes.
        await page
          .locator(".chat-empty-title")
          .filter({ hasText: "Sign in to chat" })
          .waitFor();
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );

  test(
    "refuses Chat for the public phrase and stores nothing",
    async () => {
      const context = await newContext();
      try {
        const page = await openHost(context);
        await page.click('#dock [data-destination="chat"]');
        await page.locator("#chat-screen").waitFor({ state: "visible" });
        expect(await page.locator("#product-slot iframe").count()).toBe(0);
        expect(await page.locator(".chat-empty-title").innerText()).toBe(
          "Sign in to chat",
        );

        await signIn(page);
        await page
          .locator(".chat-empty-title")
          .filter({ hasText: "Chat is unavailable" })
          .waitFor();
        expect(await page.locator(".chat-empty-body").innerText()).toContain(
          "public development phrase",
        );
        expect(
          await page.getAttribute("#chat-screen", "data-chat-engine"),
        ).toBe("off");
        expect(
          await page.evaluate(() =>
            Object.keys(localStorage).filter((key) => key.includes(":chat:")),
          ),
        ).toEqual([]);
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );

  test(
    "starts Chat at sign-in, with Chat hidden, and stops it at sign-out",
    async () => {
      const context = await newContext();
      // No chain traffic: the light client gets no peer, so nothing is read
      // or published, while the engine still starts and subscribes.
      await context.routeWebSocket(/.*/, (socket) => socket.close());
      try {
        const page = await openHost(context);
        await signInWith(page, SYNTHETIC_MNEMONIC);
        // Chat runs for the session while Browse or Pocket is in front.
        await page.waitForFunction(
          () =>
            document.querySelector<HTMLElement>("#chat-screen")?.dataset
              .chatEngine !== "off",
        );
        expect(await page.locator("#chat-screen").isHidden()).toBe(true);

        await page.click('#dock [data-destination="chat"]');
        // The host derives the identity and chat key and does not run when
        // either differs from what the core reports.
        const who = page.locator(".chat-identity-who");
        await who.waitFor();
        const identity = (await page.getAttribute("#session", "title"))?.match(
          /Identity account (0x[0-9a-f]{64})/,
        )?.[1];
        expect(identity).toBeDefined();
        expect(await who.getAttribute("title")).toContain(
          `Identity account ${identity}`,
        );
        // There are no controls to turn chat or writes on.
        expect(
          await page.locator('[data-action^="chat-"], .chat-toggle').count(),
        ).toBe(0);

        // The draft worker-owned chat is off until turned on, then runs in a
        // Web Worker of the host's own, beside Nova in the same list.
        expect(await page.locator('[data-provider="draft"]').count()).toBe(0);
        await page.click('[data-action="manage-workers"]');
        await page.check('[data-action="draft-chat"]');
        await page.click('[aria-label="Back to chats"]');
        await page.locator('[data-conversation="draft/ada"]').click();
        await page.locator('[data-message-id="ada-120"]').waitFor();
        await page.click('[aria-label="Back to chats"]');

        // The inspector's Chat view lists the engine's events with no product
        // open, apart from the TrUAPI traces.
        await page.click("#inspector-toggle");
        await page.click('[data-inspector-view="chat"]');
        await page.locator("#chat-diagnostics").waitFor({ state: "visible" });
        expect(await page.locator("#inspector-mount").isHidden()).toBe(true);
        await page
          .locator(".chat-diag-row", { hasText: "starting" })
          .first()
          .waitFor();
        expect(await page.locator("#product-slot iframe").count()).toBe(0);
        const diagnostics = await page.locator("#chat-diagnostics").innerText();
        expect(diagnostics).not.toContain(identity!);
        await page.click('[data-inspector-view="wire"]');
        expect(await page.locator("#inspector-mount").isVisible()).toBe(true);
        await page.click('[data-inspector-view="chat"]');

        await openMenu(page);
        await page.click("#sign-out");
        await page
          .locator(".chat-empty-title")
          .filter({ hasText: "Sign in to chat" })
          .waitFor();
        expect(
          await page.getAttribute("#chat-screen", "data-chat-engine"),
        ).toBe("off");
        // The signed-out session's events are gone.
        expect(await page.locator(".chat-diag-row").count()).toBe(0);
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );

  test(
    "draws Pocket itself, as two cards, and gives the digital dollar no amount",
    async () => {
      const context = await newContext();
      try {
        const page = await openHost(context);
        await page.click('#dock [data-destination="pocket"]');
        await page.locator("#pocket-screen").waitFor({ state: "visible" });
        expect(await page.locator("#product-slot iframe").count()).toBe(0);

        // The native list: the digital dollar, then Humanity, stacked.
        expect(
          await page
            .locator(".pocket-card")
            .evaluateAll((cards) =>
              cards.map((card) => (card as HTMLElement).dataset.card),
            ),
        ).toEqual(["digital_dollar_card", "humanity"]);
        expect(await page.locator(".pocket-title").innerText()).toBe("Pocket");
        expect(await page.locator(".pocket-face").innerText()).toBe(
          "Humanity\nProof of personhood",
        );

        // No Coinage engine here, so the amount slot carries no figure.
        const amount = page.locator(".pocket-amount");
        expect(await amount.innerText()).toBe("Unavailable");
        expect(await amount.getAttribute("data-state")).toBe("unavailable");
        expect(await page.locator(".pocket-dollar").innerText()).not.toMatch(
          /[$\d]/,
        );

        // Signing in changes nothing: neither card reads the wallet.
        await signIn(page);
        expect(await page.locator(".pocket-card").count()).toBe(2);
        expect(await amount.innerText()).toBe("Unavailable");

        // An address typed over Pocket opens a product and hides the screen.
        await openAddress(page, urlProduct.url);
        await productFrame(page, new URL(urlProduct.url).origin);
        expect(await page.locator("#pocket-screen").isHidden()).toBe(true);
        expect(await destinations(page)).toEqual({
          chat: "behind/idle",
          browse: "behind/idle",
          pocket: "behind/idle",
        });
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );
});

describe("product worker chat", () => {
  const WORKER_NAME = "fixturebot.paseo";
  const MANIFEST = JSON.stringify({
    $v: 1,
    kind: "worker",
    appVersion: [0, 1, 0],
    entrypoint: "index.js",
    includes: { chat: true },
  });

  test(
    "runs a worker, shows its room and card, and carries actions and messages both ways",
    async () => {
      const worker = publishApp({
        "index.js": await buildChatWorker(),
        "images/dot.svg":
          '<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><circle cx="8" cy="8" r="8"/></svg>',
      });
      const context = await browser.newContext();
      context.setDefaultTimeout(STEP_MS);
      await serveNames(context, [
        {
          record: `worker.${WORKER_NAME}`,
          app: worker,
          text: { executable: MANIFEST },
        },
      ]);
      // The worker frame asks only this host's origin for anything, apart
      // from the loader fetching the archive from the stood-in gateway.
      const outside: string[] = [];
      context.on("request", (request) => {
        if (request.serviceWorker() !== null) return;
        if (!request.frame().url().includes("/product/")) return;
        const origin = new URL(request.url()).origin;
        if (origin !== host.origin && origin !== PASEO_DOTNS.contentGateway)
          outside.push(request.url());
      });
      try {
        const page = await openHost(context);
        await signIn(page);
        await page.click('#dock [data-destination="chat"]');
        await page.click('[data-action="manage-workers"]');
        await page.fill('[data-focus="worker-name"]', WORKER_NAME);
        await page.press('[data-focus="worker-name"]', "Enter");
        await page
          .locator(`[data-worker="${WORKER_NAME}"][data-state="running"]`)
          .waitFor();
        expect(await page.locator("#worker-frames iframe").count()).toBe(1);

        await page.click('[aria-label="Back to chats"]');
        const row = page.locator(
          `[data-conversation="product:${WORKER_NAME}/fixture"]`,
        );
        await row.waitFor();
        expect(await row.innerText()).toContain("Fixture bot");
        await row.click();

        // Text from the worker, and its card drawn by the worker through the core.
        await page
          .locator(".chat-bubble-text", { hasText: "hello from the fixture" })
          .waitFor();
        const card = page.locator(
          '.product-card[data-message-type="fixture.counter"]',
        );
        await card.locator(".pr-text", { hasText: "count 0" }).waitFor();
        // Text in the tree stays text.
        expect(await card.locator("b").count()).toBe(0);
        expect(await card.innerText()).toContain("<b>not markup</b>");
        // An image from the worker's archive is read from the storage of the
        // worker's frame, which a credentialless frame keeps apart from this page's.
        await page.waitForFunction(() => {
          const image = document.querySelector<HTMLImageElement>(
            '.product-card[data-message-type="fixture.counter"] img.pr-image',
          );
          return image?.complete === true && image.naturalWidth === 16;
        });

        // A press reaches the worker, and its new tree replaces the card.
        await card.locator("button", { hasText: "bump" }).click();
        await card.locator(".pr-text", { hasText: "count 1" }).waitFor();

        // The room has a text input: what the user types reaches the worker.
        await page.fill(".chat-composer-input", "hi there");
        await page.press(".chat-composer-input", "Enter");
        await page
          .locator(".chat-message-out .chat-bubble-text", {
            hasText: "hi there",
          })
          .waitFor();
        await page
          .locator(".chat-bubble-text", { hasText: "echo: hi there" })
          .waitFor();

        const workerFrame = page
          .frames()
          .find((frame) => frame.url().includes("__truapi-worker.html"));
        const seen = await workerFrame!.evaluate(
          () => (window as unknown as { __fixture: unknown }).__fixture,
        );
        expect(seen).toMatchObject({
          registerBot: "refused",
          room: "created",
          actions: ["bump"],
          messages: ["hi there"],
          errors: [],
        });
        expect(outside).toEqual([]);

        // The worker's frames are TrUAPI frames, so the inspector decodes them.
        await page.click("#inspector-toggle");
        const traces = await page.locator("#inspector-mount").innerText();
        for (const operation of [
          "chat.createRoom",
          "chat.postMessage",
          "renderer.render",
          "renderer.actionSubscribe",
          "chat.actionSubscribe",
        ])
          expect(traces).toContain(operation);

        // The rooms are this wallet's on this network, and nothing else is.
        const keys = await page.evaluate(() =>
          Object.keys(localStorage).filter(
            (key) =>
              key.includes(":product-chat:") || key.includes(":workers:"),
          ),
        );
        expect(keys.length).toBe(2);
        expect(keys.every((key) => key.includes(":network:paseo:"))).toBe(true);

        // Signing out stops the worker and hides its rooms.
        await openMenu(page);
        await page.click("#sign-out");
        await page.waitForFunction(
          () => document.querySelectorAll("#worker-frames iframe").length === 0,
        );
        await page
          .locator(".chat-empty-title")
          .filter({ hasText: "Sign in to chat" })
          .waitFor();
        expect(await page.locator('[data-provider^="product:"]').count()).toBe(
          0,
        );

        // Another wallet has no workers and no product rooms.
        await signInWith(page, SYNTHETIC_MNEMONIC);
        await page.locator('[data-action="manage-workers"]').waitFor();
        expect(await page.locator('[data-provider^="product:"]').count()).toBe(
          0,
        );
        expect(await page.locator("#worker-frames iframe").count()).toBe(0);

        // Back on the first wallet, its saved worker starts again and its
        // room keeps what was said in it.
        await signIn(page);
        await row.waitFor();
        await page.waitForFunction(
          () => document.querySelectorAll("#worker-frames iframe").length === 1,
        );
        await row.click();
        await page
          .locator(".chat-message-out .chat-bubble-text", {
            hasText: "hi there",
          })
          .waitFor();
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );
});

describe("Pocket's Humanity card", () => {
  const PRODUCT = "peopl.paseo";
  const WORKER = JSON.stringify({
    $v: 1,
    kind: "worker",
    appVersion: [0, 1, 0],
    entrypoint: "index.js",
    includes: { chat: false, pocket: true },
    pocket: {
      cards: [{ id: "humanity", title: "Humanity", preview: "face.json" }],
    },
  });
  const WIDGET = JSON.stringify({
    $v: 1,
    kind: "widget",
    appVersion: [0, 1, 0],
    dimensions: { height: [8] },
  });
  const APP = JSON.stringify({ $v: 1, kind: "app", appVersion: [0, 1, 0] });
  const APP_HTML = "<!doctype html><title>app</title><p>the product's app</p>";
  const ROOT = JSON.stringify({
    $v: 1,
    displayName: "Humanity",
    description: "A synthetic personhood product.",
    icon: { cid: "bafkfixtureicon", format: "png" },
  });

  /** A Widget built from `browser/pocket-widget.ts`. */
  async function publishWidget() {
    return publishApp({
      "index.html":
        '<!doctype html><title>widget</title><script type="module" src="./widget.js"></script>',
      "widget.js": await buildFixture("pocket-widget.ts"),
    });
  }

  /** What the Widget page saw, once it has asked the core. */
  async function widgetSeen(frame: Frame): Promise<unknown> {
    await frame.waitForFunction(() => "__widget" in window);
    return frame.evaluate(
      () => (window as unknown as { __widget: unknown }).__widget,
    );
  }

  /**
   * A browser profile with no way out: every socket is closed, so the light
   * client reaches no chain, and every request beyond this host and the
   * stood-in Asset Hub and gateway is refused and listed.
   */
  async function offlineContext(
    records: Parameters<typeof serveNames>[1],
  ): Promise<{ context: BrowserContext; blocked: string[] }> {
    const context = await browser.newContext();
    context.setDefaultTimeout(STEP_MS);
    await context.routeWebSocket(/.*/, (socket) => socket.close());
    const blocked: string[] = [];
    await context.route(
      (url) => url.origin !== host.origin,
      (route) => {
        blocked.push(route.request().url());
        return route.abort();
      },
    );
    await serveNames(context, records);
    return { context, blocked };
  }

  const humanity = (page: Page) =>
    page.locator('.pocket-product[data-card="humanity"]');

  async function frameWhere(
    page: Page,
    matches: (url: string) => boolean,
  ): Promise<Frame> {
    const deadline = Date.now() + STEP_MS;
    while (Date.now() < deadline) {
      const frame = page.frames().find((candidate) => matches(candidate.url()));
      if (frame) return frame;
      await page.waitForTimeout(200);
    }
    throw new Error("no such frame appeared");
  }

  test(
    "draws the face the worker streams, sends presses back, and opens the product's Widget",
    async () => {
      const worker = publishApp({
        "index.js": await buildFixture("pocket-worker.ts"),
      });
      const widget = await publishWidget();
      const app = publishApp({ "index.html": APP_HTML });
      const { context, blocked } = await offlineContext([
        {
          record: `worker.${PRODUCT}`,
          app: worker,
          text: { executable: WORKER },
        },
        {
          record: `widget.${PRODUCT}`,
          app: widget,
          text: { executable: WIDGET },
        },
        { record: `app.${PRODUCT}`, app, text: { executable: APP } },
        { record: PRODUCT, app, text: { manifest: ROOT } },
      ]);
      try {
        const page = await openHost(context);
        await signIn(page);
        // Nothing runs for Pocket before Pocket is shown.
        expect(await page.locator("#worker-frames iframe").count()).toBe(0);

        await page.click('#dock [data-destination="pocket"]');
        const card = humanity(page);
        expect(await card.getAttribute("data-face")).toBe("bundled");
        expect(await card.innerText()).toContain("Proof of personhood");
        // The worker draws the face once it has booted; the host asks again
        // until it does.
        await card
          .locator(".pr-text", { hasText: "fixture face, taps 0" })
          .waitFor();
        expect(await card.getAttribute("data-face")).toBe("live");
        expect(await card.locator(".pocket-face").isHidden()).toBe(true);

        // A press inside the face goes to the worker and opens nothing.
        await card.locator("button", { hasText: "tap" }).click();
        await card.locator(".pr-text", { hasText: "taps 1" }).waitFor();
        expect(await page.locator("#product-slot iframe").count()).toBe(0);
        const workerFrame = await frameWhere(page, (url) =>
          url.includes("__truapi-worker.html"),
        );
        const seenByWorker = (await workerFrame.evaluate(
          () =>
            (window as unknown as { __pocketWorker: unknown }).__pocketWorker,
        )) as { account: string };
        expect(seenByWorker).toMatchObject({
          cards: ["humanity:pinned"],
          renders: ["humanity"],
          actions: ["PocketCard:tap"],
          errors: [],
        });

        // Pressing the card opens the product's Widget at the card, as the
        // product: the worker's account and storage, and a Widget execution.
        await card.click({ position: { x: 24, y: 120 } });
        const widgetFrame = await frameWhere(
          page,
          (url) =>
            url.includes(`/${widget.contentCid}/`) &&
            url.includes("card=humanity"),
        );
        expect(await widgetSeen(widgetFrame)).toEqual({
          card: "humanity",
          account: seenByWorker.account,
          stored: "0x776f726b6572",
          faceShown: "Unsupported",
          errors: [],
        });
        expect(await widgetFrame.title()).toBe("widget");
        expect(await page.locator("#pocket-screen").isHidden()).toBe(true);

        // Choosing Pocket ends the Widget, and the face is still live.
        await page.click('#dock [data-destination="pocket"]');
        await page.waitForFunction(
          () => document.querySelectorAll("#product-slot iframe").length === 0,
        );
        await card.locator(".pr-text", { hasText: "taps 1" }).waitFor();

        // The worker's frames are TrUAPI frames, so the inspector decodes them.
        await page.click("#inspector-toggle");
        const traces = await page.locator("#inspector-mount").innerText();
        for (const operation of [
          "renderer.render",
          "renderer.actionSubscribe",
          "pocket.listSubscribe",
        ])
          expect(traces).toContain(operation);
        await page.click("#inspector-toggle");

        // The face is kept for this wallet on this network only.
        expect(
          await page.evaluate(() =>
            Object.keys(localStorage).filter((key) =>
              key.includes(":pocket-faces:"),
            ),
          ),
        ).toEqual([expect.stringContaining(":network:paseo:pocket-faces:")]);

        // Signing out stops the worker and puts the bundled face back.
        await openMenu(page);
        await page.click("#sign-out");
        await page.waitForFunction(
          () => document.querySelectorAll("#worker-frames iframe").length === 0,
        );
        expect(await card.getAttribute("data-face")).toBe("bundled");

        // Another wallet sees no face kept by the first.
        await signInWith(page, SYNTHETIC_MNEMONIC);
        expect(await card.getAttribute("data-face")).toBe("bundled");
        await card.locator(".pr-text", { hasText: "taps 0" }).waitFor();

        // Back on the first wallet, its kept face is drawn before the worker
        // answers, then the worker's own.
        await signIn(page);
        await page
          .locator('.pocket-product[data-card="humanity"][data-face="kept"]')
          .waitFor();
        expect(await card.innerText()).toContain("taps 1");
        await card.locator(".pr-text", { hasText: "taps 0" }).waitFor();
        expect(await card.getAttribute("data-face")).toBe("live");

        expect(blocked).toEqual([]);
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );

  /** A worker that offers another card, so Pocket does not start it for Humanity. */
  async function otherCardWorker() {
    return {
      record: `worker.${PRODUCT}`,
      app: publishApp({ "index.js": await buildFixture("pocket-worker.ts") }),
      text: {
        executable: JSON.stringify({
          ...JSON.parse(WORKER),
          pocket: {
            cards: [{ id: "other", title: "Other", preview: "o.json" }],
          },
        }),
      },
    };
  }

  test(
    "opens the base name's content as the Widget when the product publishes no widget, as iOS does",
    async () => {
      const base = await publishWidget();
      const app = publishApp({ "index.html": APP_HTML });
      const { context, blocked } = await offlineContext([
        await otherCardWorker(),
        { record: `app.${PRODUCT}`, app, text: { executable: APP } },
        { record: PRODUCT, app: base, text: { manifest: ROOT } },
      ]);
      try {
        const page = await openHost(context);
        await signIn(page);
        await page.click('#dock [data-destination="pocket"]');
        // A worker that draws no humanity card is not started for it.
        await page
          .locator("#log")
          .filter({ hasText: "draws no humanity card" })
          .waitFor({ state: "attached" });
        expect(await page.locator("#worker-frames iframe").count()).toBe(0);
        const card = humanity(page);
        expect(await card.getAttribute("data-face")).toBe("bundled");

        // The base name's content, not the app's, runs as the Widget at the card.
        await card.click();
        const frame = await frameWhere(
          page,
          (url) =>
            url.includes(`/${base.contentCid}/`) &&
            url.includes("card=humanity"),
        );
        expect(await widgetSeen(frame)).toMatchObject({
          card: "humanity",
          faceShown: "Unsupported",
          errors: [],
        });
        expect(
          page.frames().some((other) => other.url().includes(app.contentCid)),
        ).toBe(false);
        await page
          .locator("#log")
          .filter({ hasText: `${PRODUCT} publishes no widget; its base name` })
          .waitFor({ state: "attached" });

        // A session change closes the card's product and opens it again, as
        // the Widget it was, not as the product's App.
        await signInWith(page, SYNTHETIC_MNEMONIC);
        await page.waitForFunction(
          () => document.querySelectorAll("#product-slot iframe").length === 1,
        );
        expect(frame.isDetached()).toBe(true);
        const reopened = await frameWhere(
          page,
          (url) =>
            url.includes(`/${base.contentCid}/`) &&
            url.includes("card=humanity"),
        );
        expect(await widgetSeen(reopened)).toMatchObject({
          card: "humanity",
          faceShown: "Unsupported",
        });
        expect(blocked).toEqual([]);
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );

  test(
    "opens nothing, and says why, when neither a widget nor the base name has content",
    async () => {
      const app = publishApp({ "index.html": APP_HTML });
      const { context, blocked } = await offlineContext([
        await otherCardWorker(),
        { record: `app.${PRODUCT}`, app, text: { executable: APP } },
        { record: PRODUCT, text: { manifest: ROOT } },
      ]);
      try {
        const page = await openHost(context);
        await signIn(page);
        await page.click('#dock [data-destination="pocket"]');
        await humanity(page).click();
        await page
          .locator(".pocket-status")
          .filter({ hasText: "publishes no widget and has no content" })
          .waitFor();
        expect(await page.locator("#product-slot iframe").count()).toBe(0);
        expect(await page.locator("#pocket-screen").isVisible()).toBe(true);
        expect(blocked).toEqual([]);
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );
});
