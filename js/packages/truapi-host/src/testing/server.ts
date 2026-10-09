// Node half of the test host: serves the page the Playwright fixture drives.
//
// The browser entry is bundled with esbuild at startup rather than served as
// loose ESM. Two of the modules it reaches import bare specifiers
// (`@parity/truapi`, `neverthrow`), and resolving those in the browser would
// mean an import map pointing into `node_modules` -- which breaks under
// pnpm's symlinked layout, the layout the consuming suites actually use.
// Bundling resolves them the same way the rest of the toolchain does.

import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { dirname, extname, join, normalize, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { hostPageUrl } from "./host-page-url.js";
import type { HostPageConfig } from "./host-page-url.js";

const __dirname = dirname(fileURLToPath(import.meta.url));
/**
 * Built artefacts always live in `dist`, whether this module is running from
 * `dist/testing` or straight from `src/testing` under a TS runner. Both sit two
 * levels below the package root, so resolve that and go down into `dist`.
 */
const distRoot = resolve(__dirname, "../..", "dist");

/** Options for {@link createTestHostServer}. */
export interface TestHostServerOptions extends Partial<HostPageConfig> {
  /** Port to listen on. `0` (the default) picks a free one. */
  port?: number;
  /**
   * Accepted only to fail with an explanation: TrUAPI derives a product
   * account from (session root, product id), so it cannot be mapped to a
   * chosen one. See the error text for what to do instead.
   */
  productAccounts?: Record<string, unknown>;
  /**
   * Do not keep the process alive for this server.
   *
   * Set when the fixture starts the server itself: nothing then calls `close`,
   * and a referenced listener would stall worker exit at the end of a run.
   */
  unref?: boolean;
}

/** A running test host server. */
export interface TestHostServer {
  /**
   * URL to open. Carries whatever host configuration was passed here; with no
   * configuration it is the bare base the fixture appends its own to.
   */
  url: string;
  /** Stop listening. */
  close(): Promise<void>;
}

const PAGE = `<!doctype html>
<html>
  <head>
    <meta charset="utf-8" />
    <title>TrUAPI test host</title>
    <style>
      html, body, #product-container { margin: 0; height: 100%; }
      #product-container > iframe { width: 100%; height: 100%; border: 0; }
    </style>
  </head>
  <body>
    <div id="product-container"></div>
    <script type="module" src="/test-host.js"></script>
  </body>
</html>
`;

const CONTENT_TYPES: Record<string, string> = {
  ".js": "text/javascript; charset=utf-8",
  ".mjs": "text/javascript; charset=utf-8",
  ".wasm": "application/wasm",
  ".json": "application/json; charset=utf-8",
  ".ts": "text/plain; charset=utf-8",
};

/**
 * Bundle one browser entry, resolving its bare imports.
 *
 * `wasmBundle` picks which WASM the output loads. The production worker
 * imports `./wasm/web/truapi_server.js` as a literal so bundlers can emit the
 * glue statically -- correct for production, but the test host needs the
 * `testing` bundle, the only one with a signing host. Rewriting the specifier
 * here keeps that production property untouched instead of making the worker's
 * import dynamic, which would change how every web host loads its core.
 */
async function bundle(
  entry: string,
  wasmBundle: "web" | "testing" = "testing",
): Promise<string> {
  // esbuild is an optional peer: it bundles the host page and nothing else in
  // this package needs it, so a product that only imports `./web` never pulls
  // it in. Name it when it is absent, since the module-not-found alone does
  // not say which dependency to add.
  const { build } = await import("esbuild").catch(() => {
    throw new Error(
      "@parity/truapi-host/testing/server needs esbuild. Install it as a dev " +
        "dependency alongside this package.",
    );
  });
  const result = await build({
    entryPoints: [join(distRoot, entry)],
    bundle: true,
    format: "esm",
    platform: "browser",
    write: false,
    plugins: [
      {
        name: "truapi-wasm-path",
        setup(build) {
          // Left external: the glue fetches a sibling `.wasm` by relative URL,
          // and bundling it would break that. Only the path is redirected, to
          // an absolute one the server serves.
          build.onResolve(
            { filter: /wasm\/(web|testing)\/truapi_server\.js$/ },
            () => ({
              path: `/wasm/${wasmBundle}/truapi_server.js`,
              external: true,
            }),
          );
        },
      },
    ],
    external: ["*.wasm"],
  });
  const [output] = result.outputFiles;
  if (!output) throw new Error(`esbuild produced no output for ${entry}`);
  return output.text;
}

/**
 * Start the test host server.
 *
 * ```ts
 * const server = await createTestHostServer();
 * const test = base.extend(createTestHostFixture({
 *   productUrl: "http://127.0.0.1:5173",
 *   hostUrl: server.url,
 * }));
 * ```
 */
export async function createTestHostServer(
  options: TestHostServerOptions = {},
): Promise<TestHostServer> {
  if (options.productAccounts) {
    throw new Error(
      "createTestHostServer `productAccounts` is not supported by the TrUAPI " +
        "test host: a product account is DERIVED from (session root, product " +
        "id), so it cannot be mapped to a chosen account. Read the address " +
        "back from the host and fund that, rather than pinning one.",
    );
  }
  // Two bundles: the page entry, and the worker the production topology runs
  // the core in. The worker is a separate script because that is what `new
  // Worker(url)` needs.
  const [pageBundle, workerBundle] = await Promise.all([
    bundle("testing/browser-entry.js"),
    bundle("worker-runtime.js"),
  ]);

  const server = createServer((req, res) => {
    const path = new URL(req.url ?? "/", "http://127.0.0.1").pathname;

    if (path === "/test-host.js") {
      res.writeHead(200, { "Content-Type": CONTENT_TYPES[".js"] });
      res.end(pageBundle);
      return;
    }

    if (path === "/test-host-worker.js") {
      res.writeHead(200, { "Content-Type": CONTENT_TYPES[".js"] });
      res.end(workerBundle);
      return;
    }

    // The WASM bundle and its glue are served from disk so the browser fetches
    // the same artifact the build produced.
    if (path.startsWith("/wasm/")) {
      void serveFromDist(path.slice(1), res);
      return;
    }

    res.writeHead(200, {
      "Content-Type": "text/html; charset=utf-8",
      // The product runs cross-origin in an iframe; without this the browser
      // refuses to delegate clipboard access however the iframe is marked.
      "Permissions-Policy": "clipboard-read=*, clipboard-write=*",
    });
    res.end(PAGE);
  });

  const url = await new Promise<string>((resolveUrl, reject) => {
    server.once("error", reject);
    if (options.unref) server.unref();
    server.listen(options.port ?? 0, "127.0.0.1", () => {
      const address = server.address();
      if (!address || typeof address === "string") {
        reject(new Error("test host server reported no address"));
        return;
      }
      const base = `http://127.0.0.1:${address.port}`;
      // With a `productUrl` this server is the whole harness: the returned URL
      // is ready to open. Without one it is the bare base the Playwright
      // fixture appends its own per-test configuration to.
      resolveUrl(
        options.productUrl
          ? hostPageUrl(base, options as HostPageConfig)
          : base,
      );
    });
  });

  return {
    url,
    close: () =>
      new Promise<void>((done, reject) => {
        server.close((err) => (err ? reject(err) : done()));
      }),
  };
}

/** Serve one file from `dist`, refusing anything that escapes it. */
async function serveFromDist(
  relativePath: string,
  res: import("node:http").ServerResponse,
): Promise<void> {
  const target = resolve(distRoot, normalize(relativePath));
  if (target !== distRoot && !target.startsWith(distRoot + sep)) {
    res.writeHead(403).end("forbidden");
    return;
  }
  try {
    const body = await readFile(target);
    res.writeHead(200, {
      "Content-Type":
        CONTENT_TYPES[extname(target)] ?? "application/octet-stream",
    });
    res.end(body);
  } catch {
    res.writeHead(404).end("not found");
  }
}
