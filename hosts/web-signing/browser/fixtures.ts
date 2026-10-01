import { blake2b } from "@noble/hashes/blake2.js";
import { keccak_256 } from "@noble/hashes/sha3.js";
import {
  createServer as createHttpServer,
  type IncomingMessage,
  type Server,
  type ServerResponse,
} from "node:http";
import { createServer as createTcpServer } from "node:net";
import { fileURLToPath } from "node:url";
import type { BrowserContext } from "playwright-core";
import { createServer as createViteServer, type ViteDevServer } from "vite";
import { PASEO_DOTNS, bytesToHex, namehash } from "../src/dotns.js";
import { Blocks, car } from "../src/archive/fixtures.js";
import { cidToString } from "../src/archive/cid.js";

const hostRoot = fileURLToPath(new URL("..", import.meta.url));

/** A recovery phrase that is public in every Substrate tutorial. It holds nothing. */
export const TEST_MNEMONIC =
  "bottom drive obey lake curtain smoke basket hold race lonely fit walk";

/** A free loopback port. Another process could take it before it is used. */
export async function freePort(): Promise<number> {
  const server = createTcpServer();
  await new Promise<void>((resolve) =>
    server.listen(0, "127.0.0.1", () => resolve()),
  );
  const address = server.address();
  await new Promise((done) => server.close(done));
  if (address === null || typeof address === "string")
    throw new Error("no port was assigned");
  return address.port;
}

function listen(
  handler: (request: IncomingMessage, response: ServerResponse) => void,
): Promise<{ server: Server; port: number }> {
  return freePort().then(
    (port) =>
      new Promise((resolve) => {
        const server = createHttpServer(handler);
        server.listen(port, "127.0.0.1", () => resolve({ server, port }));
      }),
  );
}

/** The host page from this tree, served by Vite on loopback with its default settings. */
export interface RunningHost {
  /** The host's own origin. Its wallets live in this origin's storage. */
  origin: string;
  close(): Promise<void>;
}

export async function startHost(): Promise<RunningHost> {
  const port = await freePort();
  const vite: ViteDevServer = await createViteServer({
    root: hostRoot,
    logLevel: "error",
    server: { host: "127.0.0.1", port, strictPort: true },
  });
  await vite.listen();
  return { origin: `http://localhost:${port}`, close: () => vite.close() };
}

/**
 * A server that counts what reaches it. What a product's request does is only
 * known by what arrives here, never by what the page reports.
 */
export interface CountingTarget {
  origin: string;
  /** Requests received, by path. */
  readonly hits: string[];
  close(): void;
}

export async function startCountingTarget(): Promise<CountingTarget> {
  const hits: string[] = [];
  const { server, port } = await listen((request, response) => {
    if (request.method === "OPTIONS") {
      response.writeHead(204, {
        "access-control-allow-origin": "*",
        "access-control-allow-headers": "*",
      });
      return void response.end();
    }
    hits.push(new URL(request.url ?? "/", "http://target").pathname);
    response.writeHead(200, {
      "access-control-allow-origin": "*",
      "content-type": "text/plain",
    });
    response.end("reached");
  });
  return {
    origin: `http://127.0.0.1:${port}`,
    hits,
    close: () => void server.close(),
  };
}

/**
 * A product page that sends one request from its first script, before anything
 * else on the page runs, and exposes what it sees of its own origin. Its
 * requests go to `targetOrigin`. With `rootRelativeScript` it also loads
 * `/root.js` by a root-relative link, as a build made for the site root does.
 */
export function productHtml(
  targetOrigin: string,
  options: { rootRelativeScript?: boolean } = {},
): string {
  return `<!doctype html>
<meta charset="utf-8">
<title>fixture product</title>
<body>
<script>
  window.__first = { state: "pending" };
  fetch(${JSON.stringify(`${targetOrigin}/first`)}).then(
    () => { window.__first = { state: "sent" }; },
    (error) => { window.__first = { state: "refused", message: String(error) }; },
  );
</script>
<script>
  window.__loadedAt = performance.timeOrigin;
  window.__ask = (path) =>
    fetch(${JSON.stringify(targetOrigin)} + path).then(
      () => "sent",
      () => "refused",
    );
  document.body.dataset.origin = location.origin;
  document.body.dataset.localKeys = JSON.stringify(Object.keys(localStorage));
</script>${options.rootRelativeScript ? '\n<script src="/root.js"></script>' : ""}
</body>`;
}

/** What a site published as an app archive looks like on the gateway and on chain. */
export interface PublishedApp {
  /** Base32 CIDv1 of the archive file, as the name's record holds it. */
  contentCid: string;
  /** The archive's first bytes, which the host reads to see what the record holds. */
  prefix(length: number): Uint8Array;
  /** One block of the archive file, by its CID string. */
  block(cid: string): Uint8Array | undefined;
  /** The contenthash record for this content. */
  contenthash: Uint8Array;
}

