import {
  classifyContent,
  httpRpc,
  resolveContentCid,
  resolveTextRecord,
  type ContentKind,
  type RpcCall,
} from "./dotns.js";
import type { NetworkConfig } from "./network-config.js";
import { archivePath } from "./sandbox/serve.js";

/**
 * A product's worker executable, as its `worker.<name>` subname publishes it:
 * the `executable` text record describes it, and the `contenthash` locates
 * its files (docs/rfcs/product-manifest.md).
 */
export interface WorkerManifest {
  appVersion: [number, number, number];
  /** The entry module, a path inside the executable directory. */
  entrypoint: string;
  /** Surfaces the worker serves; an omitted key is `false`. */
  includes: { chat: boolean; pocket: boolean; input: boolean };
  /**
   * The ids of the Pocket cards the worker draws, read only when it serves
   * Pocket. A malformed list costs the worker its cards, not its other
   * surfaces.
   */
  pocketCards: string[];
}

const MODULE = /\.m?js$/;

function semver(value: unknown): [number, number, number] {
  if (
    !Array.isArray(value) ||
    value.length !== 3 ||
    !value.every((part) => Number.isInteger(part) && part >= 0)
  )
    throw new Error("appVersion is not three whole numbers");
  return value as [number, number, number];
}

function flag(value: unknown, key: string): boolean {
  if (value === undefined) return false;
  if (typeof value !== "boolean")
    throw new Error(`includes.${key} is not true or false`);
  return value;
}

/**
 * Read a `worker.<name>` executable manifest. Throws with the reason when it
 * is not one: the wrong `$v` or `kind`, or an entry point that is not a plain
 * module path inside the directory.
 */
export function parseWorkerManifest(json: string): WorkerManifest {
  let raw: unknown;
  try {
    raw = JSON.parse(json);
  } catch {
    throw new Error("the executable record is not JSON");
  }
  if (typeof raw !== "object" || raw === null || Array.isArray(raw))
    throw new Error("the executable record is not an object");
  const record = raw as Record<string, unknown>;
  if (record.$v !== 1)
    throw new Error(`manifest version ${String(record.$v)} is not 1`);
  if (record.kind !== "worker")
    throw new Error(`the executable is a ${String(record.kind)}, not a worker`);
  const entrypoint = record.entrypoint;
  if (
    typeof entrypoint !== "string" ||
    archivePath(`/${entrypoint}`) !== entrypoint ||
    !MODULE.test(entrypoint)
  )
    throw new Error(
      "the entry point is not a .js module inside the executable",
    );
  const includes = record.includes ?? {};
  if (
    typeof includes !== "object" ||
    includes === null ||
    Array.isArray(includes)
  )
    throw new Error("includes is not an object");
  const surfaces = includes as Record<string, unknown>;
  const pocket = flag(surfaces.pocket, "pocket");
  return {
    appVersion: semver(record.appVersion),
    entrypoint,
    includes: {
      chat: flag(surfaces.chat, "chat"),
      pocket,
      input: flag(surfaces.input, "input"),
    },
    pocketCards: pocket ? pocketCards(record.pocket) : [],
  };
}

/**
 * The `pocket.cards` list. Any card without an id, a title or a preview path,
 * with a `faceShown` that is not true or false, or with an id used twice
 * drops the whole list, as the Pocket RFC says.
 */
function pocketCards(section: unknown): string[] {
  const cards = (section as { cards?: unknown } | null | undefined)?.cards;
  if (!Array.isArray(cards)) return [];
  const ids: string[] = [];
  for (const card of cards) {
    const { id, title, preview, faceShown } = (card ?? {}) as Record<
      string,
      unknown
    >;
    if (typeof id !== "string" || id === "") return [];
    if (typeof title !== "string" || title.trim() === "") return [];
    if (typeof preview !== "string" || preview.trim() === "") return [];
    if (faceShown !== undefined && typeof faceShown !== "boolean") return [];
    if (ids.includes(id)) return [];
    ids.push(id);
  }
  return ids;
}

/** A worker found on chain, ready to mount. */
export interface ResolvedWorker {
  record: string;
  cid: string;
  kind: Exclude<ContentKind, "other">;
  manifest: WorkerManifest;
}

/** What resolving reads from the outside, so tests can stand in for it. */
export interface WorkerReaders {
  readCid(record: string): Promise<string | null>;
  readText(record: string, key: string): Promise<string | null>;
  classify(cid: string): Promise<ContentKind>;
}

/**
 * Find `name`'s worker: `worker.<name>` must hold both an `executable`
 * manifest and content. Throws with what is missing.
 */
export async function findWorker(
  name: string,
  readers: WorkerReaders,
): Promise<ResolvedWorker> {
  const record = `worker.${name}`;
  const [json, cid] = await Promise.all([
    readers.readText(record, "executable"),
    readers.readCid(record),
  ]);
  if (json === null) throw new Error(`${record} has no executable manifest.`);
  let manifest: WorkerManifest;
  try {
    manifest = parseWorkerManifest(json);
  } catch (error) {
    throw new Error(
      `${record}'s manifest is not usable: ${error instanceof Error ? error.message : String(error)}.`,
    );
  }
  if (cid === null) throw new Error(`${record} has no content.`);
  const kind = await readers.classify(cid);
  if (kind === "other")
    throw new Error(
      `${record}'s content is neither an archive nor a directory.`,
    );
  return { record, cid, kind, manifest };
}

/** Find `name`'s worker on `network`'s Asset Hub and Bulletin gateway. */
export function resolveWorker(
  name: string,
  network: NetworkConfig,
  rpc: RpcCall = httpRpc(network.dotns.assetHubRpc),
): Promise<ResolvedWorker> {
  return findWorker(name, {
    readCid: (record) => resolveContentCid(record, rpc, network.dotns),
    readText: (record, key) =>
      resolveTextRecord(record, key, rpc, network.dotns),
    classify: (cid) =>
      classifyContent(cid, fetch, network.dotns.contentGateway),
  });
}
