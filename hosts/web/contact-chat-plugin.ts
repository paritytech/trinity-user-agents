import type { IncomingMessage, ServerResponse } from "node:http";
import { fileURLToPath } from "node:url";
import { type Plugin, build } from "vite";
import { cidToString } from "./src/products/archive/cid.js";
import { Blocks, car } from "./fixtures/archive/builders.js";
import {
  CONTACT_CHAT_PATH,
  type ContactChatWorkerAnswer,
} from "./src/chat/contact-chat/route.js";

const hostRoot = fileURLToPath(new URL(".", import.meta.url));

/** Contact Chat's worker entry module. */
const ENTRY = "src/chat/contact-chat/worker.ts";

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

async function bundle(production: boolean): Promise<string> {
  const result = await build({
    configFile: false,
    root: hostRoot,
    logLevel: "warn",
    publicDir: false,
    define: {
      "process.env.NODE_ENV": production ? '"production"' : '"development"',
    },
    build: {
      write: false,
      minify: production,
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

const answer = (cid: string, record: string): ContactChatWorkerAnswer => ({
  record,
  cid,
  kind: "car",
  manifest: MANIFEST,
});

type Middleware = (
  request: IncomingMessage,
  response: ServerResponse,
  next: () => void,
) => void;

/** Build the worker again on each ask, so a restart picks up an edit, keeping recent archives. */
function serveWorker(base: string): Middleware {
  const builds: Built[] = [];
  let building: Promise<Built> | null = null;
  const latest = () =>
    (building ??= bundle(false)
      .then(pack)
      .then((built) => {
        if (builds[0]?.cid !== built.cid) builds.unshift(built);
        builds.splice(KEPT_BUILDS);
        return built;
      })
      .finally(() => (building = null)));

  return (request, response, next) => {
    const url = new URL(request.url ?? "/", "http://dev.invalid");
    if (
      url.pathname.startsWith("/ipfs/") &&
      url.searchParams.get("format") === "raw"
    ) {
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
    if (url.pathname !== `${base}${CONTACT_CHAT_PATH}`) return next();
    latest().then(
      (built) => {
        response.setHeader("content-type", "application/json");
        response.setHeader("cache-control", "no-store");
        response.end(
          JSON.stringify(
            answer(built.cid, `this dev server, built from ${ENTRY}`),
          ),
        );
      },
      (error: unknown) => {
        response.statusCode = 500;
        response.end(error instanceof Error ? error.message : String(error));
      },
    );
  };
}

/**
 * Contact Chat's worker, the host's own chat product. The dev server builds it
 * on request and a build writes it beside the page: packed as an app archive,
 * described at {@link CONTACT_CHAT_PATH}, with the archive's blocks, and only
 * those, at `/ipfs/<cid>` as a gateway serves them. The host's loader fetches
 * and checks the archive as it checks any worker's. The blocks sit at the
 * origin's root, so a build served under another base path finds none.
 */
export function contactChatWorker(): Plugin {
  let base = "/";
  return {
    name: "truapi-contact-chat-worker",
    configResolved(config) {
      base = config.base.startsWith("/") ? config.base : "/";
    },
    configureServer(server) {
      server.middlewares.use(serveWorker(base));
    },
    async generateBundle() {
      const built = pack(await bundle(true));
      this.emitFile({
        type: "asset",
        fileName: CONTACT_CHAT_PATH,
        source: JSON.stringify(answer(built.cid, "this host's build")),
      });
      for (const [cid, data] of built.blocks)
        this.emitFile({ type: "asset", fileName: `ipfs/${cid}`, source: data });
    },
  };
}