export function publishApp(files: Record<string, string>): PublishedApp {
  const site = new Blocks();
  const siteRoot = site.tree(files);
  const archive = car(siteRoot, site);
  const outer = new Blocks();
  const outerRoot = outer.file(archive, 512);
  const blocks = new Map(
    [...outer.all.values()].map(({ cid, data }) => [cidToString(cid), data]),
  );
  return {
    contentCid: cidToString(outerRoot),
    prefix: (length) => archive.subarray(0, length),
    block: (cid) => blocks.get(cid),
    contenthash: Uint8Array.from([0xe3, 0x01, ...outerRoot.bytes]),
  };
}

/**
 * Asset Hub as the host reads a name: the resolver's child trie id, and one
 * Solidity `bytes` value under `keccak256(namehash ++ slot)`, spread over words
 * from `keccak256(slotKey)` on. Nothing else is answered.
 */
function contenthashStore(record: string, contenthash: Uint8Array) {
  const trieId = Uint8Array.from({ length: 32 }, (_, i) => i + 1);
  const store = new Map<string, Uint8Array>();
  const put = (key: Uint8Array, value: Uint8Array) =>
    store.set(bytesToHex(blake2b(key, { dkLen: 32 })), value);
  const slotKey = keccak_256(
    Uint8Array.from([...namehash(record), ...new Uint8Array(32)]),
  );
  const lengthWord = new Uint8Array(32);
  new DataView(lengthWord.buffer).setUint32(28, contenthash.length * 2 + 1);
  put(slotKey, lengthWord);
  let base: Uint8Array = keccak_256(slotKey);
  for (let at = 0; at < contenthash.length; at += 32) {
    const word = new Uint8Array(32);
    word.set(contenthash.subarray(at, at + 32));
    put(base, word);
    base = Uint8Array.from(base);
    for (let i = 31; i >= 0; i -= 1) {
      if (base[i] === 0xff) base[i] = 0;
      else {
        base[i] += 1;
        break;
      }
    }
  }
  const accountKey =
    "0x735f040a5d490f1107ad9c56f5ca00d2ae37ff0591fdbbcd9c2406df7147a9dc" +
    PASEO_DOTNS.contentResolver;
  const childPrefix = `0x${bytesToHex(
    new TextEncoder().encode(":child_storage:default:"),
  )}${bytesToHex(trieId)}`;
  return (method: string, params: string[]): unknown => {
    if (method === "state_getStorage")
      return params[0] === accountKey
        ? `0x00${bytesToHex(Uint8Array.of(trieId.length << 2))}${bytesToHex(trieId)}ff`
        : null;
    if (method === "childstate_getStorage") {
      if (params[0] !== childPrefix) return null;
      const value = store.get(params[1].slice(2));
      return value ? `0x${bytesToHex(value)}` : null;
    }
    throw new Error(`unexpected RPC method ${method}`);
  };
}

/**
 * Stand in for the two live services a name needs, inside one browser context:
 * Asset Hub's JSON-RPC socket, which holds `app.<name>`'s record, and the
 * Bulletin gateway, which serves the archive. Nothing leaves the machine.
 */
export async function serveName(
  context: BrowserContext,
  name: string,
  app: PublishedApp,
): Promise<void> {
  const read = contenthashStore(`app.${name}`, app.contenthash);
  const cors = {
    "access-control-allow-origin": "*",
    "access-control-allow-headers": "*",
  };
  await context.route(
    `${PASEO_DOTNS.assetHubRpc.replace(/^ws/, "http")}/**`,
    async (route) => {
      const request = route.request();
      if (request.method() === "OPTIONS")
        return route.fulfill({ status: 204, headers: cors });
      const { id, method, params } = request.postDataJSON() as {
        id: number;
        method: string;
        params: string[];
      };
      return route.fulfill({
        status: 200,
        headers: { ...cors, "content-type": "application/json" },
        body: JSON.stringify({
          jsonrpc: "2.0",
          id,
          result: read(method, params),
        }),
      });
    },
  );
  await context.route(`${PASEO_DOTNS.contentGateway}/**`, async (route) => {
    const request = route.request();
    if (request.method() === "OPTIONS")
      return route.fulfill({ status: 204, headers: cors });
    const url = new URL(request.url());
    const cid = url.pathname.split("/").filter(Boolean)[1] ?? "";
    if (url.searchParams.get("format") === "raw") {
      const block = app.block(cid);
      return block === undefined
        ? route.fulfill({ status: 404, headers: cors })
        : route.fulfill({
            status: 200,
            headers: { ...cors, "content-type": "application/vnd.ipld.raw" },
            body: Buffer.from(block),
          });
    }
    return cid === app.contentCid
      ? route.fulfill({
          status: 206,
          headers: { ...cors, "content-type": "application/octet-stream" },
          body: Buffer.from(app.prefix(256)),
        })
      : route.fulfill({ status: 404, headers: cors });
  });
}

/** A product served by URL, on loopback and on a port of its own. */
export interface UrlProduct {
  url: string;
  close(): void;
}

export async function startUrlProduct(
  targetOrigin: string,
): Promise<UrlProduct> {
  const html = productHtml(targetOrigin);
  const { server, port } = await listen((_request, response) => {
    response.writeHead(200, { "content-type": "text/html; charset=utf-8" });
    response.end(html);
  });
  return { url: `http://localhost:${port}/`, close: () => void server.close() };
}
