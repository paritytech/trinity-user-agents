import { readFileSync } from "node:fs";
import type { IncomingMessage, ServerResponse } from "node:http";
import { fileURLToPath } from "node:url";
import { type Plugin, build } from "vite";
import {
  type AssetKind,
  classifyAssetPath,
  decideAsset,
} from "./src/sandbox/access.js";
import type { SandboxPolicy } from "./src/sandbox/policy.js";

const hostRoot = fileURLToPath(new URL(".", import.meta.url));
const repoRoot = fileURLToPath(new URL("../..", import.meta.url));

/**
 * The scripts a product archive's origin serves, bundled as one classic script
 * each. The container has to run synchronously before the product's scripts, so
 * it cannot be a module, and the worker and loader share the same build.
 */
const SCRIPTS: Record<string, string> = {
  "/__sandbox/container.js": `${repoRoot}js/container/src/index.ts`,
  "/__sandbox/page.js": `${hostRoot}src/sandbox/page.ts`,
  "/__sandbox-sw.js": `${hostRoot}src/sandbox/sw.ts`,
};
const LOADER_PAGE = "/__sandbox/index.html";
const WORKER_SCRIPT = "/__sandbox-sw.js";

const KNOWN: Record<string, AssetKind> = {
  [LOADER_PAGE]: "loader",
  [WORKER_SCRIPT]: "worker",
  ...Object.fromEntries(
    Object.keys(SCRIPTS)
      .filter((path) => path !== WORKER_SCRIPT)
      .map((path) => [path, "script" as const]),
  ),
};

async function bundleScript(
  entry: string,
  policy: SandboxPolicy,
): Promise<string> {
  const result = await build({
    configFile: false,
    logLevel: "warn",
    publicDir: false,
    define: {
      "process.env.NODE_ENV": '"production"',
      __SANDBOX_POLICY__: JSON.stringify(policy),
    },
    build: {
      write: false,
      minify: false,
      target: "es2020",
      modulePreload: false,
      rollupOptions: {
        input: entry,
        output: { format: "iife", inlineDynamicImports: true },
      },
    },
  });
  const outputs = Array.isArray(result) ? result : [result];
  for (const output of outputs) {
    if (!("output" in output)) continue;
    const chunk = output.output.find((item) => item.type === "chunk");
    if (chunk?.type === "chunk") return chunk.code;
  }
  throw new Error(`no script was built from ${entry}`);
}

async function sandboxFile(
  path: string,
  policy: SandboxPolicy,
): Promise<{ type: string; body: string } | null> {
  if (path === LOADER_PAGE)
    return {
      type: "text/html; charset=utf-8",
      body: readFileSync(`${hostRoot}src/sandbox/index.html`, "utf8"),
    };
  const entry = SCRIPTS[path];
  return entry === undefined
    ? null
    : {
        type: "text/javascript; charset=utf-8",
        body: await bundleScript(entry, policy),
      };
}

type Next = () => void;
type Middleware = (
  request: IncomingMessage,
  response: ServerResponse,
  next: Next,
) => void;

/**
 * Serve the sandbox's own files to the product origins of `policy`, which is
 * how a product gets an origin of its own from the one server. Any other
 * origin, the wallets' among them, gets none of them: see `decideAsset`.
 */
function serveSandbox(policy: SandboxPolicy): Middleware {
  return (request, response, next) => {
    const path = (request.url ?? "/").split(/[?#]/, 1)[0];
    const kind = classifyAssetPath(path, KNOWN);
    if (kind === null) return next();
    if (kind === "reserved") {
      response.statusCode = 404;
      response.setHeader("content-type", "text/plain; charset=utf-8");
      response.setHeader("cache-control", "no-store");
      response.end("Not found.");
      return;
    }
    const decision = decideAsset(policy, {
      kind,
      method: request.method ?? "GET",
      host: request.headers.host,
      fetchDest: request.headers["sec-fetch-dest"] as string | undefined,
      secure:
        "encrypted" in request.socket && request.socket.encrypted === true,
    });
    if (decision.type === "refuse") {
      response.statusCode = decision.status;
      response.setHeader("content-type", "text/plain; charset=utf-8");
      response.setHeader("cache-control", "no-store");
      response.end(decision.reason);
      return;
    }
    sandboxFile(path, policy).then(
      (file) => {
        if (file === null) return next();
        response.setHeader("content-type", file.type);
        response.setHeader("cache-control", "no-store");
        response.setHeader("x-content-type-options", "nosniff");
        if (decision.frameAncestors !== undefined)
          response.setHeader(
            "content-security-policy",
            `frame-ancestors ${decision.frameAncestors.join(" ")}`,
          );
        response.end(file.body);
      },
      (error: unknown) => {
        response.statusCode = 500;
        response.end(String(error));
      },
    );
  };
}

/** The sandbox origin's assets: served by the dev and preview servers, emitted into a build. */
export function sandboxAssets(policy: SandboxPolicy): Plugin {
  return {
    name: "truapi-sandbox-assets",
    configureServer(server) {
      server.middlewares.use(serveSandbox(policy));
    },
    configurePreviewServer(server) {
      server.middlewares.use(serveSandbox(policy));
    },
    async generateBundle() {
      for (const path of [...Object.keys(SCRIPTS), LOADER_PAGE]) {
        const file = await sandboxFile(path, policy);
        if (file !== null)
          this.emitFile({
            type: "asset",
            fileName: path.slice(1),
            source: file.body,
          });
      }
    },
  };
}
