import { Reader } from "../scale-reader.js";

/** SCALE pieces the chat wire is written with. Integers are little-endian. */

export const utf8 = (text: string): Uint8Array =>
  new TextEncoder().encode(text);

export function concat(...parts: Uint8Array[]): Uint8Array {
  const out = new Uint8Array(parts.reduce((sum, part) => sum + part.length, 0));
  let at = 0;
  for (const part of parts) {
    out.set(part, at);
    at += part.length;
  }
  return out;
}

/** A compact integer, for lengths below 2^30. */
export function compact(value: number): Uint8Array {
  if (!Number.isInteger(value) || value < 0 || value >= 2 ** 30)
    throw new RangeError(`${value} is not a length this encodes`);
  if (value < 1 << 6) return Uint8Array.of(value << 2);
  if (value < 1 << 14)
    return Uint8Array.of(((value << 2) | 1) & 0xff, value >> 6);
  const word = (value << 2) | 2;
  return Uint8Array.of(
    word & 0xff,
    (word >> 8) & 0xff,
    (word >> 16) & 0xff,
    word >>> 24,
  );
}

/** `Vec<u8>`: a compact length, then the bytes. */
export const bytes = (value: Uint8Array): Uint8Array =>
  concat(compact(value.length), value);

/** `String`: a compact length, then UTF-8. */
export const text = (value: string): Uint8Array => bytes(utf8(value));

export function u32(value: number): Uint8Array {
  if (!Number.isInteger(value) || value < 0 || value >= 2 ** 32)
    throw new RangeError(`${value} is not a u32`);
  const out = new Uint8Array(4);
  new DataView(out.buffer).setUint32(0, value, true);
  return out;
}

export function u64(value: bigint): Uint8Array {
  const out = new Uint8Array(8);
  new DataView(out.buffer).setBigUint64(0, value, true);
  return out;
}

/** `Option<T>`: `0x00` for none, or `0x01` and the value. */
export function option<T>(
  value: T | null,
  encode: (value: T) => Uint8Array,
): Uint8Array {
  return value === null
    ? Uint8Array.of(0)
    : concat(Uint8Array.of(1), encode(value));
}

/** `Vec<T>`: a compact count, then each item. */
export function list<T>(
  items: T[],
  encode: (item: T) => Uint8Array,
): Uint8Array {
  return concat(compact(items.length), ...items.map(encode));
}

/** Read a `Vec<u8>`. */
export const readBytes = (reader: Reader): Uint8Array =>
  reader.take(reader.compact()).slice();

/** Read an `Option<T>`, refusing a tag other than 0 or 1. */
export function readOption<T>(
  reader: Reader,
  read: (reader: Reader) => T,
): T | null {
  const tag = reader.u8();
  if (tag === 0) return null;
  if (tag === 1) return read(reader);
  throw new Error(`an option tag is ${tag}`);
}

/** Read a `Vec<T>`. */
export function readList<T>(reader: Reader, read: (reader: Reader) => T): T[] {
  const count = reader.compact();
  return Array.from({ length: count }, () => read(reader));
}

export function equalBytes(left: Uint8Array, right: Uint8Array): boolean {
  return (
    left.length === right.length &&
    left.every((byte, index) => byte === right[index])
  );
}

export { Reader };
