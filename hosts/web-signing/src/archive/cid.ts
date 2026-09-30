import { sha256 } from "@noble/hashes/sha2.js";

/** Multicodec of a dag-pb node. */
export const DAG_PB = 0x70;
/** Multicodec of a raw block. */
export const RAW = 0x55;

const SHA2_256 = 0x12;
const IDENTITY = 0x00;
const MAX_VARINT_BYTES = 8;

/** A parsed content identifier. */
export interface Cid {
  /** The identifier as stored, version prefix included. */
  bytes: Uint8Array;
  codec: number;
  hashCode: number;
  digest: Uint8Array;
}

/** A varint and the offset just after it. */
export function readVarint(
  bytes: Uint8Array,
  offset: number,
): { value: number; next: number } {
  let value = 0;
  for (let index = 0; index < MAX_VARINT_BYTES; index += 1) {
    const byte = bytes[offset + index];
    if (byte === undefined) throw new Error("truncated varint");
    value += (byte & 0x7f) * 2 ** (7 * index);
    if ((byte & 0x80) === 0) return { value, next: offset + index + 1 };
  }
  throw new Error("varint is too long");
}

/** A CID read from the start of `bytes[offset..]`, and the offset after it. */
export function readCid(
  bytes: Uint8Array,
  offset: number,
): { cid: Cid; next: number } {
  if (bytes[offset] === SHA2_256 && bytes[offset + 1] === 32) {
    const next = offset + 34;
    if (next > bytes.length) throw new Error("truncated CID");
    return {
      cid: {
        bytes: bytes.slice(offset, next),
        codec: DAG_PB,
        hashCode: SHA2_256,
        digest: bytes.slice(offset + 2, next),
      },
      next,
    };
  }
  if (bytes[offset] !== 1) throw new Error("unsupported CID version");
  const codec = readVarint(bytes, offset + 1);
  const hashCode = readVarint(bytes, codec.next);
  const length = readVarint(bytes, hashCode.next);
  const next = length.next + length.value;
  if (next > bytes.length) throw new Error("truncated CID");
  return {
    cid: {
      bytes: bytes.slice(offset, next),
      codec: codec.value,
      hashCode: hashCode.value,
      digest: bytes.slice(length.next, next),
    },
    next,
  };
}

const BASE32 = "abcdefghijklmnopqrstuvwxyz234567";

/** A CID from its lower-case base32 text form, `bafy…`. */
export function parseCidString(text: string): Cid {
  if (!/^b[a-z2-7]+$/.test(text)) throw new Error("not a base32 CIDv1");
  let bits = 0;
  let accumulated = 0;
  const bytes: number[] = [];
  for (const char of text.slice(1)) {
    accumulated = (accumulated << 5) | BASE32.indexOf(char);
    bits += 5;
    if (bits >= 8) {
      bits -= 8;
      bytes.push((accumulated >> bits) & 0xff);
    }
  }
  const { cid, next } = readCid(Uint8Array.from(bytes), 0);
  if (next !== bytes.length) throw new Error("trailing bytes after the CID");
  return cid;
}

/** The lower-case base32 text form of a CIDv1. */
export function cidToString(cid: Cid): string {
  let bits = 0;
  let accumulated = 0;
  let out = "b";
  for (const byte of cid.bytes) {
    accumulated = (accumulated << 8) | byte;
    bits += 8;
    while (bits >= 5) {
      bits -= 5;
      out += BASE32[(accumulated >> bits) & 31];
    }
    accumulated &= (1 << bits) - 1;
  }
  return bits > 0 ? out + BASE32[(accumulated << (5 - bits)) & 31] : out;
}

/** A key for maps: the identifier's bytes as hex. */
export function cidKey(cid: Cid): string {
  let key = "";
  for (const byte of cid.bytes) key += byte.toString(16).padStart(2, "0");
  return key;
}

/** Whether `data` is the block `cid` names. An unknown hash is an error, never a pass. */
export function verifyBlock(cid: Cid, data: Uint8Array): boolean {
  if (cid.hashCode === SHA2_256) {
    const actual = sha256(data);
    return (
      actual.length === cid.digest.length &&
      actual.every((byte, index) => byte === cid.digest[index])
    );
  }
  if (cid.hashCode === IDENTITY)
    return (
      data.length === cid.digest.length &&
      data.every((byte, index) => byte === cid.digest[index])
    );
  throw new Error(`unsupported hash function 0x${cid.hashCode.toString(16)}`);
}

/** A CIDv1 for `data`, hashed with sha2-256. */
export function cidFor(codec: number, data: Uint8Array): Cid {
  const digest = sha256(data);
  return {
    bytes: Uint8Array.from([1, codec, SHA2_256, 32, ...digest]),
    codec,
    hashCode: SHA2_256,
    digest,
  };
}
