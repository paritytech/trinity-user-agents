// Browser check of the sandbox's origin boundary, against the real server
// plugin, loader and worker. Run it with `npm run test:sandbox` from
// hosts/web-signing. It needs a Chromium: playwright-core finds the one it
// installs, or set CHROMIUM_PATH to a Chromium or headless-shell binary, and
// PLAYWRIGHT_CORE to another playwright-core's index.mjs. Everything runs on
// loopback with a harmless sentinel standing in for wallet storage.
import { createServer as createHttp, request as httpRequest } from "node:http";
import { createServer as createTcp } from "node:net";
import { fileURLToPath } from "node:url";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { build, createServer as createVite } from "vite";
import { sandboxAssets } from "../../../sandbox-plugin.ts";
import { productLabel, sandboxUrl } from "../origin.ts";
import { cidToString } from "../../archive/cid.ts";
import { Blocks } from "../../archive/fixtures.ts";

const { chromium } = await import(
  process.env.PLAYWRIGHT_CORE ?? "playwright-core"
);
const hostRoot = fileURLToPath(new URL("../../../", import.meta.url));
const SENTINEL = "SENTINEL-NOT-A-MNEMONIC";

const results = [];
function check(name, ok, detail = "") {
  results.push({ name, ok });
  console.log(`${ok ? "PASS" : "FAIL"}  ${name}${ok ? "" : `  -- ${detail}`}`);
}

async function freePorts(count) {
  const servers = await Promise.all(
    Array.from(
      { length: count },
      () =>
        new Promise((resolve) => {
          const server = createTcp().listen(0, "127.0.0.1", () =>
            resolve(server),
          );
        }),
    ),
  );
  const ports = servers.map((server) => server.address().port);
  await Promise.all(
    servers.map((server) => new Promise((done) => server.close(done))),
  );
  return ports;
}

async function freeRun(count) {
  for (
    let base = 31000 + Math.floor(Math.random() * 20000);
    ;
    base += count + 1
  ) {
    const ports = Array.from({ length: count }, (_, i) => base + i);
    const servers = [];
    try {
      for (const port of ports)
        servers.push(
          await new Promise((resolve, reject) => {
            const server = createTcp()
              .once("error", reject)
              .listen(port, "127.0.0.1", () => resolve(server));
          }),
        );
    } catch {
      continue;
    } finally {
      await Promise.all(
        servers.map((server) => new Promise((done) => server.close(done))),
      );
    }
    return ports;
  }
}

// A product: one page that reports what it can see of the wallets' storage.
const blocks = new Blocks();
const siteCid = cidToString(
  blocks.tree({
    "index.html":
      "<!doctype html><title>product</title><body><script>" +
      "document.body.dataset.origin = location.origin;" +
      `document.body.dataset.sentinel = String(localStorage.getItem("wallet-sentinel"));` +
      "</script></body>",
  }),
);

