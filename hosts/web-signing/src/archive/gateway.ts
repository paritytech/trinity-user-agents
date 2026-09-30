import {
  type Cid,
  cidKey,
  cidToString,
  parseCidString,
  verifyBlock,
} from "./cid.js";
import { type Car, type CarLimits, parseCar } from "./car.js";
import {
  type GetBlock,
  type UnpackLimits,
  readFile,
  unpackDirectory,
} from "./unixfs.js";

/** What a load may spend before it is refused. */
export type ArchiveLimits = CarLimits &
  UnpackLimits & {
    /** Wall-clock time for one whole load, network reads included. */
    deadlineMs: number;
  };

/** What this host accepts from an archive. Chosen for a product's built files, not for media libraries. */
export const ARCHIVE_LIMITS: ArchiveLimits = {
  maxBytes: 64 * 1024 * 1024,
  maxBlocks: 50_000,
  maxBlockBytes: 4 * 1024 * 1024,
  maxFiles: 10_000,
  maxNodes: 100_000,
  deadlineMs: 120_000,
  maxTotalBytes: 128 * 1024 * 1024,
  maxDirectoryDepth: 32,
  maxFileTreeDepth: 8,
  maxPathBytes: 1024,
};

const BLOCK_TIMEOUT_MS = 30_000;
/** Blocks kept so a block linked many times is fetched once. */
const CACHE_BYTES = 8 * 1024 * 1024;

/** The part of `fetch` the gateway reader uses, so a test can stand in for it. */
export type FetchLike = (url: string, init?: RequestInit) => Promise<Response>;

/**
 * Fetch one block from the gateway and check it against `cid`. The gateway is
 * a transport: what it sends is trusted only as far as the hash says.
 *
 * A verified block is kept, up to a small byte cap, so links that repeat a
 * block cost one request. The returned reader belongs to one load.
 */
export function gatewayBlocks(
  gateway: string,
  limits: Pick<CarLimits, "maxBlockBytes">,
  fetchFn: FetchLike = fetch,
): GetBlock {
  const cache = new Map<string, Uint8Array>();
  let cachedBytes = 0;
  return async (cid: Cid, signal?: AbortSignal) => {
    const key = cidKey(cid);
    const hit = cache.get(key);
    if (hit !== undefined) return hit;
    const url = new URL(`/ipfs/${cidToString(cid)}`, gateway);
    url.searchParams.set("format", "raw");
    const timeout = AbortSignal.timeout(BLOCK_TIMEOUT_MS);
    const response = await fetchFn(url.href, {
      signal:
        signal === undefined ? timeout : AbortSignal.any([signal, timeout]),
    });
    if (!response.ok) {
      await response.body?.cancel();
      throw new Error(`the gateway answered ${response.status} for a block`);
    }
    const data = await readCapped(response, limits.maxBlockBytes);
    if (!verifyBlock(cid, data))
      throw new Error(
        "the gateway sent a block that does not match its identifier",
      );
    if (cachedBytes + data.length <= CACHE_BYTES) {
      cache.set(key, data);
      cachedBytes += data.length;
    }
    return data;
  };
}

async function readCapped(
  response: Response,
  maxBytes: number,
): Promise<Uint8Array> {
  const reader = response.body?.getReader();
  if (reader === undefined) throw new Error("the gateway sent no body");
  const chunks: Uint8Array[] = [];
  let total = 0;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    total += value.length;
    if (total > maxBytes) {
      await reader.cancel();
      throw new Error("the gateway sent a block larger than this host accepts");
    }
    chunks.push(value);
  }
  const data = new Uint8Array(total);
  let at = 0;
  for (const chunk of chunks) {
    data.set(chunk, at);
    at += chunk.length;
  }
  return data;
}

/** The files of the site archived under `contentCid`, every byte verified. */
export interface LoadedArchive {
  files: Map<string, Uint8Array>;
  /** The directory root inside the archive. Not committed to by the DotNS record. */
  innerRoot: Cid;
}

/**
 * Load a product's executable: a UnixFS file, bound by the DotNS record, that
 * holds a CARv1 archive of the site's directory.
 *
 * Integrity is checked in two steps. The bytes of the archive file are checked
 * block by block against the record's CID. The blocks inside the archive are
 * then checked against their own identifiers. The second step proves only that
 * the site is consistent, since the archive's root is not on chain; the first
 * is what ties the site to the name. The RPC that resolved the name is a
 * separate trust decision, made before this function is called.
 */
export async function loadArchive(
  contentCid: string,
  getBlock: GetBlock,
  limits: ArchiveLimits = ARCHIVE_LIMITS,
  signal?: AbortSignal,
): Promise<LoadedArchive> {
  const within = deadline(limits, signal);
  const outer = await readFile(
    parseCidString(contentCid),
    getBlock,
    {
      maxNodes: limits.maxNodes,
      maxTotalBytes: limits.maxBytes,
      maxFileTreeDepth: limits.maxFileTreeDepth,
    },
    within,
  );
  const car: Car = parseCar(outer, limits);
  if (car.roots.length !== 1)
    throw new Error("the archive must have exactly one root");
  const innerRoot = car.roots[0];
  const files = await unpackDirectory(
    innerRoot,
    async (cid) => {
      const block = car.blocks.get(cidKey(cid));
      if (block === undefined)
        throw new Error("the archive is missing a block it refers to");
      return block;
    },
    limits,
    within,
  );
  if (!files.has("index.html"))
    throw new Error("the archive has no index.html at its root");
  return { files, innerRoot };
}

/**
 * Load a website: a UnixFS directory the DotNS record binds directly. Every
 * block is checked against the identifier that names it, from the record's own
 * CID down, so the whole site is tied to the name.
 */
export async function loadSite(
  contentCid: string,
  getBlock: GetBlock,
  limits: ArchiveLimits = ARCHIVE_LIMITS,
  signal?: AbortSignal,
): Promise<LoadedArchive> {
  const innerRoot = parseCidString(contentCid);
  const files = await unpackDirectory(
    innerRoot,
    getBlock,
    limits,
    deadline(limits, signal),
  );
  if (!files.has("index.html"))
    throw new Error("the site has no index.html at its root");
  return { files, innerRoot };
}

/** A signal that fires at the load's deadline, or when the caller's does. */
function deadline(limits: ArchiveLimits, signal?: AbortSignal): AbortSignal {
  const expired = AbortSignal.timeout(limits.deadlineMs);
  return signal === undefined ? expired : AbortSignal.any([signal, expired]);
}
