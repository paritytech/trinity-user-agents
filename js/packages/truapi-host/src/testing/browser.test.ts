// Drives the test host page in a real browser, with a real product on the
// other side of the iframe handshake. The unit suites fake the MessagePort and
// the core; what they cannot show is that a product's own `@parity/truapi`
// reconnects through `createIframeHost` and reaches the core again, or that a
// held login travels from the product to the core and back.
import {
  afterAll,
  beforeAll,
  describe,
  expect,
  it,
  setDefaultTimeout,
} from "bun:test";
import { existsSync } from "node:fs";
import { createServer, type Server } from "node:http";
import { fileURLToPath } from "node:url";
import {
  chromium,
  type Browser,
  type Frame,
  type Page,
} from "@playwright/test";

import { hostPageUrl, type HostPageConfig } from "./host-page-url.js";
import { wasmIsBuilt } from "./require-wasm.js";
import { createTestHostServer, type TestHostServer } from "./server.js";

/** Whether Chromium is installed, failing under `REQUIRE_WASM=1` as the WASM gate does. */
function chromiumIsInstalled(): boolean {
  if (existsSync(chromium.executablePath())) return true;
  if (process.env.REQUIRE_WASM === "1") {
    throw new Error(
      "REQUIRE_WASM=1 but Chromium is missing. Run `npx playwright install chromium`.",
    );
  }
  return false;
}

const suite =
  wasmIsBuilt("testing/truapi_server.js") && chromiumIsInstalled()
    ? describe
    : describe.skip;

// The product connects on load, and answers a login request with the host's
// verdict, so a test can read what reached it.
const PRODUCT_SOURCE = `
  import { getClientSync } from "@parity/truapi/sandbox";
  getClientSync();
  window.requestLogin = async () => {
    const result = await getClientSync().account.requestLogin({});
    return result.isOk() ? result.value : "error";
  };
`;

/** Serve the product: one script, and an HTML page at every path. */
async function serveProduct(): Promise<{
  url: string;
  close(): Promise<void>;
}> {
  const { build } = await import("esbuild");
  const bundled = await build({
    stdin: {
      contents: PRODUCT_SOURCE,
      resolveDir: fileURLToPath(new URL(".", import.meta.url)),
    },
    bundle: true,
    format: "esm",
    platform: "browser",
    write: false,
  });
  const script = bundled.outputFiles[0]!.text;
  const server: Server = createServer((req, res) => {
    if (req.url === "/product.js") {
      res.writeHead(200, { "Content-Type": "text/javascript" });
      res.end(script);
      return;
    }
    res.writeHead(200, { "Content-Type": "text/html; charset=utf-8" });
    res.end('<!doctype html><script type="module" src="/product.js"></script>');
  });
  await new Promise<void>((done) => server.listen(0, "127.0.0.1", done));
  const address = server.address();
  if (!address || typeof address === "string") throw new Error("no port");
  return {
    url: `http://127.0.0.1:${address.port}/`,
    close: () => new Promise((done) => server.close(() => done())),
  };
}

// Each case boots the core in Chromium, which the default 5 s does not cover
// on a loaded runner.
setDefaultTimeout(60_000);

suite("the test host in a browser", () => {
  let browser: Browser;
  let host: TestHostServer;
  let product: { url: string; close(): Promise<void> };

  beforeAll(async () => {
    [browser, host, product] = await Promise.all([
      chromium.launch(),
      createTestHostServer(),
      serveProduct(),
    ]);
  });
  afterAll(async () => {
    await Promise.all([browser?.close(), host?.close(), product?.close()]);
  });

  /** Open the host page with the product in it, once both are connected. */
  async function open(
    config: Omit<HostPageConfig, "productUrl"> = {},
    page?: Page,
  ): Promise<Page> {
    page ??= await browser.newPage();
    await page.goto(
      hostPageUrl(host.url, { productUrl: product.url, ...config }),
    );
    await page.waitForFunction(() => !!window.__TRUAPI_TEST_HOST__, {
      timeout: 30_000,
    });
    return page;
  }

  /** The product's frame, waiting for the document at `path` to load. */
  async function productFrame(page: Page, path = "/"): Promise<Frame> {
    const url = new URL(path, product.url).href;
    const deadline = Date.now() + 30_000;
    while (Date.now() < deadline) {
      const frame = page.frames().find((candidate) => candidate.url() === url);
      if (frame) {
        await frame.waitForFunction(() => "requestLogin" in window);
        return frame;
      }
      await page.waitForTimeout(50);
    }
    throw new Error(`the product never loaded ${url}`);
  }

  const requestLogin = (frame: Frame) =>
    frame.evaluate(() =>
      (window as unknown as { requestLogin(): Promise<string> }).requestLogin(),
    );

  it("reaches the core again after the product navigates its document", async () => {
    const page = await open();
    try {
      expect(await requestLogin(await productFrame(page))).toBe(
        "AlreadyConnected",
      );

      await (
        await productFrame(page)
      ).evaluate(() => {
        window.location.href = "/next";
      });
      expect(await requestLogin(await productFrame(page, "/next"))).toBe(
        "AlreadyConnected",
      );
    } finally {
      await page.close();
    }
  });

  it("holds a signed-out product's login until the test answers it", async () => {
    const page = await open({ loginBehavior: "manual" });
    const control = (
      name: "approveLogin" | "declineLogin" | "reset" | "getActiveAccount",
      ...args: string[]
    ) =>
      page.evaluate(
        ([method, rest]) =>
          (
            window.__TRUAPI_TEST_HOST__ as unknown as Record<
              string,
              (...values: string[]) => Promise<unknown>
            >
          )[method]!(...rest),
        [name, args] as const,
      );
    try {
      const frame = await productFrame(page);

      // A decline answers the request whichever reaches the host first.
      const declined = requestLogin(frame);
      await control("declineLogin");
      expect(await declined).toBe("Rejected");

      // Approving signs in before it answers, so a request racing it can find
      // the session first. Given before the request, the answer is certain.
      await control("approveLogin", "alice");
      expect(await requestLogin(frame)).toBe("Success");

      // A reset signs the host out again, so the next case starts held.
      await control("reset");
      expect(await control("getActiveAccount")).toBeUndefined();
    } finally {
      await page.close();
    }
  });

  it("serves a second tab on the same host URL as the same account", async () => {
    // Everything the host needs is in the URL, so a suite opening the product
    // twice, say for two peers, gets two hosts signing with one product account.
    const [first, second] = await Promise.all([open(), open()]);
    const address = (page: Page) =>
      page.evaluate(() =>
        window.__TRUAPI_TEST_HOST__!.getProductAccountAddress(),
      );
    try {
      expect([
        await requestLogin(await productFrame(first)),
        await requestLogin(await productFrame(second)),
        await address(second),
      ]).toEqual([
        "AlreadyConnected",
        "AlreadyConnected",
        await address(first),
      ]);
    } finally {
      await Promise.all([first.close(), second.close()]);
    }
  });

  it("runs on a page without IndexedDB", async () => {
    // A suite stubbing it for its product with `addInitScript` stubs it for
    // the host page too.
    const page = await browser.newPage();
    await page.addInitScript(() => {
      Object.defineProperty(window, "indexedDB", { value: undefined });
    });
    try {
      await open({}, page);
      expect([
        await page.evaluate(() => typeof window.indexedDB),
        await requestLogin(await productFrame(page)),
      ]).toEqual(["undefined", "AlreadyConnected"]);
    } finally {
      await page.close();
    }
  });
});
