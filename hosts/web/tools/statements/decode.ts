import { blake2b256 } from "../../src/platform/crypto.js";
import { Reader } from "../../src/platform/scale-reader.js";
import { bytesToHex, hexToBytes } from "@parity/truapi/scale";

/**
 * Reading what crosses a People chain connection: statements, the store's
 * answers to a submission, subscription pages and topic filters. What a
 * reader does not recognise is kept as its shape ({@link shapeOf}): types,
 * keys, lengths and plain words, never a long string's value, so a node that
 * answers in another shape shows that shape without carrying statement bytes
 * into the view or the evidence.
 *
 * Shapes follow polkadot-sdk `substrate/primitives/statement-store`
 * (`Field`, `Proof`, `SubmitResult`, `RejectionReason`, `InvalidReason`),
 * master read 2026-11-01, which smoldot's light client mirrors. What the
 * answers mean depends on the People backend: see `transport.ts`.
 *
 * Over the light client (smoldot-light 2.1.0), `statement_submit` answers an
 * RPC error (code 7001) when it cannot decode the statement or no peer took
 * the broadcast; {@link clientRefusal} reads those.
 */

/** Which proof a statement carries. `OnChain` is the older layout the core still reads. */
export type ProofKind = "Sr25519" | "Ed25519" | "Ecdsa" | "OnChain" | "none";

/** What a statement shows without its data. */
export interface StatementMeta {
  /** BLAKE2b-256 of the encoded statement, `0x` hex: the store's hash for it. */
  hash: string;
  encodedBytes: number;
  proof: ProofKind;
  /** The signer's public key, or the on-chain account, `0x` hex. */
  signer?: string;
  /** The raw expiry, as a decimal string. */
  expiry?: string;
  /** Its high 32 bits: Unix seconds, or null for `0xffffffff`, which never lapses. */
  expiresAt?: number | null;
  /** Its low 32 bits, which order statements of one expiry second. */
  priority?: number;
  channel?: string;
  topics: string[];
  /** Whether it names a decryption key. The key id itself is not kept. */
  decryptionKey: boolean;
  /** Bytes of data: what the store counts against an account's `max_size`. */
  dataBytes: number;
  /** Why the rest could not be read, when a field is one this reader does not know. */
  undecoded?: string;
}

const NEVER = 0xffff_ffff;

const PROOF_LENGTHS: Record<number, [ProofKind, number, number]> = {
  // kind: [name, bytes before the signer, signer bytes]
  0: ["Sr25519", 64, 32],
  1: ["Ed25519", 64, 32],
  2: ["Ecdsa", 65, 33],
};

/**
 * The metadata of an encoded statement, and its data. A field this reader
 * does not know stops it: what came before is kept and `undecoded` says why.
 */
export function readStatement(encoded: Uint8Array): {
  meta: StatementMeta;
  data: Uint8Array | null;
} {
  const meta: StatementMeta = {
    hash: bytesToHex(blake2b256(encoded)),
    encodedBytes: encoded.length,
    proof: "none",
    topics: [],
    decryptionKey: false,
    dataBytes: 0,
  };
  let data: Uint8Array | null = null;
  const reader = new Reader(encoded);
  try {
    const count = reader.compact();
    for (let index = 0; index < count; index++) {
      const tag = reader.u8();
      switch (tag) {
        case 0: {
          const kind = reader.u8();
          const known = PROOF_LENGTHS[kind];
          if (known !== undefined) {
            const [name, before, signer] = known;
            reader.take(before);
            meta.proof = name;
            meta.signer = bytesToHex(reader.take(signer));
          } else if (kind === 3) {
            meta.proof = "OnChain";
            meta.signer = bytesToHex(reader.take(32));
            reader.take(40);
          } else throw new Error(`a proof of kind ${kind}`);
          break;
        }
        case 1:
          reader.take(32);
          meta.decryptionKey = true;
          break;
        case 2: {
          const expiry = reader.u64();
          meta.expiry = expiry.toString();
          const seconds = Number(expiry >> 32n);
          meta.expiresAt = seconds === NEVER ? null : seconds;
          meta.priority = Number(expiry & 0xffff_ffffn);
          break;
        }
        case 3:
          meta.channel = bytesToHex(reader.take(32));
          break;
        case 4:
        case 5:
        case 6:
        case 7:
          meta.topics.push(bytesToHex(reader.take(32)));
          break;
        case 8:
          data = reader.take(reader.compact()).slice();
          meta.dataBytes = data.length;
          break;
        default:
          throw new Error(`a field tagged ${tag}`);
      }
    }
    if (reader.offset !== encoded.length)
      throw new Error(
        `${encoded.length - reader.offset} bytes after its fields`,
      );
  } catch (error) {
    meta.undecoded = error instanceof Error ? error.message : String(error);
  }
  return { meta, data };
}

