import { readFileSync } from "node:fs";
import type { IncomingMessage, ServerResponse } from "node:http";
import { fileURLToPath } from "node:url";
import { type Plugin, build } from "vite";

const hostRoot = fileURLToPath(new URL(".", import.meta.url));

/**
 * The scripts a mounted product needs from the host, bundled as one classic
 * script each, by path under the host's base. The service worker cannot be a
 * module, and the loader shares the same build.
 */
const SCRIPTS: Record<string, string> = {
  "truapi-sandbox/page.js": `${hostRoot}src/sandbox/page.ts`,
  "sw.js": `${hostRoot}src/sandbox/sw.ts`,
};
const LOADER_PAGE = "truapi-sandbox/index.html";

async function bundleScript(entry: string): Promise<string> {
  const result = await build({
    configFile: false,
    logLevel: "warn",
    publicDir: false,
    define: { "process.env.NODE_ENV": '"production"' },
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

/** A sandbox file by its path under the base, or null for a path that is none. */
async function sandboxFile(
  path: string,
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
        body: await bundleScript(entry),
      };
}

type Next = () => void;
type Middleware = (
  request: IncomingMessage,
  response: ServerResponse,
  next: Next,
) => void;

/** Serve the files a build emits, for the dev and preview servers. */
function serveSandbox(base: string): Middleware {
  return (request, response, next) => {
    const url = (request.url ?? "/").split(/[?#]/, 1)[0];
    if (!url.startsWith(base)) return next();
    const path = url.slice(base.length);
    if (path !== LOADER_PAGE && !(path in SCRIPTS)) return next();
    sandboxFile(path).then(
      (file) => {
        if (file === null) return next();
        response.setHeader("content-type", file.type);
        response.setHeader("cache-control", "no-store");
        response.end(file.body);
      },
      (error: unknown) => {
        response.statusCode = 500;
        response.end(String(error));
      },
    );
  };
}

/**
 * The files a mounted product needs from the host: the loader page,
 * its script and the service worker. A build emits them as ordinary static files
 * beside the host, so any static host can serve them. The dev and preview
 * servers answer for the same paths.
 */
export function sandboxAssets(): Plugin {
  let base = "/";
  return {
    name: "truapi-sandbox-assets",
    configResolved(config) {
      base = config.base.startsWith("/") ? config.base : "/";
    },
    configureServer(server) {
      server.middlewares.use(serveSandbox(base));
    },
    configurePreviewServer(server) {
      server.middlewares.use(serveSandbox(base));
    },
    async generateBundle() {
      for (const path of [...Object.keys(SCRIPTS), LOADER_PAGE]) {
        const file = await sandboxFile(path);
        if (file !== null)
          this.emitFile({ type: "asset", fileName: path, source: file.body });
      }
      // GitHub Pages runs Jekyll, which skips some paths unless this file is present.
      this.emitFile({ type: "asset", fileName: ".nojekyll", source: "" });
    },
  };
}
