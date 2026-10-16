// Trinity stack descriptors, as docs/rfcs/trinity-stack-descriptor.md defines them. Every rule a
// reader applies runs in the trinity-stack crate; this module adds the operator side and I/O.
import { wasm } from "./wasm.js";

/** A `$v: 1` descriptor. */
export type Descriptor = {
  $v: 1;
  sequence: number;
  generation: string;
  id: string;
  name: string;
  network: string;
  tld: string;
  /** Unix seconds. */
  expiresAt?: number;
  /** Unix seconds. */
  validUntil?: number;
  fork?: Fork;
  chains: Chain[];
  services: Service[];
  assets?: Asset[];
  contracts?: Record<string, string>;
  coinage?: { chain: string; instance: number };
};

export type Chain = {
  role: string;
  genesisHash: string;
  paraId?: number;
  rpc: string[];
  statementStore?: boolean;
  lightSpec?: SpecFile;
  fullSpec?: SpecFile;
  forkedAt?: { number: number; hash: string };
};

export type SpecFile = { url: string; sha256: string };

export type Service = { kind: string; urls: string[]; chainId?: number };

export type Asset = { role: string; chain: string; pallet: string; id: number };

/** `at` is Unix seconds. */
export type Fork = { of: Pointer; at: number };

/** Where a descriptor lives, and the key that signs it when it is signed. */
export type Pointer = { url: string; key?: string };

/** The link base the Polkadot app opens. */
export const DEFAULT_LINK_BASE = "polkadotapp://stack";

/** An error from a rule of the RFC. `code` names the rule, as the conformance vectors do. */
export type TrinityStackError = Error & { code: string };

/** Whether `error` came from a rule of the RFC. */
export function isTrinityStackError(
  error: unknown,
): error is TrinityStackError {
  return error instanceof Error && error.name === "TrinityStackError";
}

/** A descriptor that passed a pointer's rules, or was parsed without one. */
export type Opened = { descriptor: Descriptor; signed: boolean };

/** Parses a document as served, without checking any key. `signed` says it is an envelope. */
export function parse(bytes: Uint8Array): Opened {
  return JSON.parse(wasm.parse(bytes));
}

/** Parses a document and applies the pointer's rules. */
export function open(bytes: Uint8Array, pointer: Pointer): Opened {
  return JSON.parse(wasm.open(bytes, pointer.url, pointer.key));
}

export type Status =
  | "updated"
  | "unchanged"
  | "conflict"
  | "rollback"
  | "unreachable"
  | "refused";

/** The descriptor a reader uses next, and what changed. */
export type Accepted = Opened & {
  status: Status;
  /** The rule the fetched bytes broke, when `status` is `refused`. */
  refused?: string;
  /** The bytes to store and pass back as `stored` next time. */
  document: Uint8Array;
  /** The chains restarted: drop cached chain data. */
  newGeneration: boolean;
  /** Past `validUntil`: usable, but show it as stale. */
  stale: boolean;
  /** Past `expiresAt`: the stack is gone. */
  expired: boolean;
};

/**
 * Decides what a reader does with a fetched descriptor, given the copy it stored last.
 * `fetched` is undefined when the fetch failed. Throws only when nothing usable is left.
 */
export function accept(
  pointer: Pointer,
  stored: Uint8Array | undefined,
  fetched: Uint8Array | undefined,
  now: Date = new Date(),
): Accepted {
  const result = wasm.accept(
    pointer.url,
    pointer.key,
    stored,
    fetched,
    unixSeconds(now),
  );
  try {
    return {
      status: result.status as Status,
      refused: result.refused ?? undefined,
      document: result.document,
      descriptor: JSON.parse(result.descriptor),
      signed: result.signed,
      newGeneration: result.newGeneration,
      stale: result.stale,
      expired: result.expired,
    };
  } finally {
    result.free();
  }
}

/** The pointer a link carries, whatever its scheme or domain. */
export function decodeLink(link: string): Pointer {
  return JSON.parse(wasm.decodeLink(link));
}

/** The link that carries a pointer. */
export function encodeLink(
  pointer: Pointer,
  base: string = DEFAULT_LINK_BASE,
): string {
  return wasm.encodeLink(pointer.url, pointer.key, base);
}

