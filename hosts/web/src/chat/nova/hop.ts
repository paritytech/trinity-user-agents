import { bytesToHex, hexToBytes } from "@parity/truapi/scale";
import * as sr25519 from "@scure/sr25519";
import {
  Reader,
  bytes,
  concat,
  equalBytes,
  list,
  readBytes,
  readList,
  u64,
  utf8,
} from "./codec.js";
import {
  blake2b256,
  keyedBlake2b256,
  seal,
  sr25519Verify,
  unseal,
  type RandomBytes,
} from "./crypto.js";

/**
 * HOP, the encrypted handoff pool the Polkadot app moves chat attachments
 * through, written from the app's protocol (Android `feature/chats` `data/hop`,
 * iOS `Packages/HandoffService`, vendored under `hosts/`). No code is taken
 * from them.
 *
 * A sender picks a random 32-byte ticket. The ticket gives a ChaCha20-Poly1305
 * key and an sr25519 key. The file is sealed under the key, whole when small,
 * else in 2,000,000-byte chunks, and each sealed piece is submitted to a HOP
 * node over JSON-RPC, signed by the sender's Bulletin account and addressed to
 * the ticket's sr25519 key. A sealed root entry names the chunks. The chat
 * message carries the root entry's hash, the ticket and the node, so whoever
 * reads the message can claim the entries with signatures of the ticket key.
 * Every entry is addressed by the BLAKE2b-256 of its sealed bytes, and the
 * receiver checks that hash and the AEAD tag of every piece.
 */

/** Plaintext bytes in one chunk. */
export const HOP_CHUNK_BYTES = 2_000_000;
/** Files up to this size are sent whole, in the root entry. */
export const HOP_INLINE_MAX_BYTES = HOP_CHUNK_BYTES - 64;
/** The largest attachment the Android app sends: 128 MiB. */
export const HOP_MAX_FILE_BYTES = 128 * 1024 * 1024;

/** HOP's JSON-RPC error codes. */
export const HOP_ERRORS = {
  poolFull: 1002,
  notFound: 1004,
  rateLimited: 1020,
} as const;

/** What a chat message names to fetch a file: the root entry, the ticket, and the node. */
export interface HopReference {
  identifier: Uint8Array;
  ticket: Uint8Array;
  node: string;
}

/** The keys a ticket gives. The ticket is the secret; anyone holding it can claim and read. */
export function ticketKeys(ticket: Uint8Array): {
  encryption: Uint8Array;
  signingSecret: Uint8Array;
  signer: Uint8Array;
} {
  if (ticket.length !== 32) throw new Error("a HOP ticket is 32 bytes");
  const signingSecret = sr25519.secretFromSeed(
    keyedBlake2b256(ticket, utf8("signer")),
  );
  return {
    encryption: keyedBlake2b256(ticket, utf8("encryption")),
    signingSecret,
    signer: sr25519.getPublicKey(signingSecret),
  };
}

/** The root entry: `0x00` (version 1), then the file inline or the list of chunk hashes. */
export type HopRoot =
  | { kind: "inline"; data: Uint8Array }
  | { kind: "chunked"; totalSize: bigint; chunks: Uint8Array[] };

export function encodeRoot(root: HopRoot): Uint8Array {
  return root.kind === "inline"
    ? concat(Uint8Array.of(0, 0), bytes(root.data))
    : concat(
        Uint8Array.of(0, 1),
        u64(root.totalSize),
        list(root.chunks, bytes),
      );
}

export function decodeRoot(encoded: Uint8Array): HopRoot {
  const reader = new Reader(encoded);
  const version = reader.u8();
  if (version !== 0) throw new Error(`the HOP root is version tag ${version}`);
  const kind = reader.u8();
  let root: HopRoot;
  if (kind === 0) root = { kind: "inline", data: readBytes(reader) };
  else if (kind === 1)
    root = {
      kind: "chunked",
      totalSize: reader.u64(),
      chunks: readList(reader, (inner) => {
        const hash = readBytes(inner);
        if (hash.length !== 32) throw new Error("a chunk hash is 32 bytes");
        return hash;
      }),
    };
  else throw new Error(`the HOP root is payload tag ${kind}`);
  reader.finish();
  return root;
}

const context = (name: string, ...parts: Uint8Array[]) =>
  blake2b256(concat(utf8(name), ...parts));

/** What the sender signs to submit sealed `data`: `blake2b("hop-submit-v1:" || blake2b(data) || u64 ms)`. */
export const submitPayload = (dataHash: Uint8Array, timestampMs: bigint) =>
  context("hop-submit-v1:", dataHash, u64(timestampMs));
/** What the ticket key signs to claim an entry. */
export const claimPayload = (hash: Uint8Array) =>
  context("hop-claim-v1:", hash);
