import { blake2b } from "@noble/hashes/blake2.js";
import { keccak_256 } from "@noble/hashes/sha3.js";
import {
  DEFAULT_NETWORK_CONFIG,
  NETWORKS,
  type DotnsEndpoints,
  type NetworkConfig,
} from "./network-config.js";

/**
 * The DotNS endpoints of Paseo Next v2, the default network.
 *
 * @deprecated Pass a network's config to the function that needs it instead
 * (`NetworkConfig.dotns`); this alias remains for the callers not yet
 * migrated. Every value is defined in `network-config.ts`, which names the
 * canonical source it was verified against. A wrong TLD or base node still
 * namehashes, so a mistake there reads as "name not found" and does not fail
 * loudly.
 */
export const PASEO_DOTNS = {
  /** The one dotNS TLD of the network this host is connected to. */
  tld: NETWORKS.paseo.networkSuffix,
  ...NETWORKS.paseo.dotns,
};

/** A JSON-RPC call whose result is a storage value or nothing. */
export type RpcCall = (method: string, params: string[]) => Promise<unknown>;

/**
 * `twox128` of the pallet and storage item, the prefix of
 * `Revive::AccountInfoOf`. Both names are fixed, so these are constants and
 * no xxhash is bundled.
 */
const REVIVE_PREFIX = hexToBytes("735f040a5d490f1107ad9c56f5ca00d2");
const ACCOUNT_INFO_OF_PREFIX = hexToBytes("ae37ff0591fdbbcd9c2406df7147a9dc");

/** pallet-revive addresses child storage by this prefix plus the trie id. */
const CHILD_STORAGE_PREFIX = new TextEncoder().encode(
  ":child_storage:default:",
);

/** A contenthash is tens of bytes; anything larger is not one. */
const MAX_CONTENTHASH_BYTES = 1024;

/** `0xe3 0x01`: EIP-1577 contenthash of an IPFS content identifier. */
const IPFS_CONTENTHASH_TAG = [0xe3, 0x01];

/** An IPFS CID version 1 starts with this byte. */
const CID_VERSION_1 = 0x01;

/** A CIDv1 with a 32-byte hash is 36 bytes; leave room for larger digests. */
const MAX_CID_BYTES = 64;

const BASE32_ALPHABET = "abcdefghijklmnopqrstuvwxyz234567";

export function hexToBytes(text: string): Uint8Array {
  const clean = text.replace(/^0x/, "");
  if (clean.length % 2 !== 0 || /[^0-9a-f]/i.test(clean))
    throw new Error("not hexadecimal");
  return Uint8Array.from(clean.match(/../g) ?? [], (pair) =>
    Number.parseInt(pair, 16),
  );
}

export function bytesToHex(bytes: Uint8Array): string {
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join(
    "",
  );
}

function concat(...parts: Uint8Array[]): Uint8Array {
  const out = new Uint8Array(parts.reduce((sum, part) => sum + part.length, 0));
  let offset = 0;
  for (const part of parts) {
    out.set(part, offset);
    offset += part.length;
  }
  return out;
}

/** ENS `namehash`: fold the labels right to left, hashing the running node. */
export function namehash(name: string): Uint8Array {
  let node = new Uint8Array(32);
  if (name === "") return node;
  for (const label of name.split(".").reverse())
    node = keccak_256(
      concat(node, keccak_256(new TextEncoder().encode(label))),
    );
  return node;
}

/** Solidity `mapping(bytes32 => ...)` slot: `keccak256(key ++ uint256(slot))`. */
function mappingSlot(key: Uint8Array, slot: number): Uint8Array {
  const word = new Uint8Array(32);
  new DataView(word.buffer).setUint32(28, slot);
  return keccak_256(concat(key, word));
}

/** RFC 4648 base32, lowercase, unpadded: the multibase `b` alphabet. */
function base32(bytes: Uint8Array): string {
  let out = "";
  let buffer = 0;
  let bits = 0;
  for (const byte of bytes) {
    buffer = (buffer << 8) | byte;
    bits += 8;
    while (bits >= 5) {
      out += BASE32_ALPHABET[(buffer >>> (bits - 5)) & 31];
      bits -= 5;
    }
  }
  if (bits > 0) out += BASE32_ALPHABET[(buffer << (5 - bits)) & 31];
  return out;
}

