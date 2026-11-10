import { readFile } from "node:fs/promises";
import type { IncomingMessage, ServerResponse } from "node:http";
import { fileURLToPath } from "node:url";
import { type Plugin, build } from "vite";
import { cidToString } from "./src/products/archive/cid.js";
import { Blocks, car } from "./fixtures/archive/builders.js";
import { CONTACT_CHAT_PATH } from "./apps/contact-chat/route.js";
import type { BundledWorkerAnswer } from "./src/products/worker-manifest.js";
import {
  KETTLE_CHAT_PATH,
  KETTLE_GLUE,
  KETTLE_WASM,
  KETTLE_WASM_PACKAGE,
} from "./apps/kettle-chat/route.js";
import { NOVA_CHAT_PATH } from "./apps/nova-chat/route.js";

const hostRoot = fileURLToPath(new URL(".", import.meta.url));

/** Contact Chat's worker entry module. */
const ENTRY = "apps/contact-chat/worker.ts";

/** Kettle Chat's worker entry module. */
const KETTLE_ENTRY = "apps/kettle-chat/worker.ts";
/** Nova Chat's worker entry. */
const NOVA_ENTRY = "apps/nova-chat/worker.ts";

/**
 * `1` builds Kettle Chat, and the developer tools' Chat tab that turns it
 * on, into the host, for a private test deployment. Absent: a build has
 * neither; the dev server always serves both.
 */
export const KETTLE_CHAT_ENV = "WEB_SIGNING_KETTLE_CHAT";

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

async function bundle(production: boolean, entry = ENTRY): Promise<string> {
  const result = await build({
    configFile: false,
    root: hostRoot,
    logLevel: "warn",
    publicDir: false,
    define: {
      "process.env.NODE_ENV": production ? '"production"' : '"development"',
      __WORKER_PROBE__: JSON.stringify(!production),
    },
    build: {
      write: false,
      minify: production,
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

/** Pack the module, and any files beside it, as an app archive: a CAR of the site, stored as a UnixFS file. */
function pack(code: string, files: Record<string, Uint8Array> = {}): Built {
  const site = new Blocks();
  const archive = car(site.tree({ ...files, "index.js": code }), site);
  const outer = new Blocks();
  const root = outer.file(archive, 256 * 1024);
  return {
    cid: cidToString(root),
    blocks: new Map(
      [...outer.all.values()].map(({ cid, data }) => [cidToString(cid), data]),
    ),
  };
}

const answer = (cid: string, record: string): BundledWorkerAnswer => ({
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

/**
 * Kettle Chat's worker with the WebAssembly `npm run build:kettle` wrote
 * beside it, in one archive. Fails, saying so, when it was not built.
 */
async function kettleArchive(production = false): Promise<Built> {
  const read = (name: string) =>
    readFile(`${hostRoot}${KETTLE_WASM_PACKAGE}/${name}`).then(
      (bytes) => new Uint8Array(bytes),
      () => {
        throw new Error(
          `Kettle Chat's WebAssembly is not built: run \`npm run build:kettle\` in hosts/web (${KETTLE_WASM_PACKAGE}/${name} is missing).`,
        );
      },
    );
  const [glue, wasm] = await Promise.all([
    read(KETTLE_GLUE),
    read(KETTLE_WASM),
  ]);
  return pack(await bundle(production, KETTLE_ENTRY), {
    [KETTLE_GLUE]: glue,
    [KETTLE_WASM]: wasm,
  });
}

/** Build the worker again on each ask, so a restart picks up an edit, keeping recent archives. */
function serveWorker(
  base: string,
  path = CONTACT_CHAT_PATH,
  make: () => Promise<Built> = () => bundle(false).then((code) => pack(code)),
  entry = ENTRY,
): Middleware {
  const builds: Built[] = [];
  let building: Promise<Built> | null = null;
  const latest = () =>
    (building ??= make()
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
    if (url.pathname !== `${base}${path}`) return next();
    latest().then(
      (built) => {
        response.setHeader("content-type", "application/json");
        response.setHeader("cache-control", "no-store");
        response.end(
          JSON.stringify(
            answer(built.cid, `this dev server, built from ${entry}`),
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
 *
 * With {@link KETTLE_CHAT_ENV} set to `1`, a build writes Kettle Chat's
 * worker the same way, at {@link KETTLE_CHAT_PATH}, and fails when its
 * WebAssembly is not built, and Nova Chat's at {@link NOVA_CHAT_PATH}. The
 * dev server serves both on request. The flag reaches the page as
 * `__KETTLE_CHAT_BUILD__`.
 */
export function contactChatWorker(): Plugin {
  let base = "/";
  const kettle = process.env[KETTLE_CHAT_ENV] === "1";
  return {
    name: "truapi-contact-chat-worker",
    config: (_config, { command }) => ({
      define: {
        __KETTLE_CHAT_BUILD__: JSON.stringify(command === "build" && kettle),
      },
    }),
    configResolved(config) {
      base = config.base.startsWith("/") ? config.base : "/";
    },
    configureServer(server) {
      server.middlewares.use(serveWorker(base));
      server.middlewares.use(
        serveWorker(base, KETTLE_CHAT_PATH, kettleArchive, KETTLE_ENTRY),
      );
      server.middlewares.use(
        serveWorker(
          base,
          NOVA_CHAT_PATH,
          () => bundle(false, NOVA_ENTRY).then((code) => pack(code)),
          NOVA_ENTRY,
        ),
      );
    },
    async generateBundle() {
      const workers: [string, Built][] = [
        [CONTACT_CHAT_PATH, pack(await bundle(true))],
      ];
      if (kettle) {
        workers.push([KETTLE_CHAT_PATH, await kettleArchive(true)]);
        workers.push([NOVA_CHAT_PATH, pack(await bundle(true, NOVA_ENTRY))]);
      }
      for (const [path, built] of workers) {
        this.emitFile({
          type: "asset",
          fileName: path,
          source: JSON.stringify(answer(built.cid, "this host's build")),
        });
        for (const [cid, data] of built.blocks)
          this.emitFile({
            type: "asset",
            fileName: `ipfs/${cid}`,
            source: data,
          });
      }
    },
  };
}
