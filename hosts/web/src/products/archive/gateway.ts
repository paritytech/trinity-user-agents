import {
  type Cid,
  cidKey,
  cidToString,
  parseCidString,
  verifyBlock,
} from "./cid.js";
import { type Car, type CarLimits, parseCar, readCarStream } from "./car.js";
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
    /** Time for one attempt at one block, from the request to the last byte. */
    blockTimeoutMs: number;
    /**
     * A CAR stream that brings fewer than `carWindowBytes` in a window of
     * `carWindowMs` is dropped. The windows run back to back from the request,
     * so the first one includes the wait for the first byte.
     */
    carWindowMs: number;
    /** The least a CAR stream must bring in each `carWindowMs`. */
    carWindowBytes: number;
    /**
     * Time from the CAR request after which reads stop waiting for the stream
     * and read a block it has not brought yet one by one, while the stream
     * keeps running. A CAR the gateway has cached was measured to send its first
     * byte within 150 ms and 24 MB within a second; one it has not cached came
     * at a few KB/s to 500 KB/s, too slow to wait for.
     */
    carHedgeMs: number;
  };

/** What this host accepts from an archive. Chosen for a product's built files, not for media libraries. */
export const ARCHIVE_LIMITS: ArchiveLimits = {
  maxBytes: 64 * 1024 * 1024,
  maxBlocks: 50_000,
  maxBlockBytes: 4 * 1024 * 1024,
  maxFiles: 10_000,
  maxNodes: 100_000,
  deadlineMs: 120_000,
  blockTimeoutMs: 30_000,
  carWindowMs: 10_000,
  carWindowBytes: 1024 * 1024,
  carHedgeMs: 2_000,
  maxTotalBytes: 128 * 1024 * 1024,
  maxDirectoryDepth: 32,
  maxFileTreeDepth: 8,
  maxPathBytes: 1024,
};

/**
 * Block requests one load keeps open at once. The cap is here, not in the
 * traversal, so a request waiting for a browser connection does not spend its
 * attempt's time.
 */
export const MAX_BLOCK_REQUESTS = 6;
/** Statuses that say the gateway may answer a second request. */
const TRANSIENT_STATUSES = new Set([408, 429, 500, 502, 503, 504]);
/** Of those, the statuses that ask the client to slow down. */
const BUSY_STATUSES = new Set([429, 503]);
/**
 * Wait before asking again after a busy status. The wait is fixed and short,
 * and `Retry-After` is not read, so a retry cannot push a load past its
 * deadline on the gateway's word. The request keeps its slot while it waits.
 */
export const BUSY_RETRY_MS = 500;
/** Blocks kept so a block linked many times is fetched once. */
const CACHE_BYTES = 8 * 1024 * 1024;

/** The part of `fetch` the gateway reader uses, so a test can stand in for it. */
export type FetchLike = (url: string, init?: RequestInit) => Promise<Response>;

/**
 * Fetch one block from the gateway and check it against `cid`. The gateway is
 * a transport: what it sends is trusted only as far as the hash says.
 *
 * At most {@link MAX_BLOCK_REQUESTS} requests are open at once. A block whose
 * request times out, fails on the network or gets a transient status is asked
 * for once more, after {@link BUSY_RETRY_MS} for a busy status and at once
 * otherwise. A block that does not match its identifier is not.
 *
 * A verified block is kept, up to a small byte cap, and a block already being
 * fetched is not asked for again, so links that repeat a block cost one
 * request. The returned reader belongs to one load, whose reads share one
 * signal.
 */
export function gatewayBlocks(
  gateway: string,
  limits: Pick<ArchiveLimits, "maxBlockBytes" | "blockTimeoutMs">,
  fetchFn: FetchLike = fetch,
): GetBlock {
  const cache = new Map<string, Uint8Array>();
  const fetching = new Map<string, Promise<Uint8Array>>();
  const requests = slots(MAX_BLOCK_REQUESTS);
  let cachedBytes = 0;

  async function fetchBlock(
    cid: Cid,
    signal: AbortSignal | undefined,
  ): Promise<Uint8Array> {
    const url = new URL(`/ipfs/${cidToString(cid)}`, gateway);
    url.searchParams.set("format", "raw");
    const attempt = () => requestBlock(url.href, limits, fetchFn, signal);
    const outcome = await requests(async () => {
      const first = await attempt();
      if (!("retry" in first)) return first;
      if (first.afterMs > 0) await pause(first.afterMs, signal);
      return attempt();
    });
    if ("retry" in outcome) throw outcome.retry;
    if (!verifyBlock(cid, outcome.block))
      throw new Error(
        "the gateway sent a block that does not match its identifier",
      );
    if (cachedBytes + outcome.block.length <= CACHE_BYTES) {
      cache.set(cidKey(cid), outcome.block);
      cachedBytes += outcome.block.length;
    }
    return outcome.block;
  }

  return (cid: Cid, signal?: AbortSignal) => {
    const key = cidKey(cid);
    const hit = cache.get(key);
    if (hit !== undefined) return Promise.resolve(hit);
    const pending = fetching.get(key);
    if (pending !== undefined) return pending;
    const read = fetchBlock(cid, signal).finally(() => fetching.delete(key));
    fetching.set(key, read);
    return read;
  };
}