/** What the ticket key signs to acknowledge an entry, which removes it from the pool. */
export const ackPayload = (hash: Uint8Array) => context("hop-ack-v1:", hash);

/** SCALE `MultiSigner::Sr25519` and `MultiSignature::Sr25519`: index 1, then the raw bytes. */
export const sr25519Multi = (raw: Uint8Array): Uint8Array =>
  concat(Uint8Array.of(1), raw);

/** The raw key or signature of an `Sr25519` multi value, or null for another scheme. */
export function fromSr25519Multi(
  encoded: Uint8Array,
  length: number,
): Uint8Array | null {
  return encoded.length === length + 1 && encoded[0] === 1
    ? encoded.subarray(1)
    : null;
}

export class HopError extends Error {
  constructor(
    readonly code: number | null,
    message: string,
  ) {
    super(message);
  }
}

/** One JSON-RPC connection to a HOP node. */
export interface HopConnection {
  call(method: string, params: Record<string, unknown>): Promise<unknown>;
  close(): void;
}

/** The account that pays for a submission: it needs a Bulletin storage authorization. */
export interface HopSender {
  publicKey: Uint8Array;
  sign(payload: Uint8Array): Uint8Array;
}

export interface TransferOptions {
  signal?: AbortSignal;
  /** Called with the bytes moved so far and the total. */
  progress?(done: number, total: number): void;
}

const hex = (value: Uint8Array): string => bytesToHex(value);

const aborted = (signal: AbortSignal | undefined) => {
  if (signal?.aborted) throw new HopError(null, "The transfer was canceled.");
};

const RETRYABLE = new Set<number>([
  HOP_ERRORS.poolFull,
  HOP_ERRORS.rateLimited,
]);
const ATTEMPTS = 3;

async function submit(
  connection: HopConnection,
  sealed: Uint8Array,
  recipient: Uint8Array,
  sender: HopSender,
  now: () => number,
  signal: AbortSignal | undefined,
): Promise<Uint8Array> {
  const hash = blake2b256(sealed);
  for (let attempt = 1; ; attempt += 1) {
    aborted(signal);
    const timestamp = BigInt(now());
    try {
      await connection.call("hop_submit", {
        data: hex(sealed),
        recipients: [hex(sr25519Multi(recipient))],
        signature: hex(
          sr25519Multi(sender.sign(submitPayload(hash, timestamp))),
        ),
        signer: hex(sr25519Multi(sender.publicKey)),
        submit_timestamp: Number(timestamp),
      });
      return hash;
    } catch (error) {
      if (
        !(error instanceof HopError) ||
        error.code === null ||
        !RETRYABLE.has(error.code) ||
        attempt === ATTEMPTS
      )
        throw error;
      await new Promise((resolve) => setTimeout(resolve, 500 * 2 ** attempt));
    }
  }
}

/**
 * Seal and submit `file`. Resolves to the root entry's hash and the ticket,
 * which a chat message then carries. Nothing is acknowledged: the receiver
 * does that.
 */
export async function hopUpload(
  connection: HopConnection,
  file: Uint8Array,
  sender: HopSender,
  deps: { random: RandomBytes; now: () => number },
  options: TransferOptions = {},
): Promise<{ identifier: Uint8Array; ticket: Uint8Array }> {
  if (file.length > HOP_MAX_FILE_BYTES)
    throw new HopError(null, "The file is larger than 128 MiB.");
  const ticket = deps.random(32);
  const keys = ticketKeys(ticket);
  const put = (plaintext: Uint8Array) =>
    submit(
      connection,
      seal(keys.encryption, plaintext, deps.random),
      keys.signer,
      sender,
      deps.now,
      options.signal,
    );
  let root: HopRoot;
  if (file.length <= HOP_INLINE_MAX_BYTES)
    root = { kind: "inline", data: file };
  else {
    const chunks: Uint8Array[] = [];
    for (let start = 0; start < file.length; start += HOP_CHUNK_BYTES) {
      chunks.push(await put(file.subarray(start, start + HOP_CHUNK_BYTES)));
      options.progress?.(
        Math.min(start + HOP_CHUNK_BYTES, file.length),
        file.length,
      );
    }
    root = { kind: "chunked", totalSize: BigInt(file.length), chunks };
  }
  const identifier = await put(encodeRoot(root));
  options.progress?.(file.length, file.length);
  return { identifier, ticket };
}

