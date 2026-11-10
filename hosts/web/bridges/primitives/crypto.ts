import { x25519 } from "@noble/curves/ed25519.js";
import { blake2b } from "@noble/hashes/blake2.js";
import * as sr25519 from "@scure/sr25519";

/** Hashes, X25519 agreement and sr25519 verification over byte arrays. */

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