/**
 * The multibase-`b` CID string inside an EIP-1577 IPFS contenthash, or null
 * when the record is anything else.
 *
 * The string is built here from the record's bytes and is only ever
 * `[a-z2-7]`, so a hostile record cannot smuggle path or query characters into
 * the gateway URL made from it.
 */
export function contenthashToCid(record: Uint8Array): string | null {
  const [first, second] = IPFS_CONTENTHASH_TAG;
  if (record.length < 3 || record[0] !== first || record[1] !== second)
    return null;
  const cid = record.subarray(2);
  if (cid[0] !== CID_VERSION_1 || cid.length > MAX_CID_BYTES) return null;
  return `b${base32(cid)}`;
}

/** SCALE compact integer at `offset`: `[value, bytes used]`, or null. */
function readCompact(
  bytes: Uint8Array,
  offset: number,
): [number, number] | null {
  const first = bytes[offset];
  if (first === undefined) return null;
  const mode = first & 0b11;
  if (mode === 0) return [first >> 2, 1];
  if (mode === 1 && offset + 1 < bytes.length)
    return [(first | (bytes[offset + 1] << 8)) >> 2, 2];
  return null;
}

function bigEndian(bytes: Uint8Array): number {
  return bytes.reduce((value, byte) => value * 256 + byte, 0);
}

function increment(word: Uint8Array): Uint8Array {
  const out = Uint8Array.from(word);
  for (let i = out.length - 1; i >= 0; i -= 1) {
    if (out[i] === 0xff) out[i] = 0;
    else {
      out[i] += 1;
      break;
    }
  }
  return out;
}

async function storageHex(
  rpc: RpcCall,
  method: string,
  params: string[],
): Promise<string | null> {
  const result = await rpc(method, params);
  return typeof result === "string" ? result : null;
}

/**
 * `Revive::AccountInfoOf[resolver]` gives the contract's child trie id. The
 * value is a SCALE enum whose `Contract` variant (tag 0) starts with
 * `trie_id: Vec<u8>`; only that prefix is read.
 */
async function resolverTrieId(
  rpc: RpcCall,
  contentResolver: string,
): Promise<Uint8Array | null> {
  const key = concat(
    REVIVE_PREFIX,
    ACCOUNT_INFO_OF_PREFIX,
    hexToBytes(contentResolver),
  );
  const hex = await storageHex(rpc, "state_getStorage", [
    `0x${bytesToHex(key)}`,
  ]);
  if (hex === null) return null;
  const bytes = hexToBytes(hex);
  if (bytes[0] !== 0x00) return null;
  const compact = readCompact(bytes, 1);
  if (compact === null) return null;
  const [length, used] = compact;
  const start = 1 + used;
  return start + length <= bytes.length
    ? bytes.subarray(start, start + length)
    : null;
}

/** One 32-byte word of the contract's child trie, addressed as pallet-revive does. */
async function childWord(
  rpc: RpcCall,
  childKey: Uint8Array,
  slotKey: Uint8Array,
): Promise<Uint8Array | null> {
  const hex = await storageHex(rpc, "childstate_getStorage", [
    `0x${bytesToHex(childKey)}`,
    `0x${bytesToHex(blake2b(slotKey, { dkLen: 32 }))}`,
  ]);
  if (hex === null) return null;
  const word = hexToBytes(hex);
  return word.length === 32 ? word : null;
}

/**
 * A Solidity `bytes` value in storage. Short values sit in the slot with
 * `length * 2` in the last byte. Long ones put `length * 2 + 1` there and keep
 * the data from `keccak256(slotKey)` on.
 */
async function readSolidityBytes(
  rpc: RpcCall,
  childKey: Uint8Array,
  slotKey: Uint8Array,
): Promise<Uint8Array | null> {
  const slot = await childWord(rpc, childKey, slotKey);
  if (slot === null) return null;
  const marker = slot[31];
  if ((marker & 1) === 0) {
    const length = marker >> 1;
    return length === 0 ? null : slot.subarray(0, length);
  }
  const length = (bigEndian(slot) - 1) / 2;
  if (length <= 0 || length > MAX_CONTENTHASH_BYTES) return null;

  const out = new Uint8Array(length);
  let base: Uint8Array = keccak_256(slotKey);
  for (let written = 0; written < length; written += 32) {
    const word = await childWord(rpc, childKey, base);
    if (word === null) return null;
    out.set(word.subarray(0, Math.min(32, length - written)), written);
    base = increment(base);
  }
  return out;
}

