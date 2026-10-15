import { Reader, bytes, compact, concat, readBytes, u64 } from "./codec.js";
import { sr25519Verify } from "./crypto.js";

/**
 * Statement-store statements as the Polkadot app writes them: a compact field
 * count, then tagged fields in tag order.
 *
 *   0 Proof   0x00 (sr25519) || signature64 || signer32
 *   2 Expiry  u64
 *   3 Channel 32 bytes
 *   4..7 Topic1..Topic4, 32 bytes each
 *   8 Data    Vec<u8>
 *
 * The proof signs the other fields' encodings, tags included, with no count
 * prefix, as plain sr25519 in the `substrate` context.
 */

export interface Statement {
  expiry: bigint;
  channel: Uint8Array | null;
  /** Up to four topics, in order. */
  topics: Uint8Array[];
  data: Uint8Array;
}

export interface SignedStatement extends Statement {
  signer: Uint8Array;
  signature: Uint8Array;
}

const TAG = { proof: 0, expiry: 2, channel: 3, topic1: 4, data: 8 } as const;
const MAX_TOPICS = 4;

function fixed(value: Uint8Array, what: string): Uint8Array {
  if (value.length !== 32)
    throw new Error(`a statement ${what} must be 32 bytes`);
  return value;
}

function unsignedFields(statement: Statement): Uint8Array[] {
  if (statement.topics.length > MAX_TOPICS)
    throw new Error("a statement has at most four topics");
  return [
    concat(Uint8Array.of(TAG.expiry), u64(statement.expiry)),
    ...(statement.channel === null
      ? []
      : [
          concat(
            Uint8Array.of(TAG.channel),
            fixed(statement.channel, "channel"),
          ),
        ]),
    ...statement.topics.map((topic, index) =>
      concat(Uint8Array.of(TAG.topic1 + index), fixed(topic, "topic")),
    ),
    concat(Uint8Array.of(TAG.data), bytes(statement.data)),
  ];
}

/** The bytes the proof signs. */
export const signingPayload = (statement: Statement): Uint8Array =>
  concat(...unsignedFields(statement));

/** Sign and encode a statement. */
export function encodeSigned(
  statement: Statement,
  signer: Uint8Array,
  sign: (payload: Uint8Array) => Uint8Array,
): Uint8Array {
  const fields = unsignedFields(statement);
  const proof = concat(
    Uint8Array.of(TAG.proof, 0),
    sign(concat(...fields)),
    signer,
  );
  return concat(compact(fields.length + 1), proof, ...fields);
}

/** Decode a statement. Throws on a layout this does not read, including a proof that is not sr25519. */
export function decodeStatement(encoded: Uint8Array): SignedStatement {
  const reader = new Reader(encoded);
  const count = reader.compact();
  let proof: { signer: Uint8Array; signature: Uint8Array } | null = null;
  let expiry: bigint | null = null;
  let channel: Uint8Array | null = null;
  const topics: Uint8Array[] = [];
  let data: Uint8Array | null = null;
  for (let index = 0; index < count; index++) {
    const tag = reader.u8();
    switch (tag) {
      case TAG.proof: {
        const kind = reader.u8();
        if (kind !== 0)
          throw new Error(`the statement proof is kind ${kind}, not sr25519`);
        const signature = reader.take(64).slice();
        proof = { signature, signer: reader.take(32).slice() };
        break;
      }
      case 1:
        reader.take(32);
        break;
      case TAG.expiry:
        expiry = reader.u64();
        break;
      case TAG.channel:
        channel = reader.take(32).slice();
        break;
      case 4:
      case 5:
      case 6:
      case 7:
        topics[tag - TAG.topic1] = reader.take(32).slice();
        break;
      case TAG.data:
        data = readBytes(reader);
        break;
      default:
        throw new Error(`the statement has a field tagged ${tag}`);
    }
  }
  reader.finish();
  if (proof === null || expiry === null || data === null)
    throw new Error("the statement lacks a proof, an expiry or data");
  if (topics.some((topic) => topic === undefined))
    throw new Error("the statement skips a topic");
  return { ...proof, expiry, channel, topics, data };
}

/** Whether the statement's proof signs its fields. */
export const verifyStatement = (statement: SignedStatement): boolean =>
  sr25519Verify(
    signingPayload(statement),
    statement.signature,
    statement.signer,
  );

/** The protocol epoch the app counts priorities and request days from: 2025-11-15 00:00 UTC. */
export const PROTOCOL_EPOCH_SECONDS = 1_763_164_800;
const SECONDS_PER_DAY = 86_400;
const NEVER = 0xffff_ffffn << 32n;

/** An expiry that never lapses, with the seconds since the protocol epoch as its priority. */
export function expiryAt(nowMs: number): bigint {
  const priority = Math.max(
    0,
    Math.floor(nowMs / 1000) - PROTOCOL_EPOCH_SECONDS,
  );
  return NEVER | BigInt(priority);
}

/** The next expiry on a channel: above the last one, so a replacement wins. */
export const nextExpiry = (previous: bigint | null, nowMs: number): bigint => {
  const now = expiryAt(nowMs);
  return previous !== null && previous >= now ? previous + 1n : now;
};

/** The request-day number of `nowMs`: whole days since the protocol epoch. */
export const requestDay = (nowMs: number): bigint =>
  BigInt(
    Math.floor(
      (Math.floor(nowMs / 1000) - PROTOCOL_EPOCH_SECONDS) / SECONDS_PER_DAY,
    ),
  );