const pageHtml = {
  "/wallet.html": `<!doctype html><title>wallet</title><script>
    localStorage.setItem("wallet-sentinel", "${SENTINEL}");
    window.__messages = [];
    addEventListener("message", (e) => window.__messages.push({ origin: e.origin, data: e.data }));
    const src = new URLSearchParams(location.search).get("src");
    if (src) {
      // As createIframeHost builds it: sandboxed with same-origin, credentialless, no referrer.
      const f = document.createElement("iframe"); f.id = "product";
      // Storage kept beyond the frame is what ownership protects, and a
      // credentialless frame keeps none: the retention checks ask for a plain one.
      f.credentialless = !new URLSearchParams(location.search).has("persist");
      f.setAttribute("sandbox", "allow-forms allow-same-origin allow-scripts");
      f.referrerPolicy = "no-referrer";
      f.src = src; document.documentElement.append(f);
    }
  </script>`,
  "/evil.html": `<!doctype html><title>evil</title><script>
    window.__messages = [];
    addEventListener("message", (e) => window.__messages.push({ origin: e.origin, data: e.data }));
    const f = document.createElement("iframe"); f.id = "product";
    f.src = new URLSearchParams(location.search).get("src"); document.documentElement.append(f);
  </script>`,
  // Runs inside a product-origin frame and reports what that origin's storage
  // and lock manager show it: the partition the two tabs' frames share.
  "/probe.html": `<!doctype html><title>probe</title><script>
    (async () => {
      const cache = await caches.open("probe");
      const seen = await cache.match("/tab-one");
      await cache.put("/tab-" + new URLSearchParams(location.search).get("tab"), new Response("x"));
      const holds = new URLSearchParams(location.search).get("tab") === "one";
      if (holds) navigator.locks.request("probe", () => new Promise(() => {}));
      const held = (await navigator.locks.query()).held.map((lock) => lock.name);
      parent.postMessage({ probe: { tab: new URLSearchParams(location.search).get("tab"), cacheSawOtherTab: seen !== undefined, locksHeld: held } }, "*");
    })();
  </script>`,
  "/inspect.html": `<!doctype html><title>inspect</title><script>
    (async () => {
      const registrations = await navigator.serviceWorker.getRegistrations();
      window.__inspect = {
        origin: location.origin,
        registrations: registrations.length,
        caches: await caches.keys(),
        sentinel: localStorage.getItem("wallet-sentinel"),
      };
    })();
  </script>`,
};

const STALE_WORKER = `
self.addEventListener("install", (e) => e.waitUntil(self.skipWaiting()));
self.addEventListener("activate", (e) => e.waitUntil(self.clients.claim()));
self.addEventListener("fetch", (e) => {
  const path = new URL(e.request.url).pathname;
  if (path === "/__sandbox-sw.js" || path.startsWith("/__sandbox/")) return;
  e.respondWith(new Response("<!doctype html><title>ATTACKER</title>", { headers: { "content-type": "text/html" } }));
});`;

/** The real server plugin behind a front that also serves fixture pages and counts what reaches it. */
async function startStack(policy, listenPorts, extra = {}) {
  const [internal] = await freePorts(1);
  const vite = await createVite({
    configFile: false,
    root: mkdtempSync(join(tmpdir(), "boundary-e2e-")),
    logLevel: "error",
    appType: "custom",
    plugins: [sandboxAssets(policy)],
    server: { port: internal, strictPort: true, host: "127.0.0.1", hmr: false },
  });
  await vite.listen();
  const state = { stale: false, sandboxRequests: [] };
  const handler = (req, res) => {
    const path = new URL(req.url, "http://x").pathname;
    if (path === "/__sandbox-sw.js" && state.stale) {
      res.setHeader("content-type", "text/javascript");
      return res.end(STALE_WORKER);
    }
    if (path in pageHtml) {
      res.setHeader("content-type", "text/html");
      return res.end(pageHtml[path]);
    }
    if (extra[path]) {
      res.setHeader("content-type", "text/javascript");
      return res.end(extra[path]);
    }
    if (path.startsWith("/__sandbox"))
      state.sandboxRequests.push({
        host: req.headers.host,
        path,
        dest: req.headers["sec-fetch-dest"],
      });
    const upstream = httpRequest(
      {
        host: "127.0.0.1",
        port: internal,
        method: req.method,
        path: req.url,
        headers: req.headers,
      },
      (answer) => {
        res.writeHead(answer.statusCode, answer.headers);
        answer.pipe(res);
      },
    );
    req.pipe(upstream);
  };
  const fronts = listenPorts.map((port) =>
    createHttp(handler).listen(port, "127.0.0.1"),
  );
  return {
    state,
    async close() {
      fronts.forEach((front) => front.close());
      await vite.close();
    },
  };
}

/** A gateway that answers block requests, and counts them. */
async function startGateway() {
  const [port] = await freePorts(1);
  const seen = { blocks: 0 };
  const server = createHttp((req, res) => {
    res.setHeader("access-control-allow-origin", "*");
    const cid = new URL(req.url, "http://x").pathname.split("/").pop();
    const block = [...blocks.all.values()].find(
      (entry) => cidToString(entry.cid) === cid,
    );
    seen.blocks += 1;
    if (!block) {
      res.statusCode = 404;
      return res.end();
    }
    res.end(Buffer.from(block.data));
  }).listen(port, "127.0.0.1");
  return {
    origin: `http://127.0.0.1:${port}`,
    seen,
    close: () => server.close(),
  };
}

