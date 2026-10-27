import { chacha20poly1305 } from "@noble/ciphers/chacha.js";
import { x25519 } from "@noble/curves/ed25519.js";
import { blake2b } from "@noble/hashes/blake2.js";
import { hkdf } from "@noble/hashes/hkdf.js";
import { sha256 } from "@noble/hashes/sha2.js";

/**
 * PROTOTYPE, CLASSICAL. Sealing to a public key and hashing with domain
 * separation, from existing primitives: X25519, HKDF-SHA256 and
 * ChaCha20-Poly1305, and BLAKE2b-256. It lets the invitation prototype carry
 * real ciphertext and real bindings. It is not the design's suite: no
 * post-quantum key, no chosen envelope or padding.
 */

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

const lengthOf = (bytes: Uint8Array): Uint8Array => {
  const out = new Uint8Array(4);
  new DataView(out.buffer).setUint32(0, bytes.length);
  return out;
};

/**
 * BLAKE2b-256 over `domain` and each part, every one length-prefixed, so no
 * two lists of parts hash alike. Strings are UTF-8; `0x` hex is hashed as text.
 */
export function digest(
  domain: string,
  ...parts: readonly (Uint8Array | string)[]
): Uint8Array {
  const fields = [domain, ...parts].map((part) =>
    typeof part === "string" ? utf8(part) : part,
  );
  return blake2b(concat(...fields.flatMap((part) => [lengthOf(part), part])), {
    dkLen: 32,
  });
}

export interface X25519Pair {
  secret: Uint8Array;
  public: Uint8Array;
}

export function x25519Pair(secret: Uint8Array): X25519Pair {
  return { secret, public: x25519.getPublicKey(secret) };
}

export const x25519Agree = (secret: Uint8Array, peer: Uint8Array): Uint8Array =>
  x25519.getSharedSecret(secret, peer);

const NONCE = new Uint8Array(12);

function sealKey(
  shared: Uint8Array,
  ephemeral: Uint8Array,
  recipient: Uint8Array,
  purpose: string,
): Uint8Array {
  return hkdf(sha256, shared, concat(ephemeral, recipient), utf8(purpose), 32);
}

/**
 * Seal `plaintext` so only the holder of `recipient`'s secret opens it:
 * `ephemeral public (32) || ciphertext || tag (16)`. `purpose` separates
 * uses; a key is fresh per message, so the nonce is fixed.
 */
export function sealTo(
  recipient: Uint8Array,
  plaintext: Uint8Array,
  purpose: string,
  random: RandomBytes,
): Uint8Array {
  const ephemeral = x25519Pair(random(32));
  const key = sealKey(
    x25519Agree(ephemeral.secret, recipient),
    ephemeral.public,
    recipient,
    purpose,
  );
  return concat(
    ephemeral.public,
    chacha20poly1305(key, NONCE).encrypt(plaintext),
  );
}

/** Open {@link sealTo}'s output with the recipient's pair, or null when it does not open. */
export function openSealed(
  recipient: X25519Pair,
  sealed: Uint8Array,
  purpose: string,
): Uint8Array | null {
  if (sealed.length < 32 + 16) return null;
  const ephemeral = sealed.subarray(0, 32);
  try {
    const key = sealKey(
      x25519Agree(recipient.secret, ephemeral),
      ephemeral,
      recipient.public,
      purpose,
    );
    return chacha20poly1305(key, NONCE).decrypt(sealed.subarray(32));
  } catch {
    return null;
  }
}