/** {@link readStatement} over `0x` hex, or null when it is not hex. */
export function readStatementHex(
  hex: unknown,
): { meta: StatementMeta; data: Uint8Array | null } | null {
  if (typeof hex !== "string" || !/^0x(?:[0-9a-fA-F]{2})*$/.test(hex))
    return null;
  return readStatement(hexToBytes(hex));
}

/** How the store answered a submission, as one word. */
export type SubmitStatus =
  | "new"
  | "known"
  | "knownExpired"
  | "rejected"
  | "invalid"
  | "internalError"
  | "unrecognised";

/** The store's answer to `statement_submit`, read as far as it goes. */
export interface SubmitAnswer {
  status: SubmitStatus;
  /** The reason it gave, such as `noAllowance` or `dataTooLarge`. */
  reason?: string;
  /** Numbers it gave with the reason, by their own names, such as `available_size`. At most {@link MAX_FIELDS}. */
  fields: Record<string, number>;
  /** The answer's shape ({@link shapeOf}): what it held, without long values. */
  shape: string;
}

/** The most characters of a shape or a node's text kept. */
export const RAW_LIMIT = 600;
/** The most numbers kept from one answer. */
export const MAX_FIELDS = 8;
/** A word a node may put in an answer, such as `rejected` or `data_too_large`: shown as it is. */
const WORD = /^[A-Za-z_][A-Za-z0-9_.-]{0,39}$/;
/** A run of hex longer than a 32-byte id: possibly statement bytes. */
const LONG_HEX = /(?:0x)?[0-9a-fA-F]{66,}/g;

/**
 * `value`'s shape as text: numbers, booleans, null and plain words as they
 * are; any other string as its length; arrays as their length and first
 * items; objects as their first keys. Three levels deep. Nothing longer than
 * a word is kept, so no statement data survives it.
 */
export function shapeOf(value: unknown): string {
  return boundRaw(JSON.stringify(shapeValue(value, 3)) ?? "undefined");
}

function shapeValue(value: unknown, depth: number): unknown {
  if (value === null || typeof value === "boolean") return value;
  if (typeof value === "number") return Number.isFinite(value) ? value : null;
  if (typeof value === "string")
    return WORD.test(value) && !/^[0-9a-fA-F]{8,}$/.test(value)
      ? value
      : `<string, ${value.length} chars>`;
  if (Array.isArray(value))
    return depth === 0
      ? `<array, ${value.length} items>`
      : [
          ...value.slice(0, 3).map((item) => shapeValue(item, depth - 1)),
          ...(value.length > 3 ? [`<${value.length - 3} more>`] : []),
        ];
  if (typeof value === "object") {
    const entries = Object.entries(value as Record<string, unknown>);
    if (depth === 0) return `<object, ${entries.length} keys>`;
    const out: Record<string, unknown> = {};
    for (const [key, item] of entries.slice(0, 12))
      out[WORD.test(key) ? key : `<key, ${key.length} chars>`] = shapeValue(
        item,
        depth - 1,
      );
    if (entries.length > 12) out["<more keys>"] = entries.length - 12;
    return out;
  }
  return `<${typeof value}>`;
}

/**
 * A node's or the core's text, such as an error message or a reason, as the
 * view keeps it: hex longer than a 32-byte id replaced by its length, and at
 * most `max` characters.
 */
