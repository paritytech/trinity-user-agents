import { sha256 } from "@noble/hashes/sha2.js";

/**
 * Deterministic WebTransport certificate hashes for a PolkaJAM peer.
 *
 * PolkaJAM (`crates/node/src/net/cert.rs`, `dd9af78`) serves an unsigned X.509
 * certificate for its P-256 peer key: serial 0, issuer and subject `CN=jam`,
 * one dNSName SAN equal to the peer-id text, Ed25519 signature algorithm with
 * an all-zero 64-byte signature, and a validity window derived from a fixed
 * 10-day period padded by one day on both sides. A client that knows the
 * peer's compressed P-256 key can therefore compute the certificate hashes
 * offline and pass them as `serverCertificateHashes`. These bytes mirror
 * PolkaJAM's `crates/node/src/net/cert.rs` generated with rcgen 0.14.8.
 */

export const UNPADDED_VALIDITY_PERIOD_SECS = 10 * 24 * 3600;
export const VALIDITY_PERIOD_PADDING_SECS = 24 * 3600;

const BITS_TO_CHAR = "abcdefghijklmnopqrstuvwxyz234567";
const P = (1n << 256n) - (1n << 224n) + (1n << 192n) + (1n << 96n) - 1n;
const B = 0x5ac635d8aa3a93e7b3ebbd55769886bc651d06b0cc53b0f63bce3c3e27d2604bn;
const OID_ED25519 = [0x06, 0x03, 0x2b, 0x65, 0x70];
const OID_EC_PUBLIC_KEY = [0x06, 0x07, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x02, 0x01];
const OID_PRIME256V1 = [0x06, 0x08, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x03, 0x01, 0x07];
const OID_SUBJECT_ALT_NAME = [0x06, 0x03, 0x55, 0x1d, 0x11];
const ascii = new TextEncoder();

/** PolkaJAM peer-id text: prefix letter then 32 bytes, base-32 LSB-first. */
export function peerIdText(prefix: string, bytes: Uint8Array): string {
  if (bytes.length !== 32) throw new Error("peer id needs 32 bytes");
  let text = prefix;
  for (let i = 0; i < 256; i += 5) {
    const low = bytes[i >> 3]!;
    const high = bytes[(i >> 3) + 1] ?? 0;
    text += BITS_TO_CHAR[((low | (high << 8)) >> (i % 8)) & 0x1f];
  }
  return text;
}

/** Parse `e…`, `o…` or `v…` text into (prefix, 32 bytes). */
export function parsePeerIdText(text: string): { prefix: string; bytes: Uint8Array } {
  if (text.length !== 53) throw new Error(`peer id text must be 53 characters, got ${text.length}`);
  const bytes = new Uint8Array(32);
  let acc = 0;
  let n = 0;
  let i = 0;
  for (const char of text.slice(1)) {
    const bits = BITS_TO_CHAR.indexOf(char);
    if (bits < 0) throw new Error(`invalid peer id character ${JSON.stringify(char)}`);
    acc |= bits << n;
    n += 5;
    if (n >= 8) {
      bytes[i++] = acc & 0xff;
      acc >>= 8;
      n -= 8;
    }
  }
  if (acc !== 0) throw new Error("peer id has non-zero trailing bits");
  return { prefix: text[0]!, bytes };
}

/** `o…`/`v…` text to a compressed SEC1 P-256 point (0x03 odd y / 0x02 even y). */
export function p256IdToCompressed(text: string): Uint8Array {
  const { prefix, bytes } = parsePeerIdText(text);
  if (prefix !== "o" && prefix !== "v") throw new Error("P-256 peer ids begin with 'o' or 'v'");
  const out = new Uint8Array(33);
  out[0] = prefix === "o" ? 3 : 2;
  out.set(bytes, 1);
  return out;
}

/** Ed25519 `e…` text to the 32-byte public key. */
export function ed25519IdToKey(text: string): Uint8Array {
  const { prefix, bytes } = parsePeerIdText(text);
  if (prefix !== "e") throw new Error("Ed25519 peer ids begin with 'e'");
  return bytes;
}

function bigintFromBytes(bytes: Uint8Array): bigint {
  let v = 0n;
  for (const b of bytes) v = (v << 8n) | BigInt(b);
  return v;
}

function bytesFromBigint(v: bigint, length: number): Uint8Array {
  const out = new Uint8Array(length);
  for (let i = length - 1; i >= 0; i--) {
    out[i] = Number(v & 0xffn);
    v >>= 8n;
  }
  return out;
}

function modPow(base: bigint, exp: bigint, mod: bigint): bigint {
  let result = 1n;
  base %= mod;
  while (exp > 0n) {
    if (exp & 1n) result = (result * base) % mod;
    base = (base * base) % mod;
    exp >>= 1n;
  }
  return result;
}