/** Whether two pointers name the same stack: the same key, or the same URL when unsigned. */
export function sameStack(a: Pointer, b: Pointer): boolean {
  return wasm.sameStack(a.url, a.key, b.url, b.key);
}

/** What identifies a pointer's stack: its key, or its normalised URL when it has none. */
export function stackId(pointer: Pointer): string {
  return wasm.stackId(pointer.url, pointer.key);
}

/** Wraps a plain descriptor's bytes in a signed envelope. */
export function sign(descriptor: Uint8Array, secret: Uint8Array): Uint8Array {
  return wasm.sign(descriptor, secret);
}

/** The 0x-prefixed public key of a 32-byte secret key. */
export function publicKey(secret: Uint8Array): string {
  return wasm.publicKey(secret);
}

/** A new random 32-byte secret key. */
export function generateSecret(): Uint8Array {
  return crypto.getRandomValues(new Uint8Array(32));
}

/** Checks a spec's bytes, as served, against its `sha256`. */
export function checkSpec(spec: SpecFile, bytes: Uint8Array): void {
  wasm.checkSpec(spec.url, spec.sha256, bytes);
}

/** The genesis hash a light spec implies, to compare with its chain's `genesisHash`. */
export function lightSpecGenesis(bytes: Uint8Array): string {
  return wasm.lightSpecGenesis(bytes);
}

/** A QR code as an SVG document. */
export function qrSvg(text: string): string {
  return wasm.qrSvg(text);
}

/** A QR code drawn with Unicode blocks, for a terminal. */
export function qrTerminal(text: string): string {
  return wasm.qrTerminal(text);
}

/** What an operator describes: a descriptor without the fields `publish` owns. */
export type Model = Omit<Descriptor, "$v" | "sequence" | "generation">;

/** Renders a model as a given format version. */
export function render(
  model: Model,
  version: 1,
  fields: { sequence: number; generation: string },
): Descriptor {
  // A model read from JSON can still carry the fields `publish` owns; they never win.
  const {
    $v: _version,
    sequence: _sequence,
    generation: _generation,
    ...rest
  } = model as Model & Partial<Descriptor>;
  return {
    $v: version,
    sequence: fields.sequence,
    generation: fields.generation,
    ...rest,
  };
}

/** What `publish` remembers between runs. Keep it outside anything a wipe deletes. */
export type PublishState = { sequence: number; contentHash: string };

export type Published = {
  document: Uint8Array;
  descriptor: Descriptor;
  state: PublishState;
};

/**
 * Renders, checks and optionally signs a model. `sequence` stays the same when nothing changed
 * and otherwise rises to the larger of the last one plus one and the time in seconds, so an
 * operator with no stored state still never goes backwards.
 */
export async function publish(
  model: Model,
  options: {
    generation: string;
    secret?: Uint8Array;
    state?: PublishState;
    now?: Date;
  },
): Promise<Published> {
  // The crate hashes and renders the canonical form, unknown members included: equal hashes
  // mean equal published bytes, whatever key order the model has, and `truapi-host stack
  // publish` keeps the same state.
  const contentHash = wasm.contentHash(
    encode(render(model, 1, { sequence: 0, generation: options.generation })),
  );
  const seconds = unixSeconds(options.now ?? new Date());
  const sequence =
    options.state?.contentHash === contentHash
      ? options.state.sequence
      : Math.max(options.state ? options.state.sequence + 1 : 0, seconds);
  const descriptor = render(model, 1, {
    sequence,
    generation: options.generation,
  });
  const plain = wasm.canonical(encode(descriptor));
  const document = options.secret ? sign(plain, options.secret) : plain;
  return { document, descriptor, state: { sequence, contentHash } };
}

/** Where a reader keeps the last accepted document of each stack. */
export interface Store {
  get(id: string): Uint8Array | undefined | Promise<Uint8Array | undefined>;
  set(id: string, document: Uint8Array): void | Promise<void>;
}

/** A store that lives as long as the process. */
export function memoryStore(): Store {
  const documents = new Map<string, Uint8Array>();
  return {
    get: (id) => documents.get(id),
    set: (id, document) => void documents.set(id, document),
  };
}

