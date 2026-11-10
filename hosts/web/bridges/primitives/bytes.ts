/** Bytes, hex and UTF-8 helpers that both sides of a bridge use. */

/** Random bytes. Tests pass a fixed source so output is repeatable. */
export type RandomBytes = (length: number) => Uint8Array;

export const cryptoRandom: RandomBytes = (length) =>
  crypto.getRandomValues(new Uint8Array(length));

const encoder = new TextEncoder();
const decoder = new TextDecoder();

export const utf8 = (text: string): Uint8Array => encoder.encode(text);
export const fromUtf8 = (bytes: Uint8Array): string => decoder.decode(bytes);

export function toHex(bytes: Uint8Array): string {
  let hex = "0x";
  for (const byte of bytes) hex += byte.toString(16).padStart(2, "0");
  return hex;
}

/** The bytes of `0x` hex. Throws when it is not hex. */
export function fromHex(hex: string): Uint8Array {
  if (!/^0x(?:[0-9a-fA-F]{2})*$/.test(hex)) throw new Error("not 0x hex");
  const bytes = new Uint8Array((hex.length - 2) / 2);
  for (let index = 0; index < bytes.length; index += 1)
    bytes[index] = parseInt(hex.slice(2 + index * 2, 4 + index * 2), 16);
  return bytes;
}

export function concat(...parts: Uint8Array[]): Uint8Array {
  const out = new Uint8Array(parts.reduce((sum, part) => sum + part.length, 0));
  let at = 0;
  for (const part of parts) {
    out.set(part, at);
    at += part.length;
  }
  return out;
}
