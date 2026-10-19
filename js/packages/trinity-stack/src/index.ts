// Trinity stack descriptors, as docs/rfcs/trinity-stack-descriptor.md defines them. Every rule a
// reader applies runs in the trinity-stack crate; this module adds the operator side and I/O.
import type {
  Accepted,
  Descriptor,
  ParsedDocument,
  Pointer,
  PublishState,
  SpecFile,
  Verified,
} from "../wasm/trinity_stack.js";
import { wasm } from "./wasm.js";

// Generated from the crate's types by the wasm build.
export type {
  Accepted,
  Asset,
  Block,
  Chain,
  Coinage,
  Descriptor,
  Fork,
  ForkSource,
  ParsedDocument,
  Pointer,
  PublishState,
  Service,
  SpecFile,
  Status,
  Verified,
} from "../wasm/trinity_stack.js";

/** An error from a rule of the RFC. `code` names the rule, as the conformance vectors do. */
export type TrinityStackError = Error & { code: string };

/** Whether `error` came from a rule of the RFC. */
export function isTrinityStackError(
  error: unknown,
): error is TrinityStackError {
  return error instanceof Error && error.name === "TrinityStackError";
}

/** Parses a document as served, without checking any key. */
export function parse(bytes: Uint8Array): ParsedDocument {
  return wasm.parse(bytes);
}

/** Parses a document and applies the pointer's rules. */
export function open(bytes: Uint8Array, pointer: Pointer): Verified {
  return wasm.open(bytes, pointer);
}

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
  return wasm.accept(pointer, stored, fetched, unixSeconds(now));
}

/** The pointer a link carries, whatever its scheme or domain. */
export function decodeLink(link: string): Pointer {
  return wasm.decodeLink(link);
}

/** The link that carries a pointer, under `base`, which defaults to `polkadotapp://stack`. */
export function encodeLink(pointer: Pointer, base?: string): string {
  return wasm.encodeLink(pointer, base);
}

/** Whether two pointers name the same stack: the same key, or the same URL when unsigned. */
export function sameStack(a: Pointer, b: Pointer): boolean {
  return wasm.sameStack(a, b);
}

/** What identifies a pointer's stack: its key, or its normalised URL when it has none. */
export function stackId(pointer: Pointer): string {
  return wasm.stackId(pointer);
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
  wasm.checkSpec(spec, bytes);
}

/** The genesis hash a light spec implies, to compare with its chain's `genesisHash`. */
export function lightSpecGenesis(bytes: Uint8Array): string {
  return wasm.lightSpecGenesis(bytes);
}

/** What an operator describes: a descriptor without the fields `publish` owns. */
export type Model = Omit<Descriptor, "$v" | "sequence" | "generation">;

/** What `publish` returns: the bytes to serve, the descriptor they carry, and the state to keep. */
export type Published = {
  document: Uint8Array;
  descriptor: Descriptor;
  state: PublishState;
};

/**
 * Renders, checks and optionally signs a model, with the crate's `render`. `sequence` stays the
 * same when nothing changed and otherwise rises to the larger of the last one plus one and the
 * time in seconds, so an operator with no stored state still never goes backwards.
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
  const { plain, state } = wasm.render(
    new TextEncoder().encode(JSON.stringify(model)),
    options.generation,
    options.state,
    unixSeconds(options.now ?? new Date()),
  );
  const document = options.secret ? sign(plain, options.secret) : plain;
  return { document, descriptor: parse(plain).descriptor, state };
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
  wasm.stackId({ url });
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
    const bytes = response.ok ? await readCapped(response) : undefined;
    if (!bytes) controller.abort();
    return bytes;
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

// Reads through `getReader`: Safari before 27 cannot iterate a `ReadableStream` with `for await`.
async function readCapped(response: Response): Promise<Uint8Array | undefined> {
  if (Number(response.headers.get("content-length")) > MAX_DESCRIPTOR_BYTES)
    return undefined;
  if (!response.body) return new Uint8Array(await response.arrayBuffer());
  const reader = response.body.getReader();
  const chunks: Uint8Array[] = [];
  let length = 0;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    length += value.length;
    if (length > MAX_DESCRIPTOR_BYTES) {
      await reader.cancel();
      return undefined;
    }
    chunks.push(value);
  }
  const bytes = new Uint8Array(length);
  let offset = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, offset);
    offset += chunk.length;
  }
  return bytes;
}

/** Seconds since the epoch; an invalid `Date` would otherwise turn off staleness and expiry. */
function unixSeconds(date: Date): number {
  const milliseconds = date.getTime();
  if (Number.isNaN(milliseconds))
    throw new RangeError("now is an invalid Date");
  return Math.floor(milliseconds / 1000);
}