/**
 * One attempt's result: the block, or the error to report if a retry fails
 * too and how long to wait before that retry.
 */
type Attempt = { block: Uint8Array } | { retry: Error; afterMs: number };

async function requestBlock(
  url: string,
  limits: Pick<ArchiveLimits, "maxBlockBytes" | "blockTimeoutMs">,
  fetchFn: FetchLike,
  signal: AbortSignal | undefined,
): Promise<Attempt> {
  signal?.throwIfAborted();
  const timeout = AbortSignal.timeout(limits.blockTimeoutMs);
  const failed = (error: unknown): Attempt => {
    if (signal?.aborted) throw error;
    if (timeout.aborted)
      return {
        retry: new Error(
          `the gateway did not send a block within ${limits.blockTimeoutMs / 1000} s`,
        ),
        afterMs: 0,
      };
    if (error instanceof TypeError)
      return {
        retry: new Error(`the gateway could not be reached: ${error.message}`),
        afterMs: 0,
      };
    throw error;
  };
  let response: Response;
  try {
    response = await fetchFn(url, {
      signal:
        signal === undefined ? timeout : AbortSignal.any([signal, timeout]),
    });
  } catch (error) {
    return failed(error);
  }
  if (!response.ok) {
    await response.body?.cancel();
    const error = new Error(
      `the gateway answered ${response.status} for a block`,
    );
    if (TRANSIENT_STATUSES.has(response.status))
      return {
        retry: error,
        afterMs: BUSY_STATUSES.has(response.status) ? BUSY_RETRY_MS : 0,
      };
    throw error;
  }
  try {
    return { block: await readCapped(response, limits.maxBlockBytes) };
  } catch (error) {
    return failed(error);
  }
}

/** Resolve after `ms`, or reject as soon as `signal` aborts. */
function pause(ms: number, signal: AbortSignal | undefined): Promise<void> {
  return new Promise((resolve, reject) => {
    if (signal?.aborted) return reject(signal.reason);
    const stop = () => {
      clearTimeout(timer);
      reject(signal?.reason);
    };
    const timer = setTimeout(() => {
      signal?.removeEventListener("abort", stop);
      resolve();
    }, ms);
    signal?.addEventListener("abort", stop, { once: true });
  });
}

/** Run at most `size` tasks at once, starting waiting tasks in the order they came. */
function slots(size: number): <T>(task: () => Promise<T>) => Promise<T> {
  let running = 0;
  const waiting: (() => void)[] = [];
  return async (task) => {
    if (running < size) running += 1;
    else await new Promise<void>((resolve) => waiting.push(resolve));
    try {
      return await task();
    } finally {
      const next = waiting.shift();
      if (next === undefined) running -= 1;
      else next();
    }
  };
}

