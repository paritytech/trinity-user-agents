import { chacha20poly1305 } from "@noble/ciphers/chacha.js";
import { x25519 } from "@noble/curves/ed25519.js";
import { blake2b } from "@noble/hashes/blake2.js";
import { hkdf } from "@noble/hashes/hkdf.js";
import { sha256 } from "@noble/hashes/sha2.js";
import * as sr25519 from "@scure/sr25519";
import { concat } from "./codec.js";

/** The primitives the Nova chat wire is built from. */

/** Random bytes. Tests pass a fixed source so sealed output is repeatable. */
export type RandomBytes = (length: number) => Uint8Array;

export const cryptoRandom: RandomBytes = (length) =>
  crypto.getRandomValues(new Uint8Array(length));

export const blake2b256 = (message: Uint8Array): Uint8Array =>
  blake2b(message, { dkLen: 32 });

/** BLAKE2b-256 with `key` as the BLAKE2 key parameter. */
export const keyedBlake2b256 = (
  key: Uint8Array,
  message: Uint8Array,
): Uint8Array => blake2b(message, { dkLen: 32, key });

export const x25519PublicKey = (secret: Uint8Array): Uint8Array =>
  x25519.getPublicKey(secret);

/** The raw X25519 output, before any key derivation. */
export const x25519Agree = (secret: Uint8Array, peer: Uint8Array): Uint8Array =>
  x25519.getSharedSecret(secret, peer);

/** HKDF-SHA256 with empty salt and info, 32 bytes: the AEAD key of a shared secret. */
export const aeadKey = (secret: Uint8Array): Uint8Array =>
  hkdf(sha256, secret, new Uint8Array(0), new Uint8Array(0), 32);

const NONCE_BYTES = 12;
const TAG_BYTES = 16;

/** ChaCha20-Poly1305 with no associated data: `nonce12 || ciphertext || tag16`. */
export function seal(
  key: Uint8Array,
  plaintext: Uint8Array,
  random: RandomBytes,
): Uint8Array {
  const nonce = random(NONCE_BYTES);
  return concat(nonce, chacha20poly1305(key, nonce).encrypt(plaintext));
}

/** Open {@link seal}'s output. Throws when the key is wrong or the bytes were changed. */
export function unseal(key: Uint8Array, sealed: Uint8Array): Uint8Array {
  if (sealed.length < NONCE_BYTES + TAG_BYTES)
    throw new Error("the sealed data is too short");
  return chacha20poly1305(key, sealed.subarray(0, NONCE_BYTES)).decrypt(
    sealed.subarray(NONCE_BYTES),
  );
}

/** sr25519 over the raw message, in the `substrate` signing context. */
export const sr25519Sign = (
  secret: Uint8Array,
  message: Uint8Array,
): Uint8Array => sr25519.sign(secret, message);

export function sr25519Verify(
  message: Uint8Array,
  signature: Uint8Array,
  publicKey: Uint8Array,
): boolean {
  try {
    return sr25519.verify(message, signature, publicKey);
  } catch {
    return false;
  }
}