export function nodeText(text: string, max = 200): string {
  const clean = text.replace(
    LONG_HEX,
    (run) => `0x…(${Math.floor(run.replace(/^0x/, "").length / 2)} bytes)`,
  );
  return clean.length <= max
    ? clean
    : `${clean.slice(0, max)}… (${clean.length - max} more characters)`;
}

const STATUSES: readonly SubmitStatus[] = [
  "new",
  "known",
  "knownExpired",
  "rejected",
  "invalid",
  "internalError",
];

/** Bound `text` to {@link RAW_LIMIT}, saying how much was cut. */
export function boundRaw(text: string): string {
  return text.length <= RAW_LIMIT
    ? text
    : `${text.slice(0, RAW_LIMIT)}… (${text.length - RAW_LIMIT} more characters)`;
}

export function readSubmitAnswer(result: unknown): SubmitAnswer {
  const shape = shapeOf(result);
  const fields: Record<string, number> = {};
  if (typeof result !== "object" || result === null)
    return { status: "unrecognised", fields, shape };
  const record = result as Record<string, unknown>;
  const status = STATUSES.find((known) => known === record.status);
  for (const [key, value] of Object.entries(record)) {
    if (Object.keys(fields).length === MAX_FIELDS) break;
    if (WORD.test(key) && typeof value === "number" && Number.isFinite(value))
      fields[key] = value;
  }
  return {
    status: status ?? "unrecognised",
    ...(typeof record.reason === "string"
      ? {
          reason: WORD.test(record.reason)
            ? record.reason
            : `<string, ${record.reason.length} chars>`,
        }
      : {}),
    fields,
    shape,
  };
}

/**
 * Whether the answer says the statement went out: `new` (from the light
 * client: at least one peer took the broadcast; from a node: it took it into
 * its store) or `known` (a node already held it). Neither says other nodes
 * keep it.
 */
export const wentOut = (status: SubmitStatus): boolean =>
  status === "new" || status === "known";

/** What the light client's RPC errors to a submission mean, by their message. */
export function clientRefusal(message: string | undefined): string | null {
  if (message === undefined) return null;
  if (/No connected peers to broadcast/i.test(message))
    return "not broadcast: the light client had no connected peer";
  const none = /none of the (\d+) connected peers accepted/i.exec(message);
  if (none !== null)
    return `not broadcast: none of the ${none[1]} connected peers took it`;
  if (/Error decoding statement/i.test(message))
    return "the light client could not decode it";
  return null;
}

/** A subscription's topic filter, as the request named it. */
export type FilterView =
  | { kind: "matchAll" | "matchAny"; topics: string[] }
  | { kind: "any" }
  | { kind: "unrecognised"; shape: string };

export function readFilter(params: unknown): FilterView {
  const value = Array.isArray(params) ? params[0] : params;
  if (value === "any") return { kind: "any" };
  if (typeof value === "object" && value !== null) {
    const record = value as Record<string, unknown>;
    for (const kind of ["matchAll", "matchAny"] as const)
      if (Array.isArray(record[kind]))
        return {
          kind,
          topics: (record[kind] as unknown[])
            .filter((topic): topic is string => typeof topic === "string")
            .map((topic) => topic.toLowerCase()),
        };
  }
  return { kind: "unrecognised", shape: shapeOf(value) };
}

/** One notification of a statement subscription. */
export type PageView =
  | {
      kind: "statements";
      statements: unknown[];
      /** How many more of the stored statements the node said follow, when it said. */
      remaining?: number;
    }
  | { kind: "unrecognised"; shape: string };

export function readPage(result: unknown): PageView {
  if (typeof result === "object" && result !== null) {
    const { event, data } = result as {
      event?: unknown;
      data?: { statements?: unknown; remaining?: unknown };
    };
    if (event === "newStatements" && Array.isArray(data?.statements))
      return {
        kind: "statements",
        statements: data.statements,
        ...(typeof data.remaining === "number"
          ? { remaining: data.remaining }
          : {}),
      };
  }
  return { kind: "unrecognised", shape: shapeOf(result) };
}
