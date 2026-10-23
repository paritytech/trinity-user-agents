import type { IncomingMessage, ServerResponse } from "node:http";
import { fileURLToPath } from "node:url";
import { type Plugin, build } from "vite";
import { cidToString } from "./src/archive/cid.js";
import { Blocks, car } from "./src/archive/fixtures.js";
import {
  DEMO_WORKER_PATH,
  LOOPBACK_HOSTNAMES,
  type DemoWorkerAnswer,
} from "./src/chat-v2/local-demo-route.js";

const hostRoot = fileURLToPath(new URL(".", import.meta.url));

/** The synthetic chat product's worker entry, built into the archive. */
const ENTRY = "browser/chat-v2-product-worker.ts";

const MANIFEST = JSON.stringify({
  $v: 1,
  kind: "worker",
  appVersion: [0, 1, 0],
  entrypoint: "index.js",
  includes: { chat: true },
});

/** Archives kept, so a frame still loading an older build finds its blocks. */
const KEPT_BUILDS = 4;

interface Built {
  cid: string;
  blocks: Map<string, Uint8Array>;
}

async function bundle(): Promise<string> {
  const result = await build({
    configFile: false,
    root: hostRoot,
    logLevel: "warn",
    publicDir: false,
    define: { "process.env.NODE_ENV": '"development"' },
    build: {
      write: false,
      minify: false,
      target: "es2022",
      modulePreload: false,
      rollupOptions: {
        input: `${hostRoot}${ENTRY}`,
        output: { format: "es", inlineDynamicImports: true },
      },
    },
  });
  const outputs = Array.isArray(result) ? result : [result];
  for (const output of outputs) {
    if (!("output" in output)) continue;
    const chunk = output.output.find((item) => item.type === "chunk");
    if (chunk?.type === "chunk") return chunk.code;
  }
  throw new Error(`no module was built from ${ENTRY}`);
}

/** Pack the module as an app archive: a CAR of the site, stored as a UnixFS file. */
function pack(code: string): Built {
  const site = new Blocks();
  const archive = car(site.tree({ "index.js": code }), site);
  const outer = new Blocks();
  const root = outer.file(archive, 256 * 1024);
  return {
    cid: cidToString(root),
    blocks: new Map(
      [...outer.all.values()].map(({ cid, data }) => [cidToString(cid), data]),
    ),
  };
}

/** Whether the request came from this machine, to this machine's name. */
function loopback(request: IncomingMessage): boolean {
  const host = (request.headers.host ?? "").replace(/:\d+$/, "");
  const remote = request.socket.remoteAddress ?? "";
  return (
    LOOPBACK_HOSTNAMES.includes(host) &&
    (remote === "127.0.0.1" ||
      remote === "::1" ||
      remote === "::ffff:127.0.0.1")
  );
}

type Middleware = (
  request: IncomingMessage,
  response: ServerResponse,
  next: () => void,
) => void;

function serveDemo(base: string): Middleware {
  const builds: Built[] = [];
  let building: Promise<Built> | null = null;
  /** Build again on each ask, so a restart of the demo picks up an edit. */
  const latest = () =>
    (building ??= bundle()
      .then(pack)
      .then((built) => {
        if (builds[0]?.cid !== built.cid) builds.unshift(built);
        builds.splice(KEPT_BUILDS);
        return built;
      })
      .finally(() => (building = null)));

  return (request, response, next) => {
    const url = new URL(request.url ?? "/", "http://dev.invalid");
    const describe = url.pathname === `${base}${DEMO_WORKER_PATH}`;
    const block =
      url.pathname.startsWith("/ipfs/") &&
      url.searchParams.get("format") === "raw";
    if (!describe && !block) return next();
    if (!loopback(request)) {
      response.statusCode = 403;
      response.end("The chat product demo answers only on this machine.");
      return;
    }
    if (block) {
      const cid = url.pathname.slice("/ipfs/".length);
      const data = builds
        .map((built) => built.blocks.get(cid))
        .find((found) => found !== undefined);
      if (data === undefined) return next();
      response.setHeader("content-type", "application/vnd.ipld.raw");
      response.setHeader("cache-control", "no-store");
      response.end(Buffer.from(data));
      return;
    }
    latest().then(
      (built) => {
        const answer: DemoWorkerAnswer = {
          record: `this dev server, built from ${ENTRY}`,
          cid: built.cid,
          kind: "car",
          manifest: MANIFEST,
        };
        response.setHeader("content-type", "application/json");
        response.setHeader("cache-control", "no-store");
        response.end(JSON.stringify(answer));
      },
      (error: unknown) => {
        response.statusCode = 500;
        response.end(error instanceof Error ? error.message : String(error));
      },
    );
  };
}

/**
 * The local chat product demo, for the dev server only. It builds the
 * synthetic chat product's worker (`browser/chat-v2-product-worker.ts`),
 * packs it as an app archive, describes it at {@link DEMO_WORKER_PATH}, and
 * serves the archive's blocks, and only those, at `/ipfs/<cid>` as a gateway
 * would. The host's loader fetches and checks the archive as it checks any
 * worker's, so nothing skips the archive check. Requests from or to another
 * machine are refused. A build does not include it.
 */
export function chatProductDemo(): Plugin {
  let base = "/";
  return {
    name: "truapi-chat-product-demo",
    apply: "serve",
    configResolved(config) {
      base = config.base.startsWith("/") ? config.base : "/";
    },
    configureServer(server) {
      server.middlewares.use(serveDemo(base));
    },
  };
}