/** A response body, refused once it passes `maxBytes`. */
export async function readCapped(
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

/**
 * Whether a load asks the gateway for the whole DAG as one CAR stream before
 * it asks block by block. Set to false to read block by block only.
 */
export const FETCH_CAR = true;
const CAR_TYPE = "application/vnd.ipld.car";

/**
 * Read the DAG under `root` from one `?format=car` stream, and the blocks the
 * stream does not bring from `fallback`.
 *
 * The stream starts with the first read and shares its signal, so the reader
 * belongs to one load, as {@link gatewayBlocks}'s does, and the stream stops
 * when that read's traversal ends. Every block is checked against its
 * identifier as it arrives and kept for the load. Until `carHedgeMs` after the
 * request, a read waits for its block from the stream. After that, a read
 * takes its block from the stream if it is already here and otherwise reads
 * it through `fallback`, while the stream keeps running; a block can then
 * arrive both ways. The stream ends at its last byte, at the first error (a
 * failed request, a status other than 2xx, another content type, a header
 * that names another root, a block that does not match, a limit) or when it
 * runs slower than `carWindowBytes` per `carWindowMs`. It is not asked for
 * again: after it ends, a block it did not bring is read through `fallback`,
 * and one it did is not read again.
 *
 * The stream holds at most `maxBytes` of blocks, the size of the largest
 * archive file, as well as `maxBlocks` and `maxBlockBytes`. A site with more
 * block bytes than that gets the rest through `fallback`. The traversal still
 * charges each reference to its own limits.
 */
export function carBlocks(
  root: Cid,
  gateway: string,
  limits: Pick<
    ArchiveLimits,
    | "maxBytes"
    | "maxBlocks"
    | "maxBlockBytes"
    | "carWindowMs"
    | "carWindowBytes"
    | "carHedgeMs"
  >,
  fallback: GetBlock,
  fetchFn: FetchLike = fetch,
): GetBlock {
  const received = new Map<string, Uint8Array>();
  const waiting = new Map<string, (() => void)[]>();
  let streaming: Promise<void> | undefined;
  /** Reads no longer wait: the stream ended or the hedge time passed. */
  let unwaited = false;

  function wake(key: string): void {
    for (const resolve of waiting.get(key) ?? []) resolve();
    waiting.delete(key);
  }

  function release(): void {
    unwaited = true;
    for (const key of [...waiting.keys()]) wake(key);
  }

  async function stream(signal: AbortSignal | undefined): Promise<void> {
    const stop = new AbortController();
    let arrived = 0;
    let pace: ReturnType<typeof setInterval> | undefined;
    let hedge: ReturnType<typeof setTimeout> | undefined;
    try {
      pace = setInterval(() => {
        if (arrived < limits.carWindowBytes) stop.abort();
        arrived = 0;
      }, limits.carWindowMs);
      hedge = setTimeout(release, limits.carHedgeMs);
      const url = new URL(`/ipfs/${cidToString(root)}`, gateway);
      url.searchParams.set("format", "car");
      const body = carBody(
        url.href,
        fetchFn,
        signal === undefined
          ? stop.signal
          : AbortSignal.any([signal, stop.signal]),
        (bytes) => (arrived += bytes),
      );
      const blocks = readCarStream(body, root, limits);
      for await (const { cid, data } of blocks) {
        const key = cidKey(cid);
        received.set(key, data);
        wake(key);
      }
    } catch {
      // However the stream ended, the reads it did not answer go to the fallback.
    } finally {
      clearInterval(pace);
      clearTimeout(hedge);
      stop.abort();
      release();
    }
  }

  return async (cid, signal) => {
    streaming ??= stream(signal);
    const key = cidKey(cid);
    if (!unwaited && !received.has(key))
      await new Promise<void>((resolve) =>
        waiting.set(key, [...(waiting.get(key) ?? []), resolve]),
      );
    return received.get(key) ?? fallback(cid, signal);
  };
}

/**
 * The body of a CAR response, chunk by chunk, with each chunk's size told to
 * `arrived`. Anything but a 2xx CAR is an error.
 */
async function* carBody(
  url: string,
  fetchFn: FetchLike,
  signal: AbortSignal,
  arrived: (bytes: number) => void,
): AsyncGenerator<Uint8Array> {
  const response = await fetchFn(url, {
    signal,
    headers: { accept: CAR_TYPE },
  });
  const type = response.headers
    .get("content-type")
    ?.split(";")[0]
    .trim()
    .toLowerCase();
  if (!response.ok || type !== CAR_TYPE || response.body === null) {
    await response.body?.cancel();
    throw new Error("the gateway did not send a CAR");
  }
  const reader = response.body.getReader();
  try {
    for (;;) {
      const { done, value } = await reader.read();
      if (done) return;
      arrived(value.length);
      yield value;
    }
  } finally {
    await reader.cancel().catch(() => {});
  }
}

/** The files of the site archived under `contentCid`, every byte verified. */
export interface LoadedArchive {
  files: Map<string, Uint8Array>;
  /** The directory root inside the archive. Not committed to by the DotNS record. */
  innerRoot: Cid;
}

/**
 * Load a product's executable: a UnixFS file, bound by the DotNS record, that
 * holds a CARv1 archive of the site's directory. It must hold `entry` at its
 * root: `index.html` for an app, the entry module for a worker.
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
  entry = "index.html",
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
  if (!files.has(entry)) throw new Error(`the archive has no ${entry}`);
  return { files, innerRoot };
}

/**
 * Load a website: a UnixFS directory the DotNS record binds directly. Every
 * block is checked against the identifier that names it, from the record's own
 * CID down, so the whole site is tied to the name. It must hold `entry`, as
 * {@link loadArchive}'s must.
 */
export async function loadSite(
  contentCid: string,
  getBlock: GetBlock,
  limits: ArchiveLimits = ARCHIVE_LIMITS,
  signal?: AbortSignal,
  entry = "index.html",
): Promise<LoadedArchive> {
  const innerRoot = parseCidString(contentCid);
  const files = await unpackDirectory(
    innerRoot,
    getBlock,
    limits,
    deadline(limits, signal),
  );
  if (!files.has(entry)) throw new Error(`the site has no ${entry}`);
  return { files, innerRoot };
}

/** A signal that fires at the load's deadline, or when the caller's does. */
function deadline(limits: ArchiveLimits, signal?: AbortSignal): AbortSignal {
  const expired = AbortSignal.timeout(limits.deadlineMs);
  return signal === undefined ? expired : AbortSignal.any([signal, expired]);
}
