/**
 * What the host learns about each product load, for the inspector's Products
 * view. Two sides report it:
 *
 * - The host page, when it opens a mount: the DotNS record that answered, the
 *   content id, what the content is, the gateway and how long the lookup
 *   took ({@link ProductLoadEvent} `opening`).
 * - The loader page (`loader/page.ts`), in the product's frame, once per
 *   load: a {@link ProductLoadSummary} of what it fetched, checked and
 *   stored. It travels as a window message to this origin only. The host
 *   attributes it to the frame that posted it, never to a name inside it,
 *   and keeps the first one per frame, which the loader sends before it
 *   hands the frame to the product.
 *
 * Everything here is metadata: counts, bytes, milliseconds, a transport and
 * a short reason. No block bytes, keys or product data.
 */
import type {
  CarCounts,
  CarEnd,
  CarSkip,
  RawCounts,
} from "./archive/gateway.js";
import type { GetBlock } from "./archive/unixfs.js";
import type { Opened } from "./status.js";

/** The `type` of the loader's window message. */
export const PRODUCT_LOAD = "truapi-sandbox-load";

/** The verified files of a load: how many, their bytes, and the largest. */
export interface FileStats {
  count: number;
  bytes: number;
  largest?: { path: string; bytes: number };
}

/** What one load did, as the loader counted it. */
export interface ProductLoadSummary {
  /**
   * `fetched`: read from the gateway, checked and stored. `cached`: the
   * verified copy in Cache Storage was used and nothing was fetched.
   * `failed`: the product did not start.
   */
  outcome: "fetched" | "cached" | "failed";
  /** The loader's error, for a failed load. */
  reason?: string;
  /** From the loader's start until it hands the frame over or gives up. */
  totalMs: number;
  /** Starting the service worker that serves the mount. */
  workerMs?: number;
  /** Fetching and checking the content, inside the store's lock. */
  loadMs?: number;
  /**
   * For an archive (`kind` `car` in the host's `opening`): from the load's
   * start until the last block read was answered. Every block of the archive
   * file is read before the file is parsed, so the rest of `loadMs` is
   * parsing it and unpacking its files. A CAR stream may still be bringing
   * blocks no read asked for.
   */
  fetchMs?: number;
  /**
   * For a site, whose blocks are fetched and unpacked at once: the time at
   * least one block read was waiting.
   */
  waitMs?: number;
  /** Block reads the unpacking made. */
  reads?: number;
  /** Writing the files to Cache Storage. */
  storeMs?: number;
  files?: FileStats;
  raw?: RawCounts;
  car?: CarCounts;
  /** Why the load did not ask for a CAR stream, when it did not. */
  carSkipped?: CarSkip;
}

/** How the blocks of a load arrived. */
export type Transport = "car" | "car+raw" | "raw" | "cache" | "none";

/**
 * Where the host told the loader to read a load's blocks. `gateway`: the
 * network's IPFS gateway. `host`: this host's origin, for content the host
 * serves itself, such as Contact Chat's worker archive. `seed`: this host's
 * origin, for a name's content: blocks the host serves in place of the
 * gateway, such as a seed shipped with it. The loader checks each block against
 * its CID whatever the source; {@link Transport} says how they arrived.
 */
export type ContentSource = "gateway" | "host" | "seed";

/** The transport `summary`'s counts show. */
export function transportOf(summary: ProductLoadSummary): Transport {
  if (summary.outcome === "cached") return "cache";
  const fromCar = (summary.car?.fromCar ?? 0) > 0;
  const raw = (summary.raw?.requests ?? 0) > 0;
  if (fromCar) return raw ? "car+raw" : "car";
  return raw ? "raw" : "none";
}

/** The message the loader posts. */
export interface ProductLoadMessage {
  type: typeof PRODUCT_LOAD;
  summary: ProductLoadSummary;
}

/**
 * What the host reports, and what it hears from a frame's loader. Each load
 * has its own `load` id, so a report from an earlier frame of the same mount
 * never lands on a later load.
 */
