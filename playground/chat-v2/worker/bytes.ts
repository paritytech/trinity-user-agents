import type { HexString } from "@parity/truapi/scale";

const HEX = "0123456789abcdef";
const BASE64 =
  "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

export function bytesToHex(bytes: Uint8Array): HexString {
  let out = "0x";
  for (const byte of bytes) out += HEX[byte >> 4] + HEX[byte & 15];
  return out as HexString;
}

export function hexToBytes(hex: string): Uint8Array {
  const digits = hex.startsWith("0x") ? hex.slice(2) : hex;
  if (digits.length % 2 !== 0 || /[^0-9a-fA-F]/.test(digits)) {
    throw new Error(`not a hex string: ${hex.slice(0, 16)}`);
  }
  const out = new Uint8Array(digits.length / 2);
  for (let i = 0; i < out.length; i++) {
    out[i] = Number.parseInt(digits.slice(i * 2, i * 2 + 2), 16);
  }
  return out;
}

export function utf8ToHex(text: string): HexString {
  return bytesToHex(new TextEncoder().encode(text));
}

export function hexToUtf8(hex: string): string {
  return new TextDecoder().decode(hexToBytes(hex));
}

/** The same byte, `count` times, as hex. */
export function repeatedByte(byte: number, count: number): HexString {
  return bytesToHex(new Uint8Array(count).fill(byte));
}

/**
 * Decode base64 without `atob`, which some worker runtimes do not have.
 */
export function base64ToBytes(base64: string): Uint8Array {
  const clean = base64.replace(/[^A-Za-z0-9+/]/g, "");
  const out = new Uint8Array(Math.floor((clean.length * 3) / 4));
  let bits = 0;
  let value = 0;
  let index = 0;
  for (const char of clean) {
    value = (value << 6) | BASE64.indexOf(char);
    bits += 6;
    if (bits >= 8) {
      bits -= 8;
      out[index++] = (value >> bits) & 0xff;
    }
  }
  return out.subarray(0, index);
}

/** Encode bytes as base64 without `btoa`, which some worker runtimes do not have. */
export function bytesToBase64(bytes: Uint8Array): string {
  let out = "";
  for (let i = 0; i < bytes.length; i += 3) {
    const chunk = (bytes[i] << 16) | ((bytes[i + 1] ?? 0) << 8) | (bytes[i + 2] ?? 0);
    const count = Math.min(3, bytes.length - i);
    for (let j = 0; j < 4; j++) out += j <= count ? BASE64[(chunk >> (18 - j * 6)) & 63] : "=";
  }
  return out;
}

/** The bytes of a base64 `data:` URL, as Vite inlines an asset. */
export function dataUrlToHex(dataUrl: string): HexString {
  const comma = dataUrl.indexOf(",");
  if (!dataUrl.startsWith("data:") || comma < 0 || !dataUrl.slice(0, comma).endsWith(";base64")) {
    throw new Error("not a base64 data URL");
  }
  return bytesToHex(base64ToBytes(dataUrl.slice(comma + 1)));
}

/** How many bytes a hex string holds. */
export function hexLength(hex: string): number {
  return (hex.length - (hex.startsWith("0x") ? 2 : 0)) / 2;
}