const portsBundle = async () => {
  const entry = join(mkdtempSync(join(tmpdir(), "ports-e2e-")), "entry.ts");
  await Bun.write(
    entry,
    `import { PortLedger } from ${JSON.stringify(join(hostRoot, "src/sandbox/ports.ts"))};\n(globalThis as any).PortLedger = PortLedger;`,
  );
  const out = await build({
    configFile: false,
    logLevel: "error",
    build: {
      write: false,
      minify: false,
      rollupOptions: {
        input: entry,
        output: { format: "iife", inlineDynamicImports: true },
      },
    },
  });
  const chunk = (Array.isArray(out) ? out[0] : out).output.find(
    (item) => item.type === "chunk",
  );
  return chunk.code;
};

const bundle = await portsBundle();
const browser = await chromium.launch({
  executablePath: process.env.CHROMIUM_PATH,
});
const gateway = await startGateway();
const label = productLabel("a.paseo");
const otherLabel = productLabel("b.paseo");

/** The product frame's state once it left the loader, or what stopped it. */
async function outcome(page, productOrigin, timeout = 8000) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    const frame = page
      .frames()
      .find(
        (f) =>
          f.url().startsWith(productOrigin) && !f.url().includes("/__sandbox/"),
      );
    if (frame) {
      const data = await frame
        .evaluate(() => ({ ...document.body?.dataset }))
        .catch(() => null);
      if (data?.origin) return { type: "product", ...data };
    }
    const messages = await page
      .evaluate(() => window.__messages)
      .catch(() => []);
    const failure = messages.find((m) => m.data?.state === "error");
    if (failure)
      return { type: "error", message: failure.data, from: failure.origin };
    await page.waitForTimeout(150);
  }
  return { type: "none" };
}

async function inspect(context, origin) {
  const page = await context.newPage();
  await page.goto(`${origin}/inspect.html`);
  await page.waitForFunction(() => window.__inspect);
  const seen = await page.evaluate(() => window.__inspect);
  await page.close();
  return seen;
}

