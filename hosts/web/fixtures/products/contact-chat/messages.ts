import { chacha20poly1305 } from "@noble/ciphers/chacha.js";
import { hkdf } from "@noble/hashes/hkdf.js";
import { sha256 } from "@noble/hashes/sha2.js";
import {
  concat,
  digest,
  fromHex,
  toHex,
  utf8,
} from "../../../src/invitations/sealing.js";
import type { Topics } from "../example-chat/setup.js";

/**
 * DEV ONLY. FIXTURE TEXT LAYER `contact-chat/fixture-v1`: how Contact Chat
 * encrypts text on the two conversation topics after the setup. It is not a
 * chat protocol and promises no security: one key per direction from the
 * setup's X25519 secret, no ratchet, no forward secrecy, no padding. It
 * stands where libsignal or OpenMLS would, behind {@link SessionKeys}, so
 * the host-mediated setup can be tried end to end.
 *
 * - Each key is HKDF-SHA256 over the setup secret, salted with the
 *   invitation and the response, one per direction.
 * - A frame is `0x01 || counter (u64 big-endian) || ChaCha20-Poly1305`,
 *   with the counter as the nonce and the topic and counter as associated
 *   data, so a frame opens only on its own topic, once per counter.
 */

/** The keys and topics of one side of a conversation, as `0x` hex. Never leave the product. */
export interface SessionKeys {
  send: string;
  receive: string;
  /** Where this side writes. */
  out: string;
  /** Where the other side writes. */
  in: string;
}

const FRAME_VERSION = 1;
const COUNTER_BYTES = 8;

const directionKey = (
  shared: Uint8Array,
  salt: Uint8Array,
  direction: "a-to-b" | "b-to-a",
): string =>
  toHex(
    hkdf(
      sha256,
      shared,
      salt,
      utf8(`contact-chat/fixture-v1/${direction}`),
      32,
    ),
  );

/**
 * The keys of `role`'s side. The sender of the invitation writes on `T_AB`
 * and reads `T_BA`; the recipient the other way round.
 */
export function sessionKeys(
  role: "sender" | "recipient",
  shared: Uint8Array,
  invitation: string,
  response: string,
  topics: Topics,
): SessionKeys {
  const salt = digest("contact-chat/fixture-v1/salt", invitation, response);
  const aToB = directionKey(shared, salt, "a-to-b");
  const bToA = directionKey(shared, salt, "b-to-a");
  return role === "sender"
    ? { send: aToB, receive: bToA, out: topics.tAB, in: topics.tBA }
    : { send: bToA, receive: aToB, out: topics.tBA, in: topics.tAB };
}

function counterBytes(counter: number): Uint8Array {
  const bytes = new Uint8Array(COUNTER_BYTES);
  new DataView(bytes.buffer).setBigUint64(0, BigInt(counter));
  return bytes;
}

const nonceOf = (counter: Uint8Array): Uint8Array =>
  concat(new Uint8Array(12 - COUNTER_BYTES), counter);

/** Seal `plaintext` as frame `counter` on `topic`. A counter is used once per key. */
export function sealFrame(
  key: string,
  topic: string,
  counter: number,
  plaintext: Uint8Array,
): string {
  if (!Number.isSafeInteger(counter) || counter < 1)
    throw new Error("A frame counter starts at 1.");
  const count = counterBytes(counter);
  const sealed = chacha20poly1305(
    fromHex(key),
    nonceOf(count),
    concat(fromHex(topic), count),
  ).encrypt(plaintext);
  return toHex(concat(Uint8Array.of(FRAME_VERSION), count, sealed));
}

/** Open a frame read on `topic`, or null when it is not one sealed under `key` for it. */
export function openFrame(
  key: string,
  topic: string,
  frame: string,
): { counter: number; plaintext: Uint8Array } | null {
  let bytes: Uint8Array;
  try {
    bytes = fromHex(frame);
  } catch {
    return null;
  }
  if (bytes.length < 1 + COUNTER_BYTES + 16 || bytes[0] !== FRAME_VERSION)
    return null;
  const count = bytes.subarray(1, 1 + COUNTER_BYTES);
  const counter = Number(
    new DataView(count.buffer, count.byteOffset, COUNTER_BYTES).getBigUint64(0),
  );
  if (!Number.isSafeInteger(counter) || counter < 1) return null;
  try {
    const plaintext = chacha20poly1305(
      fromHex(key),
      nonceOf(count),
      concat(fromHex(topic), count),
    ).decrypt(bytes.subarray(1 + COUNTER_BYTES));
    return { counter, plaintext };
  } catch {
    return null;
  }
}