/**
 * The content CID a DotNS name's `contenthash` record points at, read from
 * Asset Hub through `rpc`. Null when the name has no such record: an
 * unregistered name and a registered one with no site look the same here.
 */
export async function resolveContentCid(
  name: string,
  rpc: RpcCall,
  dotns: DotnsEndpoints = DEFAULT_NETWORK_CONFIG.dotns,
): Promise<string | null> {
  const trieId = await resolverTrieId(rpc, dotns.contentResolver);
  if (trieId === null) return null;
  const slotKey = mappingSlot(namehash(name), dotns.contenthashSlot);
  const record = await readSolidityBytes(
    rpc,
    concat(CHILD_STORAGE_PREFIX, trieId),
    slotKey,
  );
  return record === null ? null : contenthashToCid(record);
}

/**
 * The URL a resolved site is opened from: the gateway's path form, which
 * serves a UnixFS directory as a site with its relative asset paths intact.
 * `suffix` is the path, query and hash the user typed after the name.
 */
export function gatewayUrl(
  cid: string,
  suffix: string,
  contentGateway: string = DEFAULT_NETWORK_CONFIG.dotns.contentGateway,
): URL {
  if (!/^b[a-z2-7]+$/.test(cid)) throw new Error("not a base32 CIDv1");
  return new URL(`/ipfs/${cid}/${suffix.replace(/^\//, "")}`, contentGateway);
}

const RPC_TIMEOUT_MS = 15_000;

/**
 * A JSON-RPC client over HTTP POST for one-shot reads, given the endpoint's
 * `ws(s)` URL as dotkit lists it.
 *
 * Resolution needs no subscription, and a WebSocket is the wrong transport
 * for it: browsers let one WebSocket handshake per IP address be in flight,
 * and the light client dials other hosts behind the same Cloudflare addresses,
 * so a WebSocket opened here can wait behind them with no error until the
 * open timeout fires.
 */
export function httpRpc(
  url: string,
  fetchFn: typeof fetch = fetch,
  timeoutMs = RPC_TIMEOUT_MS,
): RpcCall {
  const endpoint = url.replace(/^ws/, "http");
  let nextId = 0;
  return async (method, params) => {
    let response: Response;
    try {
      response = await fetchFn(endpoint, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          jsonrpc: "2.0",
          id: (nextId += 1),
          method,
          params,
        }),
        signal: AbortSignal.timeout(timeoutMs),
      });
    } catch (error) {
      throw new Error(
        error instanceof DOMException && error.name === "TimeoutError"
          ? `no answer from ${endpoint} for ${method} after ${timeoutMs / 1000}s`
          : `cannot reach ${endpoint} for ${method}`,
      );
    }
    if (!response.ok)
      throw new Error(`${endpoint} answered ${response.status} for ${method}`);
    const reply = (await response.json()) as {
      result?: unknown;
      error?: unknown;
    };
    if (reply.error !== undefined) throw new Error(JSON.stringify(reply.error));
    return reply.result;
  };
}

/** What a CID turned out to hold when fetched from the gateway. */
export type ContentKind =
  /** A page or a directory with an `index.html`: the gateway serves it as a site. */
  | "site"
  /** A CAR archive: the product's files, which the gateway cannot serve as a site. */
  | "car"
  /** Anything else. */
  | "other";

/**
 * Whether `bytes` start a CARv1 archive: a varint header length, then a CBOR
 * map holding `roots` and `version`.
 */
export function looksLikeCar(bytes: Uint8Array): boolean {
  let at = 0;
  while (at < 4 && (bytes[at] & 0x80) !== 0) at += 1;
  at += 1;
  const rest = bytes.subarray(at, at + 9);
  const text = (from: number, to: number) =>
    new TextDecoder().decode(rest.subarray(from, to));
  if (rest[0] !== 0xa2) return false;
  return (
    (rest[1] === 0x65 && text(2, 7) === "roots") ||
    (rest[1] === 0x67 && text(2, 9) === "version")
  );
}