try {
  // E2E_ONLY=range runs just the port-range section, for iterating on it.
  if (process.env.E2E_ONLY !== "range") {
    // ---- automatic .localhost origins beside a loopback wallet --------------
    const [front] = await freePorts(1);
    const auto = await startStack({ template: "", hostOrigins: [] }, [front], {
      "/ports.js": bundle,
    });
    const wallet = `http://localhost:${front}`;
    const productOrigin = `http://${label}.localhost:${front}`;
    const loader = (over = {}) =>
      sandboxUrl({
        origin: productOrigin,
        cid: siteCid,
        gateway: gateway.origin,
        hostOrigin: wallet,
        kind: "site",
        start: "/",
        container: true,
        owner: label,
        ...over,
      });

    {
      // Positive: the wallet embeds its own loader.
      const context = await browser.newContext();
      const page = await context.newPage();
      if (process.env.E2E_DEBUG) {
        page.on("console", (m) => console.log("console:", m.text()));
        page.on("pageerror", (e) => console.log("pageerror:", e.message));
        page.on("framenavigated", (f) => console.log("frame:", f.url()));
        page.on("requestfailed", (r) =>
          console.log("requestfailed:", r.url(), r.failure()?.errorText),
        );
      }
      await page.goto(
        `${wallet}/wallet.html?src=${encodeURIComponent(loader())}`,
      );
      const got = await outcome(page, productOrigin);
      check(
        "authorized: the wallet's frame loads the product on its own origin",
        got.type === "product" && got.origin === productOrigin,
        JSON.stringify(got),
      );
      check(
        "authorized: the product cannot see the wallet's storage",
        got.sentinel === "null",
        JSON.stringify(got),
      );
      const seen = await inspect(context, wallet);
      check(
        "authorized: the wallet origin has no worker and keeps its storage",
        seen.registrations === 0 && seen.sentinel === SENTINEL,
        JSON.stringify(seen),
      );
      check(
        "authorized: the worker and the claim live on the product origin",
        context.serviceWorkers().some((w) => w.url().startsWith(productOrigin)),
        String(context.serviceWorkers().map((w) => w.url())),
      );
      await context.close();
    }

    {
      // Wallet origins: the loader and its scripts are not served there.
      const context = await browser.newContext();
      const page = await context.newPage();
      await page.goto(`${wallet}/wallet.html`);
      for (const origin of [
        wallet,
        `http://127.0.0.1:${front}`,
        `http://[::1]:${front}`,
      ]) {
        const answers = {};
        for (const path of [
          "/__sandbox/index.html",
          "/__sandbox/page.js",
          "/__sandbox/container.js",
        ]) {
          const probe = await context.request
            .get(`${origin}${path}?cid=${siteCid}`, {
              headers: { "sec-fetch-dest": "iframe" },
              failOnStatusCode: false,
            })
            .catch((e) => ({ status: () => `error ${e.message}` }));
          answers[path] = probe.status();
        }
        check(
          `wallet origin ${origin}: loader and scripts are refused`,
          Object.values(answers).every(
            (s) => s === 404 || String(s).startsWith("error"),
          ),
          JSON.stringify(answers),
        );
      }
      // The crafted link, followed in a real tab.
      const crafted = sandboxUrl({
        origin: wallet,
        cid: siteCid,
        gateway: gateway.origin,
        hostOrigin: wallet,
        kind: "site",
        start: "/",
        container: true,
        owner: label,
      });
      const before = gateway.seen.blocks;
      const response = await page.goto(crafted.href);
      const seen = await inspect(context, wallet);
      check(
        "wallet origin: the crafted link gets no loader, no worker, no fetch",
        response.status() === 404 &&
          seen.registrations === 0 &&
          seen.caches.length === 0 &&
          gateway.seen.blocks === before,
        JSON.stringify({ status: response.status(), seen }),
      );
      check(
        "wallet origin: the sentinel is intact",
        seen.sentinel === SENTINEL,
      );
      await context.close();
    }

    {
      // Top level on a product origin.
      const context = await browser.newContext();
      const page = await context.newPage();
      const before = gateway.seen.blocks;
      const response = await page.goto(loader().href);
      const seen = await inspect(context, productOrigin);
      check(
        "top level on a product origin: refused, nothing registered or fetched",
        response.status() === 403 &&
          seen.registrations === 0 &&
          seen.caches.length === 0 &&
          gateway.seen.blocks === before,
        JSON.stringify({ status: response.status(), seen }),
      );
      await context.close();
    }

    // A parent that is not the wallet, however the link names the host.
    const evilPort = (await freePorts(1))[0];
    const evilServer = createHttp((req, res) => {
      res.setHeader("content-type", "text/html");
      res.end(pageHtml["/evil.html"]);
    }).listen(evilPort, "127.0.0.1");
    for (const [name, evil] of [
      ["another site", `http://127.0.0.1:${evilPort}`],
      ["the wallet's own name on another port", `http://localhost:${evilPort}`],
    ]) {
      for (const stripped of [false, true]) {
        const context = await browser.newContext();
        if (stripped)
          // A static deployment sends no frame-ancestors, so only the page's own check stands.
          await context.route(
            `${productOrigin}/__sandbox/index.html*`,
            async (route) => {
              const response = await route.fetch({
                headers: {
                  ...route.request().headers(),
                  "sec-fetch-dest": "iframe",
                },
              });
              const headers = { ...response.headers() };
              delete headers["content-security-policy"];
              await route.fulfill({ response, headers });
            },
          );
        const page = await context.newPage();
        const before = gateway.seen.blocks;
        await page.goto(
          `${evil}/evil.html?src=${encodeURIComponent(loader().href)}`,
        );
        await page.waitForTimeout(2500);
        const seen = await inspect(context, productOrigin);
        const frame = page
          .frames()
          .find((f) => f !== page.mainFrame() && f.url().includes("__sandbox"));
        const text = frame
          ? await frame.textContent("#status").catch(() => null)
          : null;
        const ran =
          seen.registrations !== 0 ||
          seen.caches.length !== 0 ||
          gateway.seen.blocks !== before;
        check(
          `malicious parent (${name})${stripped ? ", no frame-ancestors header" : ""}: nothing registered, claimed or fetched`,
          !ran,
          JSON.stringify({ seen, text }),
        );
        if (stripped)
          check(
            "  ... and the loader says why",
            /not this host/.test(text ?? ""),
            String(text),
          );
        await context.close();
      }
    }

    {
      // The trusted parent, with a link that lies.
      for (const [name, url, expect] of [
        ["wrong owner", loader({ owner: otherLabel }), /different product/],
        [
          "wrong host",
          loader({ hostOrigin: `http://127.0.0.1:${front}` }),
          /different host/,
        ],
        [
          "host pointing at a foreign site",
          loader({ hostOrigin: `http://127.0.0.1:${evilPort}` }),
          /different host/,
        ],
      ]) {
        const context = await browser.newContext();
        const page = await context.newPage();
        const before = gateway.seen.blocks;
        await page.goto(
          `${wallet}/wallet.html?src=${encodeURIComponent(url.href)}`,
        );
        await page.waitForTimeout(1500);
        const frame = page
          .frames()
          .find((f) => f !== page.mainFrame() && f.url().includes("__sandbox"));
        const text = frame ? await frame.textContent("#status") : "";
        const seen = await inspect(context, productOrigin);
        check(
          `trusted parent, ${name}: refused before any storage or worker`,
          expect.test(text) &&
            seen.registrations === 0 &&
            seen.caches.length === 0 &&
            gateway.seen.blocks === before,
          JSON.stringify({ text, seen }),
        );
        await context.close();
      }
    }

    {
      // A worker an earlier build left on the wallet origin.
      const context = await browser.newContext();
      const page = await context.newPage();
      await page.goto(`${wallet}/wallet.html`);
      auto.state.stale = true;
      await page.evaluate(async () => {
        const registration = await navigator.serviceWorker.register(
          "/__sandbox-sw.js",
          { scope: "/" },
        );
        await new Promise((resolve) => {
          const check = () =>
            registration.active?.state === "activated" && resolve();
          check();
          registration.installing?.addEventListener("statechange", check);
        });
      });
      await page.goto(`${wallet}/wallet.html`);
      const hijacked = await page.title();
      auto.state.stale = false;
      await page.evaluate(async () =>
        (await navigator.serviceWorker.getRegistration())?.update(),
      );
      await page.waitForTimeout(2500);
      const seen = await inspect(context, wallet);
      await page.goto(`${wallet}/wallet.html`);
      check(
        "stale wallet-origin worker: it did take the origin over",
        hijacked === "ATTACKER",
        hijacked,
      );
      check(
        "stale wallet-origin worker: the server's update retires it and the wallet loads again",
        seen.registrations === 0 && (await page.title()) === "wallet",
        JSON.stringify(seen),
      );
      check(
        "stale wallet-origin worker: retiring leaves the wallet's storage alone",
        seen.sentinel === SENTINEL,
      );
      await context.close();
    }

    // ---- concurrent port allocation across two tabs --------------------------
    {
      const run = async (locks) => {
        const context = await browser.newContext();
        const [one, two] = await Promise.all([
          context.newPage(),
          context.newPage(),
        ]);
        await Promise.all(
          [one, two].map(async (page) => {
            await page.goto(`${wallet}/wallet.html`);
            await page.addScriptTag({ url: `${wallet}/ports.js` });
          }),
        );
        const taken = await Promise.all(
          [one, two].map((page, tab) =>
            page.evaluate(
              async ({ locks, tab }) => {
                // Widen the window between a read and its write, as a busy tab would.
                const slow = {
                  getItem: (key) => localStorage.getItem(key),
                  setItem: (key, value) => {
                    const until = performance.now() + 25;
                    while (performance.now() < until);
                    localStorage.setItem(key, value);
                  },
                };
                const none = {
                  request: (_name, work) => Promise.resolve().then(work),
                };
                const ledger = new PortLedger(slow, locks ? undefined : none);
                return Promise.all(
                  Array.from({ length: 6 }, (_, i) =>
                    ledger.portFor(`tab${tab}-${i}`, 9000, 9099),
                  ),
                );
              },
              { locks, tab },
            ),
          ),
        );
        await context.close();
        const all = taken.flat();
        return { distinct: new Set(all).size, total: all.length };
      };
      const withLocks = await run(true);
      check(
        "two tabs allocating ports at once, with Web Locks: every product gets its own port",
        withLocks.distinct === withLocks.total,
        JSON.stringify(withLocks),
      );
      const without = await run(false);
      check(
        "(control) the same run without a lock collides, so the check can fail",
        without.distinct < without.total,
        JSON.stringify(without),
      );
    }
    await auto.close();
    evilServer.close();
  }

  // ---- one name, products told apart by port -------------------------------
  {
    const [walletPort, ...productPorts] = [
      await freePorts(1).then((p) => p[0]),
      ...(await freeRun(3)),
    ];
    const template = `http://localhost:{${productPorts[0]}-${productPorts[2]}}`;
    const stack = await startStack(
      { template, hostOrigins: [`http://localhost:${walletPort}`] },
      [walletPort, ...productPorts],
    );
    const wallet = `http://localhost:${walletPort}`;
    const at = (index) => `http://localhost:${productPorts[index]}`;
    const link = (index, owner) =>
      sandboxUrl({
        origin: at(index),
        cid: siteCid,
        gateway: gateway.origin,
        hostOrigin: wallet,
        kind: "site",
        start: "/",
        container: true,
        owner,
      });

    {
      const context = await browser.newContext();
      const page = await context.newPage();
      await page.goto(
        `${wallet}/wallet.html?src=${encodeURIComponent(link(0, label).href)}`,
      );
      const got = await outcome(page, at(0));
      check(
        "port range: the wallet's frame loads the product on its own port",
        got.type === "product" &&
          got.origin === at(0) &&
          got.sentinel === "null",
        JSON.stringify(got),
      );
      const response = await context.request.get(
        `${wallet}/__sandbox/index.html`,
        { headers: { "sec-fetch-dest": "iframe" }, failOnStatusCode: false },
      );
      const scripts = await context.request.get(`${wallet}/__sandbox/page.js`, {
        failOnStatusCode: false,
      });
      check(
        "port range: the wallet's port serves no loader or script",
        response.status() === 404 && scripts.status() === 404,
        `${response.status()} ${scripts.status()}`,
      );
      const top = await page.goto(link(1, label).href);
      check(
        "port range: a product port refuses to be opened on its own",
        top.status() === 403,
        String(top.status()),
      );
      const foreign = await browser.newContext();
      const evilPage = await foreign.newPage();
      const evilOrigin = `http://localhost:${(await freePorts(1))[0]}`;
      await evilPage.route(`${evilOrigin}/**`, (route) =>
        route.fulfill({
          contentType: "text/html",
          body: pageHtml["/evil.html"],
        }),
      );
      const before = gateway.seen.blocks;
      await evilPage.goto(
        `${evilOrigin}/evil.html?src=${encodeURIComponent(link(2, label).href)}`,
      );
      await evilPage.waitForTimeout(2000);
      const seen = await inspect(foreign, at(2));
      check(
        "port range: a malicious parent gets nothing registered, claimed or fetched",
        seen.registrations === 0 &&
          seen.caches.length === 0 &&
          gateway.seen.blocks === before,
        JSON.stringify(seen),
      );
      await foreign.close();
      await context.close();
    }

    // Both tabs' frames must share one storage and lock partition, or a race
    // between them proves nothing: a lock in another partition serialises no one.
    {
      const context = await browser.newContext();
      const [one, two] = await Promise.all([
        context.newPage(),
        context.newPage(),
      ]);
      const probe = (tab) =>
        `${wallet}/wallet.html?persist=1&src=${encodeURIComponent(`${at(1)}/probe.html?tab=${tab}`)}`;
      const reported = async (page) => {
        await page.waitForFunction(() =>
          window.__messages.some((message) => message.data?.probe),
        );
        return (await page.evaluate(() => window.__messages)).find(
          (message) => message.data?.probe,
        ).data.probe;
      };
      await one.goto(probe("one"));
      const first = await reported(one);
      await two.goto(probe("two"));
      const second = await reported(two);
      check(
        "claim race setup: the second tab's frame sees the first tab's cache and held lock",
        first.tab === "one" &&
          second.cacheSawOtherTab === true &&
          second.locksHeld.includes("probe"),
        JSON.stringify({ first, second }),
      );
      await context.close();
    }

    // Two tabs load two different products into one port. Every ownership
    // lookup waits at a barrier until both loaders are between their look and
    // their write, so the outcome does not depend on how fast either tab is.
    // With the origin's lock the second lookup cannot start while the first
    // holds it, so the barrier is never reached by both and gives up; without
    // the lock (the control) both arrive and both claim.
    async function race({ withLock }) {
      const context = await browser.newContext();
      const waiting = [];
      let arrivals = 0;
      let together = false;
      await context.exposeBinding("__arrive", async () => {
        arrivals += 1;
        if (arrivals >= 2) together = true;
        await new Promise((resolve) => {
          waiting.push(resolve);
          if (waiting.length >= 2) waiting.forEach((release) => release());
          else setTimeout(resolve, withLock ? 2000 : 60_000);
        });
        arrivals -= 1;
      });
      await context.addInitScript(
        ({ withLock }) => {
          if (!withLock)
            navigator.locks.request = (_name, work) =>
              Promise.resolve().then(work);
          const match = Cache.prototype.match;
          Cache.prototype.match = async function (...args) {
            const found = await match.apply(this, args);
            if (String(args[0]).endsWith("/__owner")) await window.__arrive();
            return found;
          };
        },
        { withLock },
      );
      const [one, two] = await Promise.all([
        context.newPage(),
        context.newPage(),
      ]);
      await Promise.all([
        one.goto(
          `${wallet}/wallet.html?persist=1&src=${encodeURIComponent(link(1, label).href)}`,
        ),
        two.goto(
          `${wallet}/wallet.html?persist=1&src=${encodeURIComponent(link(1, otherLabel).href)}`,
        ),
      ]);
      const got = await Promise.all([
        outcome(one, at(1), 30000),
        outcome(two, at(1), 30000),
      ]);
      await context.close();
      return {
        together,
        kinds: got
          .map((one) =>
            one.type === "error" ? `error:${one.message.code}` : one.type,
          )
          .sort(),
      };
    }
    const rounds = 3;
    const locked = [];
    for (let round = 0; round < rounds; round += 1)
      locked.push(await race({ withLock: true }));
    check(
      `claim race, with the lock: exactly one product wins and the lookups never overlap (${rounds} rounds)`,
      locked.every(
        ({ together, kinds }) =>
          !together && kinds[0] === "error:owner" && kinds[1] === "product",
      ),
      JSON.stringify(locked),
    );
    const control = await race({ withLock: false });
    check(
      "claim race, control without the lock: both lookups overlap and both products claim the port",
      control.together && control.kinds.join() === "product,product",
      JSON.stringify(control),
    );
    await stack.close();
  }
} catch (error) {
  check(
    "the run finished without an error",
    false,
    String(error?.stack ?? error),
  );
} finally {
  await browser.close();
  gateway.close();
  console.log(
    `\n${results.filter((r) => r.ok).length}/${results.length} passed`,
  );
  process.exit(results.every((r) => r.ok) ? 0 : 1);
}