/** Claim one entry and check it is the one its hash names. */
async function claim(
  connection: HopConnection,
  hash: Uint8Array,
  signingSecret: Uint8Array,
  signal: AbortSignal | undefined,
): Promise<Uint8Array> {
  aborted(signal);
  const answer = await connection.call("hop_claim", {
    raw_hash: hex(hash),
    signature: hex(
      sr25519Multi(sr25519.sign(signingSecret, claimPayload(hash))),
    ),
  });
  if (typeof answer !== "string")
    throw new HopError(null, "The HOP node answered a claim with no data.");
  const sealed = hexToBytes(answer);
  if (!equalBytes(blake2b256(sealed), hash))
    throw new HopError(
      null,
      "The HOP node sent bytes that do not match their hash.",
    );
  return sealed;
}

/**
 * Claim, check and open a file. `maxBytes` bounds what the root entry may
 * announce before any chunk is fetched. The entries are not acknowledged, so
 * they stay claimable until the pool drops them.
 */
export async function hopDownload(
  connection: HopConnection,
  reference: Pick<HopReference, "identifier" | "ticket">,
  maxBytes: number,
  options: TransferOptions = {},
): Promise<Uint8Array> {
  const keys = ticketKeys(reference.ticket);
  const open = async (hash: Uint8Array) =>
    unseal(
      keys.encryption,
      await claim(connection, hash, keys.signingSecret, options.signal),
    );
  const root = decodeRoot(await open(reference.identifier));
  if (root.kind === "inline") {
    if (root.data.length > maxBytes)
      throw new HopError(null, "The file is larger than this host accepts.");
    options.progress?.(root.data.length, root.data.length);
    return root.data;
  }
  if (root.totalSize > BigInt(maxBytes))
    throw new HopError(null, "The file is larger than this host accepts.");
  const total = Number(root.totalSize);
  const file = new Uint8Array(total);
  let at = 0;
  for (const hash of root.chunks) {
    const chunk = await open(hash);
    if (at + chunk.length > total)
      throw new HopError(null, "The file's chunks are longer than its size.");
    file.set(chunk, at);
    at += chunk.length;
    options.progress?.(at, total);
  }
  if (at !== total)
    throw new HopError(null, "The file's chunks are shorter than its size.");
  return file;
}

/** Whether a node a message names is one this host was configured with. */
export const knownNode = (nodes: readonly string[], node: string): boolean =>
  nodes.includes(node);

/**
 * A JSON-RPC connection to a HOP node over WebSocket. Only `wss` is opened.
 * Closing it fails every call still waiting.
 */
export function openHopConnection(url: string): Promise<HopConnection> {
  if (!url.startsWith("wss://"))
    return Promise.reject(
      new HopError(null, "A HOP node must be a wss:// URL."),
    );
  return new Promise((resolve, reject) => {
    const socket = new WebSocket(url);
    const waiting = new Map<
      number,
      { resolve: (value: unknown) => void; reject: (error: Error) => void }
    >();
    let next = 1;
    const fail = (error: Error) => {
      for (const entry of waiting.values()) entry.reject(error);
      waiting.clear();
    };
    socket.addEventListener("message", (event) => {
      let reply: {
        id?: number;
        result?: unknown;
        error?: { code?: number; message?: string };
      };
      try {
        reply = JSON.parse(String(event.data));
      } catch {
        return;
      }
      const entry = reply.id === undefined ? undefined : waiting.get(reply.id);
      if (entry === undefined) return;
      waiting.delete(reply.id!);
      if (reply.error !== undefined)
        entry.reject(
          new HopError(
            reply.error.code ?? null,
            reply.error.message ?? "The HOP node refused the call.",
          ),
        );
      else entry.resolve(reply.result);
    });
    socket.addEventListener("close", () =>
      fail(new HopError(null, "The connection to the HOP node closed.")),
    );
    socket.addEventListener("error", () =>
      reject(new HopError(null, `Could not reach the HOP node ${url}.`)),
    );
    socket.addEventListener("open", () =>
      resolve({
        call: (method, params) =>
          new Promise((resolveCall, rejectCall) => {
            if (socket.readyState !== WebSocket.OPEN) {
              rejectCall(new HopError(null, "The HOP connection is closed."));
              return;
            }
            const id = next++;
            waiting.set(id, { resolve: resolveCall, reject: rejectCall });
            socket.send(JSON.stringify({ jsonrpc: "2.0", id, method, params }));
          }),
        close: () => socket.close(),
      }),
    );
  });
}

/** Check a submission's sender signature, as a node does. For the synthetic node. */
export function verifySubmission(params: {
  data: Uint8Array;
  signer: Uint8Array;
  signature: Uint8Array;
  timestampMs: bigint;
}): boolean {
  const signer = fromSr25519Multi(params.signer, 32);
  const signature = fromSr25519Multi(params.signature, 64);
  return (
    signer !== null &&
    signature !== null &&
    sr25519Verify(
      submitPayload(blake2b256(params.data), params.timestampMs),
      signature,
      signer,
    )
  );
}
