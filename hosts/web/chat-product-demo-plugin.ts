import type { IncomingMessage, ServerResponse } from "node:http";
import { fileURLToPath } from "node:url";
import { type Plugin, build } from "vite";
import { cidToString } from "./src/archive/cid.js";
import { Blocks, car } from "./src/archive/fixtures.js";
import {
  DEMO_WORKER_PATH,
  EXAMPLE_CHAT_PATH,
  LOOPBACK_HOSTNAMES,
  type DemoWorkerAnswer,
} from "./src/chat-v2/local-demo-route.js";

const hostRoot = fileURLToPath(new URL(".", import.meta.url));

/** The products this server builds: where each is described, its entry module, and whether it is an app. */
const WORKERS = [
  { path: DEMO_WORKER_PATH, entry: "browser/chat-v2-product-worker.ts" },
  {
    path: EXAMPLE_CHAT_PATH,
    entry: "src/dev-tools/example-chat/product.ts",
    app: true,
  },
];

/** The page an app archive opens with. */
const APP_PAGE =
  '<!doctype html><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>Example Chat</title><script type="module" src="/index.js"></script>';

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

async function bundle(entry: string): Promise<string> {
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
        input: `${hostRoot}${entry}`,
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
  throw new Error(`no module was built from ${entry}`);
}

/** Pack the module as an app archive: a CAR of the site, stored as a UnixFS file. */
function pack(code: string, app = false): Built {
  const site = new Blocks();
  const files: Record<string, string> = app
    ? { "index.html": APP_PAGE, "index.js": code }
    : { "index.js": code };
  const archive = car(site.tree(files), site);
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
export function loopback(request: IncomingMessage): boolean {
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

/** Build one worker again on each ask, so a restart picks up an edit, keeping recent archives. */
function builder(entry: string, app = false) {
  const builds: Built[] = [];
  let building: Promise<Built> | null = null;
  return {
    builds,
    latest: () =>
      (building ??= bundle(entry)
        .then((code) => pack(code, app))
        .then((built) => {
          if (builds[0]?.cid !== built.cid) builds.unshift(built);
          builds.splice(KEPT_BUILDS);
          return built;
        })
        .finally(() => (building = null))),
  };
}

function serveDemo(base: string): Middleware {
  const workers = WORKERS.map((worker) => ({
    ...worker,
    ...builder(worker.entry, worker.app),
  }));

  return (request, response, next) => {
    const url = new URL(request.url ?? "/", "http://dev.invalid");
    const described = workers.find(
      (worker) => url.pathname === `${base}${worker.path}`,
    );
    const block =
      url.pathname.startsWith("/ipfs/") &&
      url.searchParams.get("format") === "raw";
    if (described === undefined && !block) return next();
    if (!loopback(request)) {
      response.statusCode = 403;
      response.end("The chat product demo answers only on this machine.");
      return;
    }
    if (block) {
      const cid = url.pathname.slice("/ipfs/".length);
      const data = workers
        .flatMap((worker) => worker.builds)
        .map((built) => built.blocks.get(cid))
        .find((found) => found !== undefined);
      if (data === undefined) return next();
      response.setHeader("content-type", "application/vnd.ipld.raw");
      response.setHeader("cache-control", "no-store");
      response.end(Buffer.from(data));
      return;
    }
    described!.latest().then(
      (built) => {
        const answer: DemoWorkerAnswer = {
          record: `this dev server, built from ${described!.entry}`,
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
 * The local chat products, for the dev server only. It builds the synthetic
 * chat product's worker (`browser/chat-v2-product-worker.ts`), packs it as an
 * app archive and describes it at {@link DEMO_WORKER_PATH}, and does the same
 * for Example Chat, the test users' app, at {@link EXAMPLE_CHAT_PATH}. It serves the archives' blocks, and only those, at `/ipfs/<cid>` as a
 * gateway would. The host's loader fetches and checks the archive as it checks any
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