const CLASSIFY_TIMEOUT_MS = 20_000;
/** Enough for the longest CARv1 header prefix `looksLikeCar` reads. */
const CLASSIFY_PREFIX_BYTES = 16;

/**
 * The first `maxBytes` of a body, gathered across as many stream chunks as it
 * takes, then the rest is cancelled. A gateway that ignores `Range` cannot
 * make this read more.
 */
async function readPrefix(
  response: Response,
  maxBytes: number,
): Promise<Uint8Array> {
  const reader = response.body?.getReader();
  if (reader === undefined) return new Uint8Array(0);
  const prefix = new Uint8Array(maxBytes);
  let length = 0;
  try {
    while (length < maxBytes) {
      const { done, value } = await reader.read();
      if (done) break;
      const taken = Math.min(value.length, maxBytes - length);
      prefix.set(value.subarray(0, taken), length);
      length += taken;
    }
  } finally {
    await reader.cancel().catch(() => undefined);
  }
  return prefix.subarray(0, length);
}

/**
 * Fetch the start of `cid` from the gateway and say what it is. The body is
 * read only far enough to recognise a CAR archive and then dropped.
 */
export async function classifyContent(
  cid: string,
  fetchFn: typeof fetch = fetch,
  contentGateway: string = DEFAULT_NETWORK_CONFIG.dotns.contentGateway,
): Promise<ContentKind> {
  const response = await fetchFn(gatewayUrl(cid, "", contentGateway).href, {
    headers: { Range: `bytes=0-${CLASSIFY_PREFIX_BYTES - 1}` },
    signal: AbortSignal.timeout(CLASSIFY_TIMEOUT_MS),
  });
  if (!response.ok)
    throw new Error(`the gateway answered ${response.status} for ${cid}`);
  const type = response.headers.get("content-type") ?? "";
  if (type.startsWith("text/html")) {
    await response.body?.cancel();
    return "site";
  }
  return looksLikeCar(await readPrefix(response, CLASSIFY_PREFIX_BYTES))
    ? "car"
    : "other";
}

/** A published record that could not be opened, and why. */
export interface SkippedRecord {
  record: string;
  cid: string;
  kind: "other";
}

/** Where a name's product was found. */
export interface Source {
  /** The DotNS record whose content is opened, such as `app.myapp.paseo`. */
  record: string;
  cid: string;
  /**
   * `car` is an executable: an archive of the product's files, bound as one
   * file. `site` is a directory the gateway can serve. The host unpacks both
   * itself, on the product's own origin.
   */
  kind: Exclude<ContentKind, "other">;
}

/** The outcome of looking a name up. */
export interface SourceChoice {
  source: Source | null;
  /** Records that exist but hold neither a site nor an archive, in the order they were tried. */
  skipped: SkippedRecord[];
}

/** What choosing a source reads from the outside, so tests can stand in for it. */
export interface SourceReaders {
  /** The content CID a record points at, or null. */
  readCid(record: string): Promise<string | null>;
  classify(cid: string): Promise<ContentKind>;
}

/**
 * Pick the record to open for `name`.
 *
 * A product's app executable is published at `app.<name>` and its plain
 * website at `<name>`. The app executable is tried first, which is the order
 * the Polkadot browser uses, and the website is the fallback. A record that is
 * neither a site nor an archive is skipped and reported.
 */
export async function chooseSource(
  name: string,
  readers: SourceReaders,
): Promise<SourceChoice> {
  const skipped: SkippedRecord[] = [];
  for (const record of [`app.${name}`, name]) {
    const cid = await readers.readCid(record);
    if (cid === null) continue;
    const kind = await readers.classify(cid);
    if (kind === "other") skipped.push({ record, cid, kind });
    else return { source: { record, cid, kind }, skipped };
  }
  return { source: null, skipped };
}

/** Look `name` up on `network`'s Asset Hub and pick what to open. */
export async function resolveSource(
  name: string,
  network: NetworkConfig = DEFAULT_NETWORK_CONFIG,
): Promise<SourceChoice> {
  const rpc = httpRpc(network.dotns.assetHubRpc);
  return chooseSource(name, {
    readCid: (record) => resolveContentCid(record, rpc, network.dotns),
    classify: (cid) =>
      classifyContent(cid, fetch, network.dotns.contentGateway),
  });
}