export type ProductLoadEvent =
  /** The host opens `mount`: a new load there replaces the last one's facts. */
  | {
      type: "opening";
      load: number;
      mount: string;
      /** The name it was opened by, or the worker's name. */
      product: string;
      execution: "App" | "Widget" | "Worker";
      /** The DotNS record that answered, such as `app.<name>`. */
      record: string;
      cid: string;
      kind: "car" | "site";
      /** The origin the loader fetches from. */
      gateway: string;
      source: ContentSource;
      /** The DotNS lookup and content check, when this open made them. */
      resolveMs?: number;
    }
  /** The loader in `mount`'s frame reported its load. */
  | {
      type: "loaded";
      load: number;
      mount: string;
      summary: ProductLoadSummary;
    }
  /**
   * The load's frame closed, or, with a `reason`, never opened. A frame that
   * closes before its loader reports leaves no summary.
   */
  | { type: "closed"; load: number; mount: string; reason?: string };

export type ProductLoadObserver = (event: ProductLoadEvent) => void;

let lastLoad = 0;

/** A new load's id, unique in this page. */
export function nextLoadId(): number {
  return (lastLoad += 1);
}

/**
 * What opening `target` by name tells the observer, or null for a URL. The
 * gateway is the one in its loader URL, the one the loader reads blocks
 * from. A name whose loader reads from this host's `origin` is a `seed`.
 */
export function openingEvent(
  target: Opened & { url: URL; resolveMs?: number },
  execution: "App" | "Widget",
  origin: string,
): Extract<ProductLoadEvent, { type: "opening" }> | null {
  const { mount, record, cid, kind, resolveMs } = target;
  const gateway = target.url.searchParams.get("gateway");
  if (
    mount === undefined ||
    record === undefined ||
    cid === undefined ||
    kind === undefined ||
    gateway === null
  )
    return null;
  return {
    type: "opening",
    load: nextLoadId(),
    mount,
    product: target.address,
    execution,
    record,
    cid,
    kind,
    gateway,
    source: gateway === origin ? "seed" : "gateway",
    ...(resolveMs === undefined ? {} : { resolveMs }),
  };
}

/** Bounds on what the host keeps of one summary. */
export const LOAD_SUMMARY_LIMITS = { reason: 300, path: 200 } as const;

/** The count, bytes and largest of `files`. */
export function fileStats(files: Map<string, Uint8Array>): FileStats {
  let bytes = 0;
  let largest: FileStats["largest"];
  for (const [path, data] of files) {
    bytes += data.length;
    if (largest === undefined || data.length > largest.bytes)
      largest = { path, bytes: data.length };
  }
  return {
    count: files.size,
    bytes,
    ...(largest === undefined ? {} : { largest }),
  };
}

/** How long one load's block reads waited, as {@link timeReads} measures it. */
export interface ReadTiming {
  reads: number;
  /** Time at least one read was waiting. */
  waitMs: number;
  /** When the last read was answered, from the start given. */
  lastBlockMs?: number;
}

/** `getBlock`, measured into `timing` from `start` (a `performance.now()` time). */
export function timeReads(
  getBlock: GetBlock,
  start: number,
  timing: ReadTiming,
): GetBlock {
  let pending = 0;
  let since = 0;
  return async (cid, signal) => {
    timing.reads += 1;
    if (pending === 0) since = performance.now();
    pending += 1;
    try {
      return await getBlock(cid, signal);
    } finally {
      const at = performance.now();
      pending -= 1;
      if (pending === 0) timing.waitMs += at - since;
      timing.lastBlockMs = at - start;
    }
  };
}

/** The loader's end: post `summary` to the host page that embeds this frame. */
export function postLoadSummary(
  summary: ProductLoadSummary,
  parent: Window = window.parent,
  hostOrigin: string = location.origin,
): void {
  parent.postMessage(
    { type: PRODUCT_LOAD, summary } satisfies ProductLoadMessage,
    hostOrigin,
  );
}

const MAX_MS = 24 * 60 * 60 * 1000;
const MAX_COUNT = Number.MAX_SAFE_INTEGER;

function number(value: unknown, max = MAX_COUNT): number | undefined {
  return typeof value === "number" &&
    Number.isFinite(value) &&
    value >= 0 &&
    value <= max
    ? value
    : undefined;
}

function integer(value: unknown): number | undefined {
  const read = number(value);
  return read !== undefined && Number.isSafeInteger(read) ? read : undefined;
}

function text(value: unknown, max: number): string | undefined {
  return typeof value === "string" && value !== ""
    ? value.slice(0, max)
    : undefined;
}

function oneOf<T extends string>(
  value: unknown,
  choices: readonly T[],
): T | undefined {
  return choices.find((choice) => choice === value);
}

