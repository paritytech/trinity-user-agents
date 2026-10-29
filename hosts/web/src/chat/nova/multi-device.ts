import {
  Reader,
  bytes,
  compact,
  concat,
  equalBytes,
  readBytes,
} from "./codec.js";
import { aeadKey, seal, unseal, type RandomBytes } from "./crypto.js";
import type { DeviceInfo } from "./protocol.js";

/**
 * The multi-device envelope of the Polkadot app's device route, written from
 * the app's protocol (chat-spec `mds.md`, "Sending P2P Messages"; the apps'
 * `MultiDeviceStatementDataCoder` and `MultiDeviceEnvelopeEncryption`). No
 * code is taken from them.
 *
 * A request or response (session data tag 0 or 1) travels as tag 2 or 3:
 *
 *     tag || Vec<u8> encrypted || Vec<(account32, Vec<u8> encryptedKey)>
 *
 * `encrypted` is the request or response without its tag, sealed under a
 * one-time 32-byte key used as the ChaCha20-Poly1305 key itself. Each entry
 * holds that one-time key sealed for one recipient device, under the HKDF key
 * of the X25519 secret between the sender's device key and the recipient's.
 * Seals are `nonce12 || ciphertext || tag16`. The envelope is then sealed
 * again, as every statement of a pair is, under the pair's outer key.
 */

const MULTI_REQUEST = 2;
const MULTI_RESPONSE = 3;
const ONE_TIME_KEY_BYTES = 32;

/** Whether session data is an envelope, going by its tag. */
export const isDeviceEnvelope = (data: Uint8Array): boolean =>
  data[0] === MULTI_REQUEST || data[0] === MULTI_RESPONSE;

/**
 * Wrap session data (a request, tag 0, or a response, tag 1) for each of
 * `recipients`. `agreeDevice` is the X25519 secret of this device's key with
 * a peer key. The random bytes are used in order: the one-time key, the
 * payload nonce, then one nonce per recipient.
 */
export function wrapDeviceEnvelope(
  sessionData: Uint8Array,
  recipients: readonly DeviceInfo[],
  agreeDevice: (peerKey: Uint8Array) => Uint8Array,
  random: RandomBytes,
): Uint8Array {
  const tag = sessionData[0];
  if (tag !== 0 && tag !== 1)
    throw new Error("only a request or a response is wrapped");
  if (recipients.length === 0)
    throw new Error("a device envelope needs at least one recipient device");
  const oneTimeKey = random(ONE_TIME_KEY_BYTES);
  const encrypted = seal(oneTimeKey, sessionData.subarray(1), random);
  const entries = recipients.map(({ statementAccount, encryptionKey }) => {
    if (statementAccount.length !== 32)
      throw new Error("a device account must be 32 bytes");
    return concat(
      statementAccount,
      bytes(seal(aeadKey(agreeDevice(encryptionKey)), oneTimeKey, random)),
    );
  });
  return concat(
    Uint8Array.of(tag + MULTI_REQUEST),
    bytes(encrypted),
    compact(entries.length),
    ...entries,
  );
}

/**
 * Open an envelope from the device with key `senderKey`, for the device that
 * signs as `ownAccount`, and give back plain session data (tag 0 or 1).
 * Session data that is not an envelope comes back as it was. Throws when the
 * envelope names no entry for this device or any seal fails.
 */
export function openDeviceEnvelope(
  data: Uint8Array,
  ownAccount: Uint8Array,
  senderKey: Uint8Array,
  agreeDevice: (peerKey: Uint8Array) => Uint8Array,
): Uint8Array {
  if (!isDeviceEnvelope(data)) return data;
  const reader = new Reader(data);
  const tag = reader.u8();
  const encrypted = readBytes(reader);
  const count = reader.compact();
  let wrapped: Uint8Array | null = null;
  for (let index = 0; index < count; index++) {
    const account = reader.take(32);
    const sealedKey = readBytes(reader);
    if (wrapped === null && equalBytes(account, ownAccount))
      wrapped = sealedKey;
  }
  reader.finish();
  if (wrapped === null)
    throw new Error("the envelope is not addressed to this device");
  const oneTimeKey = unseal(aeadKey(agreeDevice(senderKey)), wrapped);
  if (oneTimeKey.length !== ONE_TIME_KEY_BYTES)
    throw new Error("the envelope's one-time key is not 32 bytes");
  return concat(
    Uint8Array.of(tag - MULTI_REQUEST),
    unseal(oneTimeKey, encrypted),
  );
}
