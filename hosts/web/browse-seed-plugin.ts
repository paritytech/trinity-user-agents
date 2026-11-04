import type { IncomingMessage, ServerResponse } from "node:http";
import type { Plugin } from "vite";
import { defaultAddresses } from "./src/app/dock-defaults.js";
import { cidToString } from "./src/products/archive/cid.js";
import {
  ARCHIVE_LIMITS,
  type FetchLike,
  gatewayBlocks,
  loadArchive,
  loadSite,
} from "./src/products/archive/gateway.js";
import type { GetBlock } from "./src/products/archive/unixfs.js";
import { type SourceChoice, resolveSource } from "./src/products/dotns.js";
import { type ContentSeed, SEED_MANIFEST_PATH } from "./src/products/seed.js";
import {
  DEFAULT_NETWORK_CONFIG,
  type NetworkConfig,
} from "./src/platform/network-config.js";

/** `0` builds without the seed; `1` asks the dev server for one too. */
export const SEED_ENV = "WEB_SIGNING_SEED_BROWSE";

/**
 * The most a seed may add to the build. The Browse app's archive was 5.8 MB
 * when this was set; a seed past the cap fails the build rather than growing
 * every deploy.
 */
export const SEED_MAX_BYTES = 16 * 1024 * 1024;

/** What {@link SEED_MANIFEST_PATH} holds: the seed, and when and where it was fetched. */
export interface SeedManifest extends ContentSeed {
  network: string;
  fetchedAt: string;
  bytes: number;
  /** The CIDs of the blocks written at `ipfs/<cid>`. */
  blocks: string[];
}

/** A fetched seed: its manifest and its checked blocks, by CID string. */
export interface FetchedSeed {
  manifest: SeedManifest;
  blocks: Map<string, Uint8Array>;
}

/** What fetching a seed reads from the outside, so a test can stand in for it. */
export interface SeedReaders {
  resolve(name: string, network: NetworkConfig): Promise<SourceChoice>;
  fetch: FetchLike;
  now(): Date;
}

const liveReaders: SeedReaders = {
  resolve: resolveSource,
  fetch: (url, init) => fetch(url, init),
  now: () => new Date(),
};

/**
 * Resolve `name` on `network` and read its content from the network's gateway
 * block by block, as the host's loader does: every block is checked against
 * its identifier, and the whole archive or site is unpacked, so a seed that
 * would not load is never written. Only the blocks the load read are kept.
 */
export async function fetchSeed(
  name: string,
  network: NetworkConfig,
  readers: SeedReaders = liveReaders,
  maxBytes = SEED_MAX_BYTES,
): Promise<FetchedSeed> {
  const { source } = await readers.resolve(name, network);
  if (source === null)
    throw new Error(`${name} has no app or site on ${network.displayName}`);
  const raw = gatewayBlocks(
    network.dotns.contentGateway,
    ARCHIVE_LIMITS,
    readers.fetch,
  );
  const blocks = new Map<string, Uint8Array>();
  let bytes = 0;
  const kept: GetBlock = async (cid, signal) => {
    const data = await raw(cid, signal);
    const key = cidToString(cid);
    if (!blocks.has(key)) {
      bytes += data.length;
      if (bytes > maxBytes)
        throw new Error(
          `${source.record} holds more than the ${maxBytes / 1024 / 1024} MiB a seed may add`,
        );
      blocks.set(key, data);
    }
    return data;
  };
  await (source.kind === "car" ? loadArchive : loadSite)(source.cid, kept);
  return {
    manifest: {
      record: source.record,
      cid: source.cid,
      kind: source.kind,
      network: network.id,
      fetchedAt: readers.now().toISOString(),
      bytes,
      blocks: [...blocks.keys()],
    },
    blocks,
  };
}

/** The files a build writes for `seed`: its manifest and its blocks at `ipfs/<cid>`. */
export function seedFiles(seed: FetchedSeed): Map<string, string | Uint8Array> {
  const files = new Map<string, string | Uint8Array>([
    [SEED_MANIFEST_PATH, JSON.stringify(seed.manifest, null, 2)],
  ]);
  for (const [cid, data] of seed.blocks) files.set(`ipfs/${cid}`, data);
  return files;
}

/** Answer gateway-style raw block requests for the seed's blocks; pass on the rest. */
function serveBlocks(
  seed: FetchedSeed,
): (
  request: IncomingMessage,
  response: ServerResponse,
  next: () => void,
) => void {
  return (request, response, next) => {
    const url = new URL(request.url ?? "/", "http://dev.invalid");
    const data = url.pathname.startsWith("/ipfs/")
      ? seed.blocks.get(url.pathname.slice("/ipfs/".length))
      : undefined;
    if (url.searchParams.get("format") !== "raw" || data === undefined)
      return next();
    response.setHeader("content-type", "application/vnd.ipld.raw");
    response.setHeader("cache-control", "no-store");
    response.end(Buffer.from(data));
  };
}

/**
 * Ship the Browse app with the build, so a first open does not wait on the
 * gateway. A build resolves the default network's Browse name, fetches and
 * checks its content, and writes the blocks at `/ipfs/<cid>` beside the page,
 * as Contact Chat's are, with {@link SEED_MANIFEST_PATH} describing them. The
 * host reads them only when the chain still points the name at that CID.
 *
 * A build seeds unless {@link SEED_ENV} is `0`, and fails if the seed cannot
 * be fetched, so a deploy never goes out without it unnoticed. A dev server
 * seeds only when it is `1`, and `vite preview` serves what the build wrote.
 * The blocks sit at the origin's root, so a build under another base path has
 * no seed.
 */
export function browseSeed(
  network: NetworkConfig = DEFAULT_NETWORK_CONFIG,
  readers: SeedReaders = liveReaders,
): Plugin {
  let seed: FetchedSeed | null = null;
  return {
    name: "truapi-browse-seed",
    async config(config, { command, isPreview }) {
      const setting = process.env[SEED_ENV];
      const wanted =
        command === "build"
          ? setting !== "0"
          : setting === "1" && isPreview !== true;
      const atRoot = (config.base ?? "/") === "/";
      if (wanted && !atRoot) {
        if (setting === "1")
          throw new Error(
            `${SEED_ENV}=1 needs base "/": the seed's blocks are served at the origin's root`,
          );
        console.warn(
          `Building without the Browse seed: it is served at the origin's root, and base is ${config.base}`,
        );
      }
      const name = defaultAddresses(network.id).browse;
      seed = null;
      if (wanted && atRoot && name !== "")
        seed = await fetchSeed(name, network, readers).catch(
          (error: unknown) => {
            throw new Error(
              `Could not seed ${name} from ${network.displayName}: ${error instanceof Error ? error.message : String(error)}. Set ${SEED_ENV}=0 to build without it.`,
            );
          },
        );
      const shipped: ContentSeed | null =
        seed === null
          ? null
          : {
              record: seed.manifest.record,
              cid: seed.manifest.cid,
              kind: seed.manifest.kind,
            };
      return { define: { __CONTENT_SEED__: JSON.stringify(shipped) } };
    },
    configureServer(server) {
      if (seed !== null) server.middlewares.use(serveBlocks(seed));
    },
    generateBundle() {
      if (seed === null) return;
      for (const [fileName, source] of seedFiles(seed))
        this.emitFile({ type: "asset", fileName, source });
    },
  };
}
