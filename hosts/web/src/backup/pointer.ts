import { bytesToHex, type HexString } from "@parity/truapi/scale";
import { equalBytes, utf8 } from "../../bridges/primitives/codec.js";
import {
  decodeStatement,
  verifyStatement,
  type Statement,
} from "../../bridges/primitives/statement.js";
import {
  openPointer,
  sealPointer,
  type BackupBinding,
  type BackupKeys,
} from "./sealing.js";

/**
 * The backup pointer: one statement of the backup statement account, on the
 * channel and topic the wallet's backup keys name, whose sealed data says
 * where the latest snapshot is on Bulletin. Each new pointer replaces the
 * last on that channel, so a store holds at most one.
 *
 * Its expiry is finite: the upper 32 bits are when the pointer lapses, Unix
 * seconds, and the lower 32 bits its revision, so a later revision always
 * has the higher expiry the store needs to replace the one before.
 */

/** What a pointer says. */
export interface PointerBody {
  /** One more than the revision it replaced. */
  revision: number;
  /** The snapshot's Bulletin key: BLAKE2b-256 of its sealed bytes. */
  snapshot: HexString;
  /** The sealed snapshot's size, in bytes. */
  bytes: number;
  /** BLAKE2b-256 of the snapshot's state, which tells two snapshots of the same state apart from two of different ones. */
  digest: HexString;
  /** When the snapshot was taken, Unix ms. */
  createdAt: number;
  /** The browser that published it: a random id that names no wallet. */
  source: string;
}

/** A pointer as a store holds it. */
export interface Pointer {
  body: PointerBody;
  expiry: bigint;
}

/** How long a pointer lasts unless a newer one replaces it: as long as Bulletin keeps the snapshot it names. */
export const POINTER_LIFETIME_SECONDS = 14 * 86_400;

/** The pointer statement for `body`, unsigned. */
export function pointerStatement(
  keys: BackupKeys,
  binding: BackupBinding,
  body: PointerBody,
  expiry: bigint,
): Statement {
  return {
    expiry,
    channel: keys.channel,
    topics: [keys.topic],
    data: sealPointer(keys, binding, utf8(JSON.stringify(body))),
  };
}

/**
 * The expiry of revision `revision` of a pointer to a snapshot taken at
 * `takenAtMs`: it lapses {@link POINTER_LIFETIME_SECONDS} after the
 * snapshot was taken, when Bulletin may let the snapshot go, and is above
 * `previous`, the expiry of the pointer it replaces, so the store takes it.
 */
export function pointerExpiry(
  revision: number,
  previous: bigint | null,
  takenAtMs: number,
): bigint {
  const lapses = BigInt(
    Math.floor(takenAtMs / 1000) + POINTER_LIFETIME_SECONDS,
  );
  const floor = previous === null ? 0n : previous >> 32n;
  const expiry = ((lapses > floor ? lapses : floor) << 32n) | BigInt(revision);
  return previous !== null && expiry <= previous
    ? (((previous >> 32n) + 1n) << 32n) | BigInt(revision)
    : expiry;
}

function readBody(bytes: Uint8Array): PointerBody | null {
  try {
    const body = JSON.parse(new TextDecoder().decode(bytes)) as PointerBody;
    return Number.isSafeInteger(body.revision) &&
      body.revision > 0 &&
      /^0x[0-9a-f]{64}$/.test(body.snapshot) &&
      /^0x[0-9a-f]{64}$/.test(body.digest) &&
      Number.isSafeInteger(body.bytes) &&
      Number.isSafeInteger(body.createdAt) &&
      typeof body.source === "string"
      ? body
      : null;
  } catch {
    return null;
  }
}

/**
 * The pointer among `statements` with the highest expiry, which is the one
 * a store keeps on the channel, among those `account` signed on the
 * backup's channel and that open with its keys; or null. Anything else on
 * the topic is ignored.
 */
export function newestPointer(
  statements: readonly Uint8Array[],
  account: Uint8Array,
  keys: BackupKeys,
  binding: BackupBinding,
): Pointer | null {
  let newest: Pointer | null = null;
  for (const encoded of statements) {
    try {
      const statement = decodeStatement(encoded);
      if (
        !equalBytes(statement.signer, account) ||
        statement.channel === null ||
        !equalBytes(statement.channel, keys.channel) ||
        !verifyStatement(statement)
      )
        continue;
      const body = readBody(openPointer(keys, binding, statement.data));
      if (
        body !== null &&
        (newest === null || statement.expiry > newest.expiry)
      )
        newest = { body, expiry: statement.expiry };
    } catch {
      continue;
    }
  }
  return newest;
}

/** `bytes` as `0x` hex. */
export const hex = (bytes: Uint8Array): HexString =>
  bytesToHex(bytes).toLowerCase() as HexString;