/** Uncompressed SEC1 (0x04 ‖ x ‖ y) for a compressed P-256 point; p ≡ 3 mod 4. */
export function decompressP256(compressed: Uint8Array): Uint8Array {
  if (compressed.length !== 33 || (compressed[0] !== 2 && compressed[0] !== 3)) {
    throw new Error("expected a 33-byte compressed P-256 point");
  }
  const x = bigintFromBytes(compressed.subarray(1));
  if (x >= P) throw new Error("P-256 x coordinate out of range");
  const rhs = (((x * x) % P) * x - 3n * x + B) % P;
  const alpha = (rhs + P) % P;
  let y = modPow(alpha, (P + 1n) >> 2n, P);
  if ((y * y) % P !== alpha) throw new Error("P-256 x coordinate is not on the curve");
  if ((y & 1n) !== BigInt(compressed[0]! & 1)) y = P - y;
  const out = new Uint8Array(65);
  out[0] = 4;
  out.set(bytesFromBigint(x, 32), 1);
  out.set(bytesFromBigint(y, 32), 33);
  return out;
}

function der(tag: number, ...parts: ArrayLike<number>[]): number[] {
  const body = parts.flatMap((part) => Array.from(part));
  const len = body.length;
  const header =
    len < 0x80
      ? [tag, len]
      : len < 0x100
        ? [tag, 0x81, len]
        : [tag, 0x82, len >> 8, len & 0xff];
  return header.concat(body);
}

function utcTime(unixSecs: number): number[] {
  const date = new Date(unixSecs * 1000);
  const year = date.getUTCFullYear();
  if (year < 1950 || year >= 2050) {
    throw new Error("validity outside the UTCTime range PolkaJAM certificates use");
  }
  const two = (n: number): string => String(n).padStart(2, "0");
  const text = `${two(year % 100)}${two(date.getUTCMonth() + 1)}${two(date.getUTCDate())}${two(
    date.getUTCHours(),
  )}${two(date.getUTCMinutes())}${two(date.getUTCSeconds())}Z`;
  return der(0x17, ascii.encode(text));
}

const JAM_DN = der(0x30, der(0x31, der(0x30, [0x06, 0x03, 0x55, 0x04, 0x03], der(0x0c, ascii.encode("jam")))));
const ED25519_ALG = der(0x30, OID_ED25519);

/** Fixed 10-day period index for a unix time; the server switches at boundaries. */
export function validityPeriodAt(unixSecs: number): number {
  return Math.floor(unixSecs / UNPADDED_VALIDITY_PERIOD_SECS);
}

/** `[notBefore, notAfter]` unix seconds of a period (padded by one day). */
export function validityBounds(period: number): [number, number] {
  return [
    Math.max(period * UNPADDED_VALIDITY_PERIOD_SECS - VALIDITY_PERIOD_PADDING_SECS, 0),
    (period + 1) * UNPADDED_VALIDITY_PERIOD_SECS + VALIDITY_PERIOD_PADDING_SECS,
  ];
}

/** DER certificate PolkaJAM presents for `compressed` during `period`. */
export function webTransportCertificateDer(compressed: Uint8Array, period: number): Uint8Array {
  const point = decompressP256(compressed);
  const altName = peerIdText(compressed[0] === 3 ? "o" : "v", compressed.subarray(1));
  const [notBefore, notAfter] = validityBounds(period);
  const spki = der(0x30, der(0x30, OID_EC_PUBLIC_KEY, OID_PRIME256V1), der(0x03, [0x00], point));
  const san = der(0x30, der(0x30, OID_SUBJECT_ALT_NAME, der(0x04, der(0x30, der(0x82, ascii.encode(altName))))));
  const tbs = der(
    0x30,
    der(0xa0, der(0x02, [0x02])),
    der(0x02, [0x00]),
    ED25519_ALG,
    JAM_DN,
    der(0x30, utcTime(notBefore), utcTime(notAfter)),
    JAM_DN,
    spki,
    der(0xa3, san),
  );
  return Uint8Array.from(der(0x30, tbs, ED25519_ALG, der(0x03, [0x00], new Uint8Array(64))));
}

/** SHA-256 of {@link webTransportCertificateDer}. */
export function webTransportCertificateHash(compressed: Uint8Array, period: number): Uint8Array {
  return sha256(webTransportCertificateDer(compressed, period));
}

/**
 * Hashes to pass as `serverCertificateHashes` at `unixSecs`: the current
 * period plus both neighbours, so a clock skew or a boundary crossing during
 * the handshake still matches whichever certificate the server picked.
 */
export function webTransportCertificateHashes(compressed: Uint8Array, unixSecs: number): Uint8Array[] {
  const period = validityPeriodAt(unixSecs);
  return [period - 1, period, period + 1]
    .filter((p) => p >= 0)
    .map((p) => webTransportCertificateHash(compressed, p));
}
