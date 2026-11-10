/**
 * Stateless helpers for the device-to-device signaling session: payload
 * encryption and pairwise topic derivation.
 *
 * Payload encryption is ChaCha20-Poly1305 keyed via HKDF-SHA256 (empty salt +
 * empty info — parity with Android `Hkdf.kt` + `MessageEncryption.kt`) over
 * X25519(devEncPriv, peerDevEncPub). Wire: nonce(12) || ciphertext || authTag(16).
 * Curve and AEAD follow CHAT-RFC-0004; delegated to the SDK's `createEncryption`
 * so they cannot drift from the PApp.
 *
 * Topic derivation delegates to the SDK's `createSessionId`, byte-equivalent to
 * Android's `deriveCommunicationTopic`:
 *   topic = khash(ECDH-shared-secret,
 *                 "session" || sender(32) || receiver(32) || "/" || sP || "/" || rP)
 * Sender = device that posts; receiver = device that subscribes. Pins are empty
 * for own-device sessions (literal "//" tail).
 */

import { x25519 } from '@noble/curves/ed25519.js';
import { createAccountId, createEncryption, createSessionId } from '@novasamatech/statement-store';

/** X25519 agreement for one device pair — input to both topic derivation and encryption. */
function deriveSharedSecret(devPriv: Uint8Array, peerDevPub: Uint8Array): Uint8Array {
  return x25519.getSharedSecret(devPriv, peerDevPub);
}

/**
 * Payload encryption for one device pair. Takes the shared secret rather than the
 * keypair so the caller — which already derives it for the topics — pays for the
 * key agreement and HKDF once per channel instead of once per message.
 */
function createPayloadEncryption(sharedSecret: Uint8Array) {
  const encryption = createEncryption(sharedSecret);

  const unwrap = (result: ReturnType<typeof encryption.encrypt>): Uint8Array => {
    if (result.isErr()) throw result.error;

    return result.value;
  };

  return {
    encrypt: (plaintext: Uint8Array): Uint8Array => unwrap(encryption.encrypt(plaintext)),
    decrypt: (ciphertext: Uint8Array): Uint8Array => unwrap(encryption.decrypt(ciphertext)),
  };
}

/** Directional pair topic — `sender` posts here, `receiver` subscribes here. */
function deriveDeviceSessionTopic(
  sharedSecret: Uint8Array,
  sender: { accountId: Uint8Array },
  receiver: { accountId: Uint8Array },
): Uint8Array {
  return createSessionId(
    sharedSecret,
    { accountId: createAccountId(sender.accountId), pin: undefined },
    { accountId: createAccountId(receiver.accountId), pin: undefined },
  );
}

export const deviceSessionService = {
  deriveSharedSecret,
  createPayloadEncryption,
  deriveDeviceSessionTopic,
};