/**
 * Fetches a pointer's descriptor and runs `accept` against the stored copy. Without a store
 * there is no rollback protection, so a short-lived reader should rely on `validUntil`.
 */
export async function read(
  pointer: Pointer,
  options: {
    store?: Store;
    fetch?: Fetch;
    now?: Date;
    timeoutMs?: number;
  } = {},
): Promise<Accepted> {
  const id = stackId(pointer);
  const [stored, fetched] = await Promise.all([
    options.store?.get(id),
    fetchDescriptor(pointer.url, options),
  ]);
  const accepted = accept(pointer, stored, fetched, options.now);
  // Every other status hands back the stored bytes unchanged.
  if (accepted.status === "updated")
    await options.store?.set(id, accepted.document);
  return accepted;
}

/** The part of `fetch` that `read` uses. */
export type Fetch = (url: string, init?: RequestInit) => Promise<Response>;

/** How long `read` waits for a descriptor, body included, before treating it as unreachable. */
export const DEFAULT_TIMEOUT_MS = 10_000;

/**
 * The bytes a descriptor URL serves, or undefined when it is unreachable: a failed request, a
 * redirect, a non-2xx status or the timeout. Redirects are refused because the URL rules hold
 * for the URL the pointer names, and a redirect could lead anywhere, plain http included.
 *
 * A body over `MAX_DESCRIPTOR_BYTES` is unreachable too, so a hostile URL cannot stream into
 * memory until the timeout.
 *
 * The global fetch reports network failures differently across runtimes (a `TypeError` in Node
 * and current Bun, a plain `Error` in older Bun), so any error from it reads as unreachable. A
 * custom `fetch` reads as unreachable only on a `TypeError` or an abort; anything else is its
 * bug and is rethrown. A `url` the RFC's URL rules refuse is rethrown too.
 */
export async function fetchDescriptor(
  url: string,
  options: { fetch?: Fetch; timeoutMs?: number } = {},
): Promise<Uint8Array | undefined> {
  wasm.stackId(url, undefined);
  const controller = new AbortController();
  const timer = setTimeout(
    () => controller.abort(),
    options.timeoutMs ?? DEFAULT_TIMEOUT_MS,
  );
  try {
    const response = await (options.fetch ?? fetch)(url, {
      cache: "no-store",
      redirect: "error",
      signal: controller.signal,
    });
    return response.ok ? await readCapped(response, controller) : undefined;
  } catch (error) {
    const aborted =
      error instanceof DOMException &&
      (error.name === "AbortError" || error.name === "TimeoutError");
    if (!options.fetch || error instanceof TypeError || aborted)
      return undefined;
    throw error;
  } finally {
    clearTimeout(timer);
  }
}

/** The largest descriptor `fetchDescriptor` reads. A descriptor is a few kilobytes. */
export const MAX_DESCRIPTOR_BYTES = 1024 * 1024;

async function readCapped(
  response: Response,
  controller: AbortController,
): Promise<Uint8Array | undefined> {
  if (Number(response.headers.get("content-length")) > MAX_DESCRIPTOR_BYTES)
    return undefined;
  if (!response.body) return new Uint8Array(await response.arrayBuffer());
  const chunks: Uint8Array[] = [];
  let length = 0;
  for await (const chunk of response.body as AsyncIterable<Uint8Array>) {
    length += chunk.length;
    if (length > MAX_DESCRIPTOR_BYTES) {
      controller.abort();
      return undefined;
    }
    chunks.push(chunk);
  }
  const bytes = new Uint8Array(length);
  let offset = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, offset);
    offset += chunk.length;
  }
  return bytes;
}

function encode(descriptor: Descriptor): Uint8Array {
  return new TextEncoder().encode(JSON.stringify(descriptor, null, 2) + "\n");
}

/** Seconds since the epoch; an invalid `Date` would otherwise turn off staleness and expiry. */
function unixSeconds(date: Date): number {
  const milliseconds = date.getTime();
  if (Number.isNaN(milliseconds))
    throw new RangeError("now is an invalid Date");
  return Math.floor(milliseconds / 1000);
}
