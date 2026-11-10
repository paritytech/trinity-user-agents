import { chacha20poly1305 } from "@noble/ciphers/chacha.js";
import { hkdf } from "@noble/hashes/hkdf.js";
import { sha256 } from "@noble/hashes/sha2.js";
import { concat } from "../../../bridges/primitives/codec.js";
import {
  cryptoRandom,
  x25519Agree,
  type RandomBytes,
} from "../../../bridges/primitives/crypto.js";

/**
 * The AEAD layer every Nova route uses (Android `RealCommunicationEncryption`,
 * `Hkdf.kt`, `MessageEncryption.kt` at fc186861db): HKDF-SHA256 with empty salt
 * and info over the raw X25519 output, then ChaCha20-Poly1305 with a random
 * 12-byte nonce written in front of the ciphertext.
 */

const NONCE_LENGTH = 12;

/** Raw X25519 output. Refuses a small-order peer key (all-zero output), as RFC-0004 and Android `X25519Crypto.kt` do. */
export function agree(secret: Uint8Array, peer: Uint8Array): Uint8Array {
  if (peer.length !== 32) throw new Error("an X25519 key is 32 bytes");
  const shared = x25519Agree(secret, peer);
  if (shared.every((byte) => byte === 0))
    throw new Error("small-order X25519 key");
  return shared;
}

export const aeadKey = (shared: Uint8Array): Uint8Array =>
  hkdf(sha256, shared, new Uint8Array(0), new Uint8Array(0), 32);

export function seal(
  key: Uint8Array,
  plain: Uint8Array,
  random: RandomBytes = cryptoRandom,
): Uint8Array {
  const nonce = random(NONCE_LENGTH);
  return concat(nonce, chacha20poly1305(key, nonce).encrypt(plain));
}

export function open(key: Uint8Array, sealed: Uint8Array): Uint8Array {
  if (sealed.length < NONCE_LENGTH + 16)
    throw new Error("sealed data is too short");
  return chacha20poly1305(key, sealed.subarray(0, NONCE_LENGTH)).decrypt(
    sealed.subarray(NONCE_LENGTH),
  );
}