function record(value: unknown): Record<string, unknown> | undefined {
  return typeof value === "object" && value !== null
    ? (value as Record<string, unknown>)
    : undefined;
}

/** Copy the fields of `keys` that read as `read` does. */
function pick<K extends string>(
  source: Record<string, unknown>,
  keys: readonly K[],
  read: (value: unknown) => number | undefined,
): Partial<Record<K, number>> {
  const out: Partial<Record<K, number>> = {};
  for (const key of keys) {
    const value = read(source[key]);
    if (value !== undefined) out[key] = value;
  }
  return out;
}

const ms = (value: unknown) => number(value, MAX_MS);

function readFiles(value: unknown): FileStats | undefined {
  const files = record(value);
  if (files === undefined) return undefined;
  const count = integer(files.count);
  const bytes = integer(files.bytes);
  if (count === undefined || bytes === undefined) return undefined;
  const largest = record(files.largest);
  const path = text(largest?.path, LOAD_SUMMARY_LIMITS.path);
  const largestBytes = integer(largest?.bytes);
  return {
    count,
    bytes,
    ...(path === undefined || largestBytes === undefined
      ? {}
      : { largest: { path, bytes: largestBytes } }),
  };
}

function readRaw(value: unknown): RawCounts | undefined {
  const raw = record(value);
  if (raw === undefined) return undefined;
  const keys = [
    "requests",
    "blocks",
    "bytes",
    "retries",
    "cacheHits",
    "joined",
  ] as const;
  const counts = pick(raw, keys, integer);
  return keys.every((key) => counts[key] !== undefined)
    ? (counts as RawCounts)
    : undefined;
}

const CAR_ENDS: readonly CarEnd[] = [
  "complete",
  "slow",
  "refused",
  "failed",
  "cancelled",
];

function readCar(value: unknown): CarCounts | undefined {
  const car = record(value);
  if (car === undefined || typeof car.requested !== "boolean") return undefined;
  const counts = pick(
    car,
    ["bytes", "blocks", "fromCar", "fallback"] as const,
    integer,
  );
  if (Object.keys(counts).length !== 4) return undefined;
  const status = integer(car.status);
  const firstByteMs = ms(car.firstByteMs);
  const ran = ms(car.ms);
  const end = oneOf(car.end, CAR_ENDS);
  const reason = text(car.reason, LOAD_SUMMARY_LIMITS.reason);
  return {
    requested: car.requested,
    bytes: counts.bytes!,
    blocks: counts.blocks!,
    fromCar: counts.fromCar!,
    fallback: counts.fallback!,
    hedged: car.hedged === true,
    ...(status === undefined || status > 999 ? {} : { status }),
    ...(firstByteMs === undefined ? {} : { firstByteMs }),
    ...(ran === undefined ? {} : { ms: ran }),
    ...(end === undefined ? {} : { end }),
    ...(reason === undefined ? {} : { reason }),
  };
}

/**
 * The summary in a message from a loader frame, checked field by field and
 * bounded, or null when it is not one. A field that does not read is left
 * out; a summary without its outcome or total time is no summary.
 */
export function readLoadMessage(data: unknown): ProductLoadSummary | null {
  const message = record(data);
  if (message?.type !== PRODUCT_LOAD) return null;
  const summary = record(message.summary);
  if (summary === undefined) return null;
  const outcome = oneOf(summary.outcome, [
    "fetched",
    "cached",
    "failed",
  ] as const);
  const totalMs = ms(summary.totalMs);
  if (outcome === undefined || totalMs === undefined) return null;
  const reason = text(summary.reason, LOAD_SUMMARY_LIMITS.reason);
  const files = readFiles(summary.files);
  const raw = readRaw(summary.raw);
  const car = readCar(summary.car);
  const reads = integer(summary.reads);
  const carSkipped = oneOf(summary.carSkipped, ["off", "host-origin"] as const);
  return {
    outcome,
    totalMs,
    ...(reason === undefined ? {} : { reason }),
    ...pick(
      summary,
      ["workerMs", "loadMs", "fetchMs", "waitMs", "storeMs"] as const,
      ms,
    ),
    ...(reads === undefined ? {} : { reads }),
    ...(files === undefined ? {} : { files }),
    ...(raw === undefined ? {} : { raw }),
    ...(car === undefined ? {} : { car }),
    ...(carSkipped === undefined ? {} : { carSkipped }),
  };
}
